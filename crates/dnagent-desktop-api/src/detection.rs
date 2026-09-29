//! Library features found in an open document, for the desktop's Detect features panel.
//! Matching is the shared engine's (`dnagent detect-features`); this adds only display
//! grouping (which proposals sit inside longer ones).
use crate::{Diagnostic, Direction};
use dnagent_app::library::{DEFAULT_MIN_LENGTH, detect_features, library_path};
use dnagent_domain::{SequenceRecord, Strand};
use dnagent_library::Library;
use serde::Serialize;
use std::path::Path;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct Proposal {
    /// Stable within one detection: `<library id>@<start><strand letter>`.
    pub key: String,
    #[ts(type = "number")]
    pub library_id: i64,
    pub name: String,
    pub kind: String,
    pub color: Option<String>,
    pub strand: Direction,
    pub start: u32,
    pub length: u32,
    /// Existing features with exactly this span and a compatible strand.
    pub annotated_as: Vec<String>,
    /// The first longer proposal whose span contains this one (e.g. "lac" in "lac promoter").
    pub contained_in: Option<String>,
    /// NCBI table and codon start from the library entry, for CDS proposals.
    pub translate: Option<(u32, u8)>,
    /// Shorter variants of the same library family that also match here (not listed).
    pub variants: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct DetectionResult {
    /// False when there is no library yet (see `message`).
    pub available: bool,
    pub message: String,
    pub library_features: u32,
    pub min_length: u32,
    pub proposals: Vec<Proposal>,
}

fn u32_of(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Offset of `inner`'s span inside `outer`'s (circular-aware), if it lies within it.
fn within(inner: (usize, usize), outer: (usize, usize), length: usize) -> bool {
    let offset = (inner.0 + length - outer.0) % length;
    offset + inner.1 <= outer.1
}

/// Detect with the library at `DNAGENT_FEATURE_DB` or the default location.
pub fn detect(record: &SequenceRecord) -> Result<DetectionResult, Diagnostic> {
    detect_with(record, &library_path())
}

pub fn detect_with(record: &SequenceRecord, path: &Path) -> Result<DetectionResult, Diagnostic> {
    let failed = |e: &dyn std::fmt::Display| Diagnostic {
        code: "library_failed".into(),
        message: e.to_string(),
    };
    if !path.exists() {
        return Ok(DetectionResult {
            available: false,
            message: format!(
                "No feature library yet. Build one with `dnagent library import <folder>` (expected at {}).",
                path.display()
            ),
            library_features: 0,
            min_length: u32_of(DEFAULT_MIN_LENGTH),
            proposals: Vec::new(),
        });
    }
    let library = Library::open(path).map_err(|e| failed(&e))?;
    let entries = library.entries().map_err(|e| failed(&e))?;
    let all = detect_features(record, &entries, DEFAULT_MIN_LENGTH).map_err(|e| failed(&e))?;
    // One proposal per place and family: shorter variants inside a longer match are folded in.
    let variants_of = |index: usize| {
        all.iter()
            .filter(|m| m.superseded_by == Some(index))
            .count()
    };
    let (matches, variant_counts): (Vec<_>, Vec<_>) = all
        .iter()
        .enumerate()
        .filter(|(_, m)| m.superseded_by.is_none())
        .map(|(i, m)| (m.clone(), u32_of(variants_of(i))))
        .unzip();
    let length = record.sequence().len();
    let spans: Vec<(usize, usize)> = matches
        .iter()
        .map(|m| (m.location.parts()[0].start().get(), m.length))
        .collect();
    let keys: Vec<String> = matches
        .iter()
        .zip(&spans)
        .map(|(m, (start, _))| {
            let letter = match m.strand {
                Strand::Forward => 'f',
                Strand::Reverse => 'r',
                Strand::Unknown => 'u',
            };
            format!("{}@{start}{letter}", m.library_id)
        })
        .collect();
    let entry_of = |id: i64| entries.iter().find(|e| e.id == id);
    let proposals = matches
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let contained_in = spans
                .iter()
                .enumerate()
                .find(|(j, outer)| {
                    *j != i && outer.1 > spans[i].1 && within(spans[i], **outer, length)
                })
                .map(|(j, _)| keys[j].clone());
            let qualifier = |key: &str| {
                entry_of(m.library_id)
                    .and_then(|e| e.qualifiers.iter().find(|q| q.key == key))
                    .and_then(|q| q.value.as_deref()?.trim().parse().ok())
            };
            Proposal {
                key: keys[i].clone(),
                library_id: m.library_id,
                name: m.name.clone(),
                kind: m.kind.clone(),
                color: m.color.clone(),
                strand: match m.strand {
                    Strand::Forward => Direction::Forward,
                    Strand::Reverse => Direction::Reverse,
                    Strand::Unknown => Direction::Unknown,
                },
                start: u32_of(spans[i].0),
                length: u32_of(spans[i].1),
                annotated_as: m.annotated_as.clone(),
                contained_in,
                variants: variant_counts[i],
                translate: m.kind.eq_ignore_ascii_case("CDS").then(|| {
                    (
                        qualifier("transl_table").unwrap_or(1),
                        qualifier("codon_start").map_or(1, |c: u32| u8::try_from(c).unwrap_or(1)),
                    )
                }),
            }
        })
        .collect();
    let info = library.info().map_err(|e| failed(&e))?;
    Ok(DetectionResult {
        available: true,
        message: format!("{} library features", info.features),
        library_features: u32::try_from(info.features).unwrap_or(u32::MAX),
        min_length: u32_of(DEFAULT_MIN_LENGTH),
        proposals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containment_is_circular_aware() {
        assert!(within((5, 3), (4, 10), 100));
        assert!(!within((5, 10), (4, 10), 100));
        assert!(
            within((1, 3), (98, 6), 100),
            "inside a span through the origin"
        );
        assert!(!within((10, 3), (98, 6), 100));
    }
}

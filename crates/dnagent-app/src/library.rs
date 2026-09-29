//! Building the feature library from sequence files, and detecting library features in a
//! record. Storage is `dnagent-library`; sequence identity and matching are the domain's.
use crate::{AppError, SEQUENCE_EXTENSIONS, import_path_bytes};
use dnagent_domain::feature_match::{Searcher, canonical, feature_sequence, reverse_complement};
use dnagent_domain::{
    Feature, Location, LocationOperator, Region, SequenceRecord, Strand, Topology,
};
use dnagent_formats::ImportWarning;
use dnagent_library::{
    Candidate, Entry, FamilyInput, Library, LibraryInfo, Qualifier, SourceOutcome,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Features shorter than this are not collected or detected by default: short motifs
/// match by chance and are better found by dedicated tools.
pub const DEFAULT_MIN_LENGTH: usize = 12;

/// `DNAGENT_FEATURE_DB`, else the platform data folder (`~/Library/Application Support/DNAgent/features.sqlite` on macOS).
#[must_use]
pub fn library_path() -> PathBuf {
    if let Some(path) = std::env::var_os("DNAGENT_FEATURE_DB") {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/DNAgent/features.sqlite")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map_or_else(|| home.join(".local/share"), PathBuf::from)
            .join("dnagent/features.sqlite")
    }
}

/// Why an annotated feature was not collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Shorter than the minimum length.
    TooShort,
    /// No label, or a placeholder such as "Feature 3" or the type name.
    GenericName,
    /// The label is just a DNA sequence (e.g. an unnamed primer).
    SequenceAsName,
    /// A base other than A, C, G or T.
    AmbiguousBases,
    /// GenBank `source` features describe the record, not a part.
    SourceFeature,
    /// Covers the whole molecule (a description of the construct, not a part).
    WholeMolecule,
    /// Some of its segments could not be imported (see the file's import warnings), so its
    /// bases are not the feature it names.
    IncompleteLocation,
}

fn generic_name(label: &str, kind: &str) -> bool {
    let name = label.trim().to_ascii_lowercase();
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit() || c.is_whitespace() || c == '#');
    name.is_empty()
        || !name.chars().any(char::is_alphanumeric)
        || name.starts_with("(null)")
        || name == kind.to_ascii_lowercase()
        || [
            "feature",
            "new feature",
            "misc_feature",
            "misc feature",
            "untitled",
            "unnamed",
            "region",
        ]
        .contains(&stem)
}

fn sequence_name(label: &str) -> bool {
    let label = label.trim();
    label.len() >= 8 && label.bytes().all(|b| b"ACGTNacgtn".contains(&b))
}

/// Feature ids whose import dropped segments. SnapGene warnings name the feature by its
/// 1-based number, which is also its id (`feature N` is `feature-000N`).
#[must_use]
pub fn incomplete_features(warnings: &[ImportWarning]) -> BTreeSet<String> {
    warnings
        .iter()
        .filter(|w| {
            matches!(
                w.code.as_str(),
                "snapgene_feature_range_unsupported" | "snapgene_feature_segment_missing_range"
            )
        })
        .filter_map(|w| {
            w.message
                .strip_prefix("feature ")?
                .split_whitespace()
                .next()?
                .parse::<usize>()
                .ok()
        })
        .map(|n| format!("feature-{n:04}"))
        .collect()
}

/// Candidates for the library from one record, and how many features were skipped why.
/// `incomplete` lists features whose import dropped segments ([`incomplete_features`]).
#[must_use]
pub fn candidates(
    record: &SequenceRecord,
    incomplete: &BTreeSet<String>,
    min_length: usize,
) -> (Vec<Candidate>, BTreeMap<SkipReason, usize>) {
    let mut kept = Vec::new();
    let mut skipped = BTreeMap::new();
    let length = record.sequence().len();
    for feature in record.features() {
        if incomplete.contains(feature.id().as_str()) {
            *skipped.entry(SkipReason::IncompleteLocation).or_insert(0) += 1;
            continue;
        }
        let size: usize = feature
            .location()
            .parts()
            .iter()
            .map(|p| p.length().get())
            .sum();
        let reason = if feature.kind().eq_ignore_ascii_case("source") {
            Some(SkipReason::SourceFeature)
        } else if size >= length {
            Some(SkipReason::WholeMolecule)
        } else if size < min_length {
            Some(SkipReason::TooShort)
        } else if generic_name(feature.label(), feature.kind()) {
            Some(SkipReason::GenericName)
        } else if sequence_name(feature.label()) {
            Some(SkipReason::SequenceAsName)
        } else {
            None
        };
        if let Some(reason) = reason {
            *skipped.entry(reason).or_insert(0) += 1;
            continue;
        }
        let Some((sequence, key)) =
            feature_sequence(record, feature).and_then(|s| canonical(&s).map(|k| (s, k)))
        else {
            *skipped.entry(SkipReason::AmbiguousBases).or_insert(0) += 1;
            continue;
        };
        kept.push(Candidate {
            label: feature.label().trim().to_owned(),
            kind: feature.kind().to_owned(),
            color: feature.display().color.clone(),
            qualifiers: feature
                .qualifiers()
                .iter()
                .map(|q| Qualifier {
                    key: q.key.clone(),
                    value: q.value.clone(),
                })
                .collect(),
            sequence,
            canonical: key,
            stranded: feature.location().strand() != Strand::Unknown,
            start: feature.location().parts()[0].start().get(),
            parts: feature.location().parts().len(),
        });
    }
    (kept, skipped)
}

/// One file's result.
#[derive(Debug, Clone, Serialize)]
pub struct FileImport {
    pub path: String,
    #[serde(flatten)]
    pub outcome: FileOutcome,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum FileOutcome {
    Stored {
        #[serde(flatten)]
        outcome: SourceOutcome,
        record_name: String,
        skipped_features: BTreeMap<SkipReason, usize>,
        /// Import-fidelity warning codes for this file.
        warnings: Vec<String>,
    },
    Failed {
        status: &'static str,
        reason: String,
    },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportTotals {
    pub files: usize,
    pub imported: usize,
    pub unchanged: usize,
    pub duplicate: usize,
    pub failed: usize,
    pub occurrences: usize,
    pub new_features: usize,
    pub skipped_features: BTreeMap<SkipReason, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryImport {
    pub library: LibraryInfo,
    pub min_length: usize,
    pub totals: ImportTotals,
    pub files: Vec<FileImport>,
}

/// Sequence files under `inputs`: named files as given; folders searched recursively for
/// sequence extensions, skipping hidden entries. Sorted and de-duplicated.
fn expand(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, AppError> {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) -> Result<(), AppError> {
        let entries = std::fs::read_dir(dir).map_err(|source| AppError::Read {
            path: dir.display().to_string(),
            source,
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(&path, found)?;
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| SEQUENCE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            {
                found.push(path);
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    for input in inputs {
        if input.is_dir() {
            walk(input, &mut found)?;
        } else {
            found.push(input.clone());
        }
    }
    let mut found: Vec<PathBuf> = found
        .into_iter()
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect();
    found.sort();
    found.dedup();
    Ok(found)
}

fn unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Import every sequence file under `inputs` into `library`. Unreadable and unsupported
/// files are reported per file, never silently skipped; they don't stop the import.
pub fn import_into_library(
    library: &mut Library,
    inputs: &[PathBuf],
    min_length: usize,
    rescan: bool,
) -> Result<LibraryImport, AppError> {
    let mut totals = ImportTotals::default();
    let mut files = Vec::new();
    for path in expand(inputs)? {
        totals.files += 1;
        let shown = path.display().to_string();
        let result = std::fs::read(&path)
            .map_err(|source| AppError::Read {
                path: shown.clone(),
                source,
            })
            .and_then(|bytes| import_path_bytes(&path, &bytes).map(|report| (bytes, report)));
        let (bytes, report) = match result {
            Ok(value) => value,
            Err(error) => {
                totals.failed += 1;
                files.push(FileImport {
                    path: shown,
                    outcome: FileOutcome::Failed {
                        status: "failed",
                        reason: error.to_string(),
                    },
                });
                continue;
            }
        };
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        let incomplete = incomplete_features(&report.warnings);
        let (kept, skipped) = candidates(&report.record, &incomplete, min_length);
        let outcome = library.import_source(
            &shown,
            &sha256,
            report.record.name(),
            unix_seconds(),
            &kept,
            rescan,
        )?;
        match &outcome {
            SourceOutcome::Imported {
                occurrences,
                new_features,
                ..
            } => {
                totals.imported += 1;
                totals.occurrences += occurrences;
                totals.new_features += new_features;
                for (reason, count) in &skipped {
                    *totals.skipped_features.entry(*reason).or_insert(0) += count;
                }
            }
            SourceOutcome::Unchanged => totals.unchanged += 1,
            SourceOutcome::Duplicate { .. } => totals.duplicate += 1,
        }
        let imported = matches!(outcome, SourceOutcome::Imported { .. });
        files.push(FileImport {
            path: shown,
            outcome: FileOutcome::Stored {
                outcome,
                record_name: report.record.name().to_owned(),
                skipped_features: if imported { skipped } else { BTreeMap::new() },
                warnings: report.warnings.iter().map(|w| w.code.clone()).collect(),
            },
        });
    }
    refresh_families(library)?;
    Ok(LibraryImport {
        library: library.info()?,
        min_length,
        totals,
        files,
    })
}

/// A library feature found in a record.
#[derive(Debug, Clone, Serialize)]
pub struct DetectedFeature {
    pub library_id: i64,
    pub name: String,
    pub kind: String,
    pub color: Option<String>,
    pub strand: Strand,
    pub location: Location,
    pub length: usize,
    /// Ids of existing features with exactly this span and a compatible strand.
    pub annotated_as: Vec<String>,
    pub qualifiers: Vec<Qualifier>,
    /// The library family (its head's id).
    pub family_id: i64,
    /// Index (in `matches`) of the longest match of the same family whose span contains
    /// this one: a shorter variant of the same part at the same place.
    pub superseded_by: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Detection {
    pub library: LibraryInfo,
    pub length: usize,
    pub topology: Topology,
    pub min_length: usize,
    pub matches: Vec<DetectedFeature>,
}

/// Start and length of a feature whose parts are consecutive (joined without gaps).
fn contiguous_span(feature: &Feature, length: usize) -> Option<(usize, usize)> {
    let parts = feature.location().parts();
    let mut end = parts[0].start().get();
    for part in parts {
        if part.start().get() != end % length {
            return None;
        }
        end = part.start().get() + part.length().get();
    }
    let start = parts[0].start().get();
    Some((start, parts.iter().map(|p| p.length().get()).sum()))
}

/// Exact occurrences (both strands, across the origin) of every library entry at least
/// `min_length` long, ordered by start, longer first, then name.
pub fn detect_features(
    record: &SequenceRecord,
    entries: &[Entry],
    min_length: usize,
) -> Result<Vec<DetectedFeature>, AppError> {
    let sequence = record.sequence().as_str();
    let length = sequence.len();
    let longest = entries.iter().map(|e| e.sequence.len()).max().unwrap_or(0);
    let searcher = Searcher::new(sequence, record.topology(), longest);
    let spans: Vec<(&Feature, Option<(usize, usize)>)> = record
        .features()
        .iter()
        .map(|f| (f, contiguous_span(f, length)))
        .collect();
    let mut found = Vec::new();
    for entry in entries.iter().filter(|e| e.sequence.len() >= min_length) {
        for hit in searcher.find(&entry.sequence) {
            let region = if hit.start + hit.length > length {
                Region::circular_arc(hit.start, hit.length, length)?
            } else {
                Region::linear(hit.start, hit.start + hit.length, length)?
            };
            // A strandless library feature has no direction to report.
            let strand = if entry.stranded {
                hit.strand
            } else {
                Strand::Unknown
            };
            let annotated_as = spans
                .iter()
                .filter(|(feature, span)| {
                    let existing = feature.location().strand();
                    *span == Some((hit.start, hit.length))
                        && (existing == Strand::Unknown
                            || strand == Strand::Unknown
                            || existing == strand)
                })
                .map(|(feature, _)| feature.id().as_str().to_owned())
                .collect();
            found.push(DetectedFeature {
                library_id: entry.id,
                name: entry.name.clone(),
                kind: entry.kind.clone(),
                color: entry.color.clone(),
                strand,
                location: Location::new(vec![region], strand, LocationOperator::Contiguous)?,
                length: hit.length,
                annotated_as,
                qualifiers: entry.qualifiers.clone(),
                family_id: entry.family_id,
                superseded_by: None,
            });
        }
    }
    found.sort_by(|a, b| {
        let start = |d: &DetectedFeature| d.location.parts()[0].start().get();
        start(a)
            .cmp(&start(b))
            .then(b.length.cmp(&a.length))
            .then_with(|| a.name.cmp(&b.name))
            .then(a.library_id.cmp(&b.library_id))
    });
    let spans: Vec<(usize, usize)> = found
        .iter()
        .map(|m| (m.location.parts()[0].start().get(), m.length))
        .collect();
    let length = record.sequence().len();
    for i in 0..found.len() {
        found[i].superseded_by = (0..found.len())
            .filter(|&j| {
                j != i
                    && found[j].family_id == found[i].family_id
                    && spans[j].1 > spans[i].1
                    && span_within(spans[i], spans[j], length)
            })
            .max_by_key(|&j| (spans[j].1, std::cmp::Reverse(j)));
    }
    Ok(found)
}

/// `inner` lies inside `outer` (start, length), wrapping on a circle of `length`.
#[must_use]
pub fn span_within(inner: (usize, usize), outer: (usize, usize), length: usize) -> bool {
    ((inner.0 + length - outer.0) % length) + inner.1 <= outer.1
}

/// A shorter feature joins a family when it is at least this share of the head's length.
pub const VARIANT_MIN_SHARE: (usize, usize) = (4, 5);

/// Variant families: features by length (longest first); each joins the longest family
/// head that contains its sequence on either strand and is at most 1.25× as long
/// (it is ≥ 80 % of the head), else it heads its own family. Standalone features head
/// their own. Returns (member, head) pairs for members only.
#[must_use]
pub fn compute_families(inputs: &[FamilyInput]) -> Vec<(i64, i64)> {
    let mut order: Vec<&FamilyInput> = inputs.iter().collect();
    order.sort_by(|a, b| {
        b.sequence
            .len()
            .cmp(&a.sequence.len())
            .then(a.id.cmp(&b.id))
    });
    let (num, den) = VARIANT_MIN_SHARE;
    let mut heads: Vec<&FamilyInput> = Vec::new();
    let mut members = Vec::new();
    for feature in order {
        let reverse = reverse_complement(&feature.sequence);
        let head = (!feature.standalone)
            .then(|| {
                heads.iter().find(|h| {
                    h.sequence.len() > feature.sequence.len()
                        && feature.sequence.len() * den >= h.sequence.len() * num
                        && (h.sequence.contains(&feature.sequence)
                            || reverse.as_deref().is_some_and(|r| h.sequence.contains(r)))
                })
            })
            .flatten();
        match head {
            Some(h) => members.push((feature.id, h.id)),
            None => heads.push(feature),
        }
    }
    members
}

/// Recompute families if imports or edits made them stale.
pub fn refresh_families(library: &mut Library) -> Result<(), AppError> {
    if library.families_stale()? {
        let families = compute_families(&library.family_inputs()?);
        library.set_families(&families)?;
    }
    Ok(())
}

/// Open an existing library with current families.
pub fn open_library(path: &Path) -> Result<Library, AppError> {
    let mut library = Library::open(path)?;
    refresh_families(&mut library)?;
    Ok(library)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_and_sequence_names() {
        for name in [
            "",
            "  ",
            "Feature 3",
            "feature",
            "New Feature 12",
            "misc_feature",
            "CDS",
            "untitled #2",
            "Region 1",
        ] {
            assert!(generic_name(name, "CDS"), "{name:?}");
        }
        for name in ["KanR", "Feature of interest", "lac promoter", "Region A"] {
            assert!(!generic_name(name, "CDS"), "{name:?}");
        }
        assert!(sequence_name("ACGTACGTAC") && sequence_name("acgtnnacgt"));
        assert!(!sequence_name("ACGTACG") && !sequence_name("AmpR"));
    }

    #[test]
    fn junk_names_are_generic() {
        for name in ["\"", "--", "(null) CDS", "(null)"] {
            assert!(generic_name(name, "CDS"), "{name:?}");
        }
    }

    #[test]
    fn families_group_close_nested_variants_only() {
        let seq = |s: &str| s.to_owned();
        let long = "ACGTTGCAAGGCTTACCGATCGATCGGATTACAGGCATTACGGATCGATTACGG"; // 54 bp
        let inputs = [
            FamilyInput {
                id: 1,
                sequence: seq(long),
                standalone: false,
            },
            FamilyInput {
                id: 2,
                sequence: seq(&long[2..50]),
                standalone: false,
            }, // 48/54 = 0.89: variant
            FamilyInput {
                id: 3,
                sequence: reverse_complement(&long[1..46]).unwrap(),
                standalone: false,
            }, // other strand, 45/54 = 0.83
            FamilyInput {
                id: 4,
                sequence: seq(&long[10..30]),
                standalone: false,
            }, // 20/54 = 0.37: a nested part, not a variant
            FamilyInput {
                id: 5,
                sequence: seq(&long[0..50]),
                standalone: true,
            }, // split off by hand
        ];
        let families = compute_families(&inputs);
        assert_eq!(families, vec![(2, 1), (3, 1)]);
        // Detection containment wraps on circles.
        assert!(span_within((1, 3), (98, 6), 100) && !span_within((10, 3), (98, 6), 100));
    }

    #[test]
    fn incomplete_features_come_from_segment_warnings() {
        let warnings = [
            ImportWarning::new(
                "snapgene_feature_range_unsupported",
                "feature 4 range \"6318-605\": wrapping range requires circular topology",
            ),
            ImportWarning::new(
                "snapgene_feature_segment_missing_range",
                "feature 12 contains a segment without a range",
            ),
            ImportWarning::new("snapgene_packet_not_interpreted", "feature 7 is unrelated"),
        ];
        let ids: Vec<String> = incomplete_features(&warnings).into_iter().collect();
        assert_eq!(ids, vec!["feature-0004", "feature-0012"]);
    }
}

//! Typed application use cases shared by CLI and GUI adapters.

use dnagent_domain::compatibility::{self, CompatibilityError, CompatibilityReport};
use dnagent_domain::digest::{self, Digest, DigestError};
use dnagent_domain::fragment_annotations::{self, AnnotatedDigest, AnnotationError};
use dnagent_domain::restriction::{self, RestrictionError, RestrictionScan};
use dnagent_domain::{ImportedPrimer, Location, Qualifier, SequenceRecord, Strand, Topology};
pub use dnagent_formats::genbank::ExportStrand;
use dnagent_formats::{ImportError, ImportReport, ImportWarning};
use serde::Serialize;
use std::path::Path;
use thiserror::Error;

/// Errors at the application boundary.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("unsupported input extension for {0}; milestone 1 accepts .dna")]
    UnsupportedExtension(String),
    #[error(transparent)]
    Import(#[from] ImportError),
    #[error(
        "strict mode rejected {count} warning(s); inspect the warnings or retry without --strict"
    )]
    ImportWarnings { count: usize },
    #[error(transparent)]
    Restriction(#[from] RestrictionError),
    #[error(transparent)]
    Digest(#[from] DigestError),
    #[error(transparent)]
    Compatibility(#[from] CompatibilityError),
    #[error(transparent)]
    Annotation(#[from] AnnotationError),
    #[error(transparent)]
    Genbank(#[from] dnagent_formats::genbank::GenbankError),
    #[error("invalid sequence range [{start}, {end}) for length {length}")]
    InvalidRange {
        start: usize,
        end: usize,
        length: usize,
    },
}

/// Open a supported sequence file and normalize it into the domain model.
pub fn open_path(path: &Path) -> Result<ImportReport, AppError> {
    let extension = path.extension().and_then(|value| value.to_str());
    if extension != Some("dna") {
        return Err(AppError::UnsupportedExtension(path.display().to_string()));
    }
    let bytes = std::fs::read(path).map_err(|source| AppError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("untitled");
    Ok(dnagent_format_snapgene::import_bytes(&bytes, name)?)
}

/// Reject any reported import limitation, including preserved uninterpreted metadata.
/// No claim of complete format support is made for warning-free imports.
pub fn require_warning_free_import(report: &ImportReport) -> Result<(), AppError> {
    if report.warnings.is_empty() {
        Ok(())
    } else {
        Err(AppError::ImportWarnings {
            count: report.warnings.len(),
        })
    }
}

/// Compact record projection used by terminal and GUI headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InspectView {
    pub name: String,
    pub length: usize,
    pub topology: Topology,
    pub feature_count: usize,
    pub primer_count: usize,
    pub warnings: Vec<ImportWarning>,
}

impl InspectView {
    /// Project an imported report without exposing opaque payload bytes.
    #[must_use]
    pub fn from_report(report: &ImportReport) -> Self {
        let record = &report.record;
        Self {
            name: record.name().to_owned(),
            length: record.sequence().len(),
            topology: record.topology(),
            feature_count: record.features().len(),
            primer_count: record.primers().len(),
            warnings: report.warnings.clone(),
        }
    }
}

/// Stable feature-list row shared by adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureView {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub strand: Strand,
    pub location: Location,
    pub color: Option<String>,
    pub qualifiers: Vec<Qualifier>,
}

/// Build deterministic feature-list rows in source order.
#[must_use]
pub fn feature_views(record: &SequenceRecord) -> Vec<FeatureView> {
    record
        .features()
        .iter()
        .map(|feature| FeatureView {
            id: feature.id().as_str().to_owned(),
            label: feature.label().to_owned(),
            kind: feature.kind().to_owned(),
            strand: feature.location().strand(),
            location: feature.location().clone(),
            color: feature.display().color.clone(),
            qualifiers: feature.qualifiers().to_vec(),
        })
        .collect()
}

/// Retained primer sequences in source order, not inferred binding sites or new designs.
#[must_use]
pub fn primer_views(record: &SequenceRecord) -> &[ImportedPrimer] {
    record.primers()
}

/// A restriction result plus new analysis warnings (import warnings remain on the input report).
pub struct RestrictionView {
    pub result: RestrictionScan,
    pub warnings: Vec<ImportWarning>,
}

pub fn restriction_sites(
    record: &SequenceRecord,
    names: &[String],
) -> Result<RestrictionView, AppError> {
    let result = restriction::find_sites(record.sequence(), record.topology(), names)?;
    let unavailable = result
        .sites
        .iter()
        .filter(|s| !s.cleavage_available)
        .count();
    let warnings = if unavailable == 0 {
        vec![]
    } else {
        vec![ImportWarning::new(
            "restriction_cut_out_of_bounds",
            format!(
                "{unavailable} recognition site(s) have cuts outside the linear molecule; no cleavage is inferred for those sites"
            ),
        )]
    };
    Ok(RestrictionView { result, warnings })
}

/// Complete sequence-only digest; fragment annotations are not propagated yet.
pub fn simulate_digest(record: &SequenceRecord, names: &[String]) -> Result<Digest, AppError> {
    Ok(digest::simulate_digest(
        record.sequence(),
        record.topology(),
        names,
    )?)
}

#[derive(Debug, Serialize)]
pub struct CompatibilityInput {
    pub name: String,
    pub digest: Digest,
}

#[derive(Debug, Serialize)]
pub struct CompatibilityView {
    pub inputs: Vec<CompatibilityInput>,
    pub analysis: CompatibilityReport,
}

/// Digest one or two records, then compare all distinct physical ends.
pub fn end_compatibility(
    inputs: &[(&SequenceRecord, &[String])],
) -> Result<CompatibilityView, AppError> {
    if !(1..=2).contains(&inputs.len()) {
        return Err(CompatibilityError::InputCount.into());
    }
    let digests = inputs
        .iter()
        .map(|(record, names)| simulate_digest(record, names))
        .collect::<Result<Vec<_>, _>>()?;
    let analysis = compatibility::compatible_ends(&digests)?;
    let inputs = inputs
        .iter()
        .zip(digests)
        .map(|((record, _), digest)| CompatibilityInput {
            name: record.name().to_owned(),
            digest,
        })
        .collect();
    Ok(CompatibilityView { inputs, analysis })
}

/// Feature projections on each product strand; original metadata is retained as provenance.
pub fn annotated_fragments(
    record: &SequenceRecord,
    names: &[String],
) -> Result<AnnotatedDigest, AppError> {
    Ok(fragment_annotations::annotated_digest(record, names)?)
}

/// Conservative GenBank selected-strand views; full provenance remains in JSON.
pub fn fragment_genbank(
    report: &AnnotatedDigest,
    strand: ExportStrand,
) -> Result<String, AppError> {
    Ok(dnagent_formats::genbank::export(report, strand)?)
}

/// Explicit strand FASTA, not an annotated or duplex-preserving exchange format.
#[must_use]
pub fn fragment_fasta(report: &AnnotatedDigest) -> String {
    use std::fmt::Write;
    let mut text = String::new();
    for fragment in &report.digest.fragments {
        for (label, strand) in [("top", &fragment.top), ("bottom", &fragment.bottom)] {
            writeln!(
                text,
                ">{}|{label} length={} source_start={} strand_sequence=5to3",
                fragment.id, strand.length, strand.source_start
            )
            .expect("writing to String cannot fail");
            for (i, base) in strand.sequence_5to3.chars().enumerate() {
                if i > 0 && i % 80 == 0 {
                    text.push('\n');
                }
                text.push(base);
            }
            text.push('\n');
        }
    }
    text
}

/// Checked sequence-range projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SequenceRangeView {
    pub start: usize,
    pub end: usize,
    pub sequence: String,
}

/// Extract a zero-based, half-open sequence range.
pub fn sequence_range(
    record: &SequenceRecord,
    start: usize,
    end: usize,
) -> Result<SequenceRangeView, AppError> {
    let sequence = record
        .sequence()
        .slice(start, end)
        .ok_or(AppError::InvalidRange {
            start,
            end,
            length: record.sequence().len(),
        })?;
    Ok(SequenceRangeView {
        start,
        end,
        sequence: sequence.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dnagent_domain::DnaSeq;

    #[test]
    fn warning_free_policy_rejects_even_preserved_metadata() {
        let mut report = ImportReport {
            record: SequenceRecord::new(
                "test",
                DnaSeq::new("ACGT").unwrap(),
                Topology::Linear,
                vec![],
                vec![],
            )
            .unwrap(),
            warnings: vec![],
            preserved_metadata: dnagent_formats::FormatExtensions::default(),
        };
        assert!(require_warning_free_import(&report).is_ok());
        report.warnings.push(ImportWarning::new(
            "snapgene_packet_not_interpreted",
            "preserved opaque packet",
        ));
        assert!(matches!(
            require_warning_free_import(&report),
            Err(AppError::ImportWarnings { count: 1 })
        ));
    }

    #[test]
    fn sequence_ranges_are_half_open() {
        let record = SequenceRecord::new(
            "test",
            DnaSeq::new("ACGT").unwrap(),
            Topology::Linear,
            vec![],
            vec![],
        )
        .unwrap();
        let view = sequence_range(&record, 1, 3).unwrap();
        assert_eq!(view.sequence, "CG");
        assert!(sequence_range(&record, 2, 5).is_err());
    }
}

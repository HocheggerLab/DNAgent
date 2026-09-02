//! Typed application use cases shared by CLI and GUI adapters.

use dnagent_domain::{Location, SequenceRecord, Strand, Topology};
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
        })
        .collect()
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

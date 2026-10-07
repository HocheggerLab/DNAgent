//! Record edits shared by the CLI (`annotate`) and the desktop session.
//! Every edit returns a new report; the caller keeps history. Nothing here writes files.
use crate::AppError;
use dnagent_domain::translation::{self, Translation};
use dnagent_domain::{
    DisplayHints, Feature, FeatureId, Location, LocationOperator, Qualifier, Region,
    SequenceRecord, Strand, Topology,
};
use dnagent_formats::{ImportReport, ImportWarning};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EditError {
    #[error("invalid range {start}..{end} for a {topology:?} molecule of length {length}")]
    InvalidRange {
        start: usize,
        end: usize,
        length: usize,
        topology: Topology,
    },
    #[error("feature label must not be empty")]
    EmptyLabel,
    #[error("feature kind must be a non-empty GenBank key without spaces")]
    InvalidKind,
    #[error("colour must be #rrggbb, got {0:?}")]
    InvalidColor(String),
    #[error("a translated feature needs a forward or reverse strand")]
    TranslateUnknownStrand,
    #[error("no feature with id {0:?}")]
    NoSuchFeature(String),
}

/// Translation settings for a new CDS.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TranslateSpec {
    pub table: u32,
    pub codon_start: u8,
}

/// A new single-part feature over `[start, end)`; `end < start` wraps on circular records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSpec {
    pub start: usize,
    pub end: usize,
    pub strand: Strand,
    pub kind: String,
    pub label: String,
    pub color: Option<String>,
    /// When set, the feature becomes a CDS with codon_start, transl_table and a computed
    /// /translation (without the terminal stop, as GenBank does).
    pub translate: Option<TranslateSpec>,
    /// A `/note` qualifier, e.g. where a detected feature came from.
    pub note: Option<String>,
}

/// What a new feature would look like, before it is added.
#[derive(Debug, Clone, Serialize)]
pub struct FeaturePreview {
    pub location: Location,
    pub length: usize,
    pub translation: Option<Translation>,
    pub warnings: Vec<ImportWarning>,
}

fn location_for(record: &SequenceRecord, spec: &FeatureSpec) -> Result<Location, AppError> {
    let length = record.sequence().len();
    let invalid = || EditError::InvalidRange {
        start: spec.start,
        end: spec.end,
        length,
        topology: record.topology(),
    };
    if spec.start >= length || spec.end > length || spec.start == spec.end {
        return Err(invalid().into());
    }
    let region = if spec.start < spec.end {
        Region::linear(spec.start, spec.end, length)?
    } else if record.topology() == Topology::Circular {
        Region::circular_arc(spec.start, length - spec.start + spec.end, length)?
    } else {
        return Err(invalid().into());
    };
    Ok(Location::new(
        vec![region],
        spec.strand,
        LocationOperator::Contiguous,
    )?)
}

fn validate(spec: &FeatureSpec) -> Result<&str, AppError> {
    if spec.label.trim().is_empty() {
        return Err(EditError::EmptyLabel.into());
    }
    let kind = if spec.translate.is_some() {
        "CDS"
    } else {
        spec.kind.trim()
    };
    if kind.is_empty() || kind.chars().any(char::is_whitespace) {
        return Err(EditError::InvalidKind.into());
    }
    if let Some(color) = &spec.color
        && !(color.len() == 7
            && color.starts_with('#')
            && color[1..].chars().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(EditError::InvalidColor(color.clone()).into());
    }
    if spec.translate.is_some() && spec.strand == Strand::Unknown {
        return Err(EditError::TranslateUnknownStrand.into());
    }
    Ok(kind)
}

/// codon_start / transl_table / computed translation qualifiers, with preview warnings.
fn cds_qualifiers(
    record: &SequenceRecord,
    spec: &FeatureSpec,
    settings: TranslateSpec,
    id: &FeatureId,
    location: &Location,
) -> Result<(Vec<Qualifier>, Vec<ImportWarning>, Translation), AppError> {
    let mut qualifiers = vec![
        Qualifier {
            key: "codon_start".into(),
            value: Some(settings.codon_start.to_string()),
        },
        Qualifier {
            key: "transl_table".into(),
            value: Some(settings.table.to_string()),
        },
    ];
    let draft = Feature::new(
        id.clone(),
        "CDS",
        spec.label.trim(),
        location.clone(),
        qualifiers.clone(),
        DisplayHints::default(),
    );
    let result = translation::translate_feature(record.sequence(), &draft, None)?;
    let mut warnings = Vec::new();
    if !result.internal_stops.is_empty() {
        warnings.push(ImportWarning::new(
            "translation_internal_stop",
            format!(
                "{} internal stop codon(s), first at codon {}",
                result.internal_stops.len(),
                result.internal_stops[0] + 1
            ),
        ));
    }
    if result.trailing_bases > 0 {
        warnings.push(ImportWarning::new(
            "translation_incomplete_codon",
            format!(
                "length leaves {} base(s) after the last codon",
                result.trailing_bases
            ),
        ));
    }
    if !result.terminal_stop {
        warnings.push(ImportWarning::new(
            "translation_no_terminal_stop",
            "the feature does not end with a stop codon",
        ));
    }
    if !result.starts_with_atg && !result.initiator_as_methionine {
        warnings.push(ImportWarning::new(
            "translation_no_start_codon",
            format!(
                "first codon is {}, not a start codon",
                result.codons[0].codon
            ),
        ));
    }
    let protein = result
        .protein
        .strip_suffix('*')
        .unwrap_or(&result.protein)
        .to_owned();
    qualifiers.push(Qualifier {
        key: "translation".into(),
        value: Some(protein),
    });
    Ok((qualifiers, warnings, result))
}

fn build_feature(
    record: &SequenceRecord,
    spec: &FeatureSpec,
    id: FeatureId,
) -> Result<(Feature, FeaturePreview), AppError> {
    let kind = validate(spec)?;
    let location = location_for(record, spec)?;
    let (qualifiers, warnings, shown) = match spec.translate {
        Some(settings) => {
            let (qualifiers, warnings, result) =
                cds_qualifiers(record, spec, settings, &id, &location)?;
            (qualifiers, warnings, Some(result))
        }
        None => (Vec::new(), Vec::new(), None),
    };
    let mut qualifiers = qualifiers;
    if let Some(note) = spec
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        qualifiers.push(Qualifier {
            key: "note".into(),
            value: Some(note.to_owned()),
        });
    }
    let length = location.parts().iter().map(|p| p.length().get()).sum();
    let feature = Feature::new(
        id,
        kind,
        spec.label.trim(),
        location.clone(),
        qualifiers,
        DisplayHints {
            color: spec.color.clone(),
        },
    );
    Ok((
        feature,
        FeaturePreview {
            location,
            length,
            translation: shown,
            warnings,
        },
    ))
}

fn next_id(record: &SequenceRecord) -> FeatureId {
    let taken: std::collections::HashSet<&str> =
        record.features().iter().map(|f| f.id().as_str()).collect();
    let mut n = record.features().len() + 1;
    while taken.contains(format!("feature-{n:04}").as_str()) {
        n += 1;
    }
    FeatureId::new(format!("feature-{n:04}")).expect("non-empty id")
}

/// Preview a feature without changing the record.
pub fn preview_feature(
    record: &SequenceRecord,
    spec: &FeatureSpec,
) -> Result<FeaturePreview, AppError> {
    Ok(build_feature(record, spec, next_id(record))?.1)
}

fn with_features(report: &ImportReport, features: Vec<Feature>) -> Result<ImportReport, AppError> {
    let record = &report.record;
    Ok(ImportReport {
        record: SequenceRecord::new(
            record.name(),
            record.sequence().clone(),
            record.topology(),
            features,
            record.primers().to_vec(),
        )?,
        warnings: report.warnings.clone(),
        preserved_metadata: report.preserved_metadata.clone(),
    })
}

/// Append a new feature (source order: last). Returns the new report, id and preview.
pub fn add_feature(
    report: &ImportReport,
    spec: &FeatureSpec,
) -> Result<(ImportReport, String, FeaturePreview), AppError> {
    let id = next_id(&report.record);
    let (feature, preview) = build_feature(&report.record, spec, id.clone())?;
    let mut features = report.record.features().to_vec();
    features.push(feature);
    Ok((
        with_features(report, features)?,
        id.as_str().to_owned(),
        preview,
    ))
}

/// Remove a feature by id.
pub fn remove_feature(report: &ImportReport, feature_id: &str) -> Result<ImportReport, AppError> {
    let features: Vec<Feature> = report
        .record
        .features()
        .iter()
        .filter(|f| f.id().as_str() != feature_id)
        .cloned()
        .collect();
    if features.len() == report.record.features().len() {
        return Err(EditError::NoSuchFeature(feature_id.to_owned()).into());
    }
    with_features(report, features)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dnagent_domain::DnaSeq;
    use dnagent_formats::FormatExtensions;

    fn report(sequence: &str, topology: Topology) -> ImportReport {
        ImportReport {
            record: SequenceRecord::new(
                "t",
                DnaSeq::new(sequence).unwrap(),
                topology,
                vec![],
                vec![],
            )
            .unwrap(),
            warnings: vec![],
            preserved_metadata: FormatExtensions::default(),
        }
    }

    fn spec(start: usize, end: usize, strand: Strand, translate: bool) -> FeatureSpec {
        FeatureSpec {
            start,
            end,
            strand,
            kind: "misc_feature".into(),
            label: "new".into(),
            color: None,
            translate: translate.then_some(TranslateSpec {
                table: 1,
                codon_start: 1,
            }),
            note: None,
        }
    }

    #[test]
    fn translated_features_become_cds_with_computed_translation() {
        let base = report("CCATGGCCTAACC", Topology::Linear);
        let (next, id, preview) = add_feature(&base, &spec(2, 11, Strand::Forward, true)).unwrap();
        assert_eq!(id, "feature-0001");
        let feature = &next.record.features()[0];
        assert_eq!(feature.kind(), "CDS");
        let keys: Vec<(&str, Option<&str>)> = feature
            .qualifiers()
            .iter()
            .map(|q| (q.key.as_str(), q.value.as_deref()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("codon_start", Some("1")),
                ("transl_table", Some("1")),
                ("translation", Some("MA"))
            ]
        );
        assert_eq!(preview.warnings, [] as [dnagent_formats::ImportWarning; 0]);
        assert_eq!(preview.translation.unwrap().protein, "MA*");
    }

    #[test]
    fn previews_warn_about_frames_stops_and_starts() {
        // TAA ATG CCC G: internal stop, one trailing base, no terminal stop, no start.
        let base = report("TAAATGCCCG", Topology::Linear);
        let preview = preview_feature(&base.record, &spec(0, 10, Strand::Forward, true)).unwrap();
        let codes: Vec<&str> = preview.warnings.iter().map(|w| w.code.as_str()).collect();
        assert_eq!(
            codes,
            [
                "translation_internal_stop",
                "translation_incomplete_codon",
                "translation_no_terminal_stop",
                "translation_no_start_codon"
            ]
        );
        assert!(preview_feature(&base.record, &spec(0, 10, Strand::Unknown, true)).is_err());
    }

    #[test]
    fn wrapping_ranges_need_circular_records_and_ids_stay_unique() {
        let circular = report("ATGAAATAAGGG", Topology::Circular);
        let (next, _, preview) =
            add_feature(&circular, &spec(9, 3, Strand::Forward, false)).unwrap();
        assert_eq!(
            preview.location.parts(),
            &[Region::circular_arc(9, 6, 12).unwrap()]
        );
        let (next, second, _) = add_feature(&next, &spec(0, 3, Strand::Reverse, false)).unwrap();
        assert_eq!(second, "feature-0002");
        let removed = remove_feature(&next, "feature-0001").unwrap();
        let (_, third, _) = add_feature(&removed, &spec(0, 3, Strand::Forward, false)).unwrap();
        assert_eq!(third, "feature-0003"); // feature-0002 is still taken
        assert!(
            add_feature(
                &report("ATGAAATAAGGG", Topology::Linear),
                &spec(9, 3, Strand::Forward, false)
            )
            .is_err()
        );
        assert!(remove_feature(&circular, "missing").is_err());
    }

    #[test]
    fn invalid_specs_are_rejected() {
        let base = report("ATGAAATAAGGG", Topology::Linear);
        let mut bad = spec(0, 3, Strand::Forward, false);
        bad.label = " ".into();
        assert!(add_feature(&base, &bad).is_err());
        let mut bad = spec(0, 3, Strand::Forward, false);
        bad.kind = "two words".into();
        assert!(add_feature(&base, &bad).is_err());
        let mut bad = spec(0, 3, Strand::Forward, false);
        bad.color = Some("red".into());
        assert!(add_feature(&base, &bad).is_err());
        assert!(add_feature(&base, &spec(3, 3, Strand::Forward, false)).is_err());
    }
}

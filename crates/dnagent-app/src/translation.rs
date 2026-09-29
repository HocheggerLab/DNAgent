//! Translation and ORF services shared by the CLI and the desktop API.
use crate::AppError;
use dnagent_domain::SequenceRecord;
use dnagent_domain::translation::{
    self, CodingStrand, GC_PRT_SHA256, GC_PRT_VERSION, ImportedComparison, OrfScan, StartPolicy,
    Translation,
};
use dnagent_formats::ImportWarning;
use serde::Serialize;

/// Provenance of the genetic-code tables used for every translation result.
#[derive(Debug, Clone, Serialize)]
pub struct GeneticCodeSource {
    pub provider: &'static str,
    pub version: &'static str,
    pub sha256: &'static str,
}

pub const GENETIC_CODE_SOURCE: GeneticCodeSource = GeneticCodeSource {
    provider: "NCBI gc.prt",
    version: GC_PRT_VERSION,
    sha256: GC_PRT_SHA256,
};

/// A feature's translation with its imported-translation comparison.
#[derive(Debug, Clone, Serialize)]
pub struct FeatureTranslation {
    pub feature_id: String,
    pub label: String,
    pub kind: String,
    #[serde(flatten)]
    pub translation: Translation,
    /// Comparison with the imported `translation` qualifier, when present.
    pub imported_translation: Option<ImportedComparison>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureTranslationView {
    pub mode: &'static str,
    #[serde(flatten)]
    pub feature: FeatureTranslation,
    pub genetic_code_source: GeneticCodeSource,
}

#[derive(Debug, Clone, Serialize)]
pub struct RangeTranslationView {
    pub mode: &'static str,
    pub start: usize,
    pub end: usize,
    #[serde(flatten)]
    pub translation: Translation,
    pub genetic_code_source: GeneticCodeSource,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedFeature {
    pub feature_id: String,
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CdsTranslationsView {
    pub mode: &'static str,
    pub translations: Vec<FeatureTranslation>,
    /// CDS features that could not be translated, with the reason.
    pub skipped: Vec<SkippedFeature>,
    pub genetic_code_source: GeneticCodeSource,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrfView {
    #[serde(flatten)]
    pub scan: OrfScan,
    pub genetic_code_source: GeneticCodeSource,
}

fn feature_warnings(item: &FeatureTranslation) -> Vec<ImportWarning> {
    let name = if item.label.is_empty() {
        &item.feature_id
    } else {
        &item.label
    };
    let t = &item.translation;
    let mut warnings = Vec::new();
    if !t.internal_stops.is_empty() {
        warnings.push(ImportWarning::new(
            "translation_internal_stop",
            format!(
                "{name}: {} internal stop codon(s), first at codon {}",
                t.internal_stops.len(),
                t.internal_stops[0] + 1
            ),
        ));
    }
    if t.trailing_bases > 0 {
        warnings.push(ImportWarning::new(
            "translation_incomplete_codon",
            format!(
                "{name}: {} coding base(s) after the last complete codon",
                t.trailing_bases
            ),
        ));
    }
    if t.ambiguous_codons > 0 {
        warnings.push(ImportWarning::new(
            "translation_ambiguous_codons",
            format!(
                "{name}: {} codon(s) translated as X because of ambiguous bases",
                t.ambiguous_codons
            ),
        ));
    }
    if let Some(comparison) = item.imported_translation.as_ref().filter(|c| !c.matches) {
        warnings.push(ImportWarning::new(
            "translation_imported_mismatch",
            format!(
                "{name}: computed translation differs from the imported translation at residue {}",
                comparison.first_difference.map_or(0, |i| i + 1)
            ),
        ));
    }
    warnings
}

fn translate_one(
    record: &SequenceRecord,
    feature: &dnagent_domain::Feature,
    table: Option<u32>,
) -> Result<FeatureTranslation, AppError> {
    let translation = translation::translate_feature(record.sequence(), feature, table)?;
    let imported_translation = translation::imported_translation(feature)
        .map(|imported| translation::compare_imported(&translation, &imported));
    Ok(FeatureTranslation {
        feature_id: feature.id().as_str().to_owned(),
        label: feature.label().to_owned(),
        kind: feature.kind().to_owned(),
        translation,
        imported_translation,
    })
}

/// Translate one feature by id. Warnings describe the translation, not the import.
pub fn translate_feature(
    record: &SequenceRecord,
    feature_id: &str,
    table: Option<u32>,
) -> Result<(FeatureTranslationView, Vec<ImportWarning>), AppError> {
    let feature = record
        .features()
        .iter()
        .find(|f| f.id().as_str() == feature_id)
        .ok_or_else(|| AppError::FeatureNotFound(feature_id.to_owned()))?;
    let item = translate_one(record, feature, table)?;
    let warnings = feature_warnings(&item);
    Ok((
        FeatureTranslationView {
            mode: "feature",
            feature: item,
            genetic_code_source: GENETIC_CODE_SOURCE,
        },
        warnings,
    ))
}

/// Translate every feature whose kind is `CDS` (case-insensitive), in source order.
#[must_use]
pub fn translate_cds_features(
    record: &SequenceRecord,
    table: Option<u32>,
) -> (CdsTranslationsView, Vec<ImportWarning>) {
    let mut translations = Vec::new();
    let mut skipped = Vec::new();
    let mut warnings = Vec::new();
    for feature in record
        .features()
        .iter()
        .filter(|f| f.kind().eq_ignore_ascii_case("CDS"))
    {
        match translate_one(record, feature, table) {
            Ok(item) => {
                warnings.extend(feature_warnings(&item));
                translations.push(item);
            }
            Err(error) => {
                warnings.push(ImportWarning::new(
                    "translation_skipped",
                    format!("{}: {error}", feature.id().as_str()),
                ));
                skipped.push(SkippedFeature {
                    feature_id: feature.id().as_str().to_owned(),
                    label: feature.label().to_owned(),
                    reason: error.to_string(),
                });
            }
        }
    }
    (
        CdsTranslationsView {
            mode: "all_cds",
            translations,
            skipped,
            genetic_code_source: GENETIC_CODE_SOURCE,
        },
        warnings,
    )
}

/// Translate `[start, end)` (wrapping when `end < start` on circular records).
pub fn translate_range(
    record: &SequenceRecord,
    start: usize,
    end: usize,
    strand: CodingStrand,
    frame: usize,
    table: u32,
) -> Result<RangeTranslationView, AppError> {
    let translation = translation::translate_range(
        record.sequence(),
        record.topology(),
        start,
        end,
        strand,
        frame,
        table,
    )?;
    Ok(RangeTranslationView {
        mode: "range",
        start,
        end,
        translation,
        genetic_code_source: GENETIC_CODE_SOURCE,
    })
}

/// Six-frame ORF search with explicit parameters.
pub fn find_orfs(
    record: &SequenceRecord,
    table: u32,
    min_codons: usize,
    starts: StartPolicy,
) -> Result<OrfView, AppError> {
    let scan = translation::find_orfs(
        record.sequence(),
        record.topology(),
        table,
        min_codons,
        starts,
    )?;
    Ok(OrfView {
        scan,
        genetic_code_source: GENETIC_CODE_SOURCE,
    })
}

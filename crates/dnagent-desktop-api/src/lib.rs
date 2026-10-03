//! Small, explicit desktop transport boundary; not a serialization of domain internals.
use dnagent_domain::translation::{self, CodingStrand, StartPolicy};
use dnagent_domain::{Strand, Topology};
use serde::Serialize;
use std::path::Path;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, TS)]
pub struct Segment {
    pub start: u32,
    pub length: u32,
}

#[derive(Debug, Serialize, TS)]
pub struct Feature {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub color: Option<String>,
    pub strand: Direction,
    /// Ordered source parts. Circular parts may cross the origin.
    pub parts: Vec<Segment>,
}

#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Forward,
    Reverse,
    Unknown,
}

#[derive(Debug, Serialize, TS)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize, TS)]
pub struct UnplacedPrimer {
    pub name: String,
    pub sequence_5to3: String,
    pub description: Option<String>,
}

/// Engine translation of one CDS feature, for display under the sequence.
#[derive(Debug, Serialize, TS)]
pub struct FeatureTranslation {
    pub feature_id: String,
    pub table: u8,
    /// One letter per codon; stops are `*`.
    pub protein: String,
    /// Reference positions of each codon's three coding bases, flattened (3 per codon).
    pub codon_positions: Vec<u32>,
    pub initiator_as_methionine: bool,
    /// Comparison with the imported `translation` qualifier; null when absent.
    pub imported_matches: Option<bool>,
    pub warnings: Vec<Diagnostic>,
}

/// One whole-molecule reading frame (table 1). Forward codon `i` covers
/// `first + 3i ..= first + 3i + 2`; reverse codon `i` covers `first - 3i - 2 ..= first - 3i`.
/// Codons crossing the origin are omitted.
#[derive(Debug, Serialize, TS)]
pub struct FrameTranslation {
    pub strand: Direction,
    pub offset: u32,
    pub first: u32,
    pub protein: String,
}

/// A complete ORF (start to stop, table 1, ATG starts). `start`/`length` may wrap.
#[derive(Debug, Serialize, TS)]
pub struct Orf {
    pub id: String,
    pub strand: Direction,
    pub start: u32,
    pub length: u32,
    /// Amino acids excluding the stop.
    pub codons: u32,
    pub protein: String,
}

#[derive(Debug, Serialize, TS)]
pub struct Document {
    pub name: String,
    pub sequence: String,
    /// Complement aligned to forward coordinates, left-to-right 3′→5′.
    pub aligned_complement_3to5: String,
    /// Imported oligos only; binding coordinates are not retained by the importer.
    pub unplaced_primers: Vec<UnplacedPrimer>,
    pub circular: bool,
    pub features: Vec<Feature>,
    pub warnings: Vec<Diagnostic>,
    /// CDS translations in source order; untranslatable CDSs are listed in `translation_skipped`.
    pub translations: Vec<FeatureTranslation>,
    pub translation_skipped: Vec<Diagnostic>,
    /// Frames +1, +2, +3, -1, -2, -3.
    pub frames: Vec<FrameTranslation>,
    /// ORFs of at least `orf_min_codons`; the GUI may filter to longer ones.
    pub orfs: Vec<Orf>,
    pub orf_min_codons: u32,
    /// Isoforms, evidence and expression when the document is a gene locus.
    pub locus: Option<isoforms::LocusView>,
    /// Set for records over 100,000 bases (only gene loci open at that size): `frames` and
    /// `orfs` are empty, and the Sequence view shows at most this many bases around the selection.
    pub sequence_window: Option<u32>,
}

/// Lower bound for ORFs sent to the GUI (which filters upwards, default 75).
const ORF_MIN_CODONS: usize = 30;

fn direction(strand: CodingStrand) -> Direction {
    match strand {
        CodingStrand::Forward => Direction::Forward,
        CodingStrand::Reverse => Direction::Reverse,
    }
}

fn u32_of(value: usize) -> u32 {
    u32::try_from(value).expect("bounded record")
}

/// Prototype limit bounds frontend rendering; not a biological restriction.
const MAX_BASES: usize = 100_000;
/// Gene loci may be larger (the longest human genes are ~2.5 Mb with flanks): frames and
/// ORFs are then not computed and the Sequence view shows a window around the selection.
const LOCUS_MAX_BASES: usize = 3_000_000;
/// Most bases the Sequence view renders at once for a record over [`MAX_BASES`].
const SEQUENCE_WINDOW: usize = 50_000;

pub mod agent;
pub mod detection;
pub mod isoforms;
pub mod restriction;
pub mod session;

pub fn open_document(path: &Path) -> Result<Document, Diagnostic> {
    let report = dnagent_app::open_path(path).map_err(|error| Diagnostic {
        code: "import_failed".into(),
        message: error.to_string(),
    })?;
    document_from_report(&report)
}

/// Project an imported (or edited) report into the transport document.
pub fn document_from_report(
    report: &dnagent_formats::ImportReport,
) -> Result<Document, Diagnostic> {
    let record = &report.record;
    let length = record.sequence().len();
    let locus = report.preserved_metadata.locus.is_some();
    if length > MAX_BASES && !(locus && length <= LOCUS_MAX_BASES) {
        return Err(Diagnostic {
            code: "prototype_size_limit".into(),
            message: if locus {
                format!(
                    "The prototype viewer opens gene loci up to {LOCUS_MAX_BASES} bases (this one has {length}); use the CLI"
                )
            } else {
                format!(
                    "The prototype viewer supports at most {MAX_BASES} bases (gene loci up to {LOCUS_MAX_BASES})"
                )
            },
        });
    }
    let large = length > MAX_BASES;
    let features = record
        .features()
        .iter()
        .map(|feature| Feature {
            id: feature.id().as_str().into(),
            label: feature.label().into(),
            kind: feature.kind().into(),
            color: feature.display().color.clone(),
            strand: match feature.location().strand() {
                Strand::Forward => Direction::Forward,
                Strand::Reverse => Direction::Reverse,
                Strand::Unknown => Direction::Unknown,
            },
            parts: feature
                .location()
                .parts()
                .iter()
                .map(|part| Segment {
                    start: u32::try_from(part.start().get()).expect("bounded record"),
                    length: u32::try_from(part.length().get()).expect("bounded record"),
                })
                .collect(),
        })
        .collect();
    let (translations, translation_skipped) = translation_dtos(record);
    let (frames, orfs) = if large {
        (Vec::new(), Vec::new())
    } else {
        (frame_dtos(record), orf_dtos(record))
    };
    Ok(Document {
        name: record.name().into(),
        sequence: record.sequence().as_str().into(),
        aligned_complement_3to5: record.sequence().aligned_complement_3to5(),
        unplaced_primers: dnagent_app::primer_views(record)
            .iter()
            .map(|primer| UnplacedPrimer {
                name: primer.name.clone(),
                sequence_5to3: primer.sequence.as_str().into(),
                description: primer.description.clone(),
            })
            .collect(),
        circular: record.topology() == Topology::Circular,
        features,
        warnings: report
            .warnings
            .iter()
            .cloned()
            .map(|warning| Diagnostic {
                code: warning.code,
                message: warning.message,
            })
            .collect(),
        translations,
        translation_skipped,
        frames,
        orfs,
        orf_min_codons: u32_of(ORF_MIN_CODONS),
        locus: isoforms::locus_view(report),
        sequence_window: large.then(|| u32_of(SEQUENCE_WINDOW)),
    })
}

fn translation_dtos(
    record: &dnagent_domain::SequenceRecord,
) -> (Vec<FeatureTranslation>, Vec<Diagnostic>) {
    let (cds, _) = dnagent_app::translation::translate_cds_features(record, None);
    let translations = cds
        .translations
        .iter()
        .map(|item| {
            let t = &item.translation;
            let mut warnings = Vec::new();
            if !t.internal_stops.is_empty() {
                warnings.push(Diagnostic {
                    code: "translation_internal_stop".into(),
                    message: format!("{} internal stop codon(s)", t.internal_stops.len()),
                });
            }
            if t.trailing_bases > 0 {
                warnings.push(Diagnostic {
                    code: "translation_incomplete_codon".into(),
                    message: format!("{} trailing base(s)", t.trailing_bases),
                });
            }
            if let Some(comparison) = item.imported_translation.as_ref().filter(|c| !c.matches) {
                warnings.push(Diagnostic {
                    code: "translation_imported_mismatch".into(),
                    message: format!(
                        "differs from the imported translation at residue {}",
                        comparison.first_difference.map_or(0, |i| i + 1)
                    ),
                });
            }
            FeatureTranslation {
                feature_id: item.feature_id.clone(),
                table: t.table,
                protein: t.protein.clone(),
                codon_positions: t
                    .codons
                    .iter()
                    .flat_map(|c| c.positions.map(u32_of))
                    .collect(),
                initiator_as_methionine: t.initiator_as_methionine,
                imported_matches: item.imported_translation.as_ref().map(|c| c.matches),
                warnings,
            }
        })
        .collect();
    let translation_skipped = cds
        .skipped
        .iter()
        .map(|skip| Diagnostic {
            code: "translation_skipped".into(),
            message: format!("{}: {}", skip.feature_id, skip.reason),
        })
        .collect();
    (translations, translation_skipped)
}

fn frame_dtos(record: &dnagent_domain::SequenceRecord) -> Vec<FrameTranslation> {
    translation::six_frames(record.sequence(), 1)
        .expect("table 1 exists")
        .into_iter()
        .map(|frame| FrameTranslation {
            strand: direction(frame.strand),
            offset: u32_of(frame.offset),
            first: u32_of(frame.first),
            protein: frame.protein,
        })
        .collect()
}

fn orf_dtos(record: &dnagent_domain::SequenceRecord) -> Vec<Orf> {
    translation::find_orfs(
        record.sequence(),
        record.topology(),
        1,
        ORF_MIN_CODONS,
        StartPolicy::AtgOnly,
    )
    .expect("valid ORF parameters")
    .orfs
    .into_iter()
    .map(|orf| Orf {
        id: orf.id,
        strand: direction(orf.strand),
        start: u32_of(orf.start),
        length: u32_of(orf.length),
        codons: u32_of(orf.codons),
        protein: orf.protein,
    })
    .collect()
}

/// Generated from Rust DTOs; the frontend must not maintain parallel definitions.
#[must_use]
pub fn typescript() -> String {
    let declarations = [
        Segment::decl(),
        isoforms::CodonMark::decl(),
        isoforms::CellValue::decl(),
        isoforms::IsoformExpression::decl(),
        isoforms::MappingInfo::decl(),
        isoforms::Isoform::decl(),
        isoforms::QuantifierPanel::decl(),
        isoforms::LocusView::decl(),
        Direction::decl(),
        Feature::decl(),
        Diagnostic::decl(),
        UnplacedPrimer::decl(),
        FeatureTranslation::decl(),
        FrameTranslation::decl(),
        Orf::decl(),
        Document::decl(),
        session::EditState::decl(),
        session::DocumentState::decl(),
        session::TranslateRequest::decl(),
        session::FeatureRequest::decl(),
        session::FeaturePreview::decl(),
        session::SaveResult::decl(),
        session::RangeRequest::decl(),
        session::HandoffItem::decl(),
        session::HandoffResult::decl(),
        session::FileStamp::decl(),
        session::ViewReport::decl(),
        detection::Proposal::decl(),
        detection::DetectionResult::decl(),
        restriction::EnzymeInfo::decl(),
        restriction::EnzymeCatalogueInfo::decl(),
        restriction::EnzymeCount::decl(),
        restriction::Site::decl(),
        restriction::FragmentEndInfo::decl(),
        restriction::Fragment::decl(),
    ];
    let mut result = String::from(
        "// Generated by cargo run -p dnagent-desktop-api --example export_types. Do not edit.\n",
    );
    for declaration in declarations {
        result.push_str("export ");
        for line in declaration.lines() {
            result.push_str(line.trim_end());
            result.push('\n');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_contract_is_current() {
        assert_eq!(
            typescript(),
            include_str!("../../../desktop/src/bindings.ts")
        );
    }
    #[test]
    fn imports_multipart_and_returns_structured_failure() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/formats/snapgene");
        let doc = open_document(&root.join("synthetic_linear.dna")).unwrap();
        assert!(!doc.circular);
        assert_eq!(doc.unplaced_primers.len(), 1);
        assert_eq!(doc.unplaced_primers[0].name, "synthetic primer");
        assert_eq!(doc.unplaced_primers[0].sequence_5to3, "ACGTN");
        assert_eq!(
            doc.unplaced_primers[0].description.as_deref(),
            Some("retained description")
        );
        assert_eq!(doc.aligned_complement_3to5.len(), doc.sequence.len());
        assert_eq!(
            doc.aligned_complement_3to5,
            dnagent_domain::DnaSeq::new(&doc.sequence)
                .unwrap()
                .aligned_complement_3to5()
        );
        assert!(
            doc.features
                .iter()
                .any(|f| f.parts.len() > 1 && matches!(f.strand, Direction::Reverse))
        );
        assert_eq!(
            open_document(&root.join("missing.dna")).unwrap_err().code,
            "import_failed"
        );
    }
}

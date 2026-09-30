//! The isoform (locus) view for the desktop, projected from `dnagent_app::locus`.
//! Ordering, colouring and panel means are the app layer's; the frontend only draws.
use crate::{Direction, Segment};
use dnagent_app::locus::{self as app, Display};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct CodonMark {
    /// First base of the codon in transcription direction (zero-based on the locus).
    pub position: u32,
    pub split: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CellValue {
    pub cell_line: String,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub n_libraries: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct IsoformExpression {
    pub quantifier: String,
    /// Mean across the panel's cell lines; null without data.
    pub panel_mean: Option<f64>,
    /// `expressed`, `not_detected`, `discoverable`, `invisible` or `no_data`.
    pub display: String,
    pub cells: Vec<CellValue>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct MappingInfo {
    pub source_transcript_id: String,
    pub status: String,
    pub same_exon_chain: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct Isoform {
    pub transcript_id: String,
    pub transcript_type: Option<String>,
    pub mrna_feature_id: String,
    pub cds_feature_id: Option<String>,
    pub strand: Direction,
    pub exons: Vec<Segment>,
    pub cds: Vec<Segment>,
    pub start_codon: Option<CodonMark>,
    pub stop_codon: Option<CodonMark>,
    pub is_mane_select: bool,
    pub tsl: Option<String>,
    pub appris: Option<String>,
    pub cds_start_nf: bool,
    pub cds_end_nf: bool,
    pub aa_len: Option<u32>,
    /// `quantified`, `discoverable` or `invisible`.
    pub evidence_state: String,
    pub mappings: Vec<MappingInfo>,
    pub novel_junctions: u32,
    pub expression: Vec<IsoformExpression>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct QuantifierPanel {
    pub quantifier: String,
    pub reports_zeros: bool,
    pub cell_lines: Vec<String>,
    /// Transcript ids, most expressed first.
    pub order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct LocusView {
    pub symbol: String,
    pub gene_id: String,
    pub chrom: String,
    pub strand: Direction,
    /// 1-based genomic coordinate of locus position 0.
    pub genomic_start: f64,
    pub annotation_version: Option<String>,
    pub expression_units: Option<String>,
    pub novel_transcripts: u32,
    pub default_quantifier: Option<String>,
    pub quantifiers: Vec<QuantifierPanel>,
    pub isoforms: Vec<Isoform>,
    /// Transcripts whose features were deleted from the document.
    pub missing_features: Vec<String>,
}

fn u32_of(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn segments(spans: &[app::Span]) -> Vec<Segment> {
    spans
        .iter()
        .map(|s| Segment {
            start: u32_of(s.start),
            length: u32_of(s.length),
        })
        .collect()
}

fn display_name(display: Display) -> String {
    serde_json::to_value(display)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

#[must_use]
pub fn locus_view(report: &dnagent_formats::ImportReport) -> Option<LocusView> {
    let view = app::isoform_view(report)?;
    let strand = if view.strand == "-" {
        Direction::Reverse
    } else {
        Direction::Forward
    };
    let mark = |c: &Option<app::CodonMark>| {
        c.as_ref().map(|c| CodonMark {
            position: u32_of(c.position),
            split: c.split,
        })
    };
    Some(LocusView {
        symbol: view.symbol,
        gene_id: view.gene_id,
        chrom: view.chrom,
        strand,
        #[allow(clippy::cast_precision_loss)] // genomic coordinates are far below 2^52
        genomic_start: view.genomic_start as f64,
        annotation_version: view.annotation_version,
        expression_units: view.expression_units,
        novel_transcripts: u32::try_from(view.novel_transcripts).unwrap_or(u32::MAX),
        default_quantifier: view.default_quantifier,
        quantifiers: view
            .quantifiers
            .into_iter()
            .map(|q| QuantifierPanel { quantifier: q.quantifier, reports_zeros: q.reports_zeros, cell_lines: q.cell_lines, order: q.order })
            .collect(),
        isoforms: view
            .isoforms
            .iter()
            .map(|i| Isoform {
                transcript_id: i.transcript_id.clone(),
                transcript_type: i.transcript_type.clone(),
                mrna_feature_id: i.mrna_feature_id.clone(),
                cds_feature_id: i.cds_feature_id.clone(),
                strand,
                exons: segments(&i.exons),
                cds: segments(&i.cds),
                start_codon: mark(&i.start_codon),
                stop_codon: mark(&i.stop_codon),
                is_mane_select: i.is_mane_select,
                tsl: i.tsl.clone(),
                appris: i.appris.clone(),
                cds_start_nf: i.cds_start_nf,
                cds_end_nf: i.cds_end_nf,
                aa_len: i.aa_len.map(|a| u32::try_from(a).unwrap_or(u32::MAX)),
                evidence_state: i.evidence_state.clone(),
                mappings: i
                    .mappings
                    .iter()
                    .map(|m| MappingInfo { source_transcript_id: m.source_transcript_id.clone(), status: m.status.clone(), same_exon_chain: m.same_exon_chain })
                    .collect(),
                novel_junctions: u32::try_from(i.novel_junctions).unwrap_or(u32::MAX),
                expression: i
                    .expression
                    .iter()
                    .map(|e| IsoformExpression {
                        quantifier: e.quantifier.clone(),
                        panel_mean: e.panel_mean,
                        display: display_name(e.display),
                        cells: e
                            .cells
                            .iter()
                            .map(|c| CellValue {
                                cell_line: c.cell_line.clone(),
                                mean: c.mean,
                                min: c.min,
                                max: c.max,
                                n_libraries: u32::try_from(c.n_libraries).unwrap_or(u32::MAX),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
        missing_features: view.missing_features,
    })
}

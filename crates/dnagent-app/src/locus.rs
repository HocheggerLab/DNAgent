//! The isoform view of a locus document: isoforms ordered by expression, coloured by
//! evidence, with codon marks and per-cell-line expression. Shared by the desktop
//! isoform view and `dnagent isoforms`.
//!
//! Geometry comes from the record's features (so edits are reflected); evidence and
//! expression come from the locus metadata (see `dnagent_formats::locus`).
use dnagent_formats::ImportReport;
use dnagent_formats::locus::{self, Bundle, CellExpression, Mapping};
use serde::Serialize;

/// How an isoform is drawn for one quantifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Display {
    /// Quantified, and above zero somewhere in the panel (orange).
    Expressed,
    /// Quantified but zero across the panel (blue).
    NotDetected,
    /// Could have been discovered from long reads and was not (blue).
    Discoverable,
    /// No evidence either way: not quantified and not discoverable (grey, hatched).
    Invisible,
    /// Quantified by other quantifiers, but this one has no measurement for it (grey).
    NoData,
}

#[derive(Debug, Clone, Serialize)]
pub struct Span {
    pub start: usize,
    pub length: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodonMark {
    /// First base of the codon in transcription direction (zero-based on the locus).
    pub position: usize,
    pub split: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuantifierExpression {
    pub quantifier: String,
    /// Mean over the panel's cell lines (see [`isoform_view`]); null without data.
    pub panel_mean: Option<f64>,
    pub display: Display,
    pub cells: Vec<CellExpression>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Isoform {
    pub transcript_id: String,
    pub transcript_type: Option<String>,
    pub mrna_feature_id: String,
    pub cds_feature_id: Option<String>,
    /// Exons and coding parts from the record's features, ascending.
    pub exons: Vec<Span>,
    pub cds: Vec<Span>,
    pub start_codon: Option<CodonMark>,
    pub stop_codon: Option<CodonMark>,
    pub is_mane_select: bool,
    pub tsl: Option<String>,
    pub appris: Option<String>,
    pub cds_start_nf: bool,
    pub cds_end_nf: bool,
    pub aa_len: Option<u64>,
    /// `quantified`, `discoverable` or `invisible`.
    pub evidence_state: String,
    pub mappings: Vec<Mapping>,
    pub novel_junctions: u64,
    pub expression: Vec<QuantifierExpression>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuantifierPanel {
    pub quantifier: String,
    pub reports_zeros: bool,
    pub cell_lines: Vec<String>,
    /// Transcript ids, most expressed first (panel mean, then evidence, MANE, id).
    pub order: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IsoformView {
    pub symbol: String,
    pub gene_id: String,
    pub chrom: String,
    /// `+` or `-`.
    pub strand: String,
    /// 1-based genomic coordinate of locus position 0.
    pub genomic_start: u64,
    pub annotation_version: Option<String>,
    pub expression_units: Option<String>,
    pub novel_transcripts: u64,
    pub default_quantifier: Option<String>,
    pub quantifiers: Vec<QuantifierPanel>,
    pub isoforms: Vec<Isoform>,
    /// Transcripts in the metadata whose features were removed from the record.
    pub missing_features: Vec<String>,
}

pub const DEFAULT_QUANTIFIER: &str = "bambu_lr";

fn spans(record: &dnagent_domain::SequenceRecord, id: &str) -> Option<Vec<Span>> {
    let feature = record.features().iter().find(|f| f.id().as_str() == id)?;
    let mut parts: Vec<Span> = feature
        .location()
        .parts()
        .iter()
        .map(|p| Span {
            start: p.start().get(),
            length: p.length().get(),
        })
        .collect();
    parts.sort_by_key(|p| p.start);
    Some(parts)
}

/// Mean over the panel's cell lines. A quantifier that reports zeros lists every
/// reference transcript in every library, so a missing cell line is skipped; one that
/// lists only detected transcripts (NanoCount) means zero by omission.
fn panel_mean(cells: &[CellExpression], panel: &locus::Panel) -> Option<f64> {
    if cells.is_empty() {
        return None;
    }
    let values: Vec<f64> = panel
        .cell_lines
        .iter()
        .filter_map(|line| {
            let hit = cells
                .iter()
                .find(|c| c.cell_line == line.cell_line)
                .map(|c| c.mean);
            if panel.reports_zeros {
                hit
            } else {
                Some(hit.unwrap_or(0.0))
            }
        })
        .collect();
    #[allow(clippy::cast_precision_loss)] // at most a few dozen cell lines
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn display(state: &str, mean: Option<f64>) -> Display {
    match (state, mean) {
        ("quantified", Some(m)) if m > 0.0 => Display::Expressed,
        ("quantified", Some(_)) => Display::NotDetected,
        ("quantified", None) => Display::NoData,
        ("discoverable", _) => Display::Discoverable,
        _ => Display::Invisible,
    }
}

fn evidence_rank(state: &str) -> u8 {
    match state {
        "quantified" => 0,
        "discoverable" => 1,
        _ => 2,
    }
}

/// The isoform view of a locus document, or `None` for other records.
#[must_use]
pub fn isoform_view(report: &ImportReport) -> Option<IsoformView> {
    let bundle: Bundle = locus::metadata(&report.preserved_metadata)?;
    let record = &report.record;
    let mut missing = Vec::new();
    let mut isoforms = Vec::new();
    for t in &bundle.transcripts {
        let Some(mrna_id) = t.mrna_feature_id.clone() else {
            continue;
        };
        let Some(exons) = spans(record, &mrna_id) else {
            missing.push(t.transcript_id.clone());
            continue;
        };
        let cds_id = t
            .cds_feature_id
            .clone()
            .filter(|id| record.features().iter().any(|f| f.id().as_str() == id));
        let cds = cds_id
            .as_deref()
            .and_then(|id| spans(record, id))
            .unwrap_or_default();
        let expression = bundle
            .panel
            .iter()
            .map(|(quantifier, panel)| {
                let cells = t.expression.get(quantifier).cloned().unwrap_or_default();
                let mean = if t.evidence.state == "quantified" {
                    panel_mean(&cells, panel)
                } else {
                    None
                };
                QuantifierExpression {
                    quantifier: quantifier.clone(),
                    panel_mean: mean,
                    display: display(&t.evidence.state, mean),
                    cells,
                }
            })
            .collect();
        let mark = |c: Option<locus::Codon>| {
            c.and_then(|c| {
                usize::try_from(c.position).ok().map(|position| CodonMark {
                    position,
                    split: c.split,
                })
            })
        };
        isoforms.push(Isoform {
            transcript_id: t.transcript_id.clone(),
            transcript_type: t.transcript_type.clone(),
            mrna_feature_id: mrna_id,
            cds_feature_id: cds_id,
            exons,
            cds,
            start_codon: mark(t.start_codon),
            stop_codon: mark(t.stop_codon),
            is_mane_select: t.is_mane_select,
            tsl: t.tsl.clone(),
            appris: t.appris.clone(),
            cds_start_nf: t.cds_start_nf,
            cds_end_nf: t.cds_end_nf,
            aa_len: t.aa_len,
            evidence_state: t.evidence.state.clone(),
            mappings: t.evidence.mappings.clone(),
            novel_junctions: t.evidence.novel_junctions,
            expression,
        });
    }
    let quantifiers = quantifier_panels(&bundle, &isoforms);
    let default_quantifier = bundle
        .panel
        .contains_key(DEFAULT_QUANTIFIER)
        .then(|| DEFAULT_QUANTIFIER.to_owned())
        .or_else(|| bundle.panel.keys().next().cloned());
    Some(IsoformView {
        symbol: bundle.gene.symbol,
        gene_id: bundle.gene.gene_id,
        chrom: bundle.gene.chrom,
        strand: bundle.gene.strand,
        genomic_start: bundle.genomic_start,
        annotation_version: bundle.annotation_version,
        expression_units: bundle.expression_units,
        novel_transcripts: bundle.novel_transcripts,
        default_quantifier,
        quantifiers,
        isoforms,
        missing_features: missing,
    })
}

/// Per quantifier: its cell lines and the isoform order (panel mean, then evidence, MANE, id).
fn quantifier_panels(bundle: &Bundle, isoforms: &[Isoform]) -> Vec<QuantifierPanel> {
    bundle
        .panel
        .iter()
        .map(|(quantifier, panel)| {
            let mut order: Vec<&Isoform> = isoforms.iter().collect();
            let mean = |i: &Isoform| {
                i.expression
                    .iter()
                    .find(|e| &e.quantifier == quantifier)
                    .and_then(|e| e.panel_mean)
            };
            order.sort_by(|a, b| {
                mean(b)
                    .unwrap_or(f64::NEG_INFINITY)
                    .total_cmp(&mean(a).unwrap_or(f64::NEG_INFINITY))
                    .then(evidence_rank(&a.evidence_state).cmp(&evidence_rank(&b.evidence_state)))
                    .then(b.is_mane_select.cmp(&a.is_mane_select))
                    .then(a.transcript_id.cmp(&b.transcript_id))
            });
            QuantifierPanel {
                quantifier: quantifier.clone(),
                reports_zeros: panel.reports_zeros,
                cell_lines: panel
                    .cell_lines
                    .iter()
                    .map(|c| c.cell_line.clone())
                    .collect(),
                order: order.iter().map(|i| i.transcript_id.clone()).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dnagent_formats::locus::{Panel, PanelCellLine};
    use std::path::Path;

    fn open(name: &str) -> ImportReport {
        crate::open_path(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/formats/locus")
                .join(name),
        )
        .expect("opens")
    }

    fn order<'a>(view: &'a IsoformView, quantifier: &str) -> &'a [String] {
        &view
            .quantifiers
            .iter()
            .find(|q| q.quantifier == quantifier)
            .expect("quantifier")
            .order
    }

    fn expression<'a>(
        view: &'a IsoformView,
        transcript: &str,
        quantifier: &str,
    ) -> &'a QuantifierExpression {
        let isoform = view
            .isoforms
            .iter()
            .find(|i| i.transcript_id == transcript)
            .expect("isoform");
        isoform
            .expression
            .iter()
            .find(|e| e.quantifier == quantifier)
            .expect("quantifier")
    }

    fn cell(line: &str, mean: f64) -> CellExpression {
        CellExpression {
            cell_line: line.into(),
            n_libraries: 1,
            mean,
            min: mean,
            max: mean,
        }
    }

    fn panel(reports_zeros: bool) -> Panel {
        Panel {
            cell_lines: ["A", "B", "C", "D"]
                .map(|c| PanelCellLine {
                    cell_line: c.into(),
                    libraries: 1,
                })
                .to_vec(),
            reports_zeros,
        }
    }

    #[test]
    fn dense_panels_average_measured_lines_and_sparse_panels_count_missing_as_zero() {
        let cells = [cell("A", 4.0), cell("B", 2.0)];
        assert_eq!(panel_mean(&cells, &panel(true)), Some(3.0));
        assert_eq!(panel_mean(&cells, &panel(false)), Some(1.5));
        assert_eq!(
            panel_mean(&[], &panel(false)),
            None,
            "no rows at all is no data, not zero"
        );
    }

    #[test]
    fn orders_by_panel_mean_then_evidence_and_keeps_every_isoform() {
        let view = isoform_view(&open("synthetic_locus.locus.json")).expect("locus");
        assert_eq!(view.default_quantifier.as_deref(), Some(DEFAULT_QUANTIFIER));
        assert_eq!(
            order(&view, "bambu_lr"),
            [
                "SYNT0001.1",
                "SYNT0002.1",
                "SYNT0004.1",
                "SYNT0003.1",
                "SYNT0005.1"
            ]
        );
        assert_eq!(
            order(&view, "NanoCount_lr"),
            [
                "SYNT0002.1",
                "SYNT0001.1",
                "SYNT0004.1",
                "SYNT0003.1",
                "SYNT0005.1"
            ]
        );
        assert_eq!(
            expression(&view, "SYNT0001.1", "bambu_lr").panel_mean,
            Some(32.0)
        );
        let displays: Vec<_> = order(&view, "bambu_lr")
            .iter()
            .map(|t| expression(&view, t, "bambu_lr").display)
            .collect();
        assert_eq!(
            displays,
            [
                Display::Expressed,
                Display::Expressed,
                Display::NotDetected,
                Display::Discoverable,
                Display::Invisible
            ]
        );
        assert_eq!(
            expression(&view, "SYNT0004.1", "NanoCount_lr").display,
            Display::NoData
        );
        assert!(view.missing_features.is_empty());
    }

    #[test]
    fn geometry_comes_from_the_features() {
        let view = isoform_view(&open("synthetic_locus.locus.json")).expect("locus");
        let t1 = &view.isoforms[0];
        assert_eq!(
            t1.exons
                .iter()
                .map(|s| (s.start, s.length))
                .collect::<Vec<_>>(),
            [(1000, 300), (2000, 150), (3000, 210), (4000, 400)]
        );
        assert_eq!(
            t1.cds.first().map(|s| s.start),
            t1.start_codon.as_ref().map(|c| c.position)
        );
        let last = t1.cds.last().expect("cds");
        assert_eq!(
            t1.stop_codon.as_ref().map(|c| c.position + 3),
            Some(last.start + last.length),
            "CDS ends with the stop codon"
        );
        let minus = isoform_view(&open("synthetic_minus.locus.json")).expect("locus");
        let m1 = &minus.isoforms[0];
        assert_eq!(minus.strand, "-");
        assert_eq!(
            m1.start_codon.as_ref().map(|c| c.position + 1),
            m1.cds.last().map(|s| s.start + s.length),
            "minus-strand start codon at the CDS's high end"
        );
    }

    #[test]
    fn genbank_round_trip_keeps_the_view() {
        let report = open("synthetic_locus.locus.json");
        let (text, warnings) = crate::genbank_text(&report, "30-SEP-2026");
        assert!(warnings.is_empty(), "{warnings:?}");
        let reopened = crate::import_path_bytes(Path::new("synloc.gb"), text.as_bytes())
            .expect("reads GenBank");
        let [before, after] = [&report, &reopened]
            .map(|r| serde_json::to_value(isoform_view(r).expect("locus")).expect("json"));
        assert_eq!(before, after);
    }

    #[test]
    fn expression_values_survive_genbank_bit_for_bit() {
        // Parsed with serde_json's default (not correctly rounded) float parser, this value
        // came back one ULP off after save → reopen; `float_roundtrip` keeps it exact.
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/formats/locus/synthetic_locus.locus.json");
        let mut bundle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("fixture")).expect("json");
        bundle["transcripts"][0]["expression"]["bambu_lr"][0]["min"] =
            serde_json::json!(9.151_435_592_288_319e-11);
        let report = crate::import_path_bytes(
            Path::new("x.locus.json"),
            &serde_json::to_vec(&bundle).expect("json"),
        )
        .expect("reads");
        let (text, _) = crate::genbank_text(&report, "30-SEP-2026");
        let reopened = crate::import_path_bytes(Path::new("x.gb"), text.as_bytes()).expect("reads");
        let min = |r: &ImportReport| {
            isoform_view(r).expect("locus").isoforms[0]
                .expression
                .iter()
                .find(|e| e.quantifier == "bambu_lr")
                .expect("q")
                .cells[0]
                .min
        };
        assert_eq!(
            min(&report).to_bits(),
            9.151_435_592_288_319e-11_f64.to_bits()
        );
        assert_eq!(min(&reopened).to_bits(), min(&report).to_bits());
    }

    #[test]
    fn deleted_isoforms_are_listed_not_drawn() {
        let report = open("synthetic_locus.locus.json");
        let view = isoform_view(&report).expect("locus");
        let t3 = view
            .isoforms
            .iter()
            .find(|i| i.transcript_id == "SYNT0003.1")
            .expect("T3");
        let edited = crate::editing::remove_feature(&report, &t3.mrna_feature_id).expect("removes");
        let after = isoform_view(&edited).expect("locus");
        assert_eq!(after.missing_features, ["SYNT0003.1"]);
        assert!(
            after
                .isoforms
                .iter()
                .all(|i| i.transcript_id != "SYNT0003.1")
        );
        assert!(after.quantifiers.iter().all(|q| q.order.len() == 4));
    }

    #[test]
    fn other_documents_have_no_isoform_view() {
        let report = crate::open_path(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/formats/snapgene/synthetic_linear.dna"),
        )
        .expect("opens");
        assert!(isoform_view(&report).is_none());
    }
}

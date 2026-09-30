//! DNAgent locus bundles (format `dnagent-locus`, version 1), as written by
//! `degron-db locus`: a genomic locus with every annotated transcript of one gene.
//!
//! Each transcript becomes an `mRNA` (or `misc_RNA` for non-coding types) feature over
//! its exons and, when coding, a `CDS` feature (stop codon included, `codon_start` from
//! the bundle), coloured by evidence state. Evidence, expression, codon marks and the
//! expression panel are kept as structured locus metadata in
//! [`FormatExtensions::locus`], linked to the features by id, so the desktop isoform
//! view and `dnagent isoforms` can read them and a GenBank save keeps them.
use crate::{FormatExtensions, ImportError, ImportReport, ImportWarning};
use dnagent_domain::{
    DisplayHints, DnaSeq, Feature, FeatureId, Location, LocationOperator, Qualifier, Region,
    SequenceRecord, Strand, Topology,
};
use serde::{Deserialize, Serialize};

const FORMAT: &str = "DNAgent locus bundle";
pub const LOCUS_FORMAT: &str = "dnagent-locus";

/// Display colours per evidence state (the isoform view uses the same).
pub const COLOR_EXPRESSED: &str = "#e8870e";
pub const COLOR_NOT_FOUND: &str = "#3b7dd8";
pub const COLOR_NO_EVIDENCE: &str = "#a0a4a8";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    pub gene: Gene,
    #[serde(default)]
    pub annotation_version: Option<String>,
    pub genomic_start: u64,
    pub genomic_end: u64,
    #[serde(default)]
    pub flank: Option<u64>,
    pub sequence: String,
    pub transcripts: Vec<Transcript>,
    #[serde(default)]
    pub expression_units: Option<String>,
    #[serde(default)]
    pub panel: std::collections::BTreeMap<String, Panel>,
    #[serde(default)]
    pub novel_transcripts: u64,
    #[serde(default)]
    pub provenance: Vec<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Gene {
    pub symbol: String,
    pub gene_id: String,
    pub chrom: String,
    pub strand: String,
    #[serde(default)]
    pub gene_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // mirrors the bundle's independent annotation flags
pub struct Transcript {
    pub transcript_id: String,
    #[serde(default)]
    pub transcript_type: Option<String>,
    #[serde(default)]
    pub is_mane_select: bool,
    #[serde(default)]
    pub is_ensembl_canonical: bool,
    #[serde(default)]
    pub is_basic: bool,
    #[serde(default)]
    pub tsl: Option<String>,
    #[serde(default)]
    pub appris: Option<String>,
    #[serde(default)]
    pub cds_start_nf: bool,
    #[serde(default)]
    pub cds_end_nf: bool,
    #[serde(default)]
    pub protein_id: Option<String>,
    #[serde(default)]
    pub aa_len: Option<u64>,
    pub exons: Vec<[u64; 2]>,
    #[serde(default)]
    pub cds: Vec<[u64; 2]>,
    #[serde(default)]
    pub cds_includes_stop: bool,
    #[serde(default = "one")]
    pub codon_start: u8,
    #[serde(default)]
    pub start_codon: Option<Codon>,
    #[serde(default)]
    pub stop_codon: Option<Codon>,
    #[serde(default)]
    pub protein: String,
    pub evidence: Evidence,
    /// quantifier → per-cell-line summaries.
    #[serde(default)]
    pub expression: std::collections::BTreeMap<String, Vec<CellExpression>>,
    /// Feature ids in the imported record (set on import; not in degron-db output).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mrna_feature_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cds_feature_id: Option<String>,
}

fn one() -> u8 {
    1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Codon {
    /// First base of the codon in transcription direction (zero-based on the locus).
    pub position: u64,
    #[serde(default)]
    pub split: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evidence {
    /// `quantified`, `discoverable` or `invisible`.
    pub state: String,
    #[serde(default)]
    pub mappings: Vec<Mapping>,
    #[serde(default)]
    pub junctions: Option<u64>,
    #[serde(default)]
    pub novel_junctions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mapping {
    pub source_transcript_id: String,
    pub status: String,
    #[serde(default)]
    pub same_exon_chain: Option<bool>,
    #[serde(default)]
    pub same_cds_start: Option<bool>,
    #[serde(default)]
    pub same_cds_stop: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CellExpression {
    pub cell_line: String,
    pub n_libraries: u64,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Panel {
    pub cell_lines: Vec<PanelCellLine>,
    #[serde(default)]
    pub reports_zeros: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PanelCellLine {
    pub cell_line: String,
    pub libraries: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provenance {
    pub source: String,
    pub version: String,
}

fn invalid(reason: impl Into<String>) -> ImportError {
    ImportError::InvalidFormat {
        format: FORMAT,
        reason: reason.into(),
    }
}

/// Whether `bytes` is a locus bundle (a JSON object whose `format` is `dnagent-locus`).
#[must_use]
pub fn is_locus(bytes: &[u8]) -> bool {
    #[derive(Deserialize)]
    struct Header {
        format: Option<String>,
    }
    serde_json::from_slice::<Header>(bytes).is_ok_and(|h| h.format.as_deref() == Some(LOCUS_FORMAT))
}

fn region(block: [u64; 2], length: usize, what: &str) -> Result<Region, ImportError> {
    let (start, end) = (
        usize::try_from(block[0]).map_err(|_| invalid(format!("{what}: position too large")))?,
        usize::try_from(block[1]).map_err(|_| invalid(format!("{what}: position too large")))?,
    );
    Region::linear(start, end, length).map_err(|_| {
        invalid(format!(
            "{what}: [{start}, {end}) is outside the {length} bp locus or empty"
        ))
    })
}

fn location(
    blocks: &[[u64; 2]],
    strand: Strand,
    length: usize,
    what: &str,
) -> Result<Location, ImportError> {
    let mut sorted = blocks.to_vec();
    sorted.sort_unstable();
    let parts = sorted
        .iter()
        .map(|b| region(*b, length, what))
        .collect::<Result<Vec<_>, _>>()?;
    let operator = if parts.len() == 1 {
        LocationOperator::Contiguous
    } else {
        LocationOperator::Join
    };
    Ok(Location::new(parts, strand, operator)?)
}

fn color(transcript: &Transcript) -> &'static str {
    let measured = transcript
        .expression
        .values()
        .flatten()
        .any(|c| c.mean > 0.0);
    match transcript.evidence.state.as_str() {
        "quantified" if measured => COLOR_EXPRESSED,
        "quantified" | "discoverable" => COLOR_NOT_FOUND,
        _ => COLOR_NO_EVIDENCE,
    }
}

/// `codon_start`, the standard code and the protein id for a transcript's CDS feature.
fn cds_qualifiers(transcript: &Transcript) -> Vec<Qualifier> {
    let mut qualifiers = vec![
        Qualifier {
            key: "codon_start".into(),
            value: Some(transcript.codon_start.to_string()),
        },
        Qualifier {
            key: "transl_table".into(),
            value: Some("1".into()),
        },
    ];
    if let Some(protein_id) = &transcript.protein_id {
        qualifiers.push(Qualifier {
            key: "protein_id".into(),
            value: Some(protein_id.clone()),
        });
    }
    qualifiers
}

/// The mRNA (or `misc_RNA`) and CDS features of one transcript; records their ids on it.
fn transcript_features(
    transcript: &mut Transcript,
    symbol: &str,
    strand: Strand,
    length: usize,
    features: &mut Vec<Feature>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(), ImportError> {
    if transcript.exons.is_empty() {
        return Err(invalid(format!(
            "{} has no exons",
            transcript.transcript_id
        )));
    }
    let hints = DisplayHints {
        color: Some(color(transcript).to_owned()),
    };
    let mut flags = vec![
        transcript
            .transcript_type
            .clone()
            .unwrap_or_else(|| "transcript".into()),
    ];
    if transcript.is_mane_select {
        flags.push("MANE Select".into());
    }
    if let Some(tsl) = &transcript.tsl {
        flags.push(format!("TSL {tsl}"));
    }
    flags.push(format!("evidence: {}", transcript.evidence.state));
    let common = |extra: Vec<Qualifier>| {
        let mut q = vec![
            Qualifier {
                key: "gene".into(),
                value: Some(symbol.to_owned()),
            },
            Qualifier {
                key: "transcript_id".into(),
                value: Some(transcript.transcript_id.clone()),
            },
        ];
        q.extend(extra);
        q
    };
    let mrna_id = FeatureId::new(format!("feature-{:04}", features.len() + 1))?;
    let kind = if transcript.cds.is_empty() {
        "misc_RNA"
    } else {
        "mRNA"
    };
    features.push(Feature::new(
        mrna_id.clone(),
        kind,
        transcript.transcript_id.clone(),
        location(&transcript.exons, strand, length, &transcript.transcript_id)?,
        common(vec![Qualifier {
            key: "note".into(),
            value: Some(flags.join("; ")),
        }]),
        hints.clone(),
    ));
    transcript.mrna_feature_id = Some(mrna_id.as_str().to_owned());
    if !transcript.cds.is_empty() {
        if !transcript.cds_includes_stop && !transcript.cds_end_nf {
            warnings.push(ImportWarning::new(
                "locus_cds_without_stop",
                format!(
                    "{}: the bundle's CDS does not include a stop codon",
                    transcript.transcript_id
                ),
            ));
        }
        let cds_id = FeatureId::new(format!("feature-{:04}", features.len() + 1))?;
        let extra = cds_qualifiers(transcript);
        features.push(Feature::new(
            cds_id.clone(),
            "CDS",
            format!("{} CDS", transcript.transcript_id),
            location(
                &transcript.cds,
                strand,
                length,
                &format!("{} CDS", transcript.transcript_id),
            )?,
            common(extra),
            hints,
        ));
        transcript.cds_feature_id = Some(cds_id.as_str().to_owned());
    }
    for (codon, what) in [
        (transcript.start_codon, "start"),
        (transcript.stop_codon, "stop"),
    ] {
        if let Some(c) = codon
            && usize::try_from(c.position).map_or(true, |p| p >= length)
        {
            return Err(invalid(format!(
                "{}: {what} codon at {} is outside the locus",
                transcript.transcript_id, c.position
            )));
        }
    }
    Ok(())
}

/// Read a locus bundle into a linear record plus locus metadata.
pub fn read(bytes: &[u8], fallback_name: &str) -> Result<ImportReport, ImportError> {
    let mut bundle: Bundle = serde_json::from_slice(bytes)
        .map_err(|e| invalid(format!("not a readable locus bundle: {e}")))?;
    if bundle.format != LOCUS_FORMAT {
        return Err(invalid(format!(
            "format is {:?}, expected {LOCUS_FORMAT:?}",
            bundle.format
        )));
    }
    if bundle.version != 1 {
        return Err(invalid(format!(
            "locus bundle version {} is not supported (this DNAgent reads version 1)",
            bundle.version
        )));
    }
    let sequence = DnaSeq::new(&bundle.sequence)?;
    let length = sequence.len();
    let strand = match bundle.gene.strand.as_str() {
        "+" => Strand::Forward,
        "-" => Strand::Reverse,
        other => return Err(invalid(format!("gene strand {other:?} is not + or -"))),
    };
    let mut warnings = Vec::new();
    let mut features = Vec::new();
    let symbol = bundle.gene.symbol.clone();
    for transcript in &mut bundle.transcripts {
        transcript_features(
            transcript,
            &symbol,
            strand,
            length,
            &mut features,
            &mut warnings,
        )?;
    }
    let name = if symbol.is_empty() {
        fallback_name.to_owned()
    } else {
        format!("{symbol} locus")
    };
    let record = SequenceRecord::new(name, sequence, Topology::Linear, features, Vec::new())?;
    bundle.sequence = String::new(); // the record holds the sequence; the metadata does not repeat it
    let metadata =
        serde_json::to_value(&bundle).map_err(|e| invalid(format!("locus metadata: {e}")))?;
    Ok(ImportReport {
        record,
        warnings,
        preserved_metadata: FormatExtensions {
            locus: Some(metadata),
            ..FormatExtensions::default()
        },
    })
}

/// The locus metadata of an imported record, if it came from a locus bundle.
#[must_use]
pub fn metadata(extensions: &FormatExtensions) -> Option<Bundle> {
    extensions
        .locus
        .as_ref()
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/formats/locus")
                .join(name),
        )
        .expect("fixture")
    }

    fn edited(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut value: serde_json::Value = serde_json::from_slice(&fixture(name)).expect("json");
        edit(&mut value);
        serde_json::to_vec(&value).expect("json")
    }

    #[test]
    fn recognises_bundles_by_format_field() {
        assert!(is_locus(&fixture("synthetic_locus.locus.json")));
        assert!(!is_locus(br#"{"format": "something-else"}"#));
        assert!(!is_locus(b"LOCUS       x"));
    }

    #[test]
    fn each_transcript_becomes_an_mrna_and_a_cds_joining_its_blocks() {
        let report = read(&fixture("synthetic_locus.locus.json"), "fallback").expect("reads");
        let record = &report.record;
        assert_eq!(record.name(), "SYNLOC locus");
        assert_eq!(record.sequence().len(), 6000);
        let features = record.features();
        // T1-T4 are coding (mRNA + CDS); T5 is non-coding (misc_RNA only).
        assert_eq!(features.len(), 9);
        let t1: Vec<_> = features
            .iter()
            .filter(|f| f.label().starts_with("SYNT0001.1"))
            .collect();
        assert_eq!(
            t1.iter().map(|f| (f.kind(), f.label())).collect::<Vec<_>>(),
            [("mRNA", "SYNT0001.1"), ("CDS", "SYNT0001.1 CDS")]
        );
        assert_eq!(t1[0].location().parts().len(), 4, "one part per exon");
        assert_eq!(t1[0].location().operator(), LocationOperator::Join);
        assert!(
            features
                .iter()
                .any(|f| f.kind() == "misc_RNA" && f.label() == "SYNT0005.1")
        );
        let bundle = metadata(&report.preserved_metadata).expect("locus metadata");
        assert!(
            bundle.sequence.is_empty(),
            "the sequence lives in the record, not twice in the metadata"
        );
        assert_eq!(
            bundle.transcripts[0].mrna_feature_id.as_deref(),
            Some(t1[0].id().as_str())
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    #[test]
    fn minus_strand_features_are_reverse() {
        let report = read(&fixture("synthetic_minus.locus.json"), "fallback").expect("reads");
        assert!(
            report
                .record
                .features()
                .iter()
                .all(|f| f.location().strand() == Strand::Reverse)
        );
    }

    #[test]
    fn cds_without_stop_is_reported() {
        let bytes = edited("synthetic_locus.locus.json", |v| {
            v["transcripts"][0]["cds_includes_stop"] = false.into();
        });
        let report = read(&bytes, "x").expect("reads");
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.code == "locus_cds_without_stop")
        );
    }

    #[test]
    fn malformed_bundles_are_rejected() {
        for (what, edit) in [
            (
                "version",
                Box::new(|v: &mut serde_json::Value| v["version"] = 2.into())
                    as Box<dyn FnOnce(&mut serde_json::Value)>,
            ),
            (
                "strand",
                Box::new(|v: &mut serde_json::Value| v["gene"]["strand"] = ".".into()),
            ),
            (
                "exon past the end",
                Box::new(|v: &mut serde_json::Value| {
                    v["transcripts"][0]["exons"][3][1] = 7000.into();
                }),
            ),
            (
                "no exons",
                Box::new(|v: &mut serde_json::Value| {
                    v["transcripts"][0]["exons"] = serde_json::json!([]);
                }),
            ),
            (
                "codon outside",
                Box::new(|v: &mut serde_json::Value| {
                    v["transcripts"][0]["start_codon"]["position"] = 9999.into();
                }),
            ),
        ] {
            assert!(
                read(&edited("synthetic_locus.locus.json", edit), "x").is_err(),
                "{what} accepted"
            );
        }
    }
}

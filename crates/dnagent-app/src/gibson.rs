//! Version-1 PCR-tail Gibson plans. Sources resolve relative to the plan directory.
use crate::AppError;
use dnagent_domain::digest::reverse_complement;
use dnagent_domain::gibson::{
    CoreSelection, GibsonError, GibsonReport, Preparation, core, design, unique_duplex_site,
    validate_request,
};
use dnagent_domain::ligation::Orientation;
use dnagent_domain::{DnaSeq, ImportedPrimer, SequenceRecord, Topology};
use dnagent_formats::{FormatExtensions, ImportReport, ImportWarning};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Sequence sources accepted by every Gibson plan. The legacy `{ "path": ... }`
/// shape remains valid; literal, digest-derived and projected PCR records make
/// synthetic/mixed assemblies possible without manufacturing temporary files.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum GibsonSource {
    File(FileSource),
    Literal(LiteralSource),
    Digest(DigestSource),
    Pcr(PcrSource),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileSource {
    pub path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiteralSource {
    pub name: String,
    pub sequence: String,
    pub topology: Topology,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DigestSource {
    pub path: PathBuf,
    pub enzymes: Vec<String>,
    pub fragment_id: String,
    #[serde(default)]
    pub strand: DigestStrand,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PcrSource {
    pub path: PathBuf,
    pub start: usize,
    pub length: usize,
    pub orientation: Orientation,
    #[serde(default)]
    pub left_tail: String,
    #[serde(default)]
    pub right_tail: String,
    pub annealing_length: Option<usize>,
    pub forward_annealing_length: Option<usize>,
    pub reverse_annealing_length: Option<usize>,
}

#[derive(Debug, Default, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DigestStrand {
    #[default]
    Top,
    Bottom,
}

impl GibsonSource {
    pub(crate) fn validate(&self) -> Result<(), GibsonError> {
        match self {
            Self::File(source) => nonempty_path(&source.path),
            Self::Literal(source) => {
                if source.name.trim().is_empty() || source.sequence.is_empty() {
                    Err(GibsonError::Invalid(
                        "literal Gibson sources require a nonempty name and sequence",
                    ))
                } else {
                    DnaSeq::new(&source.sequence)
                        .map(|_| ())
                        .map_err(GibsonError::Coordinates)
                }
            }
            Self::Digest(source) => {
                nonempty_path(&source.path)?;
                if source.enzymes.is_empty() || source.fragment_id.trim().is_empty() {
                    return Err(GibsonError::Invalid(
                        "digest Gibson sources require enzymes and a fragment_id",
                    ));
                }
                Ok(())
            }
            Self::Pcr(source) => {
                nonempty_path(&source.path)?;
                let (forward, reverse) = pcr_annealing_lengths(source)?;
                if source.length < forward + reverse
                    || !valid_optional_tail(&source.left_tail)
                    || !valid_optional_tail(&source.right_tail)
                    || source.left_tail.len() > 60
                    || source.right_tail.len() > 60
                {
                    return Err(GibsonError::Invalid(
                        "PCR Gibson sources require non-overlapping annealing regions and optional 0..=60-base ACGT tails",
                    ));
                }
                Ok(())
            }
        }
    }

    pub fn resolve_relative_to(&mut self, base: &Path) {
        match self {
            Self::File(source) => source.path = base.join(&source.path),
            Self::Digest(source) => source.path = base.join(&source.path),
            Self::Pcr(source) => source.path = base.join(&source.path),
            Self::Literal(_) => {}
        }
    }
}

fn pcr_annealing_lengths(source: &PcrSource) -> Result<(usize, usize), GibsonError> {
    let lengths = match (
        source.annealing_length,
        source.forward_annealing_length,
        source.reverse_annealing_length,
    ) {
        (Some(shared), None, None) => (shared, shared),
        (None, Some(forward), Some(reverse)) => (forward, reverse),
        _ => {
            return Err(GibsonError::Invalid(
                "PCR Gibson source requires annealing_length or both forward_annealing_length and reverse_annealing_length",
            ));
        }
    };
    if !(18..=40).contains(&lengths.0) || !(18..=40).contains(&lengths.1) {
        return Err(GibsonError::Invalid(
            "PCR annealing lengths must each be 18..=40 bases",
        ));
    }
    Ok(lengths)
}

fn valid_optional_tail(sequence: &str) -> bool {
    sequence.bytes().all(|base| b"ACGTacgt".contains(&base))
}

fn nonempty_path(path: &Path) -> Result<(), GibsonError> {
    if path.as_os_str().is_empty() {
        Err(GibsonError::Invalid("Gibson source path must not be empty"))
    } else {
        Ok(())
    }
}

/// Load one heterogeneous Gibson source through the shared application layer.
pub fn load_source(source: &GibsonSource) -> Result<ImportReport, AppError> {
    source.validate()?;
    match source {
        GibsonSource::File(source) => crate::open_path(&source.path),
        GibsonSource::Literal(source) => Ok(ImportReport {
            record: SequenceRecord::new(
                &source.name,
                DnaSeq::new(&source.sequence)?,
                source.topology,
                vec![],
                vec![],
            )?,
            warnings: vec![],
            preserved_metadata: FormatExtensions::default(),
        }),
        GibsonSource::Digest(source) => {
            let mut imported = crate::open_path(&source.path)?;
            let digest = crate::simulate_digest(&imported.record, &source.enzymes)?;
            let fragment = digest
                .fragments
                .iter()
                .find(|fragment| fragment.id == source.fragment_id)
                .ok_or(GibsonError::Invalid(
                    "requested digest fragment_id was not produced",
                ))?;
            let sequence = match source.strand {
                DigestStrand::Top => &fragment.top.sequence_5to3,
                DigestStrand::Bottom => &fragment.bottom.sequence_5to3,
            };
            imported.warnings.push(ImportWarning::new(
                "gibson_digest_fragment_projection",
                "Gibson source is a selected 5-prime-to-3-prime digest strand; end geometry and source annotations remain available only from the separate digest/fragment report",
            ));
            imported.record = SequenceRecord::new(
                format!(
                    "{}:{}:{:?}",
                    imported.record.name(),
                    fragment.id,
                    source.strand
                ),
                DnaSeq::new(sequence)?,
                fragment.topology,
                vec![],
                vec![],
            )?;
            Ok(imported)
        }
        GibsonSource::Pcr(source) => load_pcr_source(source),
    }
}

fn load_pcr_source(source: &PcrSource) -> Result<ImportReport, AppError> {
    let mut imported = crate::open_path(&source.path)?;
    let selection = CoreSelection {
        input: 1,
        start: source.start,
        length: source.length,
        orientation: source.orientation,
    };
    let oriented_core = core(&imported.record, &selection)?;
    if !oriented_core.bytes().all(|base| b"ACGT".contains(&base)) {
        return Err(GibsonError::Invalid("PCR source core must be unambiguous ACGT").into());
    }
    let (forward_length, reverse_length) = pcr_annealing_lengths(source)?;
    let forward_anneal = &oriented_core[..forward_length];
    let reverse_anneal = reverse_complement(&oriented_core[oriented_core.len() - reverse_length..]);
    if !unique_duplex_site(
        imported.record.sequence().as_str(),
        forward_anneal,
        imported.record.topology(),
    ) || !unique_duplex_site(
        imported.record.sequence().as_str(),
        &reverse_anneal,
        imported.record.topology(),
    ) {
        return Err(GibsonError::Invalid(
            "PCR source annealing sequence lacks a unique exact site on the full template",
        )
        .into());
    }
    let left_tail = source.left_tail.to_ascii_uppercase();
    let right_tail = source.right_tail.to_ascii_uppercase();
    let product = format!("{left_tail}{oriented_core}{right_tail}");
    let primers = vec![
        ImportedPrimer {
            name: "dnagent_forward".into(),
            sequence: DnaSeq::new(format!("{left_tail}{forward_anneal}"))?,
            description: Some("designed PCR primer: 5-prime left assembly tail plus template-annealing segment".into()),
        },
        ImportedPrimer {
            name: "dnagent_reverse".into(),
            sequence: DnaSeq::new(format!(
                "{}{}",
                reverse_complement(&right_tail),
                reverse_anneal
            ))?,
            description: Some("designed PCR primer: reverse complement of the right assembly tail plus template-annealing segment".into()),
        },
    ];
    imported.warnings.push(ImportWarning::new(
        "gibson_pcr_product_projection",
        "Gibson source is an ideal sequence-faithful PCR product with explicit product-oriented tails; source feature mappings and thermodynamic primer validation are not propagated into this exact-overlap view",
    ));
    imported.record = SequenceRecord::new(
        format!(
            "{}:PCR:[{},{}):{:?}",
            imported.record.name(),
            source.start,
            source.start + source.length,
            source.orientation
        ),
        DnaSeq::new(product)?,
        Topology::Linear,
        vec![],
        primers,
    )?;
    Ok(imported)
}

/// A core plus how that fragment reaches the reaction. `preparation` is required: a plan
/// that left it out would silently amplify a vector the user intends to digest.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanCore {
    #[serde(flatten)]
    pub selection: CoreSelection,
    pub preparation: Preparation,
}

impl PlanCore {
    #[must_use]
    pub fn split(cores: &[Self]) -> (Vec<CoreSelection>, Vec<Preparation>) {
        (
            cores.iter().map(|c| c.selection.clone()).collect(),
            cores.iter().map(|c| c.preparation).collect(),
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GibsonPlan {
    pub schema_version: u32,
    pub inputs: Vec<GibsonSource>,
    pub cores: Vec<PlanCore>,
    pub topology: Topology,
    pub overlap_length: usize,
    pub annealing_length: usize,
}

impl GibsonPlan {
    pub fn validate(&self) -> Result<(), GibsonError> {
        if self.schema_version != 2 {
            return Err(GibsonError::Invalid(
                "expected Gibson plan version 2 (version 1 plans lack the required per-core preparation: pcr or provided)",
            ));
        }
        for source in &self.inputs {
            source.validate()?;
        }
        let (selections, _) = PlanCore::split(&self.cores);
        validate_request(
            self.inputs.len(),
            &selections,
            self.overlap_length,
            self.annealing_length,
        )
    }
}

pub fn load_plan(path: &Path) -> Result<GibsonPlan, AppError> {
    let bytes = std::fs::read(path).map_err(|source| AppError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let mut plan: GibsonPlan = serde_json::from_slice(&bytes).map_err(AppError::GibsonPlan)?;
    plan.validate()?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    for source in &mut plan.inputs {
        source.resolve_relative_to(base);
    }
    Ok(plan)
}

pub fn simulate(records: &[SequenceRecord], plan: &GibsonPlan) -> Result<GibsonReport, AppError> {
    plan.validate()?;
    if records.len() != plan.inputs.len() {
        return Err(GibsonError::Invalid("loaded record count differs from plan inputs").into());
    }
    let (selections, preparation) = PlanCore::split(&plan.cores);
    Ok(design(
        records,
        &selections,
        &preparation,
        plan.topology,
        plan.overlap_length,
        plan.annealing_length,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_versions_unknown_fields_and_record_counts_are_checked() {
        let json = r#"{"schema_version":2,"inputs":[{"path":"synthetic.dna"}],"cores":[{"input":1,"start":0,"length":100,"orientation":"forward","preparation":"pcr"}],"topology":"circular","overlap_length":30,"annealing_length":24}"#;
        let mut plan: GibsonPlan = serde_json::from_str(json).unwrap();
        assert!(plan.validate().is_ok());
        assert!(simulate(&[], &plan).is_err());
        plan.schema_version = 1;
        assert!(plan.validate().is_err());
        for bad in [
            json.replace("\"path\":", "\"enzyme\":\"EcoRI\",\"path\":"),
            json.replace(",\"orientation\":\"forward\"", ""),
            json.replace("\"overlap_length\":30", "\"overlap_length\":-1"),
            // preparation is required: an omission must fail rather than default to PCR.
            json.replace(",\"preparation\":\"pcr\"", ""),
            json.replace("\"preparation\":\"pcr\"", "\"preparation\":\"digest\""),
        ] {
            assert!(serde_json::from_str::<GibsonPlan>(&bad).is_err());
        }
    }

    #[test]
    fn literal_digest_and_pcr_sources_are_typed_and_validated() {
        let literal: GibsonSource = serde_json::from_str(
            r#"{"name":"ordered_insert","sequence":"acgtacgt","topology":"linear"}"#,
        )
        .unwrap();
        let report = load_source(&literal).unwrap();
        assert_eq!(report.record.sequence().as_str(), "ACGTACGT");
        assert_eq!(report.warnings, [] as [dnagent_formats::ImportWarning; 0]);

        let digest: GibsonSource = serde_json::from_str(
            r#"{"path":"source.dna","enzymes":["EcoRI"],"fragment_id":"fragment-0002","strand":"top"}"#,
        )
        .unwrap();
        assert!(matches!(digest, GibsonSource::Digest(_)));
        let bad: GibsonSource =
            serde_json::from_str(r#"{"name":"","sequence":"ACGT","topology":"linear"}"#).unwrap();
        assert!(bad.validate().is_err());

        let mut state = 4_219_u64;
        let template: String = (0..120)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                char::from(b"ACGT"[(state % 4) as usize])
            })
            .collect();
        let path = std::env::temp_dir().join(format!("dnagent-pcr-{}.fasta", std::process::id()));
        std::fs::write(&path, format!(">template\n{template}\n")).unwrap();
        let source = GibsonSource::Pcr(PcrSource {
            path: path.clone(),
            start: 10,
            length: 90,
            orientation: Orientation::Forward,
            left_tail: "ACGTACGTACGTACGTACGT".into(),
            right_tail: "TGCATGCATGCATGCATGCA".into(),
            annealing_length: None,
            forward_annealing_length: Some(24),
            reverse_annealing_length: Some(20),
        });
        let report = load_source(&source).unwrap();
        assert_eq!(report.record.sequence().len(), 130);
        assert_eq!(report.record.primers().len(), 2);
        assert_eq!(report.record.primers()[0].sequence.len(), 44);
        assert_eq!(report.record.primers()[1].sequence.len(), 40);
        assert_eq!(report.warnings.len(), 2);
        std::fs::remove_file(path).unwrap();
    }
}

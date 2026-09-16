//! Version-1 PCR-tail Gibson plans. Sources resolve relative to the plan directory.
use crate::AppError;
use dnagent_domain::gibson::{CoreSelection, GibsonError, GibsonReport, design, validate_request};
use dnagent_domain::{SequenceRecord, Topology};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GibsonSource {
    pub path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GibsonPlan {
    pub schema_version: u32,
    pub inputs: Vec<GibsonSource>,
    pub cores: Vec<CoreSelection>,
    pub topology: Topology,
    pub overlap_length: usize,
    pub annealing_length: usize,
}

impl GibsonPlan {
    pub fn validate(&self) -> Result<(), GibsonError> {
        if self.schema_version != 1 || self.inputs.iter().any(|s| s.path.as_os_str().is_empty()) {
            return Err(GibsonError::Invalid(
                "expected plan version 1 and nonempty source paths",
            ));
        }
        validate_request(
            self.inputs.len(),
            &self.cores,
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
        source.path = base.join(&source.path);
    }
    Ok(plan)
}

pub fn simulate(records: &[SequenceRecord], plan: &GibsonPlan) -> Result<GibsonReport, AppError> {
    plan.validate()?;
    if records.len() != plan.inputs.len() {
        return Err(GibsonError::Invalid("loaded record count differs from plan inputs").into());
    }
    Ok(design(
        records,
        &plan.cores,
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
        let json = r#"{"schema_version":1,"inputs":[{"path":"synthetic.dna"}],"cores":[{"input":1,"start":0,"length":100,"orientation":"forward"}],"topology":"circular","overlap_length":30,"annealing_length":24}"#;
        let mut plan: GibsonPlan = serde_json::from_str(json).unwrap();
        assert!(plan.validate().is_ok());
        assert!(simulate(&[], &plan).is_err());
        plan.schema_version = 2;
        assert!(plan.validate().is_err());
        for bad in [
            json.replace("\"path\":", "\"enzyme\":\"EcoRI\",\"path\":"),
            json.replace(",\"orientation\":\"forward\"", ""),
            json.replace("\"overlap_length\":30", "\"overlap_length\":-1"),
        ] {
            assert!(serde_json::from_str::<GibsonPlan>(&bad).is_err());
        }
    }
}

//! Versioned ligation-plan intake and shared typed simulation operation.
use crate::AppError;
use dnagent_domain::ligation::{
    FragmentSelection, LigationError, LigationReport, simulate_ligation, validate_request,
};
use dnagent_domain::{SequenceRecord, Topology};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LigationSource {
    pub path: PathBuf,
    pub enzymes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LigationPlan {
    pub schema_version: u32,
    pub inputs: Vec<LigationSource>,
    pub fragments: Vec<FragmentSelection>,
    pub topology: Topology,
}

impl LigationPlan {
    pub fn validate(&self) -> Result<(), LigationError> {
        if self.schema_version != 1 {
            return Err(LigationError::InvalidPlan(
                "unsupported plan schema_version; expected 1",
            ));
        }
        validate_request(self.inputs.len(), &self.fragments)?;
        if self
            .inputs
            .iter()
            .any(|s| s.path.as_os_str().is_empty() || s.enzymes.is_empty())
        {
            return Err(LigationError::InvalidPlan(
                "each source requires a nonempty path and enzyme selection",
            ));
        }
        Ok(())
    }
}

/// Relative input paths are relative to the plan's directory, never the caller's cwd.
pub fn load_plan(path: &Path) -> Result<LigationPlan, AppError> {
    let bytes = std::fs::read(path).map_err(|source| AppError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let mut plan: LigationPlan = serde_json::from_slice(&bytes)?;
    plan.validate()?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    for source in &mut plan.inputs {
        source.path = base.join(&source.path);
    }
    Ok(plan)
}

pub fn simulate(
    records: &[SequenceRecord],
    plan: &LigationPlan,
) -> Result<LigationReport, AppError> {
    plan.validate()?;
    if records.len() != plan.inputs.len() {
        return Err(
            LigationError::InvalidPlan("loaded record count differs from plan inputs").into(),
        );
    }
    let inputs: Vec<_> = records
        .iter()
        .zip(&plan.inputs)
        .map(|(record, source)| (record, source.enzymes.as_slice()))
        .collect();
    Ok(simulate_ligation(&inputs, &plan.fragments, plan.topology)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_schema_rejects_unknown_fields_versions_and_implicit_orientation() {
        let valid = r#"{"schema_version":1,"inputs":[{"path":"synthetic.dna","enzymes":["EcoRI"]}],"fragments":[{"input":1,"fragment_id":"fragment-0001","orientation":"forward"}],"topology":"circular"}"#;
        let plan: LigationPlan = serde_json::from_str(valid).unwrap();
        assert!(plan.validate().is_ok());
        for malformed in [
            valid.replace("\"forward\"", "\"unknown\""),
            valid.replace(",\"orientation\":\"forward\"", ""),
            valid.replace("\"topology\":", "\"typo\":0,\"topology\":"),
            valid.replace("\"enzymes\":", "\"typo\":0,\"enzymes\":"),
        ] {
            assert!(serde_json::from_str::<LigationPlan>(&malformed).is_err());
        }
        let unsupported: LigationPlan =
            serde_json::from_str(&valid.replace("\"schema_version\":1", "\"schema_version\":2"))
                .unwrap();
        assert!(unsupported.validate().is_err());
        assert!(simulate(&[], &plan).is_err());
    }
}

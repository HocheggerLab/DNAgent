//! Typed intake for optimisation and existing-overlap assembly; no adapter biology.

use crate::{AppError, gibson::GibsonSource};
use dnagent_domain::{
    SequenceRecord, Topology, existing_overlaps,
    gibson::{CoreSelection, GibsonError, validate_request},
    primer_optimisation::{self, PrimerConstraints},
};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptimisationPlan {
    pub schema_version: u32,
    pub inputs: Vec<GibsonSource>,
    pub cores: Vec<CoreSelection>,
    pub topology: Topology,
    pub overlap_length: usize,
    pub constraints: PrimerConstraints,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExistingPlan {
    pub schema_version: u32,
    pub inputs: Vec<GibsonSource>,
    pub fragments: Vec<CoreSelection>,
    pub topology: Topology,
    pub overlaps: Vec<usize>,
}
fn header(version: u32, sources: &[GibsonSource]) -> Result<(), GibsonError> {
    if version != 1 {
        return Err(GibsonError::Invalid("expected Gibson plan version 1"));
    }
    for source in sources {
        source.validate()?;
    }
    Ok(())
}
impl OptimisationPlan {
    pub fn validate(&self) -> Result<(), GibsonError> {
        header(self.schema_version, &self.inputs)?;
        self.constraints.validate()?;
        validate_request(
            self.inputs.len(),
            &self.cores,
            self.overlap_length,
            self.constraints.min_length,
        )
    }
}
impl ExistingPlan {
    pub fn validate(&self) -> Result<(), GibsonError> {
        header(self.schema_version, &self.inputs)?;
        existing_overlaps::validate_request(
            self.inputs.len(),
            &self.fragments,
            self.topology,
            &self.overlaps,
        )
    }
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AppError> {
    let data = std::fs::read(path).map_err(|source| AppError::Read {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_slice(&data).map_err(AppError::GibsonPlan)
}
fn resolve(path: &Path, sources: &mut [GibsonSource]) {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    for source in sources {
        source.resolve_relative_to(base);
    }
}
pub fn load_optimisation(path: &Path) -> Result<OptimisationPlan, AppError> {
    let mut plan: OptimisationPlan = read(path)?;
    plan.validate()?;
    resolve(path, &mut plan.inputs);
    Ok(plan)
}
pub fn load_existing(path: &Path) -> Result<ExistingPlan, AppError> {
    let mut plan: ExistingPlan = read(path)?;
    plan.validate()?;
    resolve(path, &mut plan.inputs);
    Ok(plan)
}
pub fn optimise(
    records: &[SequenceRecord],
    plan: &OptimisationPlan,
) -> Result<primer_optimisation::OptimisedGibson, AppError> {
    plan.validate()?;
    if records.len() != plan.inputs.len() {
        return Err(GibsonError::Invalid("loaded record count differs from plan inputs").into());
    }
    Ok(primer_optimisation::optimise(
        records,
        &plan.cores,
        plan.topology,
        plan.overlap_length,
        &plan.constraints,
    )?)
}
pub fn assemble(
    records: &[SequenceRecord],
    plan: &ExistingPlan,
) -> Result<existing_overlaps::ExistingAssembly, AppError> {
    plan.validate()?;
    if records.len() != plan.inputs.len() {
        return Err(GibsonError::Invalid("loaded record count differs from plan inputs").into());
    }
    Ok(existing_overlaps::assemble(
        records,
        &plan.fragments,
        plan.topology,
        &plan.overlaps,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_constraints_and_record_counts_are_checked() {
        let mut optimisation: OptimisationPlan = serde_json::from_str(include_str!(
            "../../../fixtures/plans/synthetic-gibson-optimisation.json"
        ))
        .unwrap();
        let mut existing: ExistingPlan = serde_json::from_str(include_str!(
            "../../../fixtures/plans/synthetic-gibson-existing.json"
        ))
        .unwrap();
        assert!(optimisation.validate().is_ok());
        assert!(existing.validate().is_ok());
        assert!(optimise(&[], &optimisation).is_err());
        assert!(assemble(&[], &existing).is_err());
        optimisation.constraints.min_tm_c = 99.0;
        assert!(optimisation.validate().is_err());
        existing.schema_version = 2;
        assert!(existing.validate().is_err());
    }
}

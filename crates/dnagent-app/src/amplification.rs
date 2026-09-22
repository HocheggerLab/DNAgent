//! Hashed intake and typed amplification service shared by adapters.
use crate::{AppError, import_path_bytes};
use dnagent_domain::{
    Topology,
    amplification::{self, Design, DesignError, Request},
};
use dnagent_formats::ImportWarning;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: u32,
    pub inputs: Vec<PathBuf>,
    /// Optional exact feature ID on reference_input. Single-part features only;
    /// window_start/length must still equal that feature (no silent widening).
    pub feature_id: Option<String>,
    pub request: Request,
}
#[derive(Debug, Serialize)]
pub struct Source {
    pub input: usize,
    pub name: String,
    pub path: PathBuf,
    pub file_sha256: String,
    /// Exact stored forward sequence; not rotation-invariant identity.
    pub sequence_sha256: String,
    pub length: usize,
    pub topology: Topology,
    pub warnings: Vec<ImportWarning>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub sources: Vec<Source>,
    pub plan_sha256: String,
    pub feature_id: Option<String>,
    pub engine_version: &'static str,
    pub design: Design,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Warnings are accumulated even if later intake or design fails. Strict mode
/// rejects before biological computation; this read-only operation writes no files.
pub fn run(
    path: &Path,
    strict: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Result<Report, AppError> {
    let bytes = std::fs::read(path).map_err(|source| AppError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let plan: Plan = serde_json::from_slice(&bytes).map_err(AppError::AmplificationPlan)?;
    if plan.schema_version != 1 || plan.inputs.is_empty() || plan.inputs.len() > 8 {
        return Err(DesignError::Invalid("expected plan version 1 and 1..=8 inputs").into());
    }
    let mut sources = Vec::new();
    let mut records = Vec::new();
    for (index, input) in plan.inputs.iter().enumerate() {
        let resolved = path.parent().unwrap_or_else(|| Path::new(".")).join(input);
        let data = std::fs::read(&resolved).map_err(|source| AppError::Read {
            path: resolved.display().to_string(),
            source,
        })?;
        let report = import_path_bytes(&resolved, &data)?;
        warnings.extend(report.warnings.iter().cloned());
        if strict && !warnings.is_empty() {
            return Err(AppError::ImportWarnings {
                count: warnings.len(),
            });
        }
        sources.push(Source {
            input: index + 1,
            name: report.record.name().into(),
            path: resolved,
            file_sha256: hash(&data),
            sequence_sha256: hash(report.record.sequence().as_str().as_bytes()),
            length: report.record.sequence().len(),
            topology: report.record.topology(),
            warnings: report.warnings,
        });
        records.push(report.record);
    }
    plan.request.validate(&records)?;
    if let Some(id) = &plan.feature_id {
        let record = &records[plan.request.reference_input - 1];
        let feature = record
            .features()
            .iter()
            .find(|f| f.id().as_str() == id)
            .ok_or(DesignError::Invalid(
                "feature ID not found on reference input",
            ))?;
        let parts = feature.location().parts();
        if parts.len() != 1
            || parts[0].start().get() != plan.request.window_start
            || parts[0].length().get() != plan.request.window_length
        {
            return Err(DesignError::Invalid("feature target requires a single part exactly matching the explicit window; multipart targets require an explicit interval instead").into());
        }
    }
    let design = amplification::design(&records, &plan.request)?;
    Ok(Report {
        sources,
        plan_sha256: hash(&bytes),
        feature_id: plan.feature_id,
        engine_version: env!("CARGO_PKG_VERSION"),
        design,
    })
}

//! Choosing the enzyme catalogue: an installed REBASE release (see
//! `scripts/manage_enzymes.py`) when present and valid, otherwise the built-in set.
use dnagent_domain::restriction::{self, EnzymeCatalogue};
use dnagent_formats::ImportWarning;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Pinned REBASE release the installer provides.
pub const REBASE_RELEASE: &str = "609";

#[derive(Deserialize)]
struct Manifest {
    format: String,
    release: String,
    files: std::collections::HashMap<String, ManifestFile>,
}

#[derive(Deserialize)]
struct ManifestFile {
    file: String,
    sha256: String,
}

/// `DNAGENT_ENZYME_DIR`, else the platform data folder (matches the installer).
#[must_use]
pub fn enzyme_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("DNAGENT_ENZYME_DIR") {
        return PathBuf::from(dir);
    }
    crate::paths::data_dir("enzymes")
}

fn load_rebase(dir: &Path) -> Result<EnzymeCatalogue, String> {
    let manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(dir.join("manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("invalid manifest: {e}"))?;
    if manifest.format != "dnagent-enzyme-catalogue" || manifest.release != REBASE_RELEASE {
        return Err(format!(
            "unexpected catalogue {} release {}",
            manifest.format, manifest.release
        ));
    }
    let mut texts = std::collections::HashMap::new();
    let mut hashes = Vec::new();
    for name in ["emboss_e", "emboss_r"] {
        let entry = manifest
            .files
            .get(name)
            .ok_or_else(|| format!("manifest lacks {name}"))?;
        let bytes =
            std::fs::read(dir.join(&entry.file)).map_err(|e| format!("{}: {e}", entry.file))?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        if digest != entry.sha256 {
            return Err(format!("{} does not match its manifest hash", entry.file));
        }
        hashes.push(format!("{name}:{digest}"));
        texts.insert(name, String::from_utf8_lossy(&bytes).into_owned());
    }
    EnzymeCatalogue::from_emboss(
        &texts["emboss_e"],
        &texts["emboss_r"],
        REBASE_RELEASE,
        Some(hashes.join(",")),
    )
    .map_err(|e| e.to_string())
}

/// Activate the catalogue for this process. `DNAGENT_ENZYMES=builtin` forces the built-in
/// set (deterministic tests). A present but invalid installation falls back to the
/// built-in set with a warning, never silently.
#[must_use]
pub fn activate() -> Vec<ImportWarning> {
    if std::env::var("DNAGENT_ENZYMES").is_ok_and(|v| v == "builtin") {
        restriction::install_catalogue(EnzymeCatalogue::builtin());
        return Vec::new();
    }
    let dir = enzyme_root().join(format!("rebase-{REBASE_RELEASE}"));
    if !dir.join("manifest.json").exists() {
        restriction::install_catalogue(EnzymeCatalogue::builtin());
        return Vec::new();
    }
    match load_rebase(&dir) {
        Ok(catalogue) => {
            restriction::install_catalogue(catalogue);
            Vec::new()
        }
        Err(reason) => {
            restriction::install_catalogue(EnzymeCatalogue::builtin());
            vec![ImportWarning::new(
                "enzyme_catalogue_fallback",
                format!(
                    "installed REBASE at {} was not used ({reason}); using the built-in enzymes",
                    dir.display()
                ),
            )]
        }
    }
}

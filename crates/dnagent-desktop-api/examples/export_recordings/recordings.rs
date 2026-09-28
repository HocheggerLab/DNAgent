//! Shared by the `export_recordings` example and its drift test.
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Public fixture directories recorded for the desktop end-to-end harness.
const FIXTURE_DIRS: [&str; 2] = ["fixtures/formats/snapgene", "fixtures/formats/fasta"];

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Recording {
    Ok(dnagent_desktop_api::Document),
    Err(dnagent_desktop_api::Diagnostic),
}

/// Serialise the real `open_document` result for every public fixture, keyed by
/// repository-relative path, in sorted order.
pub fn recordings_json(repo_root: &Path) -> String {
    let mut recordings = BTreeMap::new();
    for dir in FIXTURE_DIRS {
        let entries = std::fs::read_dir(repo_root.join(dir)).expect("fixture directory");
        for entry in entries {
            let path = entry.expect("fixture entry").path();
            let extension = path.extension().and_then(|e| e.to_str());
            if !matches!(extension, Some("dna" | "fasta")) {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("UTF-8 name");
            let recording = match dnagent_desktop_api::open_document(&path) {
                Ok(document) => Recording::Ok(document),
                Err(diagnostic) => Recording::Err(diagnostic),
            };
            recordings.insert(format!("{dir}/{name}"), recording);
        }
    }
    let mut json = serde_json::to_string_pretty(&recordings).expect("serialisable recordings");
    json.push('\n');
    json
}

pub fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

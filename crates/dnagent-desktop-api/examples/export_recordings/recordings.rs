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

fn record(path: &Path) -> Recording {
    match dnagent_desktop_api::open_document(path) {
        Ok(document) => Recording::Ok(document),
        Err(diagnostic) => Recording::Err(diagnostic),
    }
}

fn to_json(recordings: &BTreeMap<String, Recording>) -> String {
    let mut json = serde_json::to_string_pretty(recordings).expect("serialisable recordings");
    json.push('\n');
    json
}

/// Record explicitly named local files, keyed by the path as given. Used for the
/// gitignored design-review gallery; output must never be committed.
#[allow(dead_code)] // Used by the example only, not by the drift test.
pub fn local_recordings_json(paths: &[String]) -> String {
    let recordings = paths
        .iter()
        .map(|path| (path.clone(), record(Path::new(path))))
        .collect();
    to_json(&recordings)
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
            recordings.insert(format!("{dir}/{name}"), record(&path));
        }
    }
    to_json(&recordings)
}

pub fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

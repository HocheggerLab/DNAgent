use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/formats/snapgene")
}

fn run_json(arguments: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .args(arguments)
        .output()
        .expect("DNAagent CLI should launch");
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("CLI stdout should be JSON")
}

#[test]
fn inspect_matches_committed_fixture_contract() {
    let directory = fixture_dir();
    let input = directory.join("synthetic_circular.dna");
    let expected: Value = serde_json::from_slice(
        &std::fs::read(directory.join("synthetic_circular.inspect.json")).unwrap(),
    )
    .unwrap();
    let actual = run_json(&["inspect", input.to_str().unwrap(), "--output", "json"]);
    assert_eq!(actual, expected);
}

#[test]
fn features_match_committed_fixture_contract() {
    let directory = fixture_dir();
    let input = directory.join("synthetic_circular.dna");
    let expected: Value = serde_json::from_slice(
        &std::fs::read(directory.join("synthetic_circular.features.json")).unwrap(),
    )
    .unwrap();
    let actual = run_json(&["features", input.to_str().unwrap(), "--output", "json"]);
    assert_eq!(actual, expected);
}

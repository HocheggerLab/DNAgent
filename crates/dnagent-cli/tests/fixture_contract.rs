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
fn linear_sequence_ranges_use_zero_based_half_open_coordinates() {
    let input = fixture_dir().join("synthetic_linear.dna");
    for (range, expected) in [("0..1", "A"), ("1..4", "CGT"), ("14..15", "N")] {
        let actual = run_json(&[
            "sequence",
            input.to_str().unwrap(),
            "--range",
            range,
            "--output",
            "json",
        ]);
        assert_eq!(actual["result"]["sequence"], expected);
    }
}

#[test]
fn inspect_exposes_partial_import_warnings() {
    let input = fixture_dir().join("synthetic_partial.dna");
    let actual = run_json(&["inspect", input.to_str().unwrap(), "--output", "json"]);
    let warnings = actual["result"]["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w["code"] == "snapgene_feature_range_unsupported")
    );
    assert!(
        warnings
            .iter()
            .any(|w| w["code"] == "snapgene_primer_skipped")
    );
    // Do not bless the current omission of warnings from other commands.
}

#[test]
fn malformed_files_produce_json_failures_and_nonzero_exit() {
    for name in [
        "invalid_duplicate_sequence.dna",
        "invalid_missing_sequence.dna",
        "invalid_truncated.dna",
        "invalid_feature_xml.dna",
    ] {
        let input = fixture_dir().join(name);
        let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
            .args(["inspect", input.to_str().unwrap(), "--output", "json"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {name}");
        let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(actual["ok"], false);
        assert_eq!(actual["command"], "inspect");
    }
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

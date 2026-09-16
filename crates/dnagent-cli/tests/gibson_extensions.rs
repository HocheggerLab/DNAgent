use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/plans")
        .join(name)
}
#[test]
fn optimisation_alias_returns_canonical_envelope_and_variable_lengths() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .current_dir(std::env::temp_dir())
        .arg("gibson-optimize")
        .arg(fixture("synthetic-gibson-optimisation.json"))
        .arg("--strict")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["command"], "gibson-optimise");
    assert_eq!(body["schema_version"], "0.9.0");
    let report = &body["result"];
    assert_eq!(report["pairs"].as_array().unwrap().len(), 2);
    assert!(report["design"]["annealing_length"].is_null());
    assert!(report["pairs"][0]["feasible_pairs"].as_u64().unwrap() > 0);
}
#[test]
fn existing_overlap_keeps_shared_provenance_and_rejects_wrong_plan_kind() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .current_dir(std::env::temp_dir())
        .arg("gibson-assemble")
        .arg(fixture("synthetic-gibson-existing.json"))
        .arg("--strict")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    let report = &body["result"];
    assert_eq!(report["product_sequence_5to3"].as_str().unwrap().len(), 300);
    assert_eq!(report["components"][1]["product_start"], 155);
    assert_eq!(report["components"][1]["wraps_origin"], true);
    for command in ["gibson-assemble", "gibson-optimise"] {
        let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
            .arg(command)
            .arg(fixture("synthetic-gibson.json"))
            .output()
            .unwrap();
        assert!(!output.status.success());
        let body: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(body["error"]["code"], "gibson_failed");
        assert!(body.get("result").is_none());
    }
}

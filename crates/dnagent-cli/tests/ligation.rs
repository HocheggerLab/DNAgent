use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn relative_sources_resolve_against_plan_and_both_strands_are_conserved() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .current_dir(std::env::temp_dir())
        .arg("ligate")
        .arg(path("fixtures/plans/synthetic-religation.json"))
        .arg("--strict")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["schema_version"], "0.8.0");
    assert_eq!(body["command"], "ligate");
    let product = &body["result"]["product"];
    assert_eq!(product["paired_length"], 156);
    assert_eq!(product["top_sequence_5to3"].as_str().unwrap().len(), 156);
    assert_eq!(product["bottom_sequence_5to3"].as_str().unwrap().len(), 156);
    assert_eq!(product["components"].as_array().unwrap().len(), 3);
    assert_eq!(product["junctions"].as_array().unwrap().len(), 2);
    assert_eq!(product["bottom_forward_start"], 0);
    assert!(
        body["result"]["unused_fragments"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn explicit_reverse_closure_has_phase_and_no_free_ends() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .arg("ligate")
        .arg(path("fixtures/plans/synthetic-closure.json"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    let product = &body["result"]["product"];
    assert_eq!(product["topology"], "circular");
    assert_eq!(product["bottom_forward_start"], 4);
    assert!(product["left_end"].is_null());
    assert!(product["right_end"].is_null());
    assert_eq!(product["junctions"][0]["closure"], true);
    assert_eq!(product["components"][0]["top"]["source_strand"], "reverse");
}

#[test]
fn malformed_or_missing_plans_have_runtime_envelopes_not_partial_products() {
    for (file, code) in [
        (
            "fixtures/formats/snapgene/synthetic_restriction_linear.dna",
            "ligation_failed",
        ),
        ("fixtures/plans/does-not-exist.json", "command_failed"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
            .arg("ligate")
            .arg(path(file))
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stderr.is_empty());
        let body: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(body["error"]["code"], code);
        assert!(body.get("result").is_none());
    }
}

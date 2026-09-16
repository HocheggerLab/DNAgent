use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn gibson_relative_plan_runs_strictly_from_another_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .current_dir(std::env::temp_dir())
        .arg("gibson")
        .arg(path("fixtures/plans/synthetic-gibson.json"))
        .arg("--strict")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["schema_version"], "0.9.0");
    assert_eq!(body["command"], "gibson");
    let product = &body["result"];
    assert_eq!(
        product["product_sequence_5to3"].as_str().unwrap().len(),
        190
    );
    assert_eq!(product["junctions"].as_array().unwrap().len(), 2);
    assert_eq!(product["junctions"][1]["closure"], true);
    assert_eq!(
        product["components"][1]["selection"]["orientation"],
        "reverse"
    );
    assert_eq!(
        product["components"][0]["forward_primer"]["sequence_5to3"]
            .as_str()
            .unwrap()
            .len(),
        22
    );
    assert_eq!(
        product["components"][0]["reverse_primer"]["sequence_5to3"]
            .as_str()
            .unwrap()
            .len(),
        47
    );
}

#[test]
fn gibson_invalid_plans_have_no_partial_products_and_text_discloses_limits() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .arg("gibson")
        .arg(path("fixtures/plans/synthetic-religation.json"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["error"]["code"], "gibson_failed");
    assert!(body.get("result").is_none());
    let text = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .arg("gibson")
        .arg(path("fixtures/plans/synthetic-gibson.json"))
        .args(["--output", "text"])
        .output()
        .unwrap();
    assert!(text.status.success());
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.contains("thermodynamic"));
    assert!(text.contains("reverse 5to3:"));
}

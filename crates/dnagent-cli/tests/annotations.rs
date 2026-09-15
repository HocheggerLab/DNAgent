use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;

fn run(command: &str, file: &str, strict: bool) -> (bool, Value) {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats/snapgene")
        .join(file);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dnagent"));
    cmd.arg(command).arg(input).args(["--output", "json"]);
    if strict {
        cmd.arg("--strict");
    }
    let output = cmd.output().unwrap();
    assert!(output.stderr.is_empty());
    (
        output.status.success(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

#[test]
fn qualifiers_retain_order_repetition_and_null_values() {
    let (ok, body) = run("features", "synthetic_linear.dna", true);
    assert!(ok);
    assert_eq!(
        body["result"][0]["qualifiers"],
        json!([
            {"key":"note", "value":"first"}, {"key":"note", "value":"second"},
            {"key":"pseudo", "value":null}
        ])
    );
}

#[test]
fn primers_retain_sequence_and_description_without_inventing_binding_sites() {
    let (ok, body) = run("primers", "synthetic_linear.dna", true);
    assert!(ok);
    assert_eq!(body["command"], "primers");
    assert_eq!(
        body["result"],
        json!([{"name":"synthetic primer", "sequence":"ACGTN", "description":"retained description"}])
    );
    assert_eq!(body["warnings"], json!([]));
    let (ok, body) = run("primers", "synthetic_unannotated.dna", true);
    assert!(ok);
    assert_eq!(body["result"], json!([]));
}

#[test]
fn primers_report_partial_imports_and_obey_strict_policy() {
    let (ok, body) = run("primers", "synthetic_partial.dna", false);
    assert!(ok);
    assert_eq!(
        body["result"],
        json!([{"name":"valid", "sequence":"ACGT", "description":null}])
    );
    assert!(
        body["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "snapgene_primer_skipped")
    );
    let (ok, strict) = run("primers", "synthetic_partial.dna", true);
    assert!(!ok);
    assert_eq!(strict["warnings"], body["warnings"]);
    assert_eq!(strict["error"]["code"], "import_warnings");
}

#[test]
fn gui_help_matches_build_feature() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        help.lines()
            .any(|line| line.trim_start().starts_with("gui ")),
        cfg!(feature = "gui")
    );
}

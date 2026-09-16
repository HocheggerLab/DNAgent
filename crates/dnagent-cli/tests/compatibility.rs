use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats/snapgene")
        .join(name)
}
fn run(first: &str, enzyme: &str, second: Option<(&str, &str)>, strict: bool) -> (bool, Value) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dnagent"));
    cmd.arg("compatible-ends")
        .arg(fixture(first))
        .args(["--enzymes", enzyme, "--output", "json"]);
    if let Some((file, enzymes)) = second {
        cmd.arg("--other")
            .arg(fixture(file))
            .args(["--other-enzymes", enzymes]);
    }
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
fn circular_closure_candidate_and_uncut_circle_are_explicit() {
    let (ok, body) = run("synthetic_restriction_circular.dna", "EcoRI", None, true);
    assert!(ok);
    let pairs = body["result"]["analysis"]["pairs"].as_array().unwrap();
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0]["same_fragment"], true);
    assert_eq!(pairs[0]["assessment"]["compatible"], true);
    let (ok, body) = run("synthetic_restriction_circular.dna", "BamHI", None, true);
    assert!(ok);
    assert!(
        body["result"]["analysis"]["endpoints"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn two_inputs_have_unique_ids_and_cross_enzyme_compatibility() {
    let file = "synthetic_restriction_linear.dna";
    let (ok, body) = run(file, "BsaI", Some((file, "BsmBI")), true);
    assert!(ok);
    assert_eq!(body["result"]["inputs"].as_array().unwrap().len(), 2);
    let pairs = body["result"]["analysis"]["pairs"].as_array().unwrap();
    assert!(
        pairs
            .iter()
            .any(|p| p["first"].as_str().unwrap().starts_with("input-1:")
                && p["second"].as_str().unwrap().starts_with("input-2:")
                && p["assessment"]["reason"] == "complementary_overhangs")
    );
    assert!(
        pairs
            .iter()
            .any(|p| p["assessment"]["reason"] == "overhang_sequence_mismatch")
    );
    assert!(
        pairs
            .iter()
            .any(|p| p["assessment"]["second_fragment_orientation"] == "reverse")
    );
}
#[test]
fn warnings_survive_second_input_and_failures() {
    let partial = "synthetic_partial.dna";
    let clean = "synthetic_restriction_circular.dna";
    let (ok, single) = run(partial, "EcoRI", None, false);
    assert!(ok);
    for second in [clean, "invalid_truncated.dna"] {
        let (ok, body) = run(partial, "EcoRI", Some((second, "EcoRI")), false);
        assert_eq!(ok, second == clean);
        assert_eq!(body["warnings"], single["warnings"]);
    }
    let (ok, body) = run(clean, "EcoRI", Some((partial, "EcoRI")), true);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "import_warnings");
    assert!(!body["warnings"].as_array().unwrap().is_empty());
    let (ok, body) = run("synthetic_restriction_end.dna", "BsaI", None, false);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "digest_failed");
}
#[test]
fn second_input_requires_explicit_enzymes() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .arg("compatible-ends")
        .arg(fixture("synthetic_restriction_linear.dna"))
        .args(["--enzymes", "EcoRI", "--other"])
        .arg(fixture("synthetic_partial.dna"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("--other-enzymes")
    );
}

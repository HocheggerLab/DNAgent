use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn sites(file: &str, enzymes: &str, strict: bool) -> (bool, Value) {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats/snapgene")
        .join(file);
    let mut command = Command::new(env!("CARGO_BIN_EXE_dnagent"));
    command
        .arg("sites")
        .arg(input)
        .args(["--enzymes", enzymes, "--output", "json"]);
    if strict {
        command.arg("--strict");
    }
    let output = command.output().unwrap();
    assert!(output.stderr.is_empty());
    (
        output.status.success(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

#[test]
fn lists_catalogue_and_scans_all_six_enzymes() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .args(["enzymes", "--output", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let catalogue: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(catalogue["result"].as_array().unwrap().len(), 6);
    let (ok, body) = sites(
        "synthetic_restriction_linear.dna",
        "EcoRI,BamHI,EcoRV,KpnI,BsaI,BsmBI",
        true,
    );
    assert!(ok);
    assert_eq!(body["result"]["sites"].as_array().unwrap().len(), 8);
    assert!(body["warnings"].as_array().unwrap().is_empty());
    assert_eq!(body["result"]["sites"][0]["top_cut"], 13);
}

#[test]
fn circular_sites_and_unavailable_linear_cuts_are_explicit() {
    let (ok, body) = sites("synthetic_restriction_circular.dna", "EcoRI", true);
    assert!(ok);
    assert_eq!(
        body["result"]["sites"][0]["recognition"]["kind"],
        "circular_arc"
    );
    assert_eq!(body["result"]["sites"][0]["top_cut"], 24);
    assert_eq!(body["result"]["sites"][0]["bottom_cut"], 2);
    let (ok, body) = sites("synthetic_restriction_end.dna", "BsaI", false);
    assert!(ok);
    assert_eq!(body["warnings"][0]["code"], "restriction_cut_out_of_bounds");
    assert_eq!(body["result"]["sites"][0]["cleavage_available"], false);
    assert!(body["result"]["sites"][0]["top_cut"].is_null());
    let (ok, strict) = sites("synthetic_restriction_end.dna", "BsaI", true);
    assert!(!ok);
    assert_eq!(strict["error"]["code"], "import_warnings");
    assert_eq!(strict["warnings"], body["warnings"]);
}

#[test]
fn bad_enzyme_and_ambiguous_sequence_fail_without_false_negative_results() {
    for (file, enzyme) in [
        ("synthetic_restriction_linear.dna", "made-up"),
        ("synthetic_linear.dna", "EcoRI"),
    ] {
        let (ok, body) = sites(file, enzyme, false);
        assert!(!ok);
        assert_eq!(body["error"]["code"], "restriction_scan_failed");
        assert!(body.get("result").is_none());
    }
    let (ok, body) = sites("synthetic_partial.dna", "EcoRI", false);
    assert!(ok);
    assert!(!body["warnings"].as_array().unwrap().is_empty());
    let (ok, _) = sites("synthetic_partial.dna", "EcoRI", true);
    assert!(!ok);
}

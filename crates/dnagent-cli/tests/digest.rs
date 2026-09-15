use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn run(file: &str, enzymes: &str, strict: bool) -> (bool, Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats/snapgene")
        .join(file);
    let mut command = Command::new(env!("CARGO_BIN_EXE_dnagent"));
    command
        .arg("digest")
        .arg(path)
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
fn digest_reports_complete_strands_and_conserves_bases() {
    let (ok, body) = run(
        "synthetic_restriction_linear.dna",
        "EcoRI,BamHI,EcoRV,KpnI,BsaI,BsmBI",
        true,
    );
    assert!(ok);
    assert_eq!(body["schema_version"], "0.4.0");
    let result = &body["result"];
    assert_eq!(result["cuts"].as_array().unwrap().len(), 8);
    let fragments = result["fragments"].as_array().unwrap();
    assert_eq!(fragments.len(), 9);
    for strand in ["top", "bottom"] {
        assert_eq!(
            fragments
                .iter()
                .map(|f| f[strand]["length"].as_u64().unwrap())
                .sum::<u64>(),
            result["input_length"].as_u64().unwrap()
        );
    }
    assert_eq!(fragments[0]["left_end"]["original_terminus"], true);
    assert_eq!(fragments[0]["right_end"]["overhang_sequence"], "AATT");
}

#[test]
fn circular_single_cut_and_no_cut_are_distinct() {
    let (ok, body) = run("synthetic_restriction_circular.dna", "EcoRI", true);
    assert!(ok);
    assert_eq!(body["result"]["fragments"][0]["topology"], "linear");
    assert_eq!(body["result"]["fragments"][0]["top"]["length"], 26);
    let (ok, body) = run("synthetic_restriction_circular.dna", "BamHI", true);
    assert!(ok);
    assert_eq!(body["result"]["fragments"][0]["topology"], "circular");
    assert!(body["result"]["fragments"][0]["left_end"].is_null());
}

#[test]
fn incomplete_or_uncertain_digests_fail_and_import_warnings_remain_visible() {
    for (file, enzyme) in [
        ("synthetic_restriction_end.dna", "BsaI"),
        ("synthetic_linear.dna", "EcoRI"),
        ("synthetic_restriction_linear.dna", "unknown"),
    ] {
        let (ok, body) = run(file, enzyme, false);
        assert!(!ok);
        assert_eq!(body["error"]["code"], "digest_failed");
        assert!(body.get("result").is_none());
    }
    let (ok, body) = run("synthetic_partial.dna", "EcoRI", false);
    assert!(ok);
    assert!(!body["warnings"].as_array().unwrap().is_empty());
    let (ok, body) = run("synthetic_partial.dna", "EcoRI", true);
    assert!(!ok);
    assert_eq!(body["error"]["code"], "import_warnings");
}

//! CLI contract for `translate` and `orfs` on the synthetic translation fixture.
//! Protein-level correctness is checked independently by scripts/check_translation.py.
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

const FIXTURE: &str = "synthetic_translation.dna";

fn run(args: &[&str]) -> Output {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats/snapgene")
        .join(FIXTURE);
    let mut command = Command::new(env!("CARGO_BIN_EXE_dnagent"));
    command.arg(args[0]).arg(fixture).args(&args[1..]);
    command.output().unwrap()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout must contain one JSON envelope")
}

fn codes(body: &Value) -> Vec<String> {
    body["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn feature_translation_reports_codons_provenance_and_imported_match() {
    let output = run(&["translate", "--feature", "feature-0001", "--output", "json"]);
    assert!(output.status.success());
    let body = json(&output);
    assert_eq!(
        (body["command"].as_str(), body["schema_version"].as_str()),
        (Some("translate"), Some("0.10.0"))
    );
    let result = &body["result"];
    assert_eq!(result["mode"], "feature");
    assert_eq!(result["label"], "forward CDS");
    let protein = result["protein"].as_str().unwrap();
    assert_eq!(protein.len(), 82); // ATG + 80 sense codons + stop
    assert!(protein.starts_with('M') && protein.ends_with('*'));
    assert_eq!(result["codons"].as_array().unwrap().len(), 82);
    assert_eq!(result["imported_translation"]["matches"], true);
    assert_eq!(result["genetic_code_source"]["version"], "4.6");
    assert_eq!(codes(&body), [] as [std::string::String; 0]);
}

#[test]
fn all_cds_reports_mismatches_initiator_methionine_and_skips() {
    let body = json(&run(&["translate", "--all-cds", "--output", "json"]));
    let result = &body["result"];
    assert_eq!(result["mode"], "all_cds");
    assert_eq!(result["translations"].as_array().unwrap().len(), 6);
    assert_eq!(result["skipped"][0]["label"], "unknown-strand CDS");
    let gtg = &result["translations"][4];
    assert_eq!(
        (
            gtg["table"].as_u64(),
            gtg["initiator_as_methionine"].as_bool()
        ),
        (Some(11), Some(true))
    );
    let ambiguous = &result["translations"][5];
    assert_eq!(ambiguous["imported_translation"]["matches"], false);
    assert_eq!(ambiguous["ambiguous_codons"], 2);
    let warnings = codes(&body);
    for code in [
        "translation_ambiguous_codons",
        "translation_imported_mismatch",
        "translation_skipped",
    ] {
        assert!(
            warnings.iter().any(|w| w == code),
            "{code} missing from {warnings:?}"
        );
    }
    // Strict mode refuses results with translation warnings, with a JSON error envelope.
    let strict = run(&["translate", "--all-cds", "--strict", "--output", "json"]);
    assert!(!strict.status.success());
    assert_eq!(json(&strict)["error"]["code"], "import_warnings");
}

#[test]
fn ranges_wrap_on_circular_records_and_reverse_strands_read_5_to_3() {
    let body = json(&run(&[
        "translate",
        "--range",
        "552..12",
        "--output",
        "json",
    ]));
    assert_eq!(body["result"]["mode"], "range");
    assert_eq!(body["result"]["protein"].as_str().unwrap().len(), 8);
    assert_eq!(
        body["result"]["codons"][0]["positions"],
        serde_json::json!([552, 553, 554])
    );
    let reverse = json(&run(&[
        "translate",
        "--range",
        "0..9",
        "--strand",
        "reverse",
        "--output",
        "json",
    ]));
    assert_eq!(reverse["result"]["strand"], "reverse");
    assert_eq!(
        reverse["result"]["codons"][0]["positions"],
        serde_json::json!([8, 7, 6])
    );
}

#[test]
fn failures_use_translation_failed() {
    for args in [
        &["translate", "--feature", "missing", "--output", "json"][..],
        &["translate", "--feature", "feature-0007", "--output", "json"],
        &[
            "translate",
            "--range",
            "0..9",
            "--table",
            "7",
            "--output",
            "json",
        ],
        &["orfs", "--min-codons", "0", "--output", "json"],
    ] {
        let output = run(args);
        assert!(!output.status.success(), "{args:?}");
        assert_eq!(
            json(&output)["error"]["code"],
            "translation_failed",
            "{args:?}"
        );
    }
}

#[test]
fn orfs_report_parameters_and_wrapping_frames() {
    let body = json(&run(&["orfs", "--min-codons", "5", "--output", "json"]));
    let result = &body["result"];
    assert_eq!(
        result["options"],
        serde_json::json!({"table": 1, "min_codons": 5, "starts": "atg_only"})
    );
    let orfs = result["orfs"].as_array().unwrap();
    assert!(orfs.iter().any(|orf| orf["wraps_origin"] == true));
    for (index, orf) in orfs.iter().enumerate() {
        assert_eq!(orf["id"], format!("orf-{:04}", index + 1));
        assert!(orf["codons"].as_u64().unwrap() >= 5);
        assert_eq!(
            orf["protein"].as_str().unwrap().len() as u64,
            orf["codons"].as_u64().unwrap()
        );
    }
    let default = json(&run(&["orfs", "--output", "json"]));
    assert!(
        default["result"]["orfs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|orf| orf["codons"].as_u64().unwrap() >= 75)
    );
}

#[test]
fn text_output_is_fasta_like() {
    let output = run(&["translate", "--feature", "feature-0002"]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with(">feature-0002 reverse split CDS table=1\nM"));
}

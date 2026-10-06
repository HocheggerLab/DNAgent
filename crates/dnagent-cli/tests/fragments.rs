use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

fn run(file: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .arg("fragments")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/formats/snapgene")
                .join(file),
        )
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn json_retains_source_metadata_and_separate_strand_mappings() {
    let output = run("synthetic_multipart_origin.dna", &["--enzymes", "EcoRI"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], "0.10.0");
    assert_eq!(value["command"], "fragments");
    let view = &value["result"];
    assert_eq!(view["source_features"].as_array().unwrap().len(), 1);
    assert_eq!(
        view["source_features"][0]["location"]["parts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(view["annotations"][0]["top"].as_array().unwrap().len(), 1);
    assert_eq!(
        view["annotations"][0]["bottom"].as_array().unwrap().len(),
        1
    );
    assert!(
        view["annotations"][0]["top"][0]["complete"]
            .as_bool()
            .unwrap()
    );
}

#[test]
fn fasta_is_explicitly_strand_sequence_only_and_strict_has_no_partial_output() {
    let output = run(
        "synthetic_multipart_origin.dna",
        &["--enzymes", "EcoRI", "--output", "fasta"],
    );
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with(">fragment-0001|top "));
    assert!(text.contains(">fragment-0001|bottom "));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("Sequence-only")
    );
    let strict = run(
        "synthetic_circular.dna",
        &["--enzymes", "EcoRI", "--output", "fasta", "--strict"],
    );
    assert!(!strict.status.success());
    assert!(strict.stdout.is_empty());
}

#[test]
fn genbank_requires_explicit_selection_and_emits_only_the_selected_view() {
    let missing = run(
        "synthetic_multipart_origin.dna",
        &["--enzymes", "EcoRI", "--output", "genbank"],
    );
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
    let output = run(
        "synthetic_multipart_origin.dna",
        &[
            "--enzymes",
            "EcoRI",
            "--output",
            "genbank",
            "--strand",
            "bottom",
        ],
    );
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("LOCUS       frag1_bottom"));
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("LOCUS "))
            .count(),
        1
    );
    assert!(text.contains("misc_feature"));
    assert!(!text.contains("/translation="));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("not a duplex product")
    );
}

#[test]
fn genbank_strict_rejection_and_inapplicable_strand_have_no_partial_exports() {
    let strict = run(
        "synthetic_circular.dna",
        &[
            "--enzymes",
            "EcoRI",
            "--output",
            "genbank",
            "--strand",
            "top",
            "--strict",
        ],
    );
    assert!(!strict.status.success());
    assert!(strict.stdout.is_empty());
    let wrong = run(
        "synthetic_multipart_origin.dna",
        &["--enzymes", "EcoRI", "--strand", "top"],
    );
    assert!(!wrong.status.success());
    let value: Value = serde_json::from_slice(&wrong.stdout).unwrap();
    assert_eq!(value["error"]["code"], "command_failed");
    assert!(value.get("result").is_none());
    let failed = run(
        "synthetic_restriction_end.dna",
        &[
            "--enzymes",
            "BsaI",
            "--output",
            "genbank",
            "--strand",
            "top",
        ],
    );
    assert!(!failed.status.success());
    assert!(failed.stdout.is_empty());
}

#[test]
fn unavailable_cut_has_annotation_error_envelope_without_partial_products() {
    let output = run("synthetic_restriction_end.dna", &["--enzymes", "BsaI"]);
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "annotation_failed");
    assert!(value.get("result").is_none());
}

#[test]
fn strict_warning_rejection_retains_json_warnings() {
    let output = run(
        "synthetic_circular.dna",
        &["--enzymes", "EcoRI", "--strict"],
    );
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "import_warnings");
    assert!(!value["warnings"].as_array().unwrap().is_empty());
}

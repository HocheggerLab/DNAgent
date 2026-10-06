//! `convert` and `annotate`: lossless GenBank round trips and edits through the CLI.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .args(args)
        .output()
        .unwrap()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("one JSON envelope")
}

fn projection(path: &Path, command: &str) -> Value {
    let mut args = vec![command, path.to_str().unwrap()];
    if command == "translate" {
        args.push("--all-cds");
    }
    args.extend(["--output", "json"]);
    let body = json(&run(&args));
    serde_json::json!([body["result"], body["warnings"]])
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dnagent-editing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn every_public_fixture_round_trips_through_genbank() {
    let fixtures = [
        "synthetic_circular.dna",
        "synthetic_linear.dna",
        "synthetic_multipart_origin.dna",
        "synthetic_partial.dna",
        "synthetic_overlaps.dna",
        "synthetic_translation.dna",
        "pUC19_M77789.dna",
        "synthetic_unannotated.dna",
    ];
    for name in fixtures {
        let source = root().join("fixtures/formats/snapgene").join(name);
        let out = temp(&name.replace(".dna", ".gb"));
        let report = run(&[
            "convert",
            source.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);
        assert!(
            report.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&report.stdout)
        );
        assert_eq!(json(&report)["result"]["format"], "genbank");
        for command in ["inspect", "features", "primers", "translate"] {
            assert_eq!(
                projection(&source, command),
                projection(&out, command),
                "{name} {command}"
            );
        }
        // A second save of the GenBank file is byte-identical apart from nothing.
        let again = temp(&name.replace(".dna", ".again.gb"));
        assert!(
            run(&[
                "convert",
                out.to_str().unwrap(),
                "--out",
                again.to_str().unwrap()
            ])
            .status
            .success()
        );
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            std::fs::read_to_string(&again).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn annotate_adds_a_translated_cds_and_removes_it_again() {
    let source = root().join("fixtures/formats/snapgene/synthetic_translation.dna");
    let out = temp("annotated.gb");
    // The ≥75-codon forward CDS spans 32..278 in this fixture.
    let added = run(&[
        "annotate",
        source.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--add",
        "--range",
        "32..278",
        "--label",
        "fusion ORF",
        "--translate",
        "--color",
        "#3366cc",
    ]);
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stdout)
    );
    let body = json(&added);
    assert_eq!(body["result"]["action"], "add");
    let id = body["result"]["feature_id"].as_str().unwrap().to_owned();
    assert_eq!(body["result"]["feature"]["kind"], "CDS");
    assert_eq!(body["result"]["translation"]["terminal_stop"], true);
    let features = json(&run(&[
        "features",
        out.to_str().unwrap(),
        "--output",
        "json",
    ]));
    let feature = features["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == id.as_str())
        .unwrap()
        .clone();
    assert_eq!(feature["label"], "fusion ORF");
    assert_eq!(feature["color"], "#3366cc");
    let translated = json(&run(&[
        "translate",
        out.to_str().unwrap(),
        "--feature",
        &id,
        "--output",
        "json",
    ]));
    assert_eq!(
        translated["result"]["imported_translation"]["matches"], true,
        "stored /translation agrees with the engine"
    );
    // Remove in place (GenBank input and output may be the same file).
    let removed = run(&[
        "annotate",
        out.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--remove",
        &id,
    ]);
    assert!(removed.status.success());
    assert_eq!(
        projection(&source, "features"),
        projection(&out, "features")
    );
}

#[test]
fn edits_fail_cleanly_and_strict_mode_writes_nothing() {
    let source = root().join("fixtures/formats/snapgene/synthetic_translation.dna");
    // `.dna` is a supported output now: an unedited record is reproduced exactly.
    let snapgene = temp("out.dna");
    let output = run(&[
        "convert",
        source.to_str().unwrap(),
        "--out",
        snapgene.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    assert_eq!(json(&output)["result"]["format"], "snapgene");
    assert_eq!(
        std::fs::read(&snapgene).unwrap(),
        std::fs::read(&source).unwrap()
    );
    let bad_extension = temp("out.txt");
    let output = run(&[
        "convert",
        source.to_str().unwrap(),
        "--out",
        bad_extension.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert_eq!(json(&output)["error"]["code"], "edit_failed");
    assert!(!bad_extension.exists());
    let refused = temp("strict.gb");
    let partial = root().join("fixtures/formats/snapgene/synthetic_partial.dna");
    let output = run(&[
        "convert",
        partial.to_str().unwrap(),
        "--out",
        refused.to_str().unwrap(),
        "--strict",
    ]);
    assert_eq!(json(&output)["error"]["code"], "import_warnings");
    assert!(!refused.exists());
    let output = run(&[
        "annotate",
        source.to_str().unwrap(),
        "--out",
        temp("x.gb").to_str().unwrap(),
        "--add",
        "--range",
        "5..5",
        "--label",
        "empty",
    ]);
    assert_eq!(json(&output)["error"]["code"], "edit_failed");
    let output = run(&[
        "annotate",
        source.to_str().unwrap(),
        "--out",
        temp("y.gb").to_str().unwrap(),
        "--remove",
        "missing",
    ]);
    assert_eq!(json(&output)["error"]["code"], "edit_failed");
}

#[test]
fn third_party_genbank_is_readable() {
    let ncbi = root().join("fixtures/formats/genbank/pUC19_M77789.gb");
    let inspect = json(&run(&[
        "inspect",
        ncbi.to_str().unwrap(),
        "--output",
        "json",
    ]));
    assert_eq!(inspect["result"]["length"], 2686);
    assert_eq!(inspect["result"]["topology"], "circular");
    assert_eq!(inspect["result"]["feature_count"], 8);
}

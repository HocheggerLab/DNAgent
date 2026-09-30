//! CLI contract for `isoforms` on the synthetic locus bundles. Agreement with the bundle
//! (and with `features`/`translate`) is checked independently by scripts/check_locus.py.
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats")
        .join(name)
}

fn run(args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .args(args)
        .output()
        .expect("runs");
    let body = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&output.stdout).into()));
    (output.status.success(), body)
}

fn ids(body: &Value) -> Vec<&str> {
    body["result"]["isoforms"]
        .as_array()
        .expect("isoforms")
        .iter()
        .map(|i| i["transcript_id"].as_str().expect("id"))
        .collect()
}

#[test]
fn orders_by_the_chosen_quantifier() {
    let locus = fixture("locus/synthetic_locus.locus.json");
    let locus = locus.to_str().expect("utf-8");
    let (ok, body) = run(&["isoforms", locus, "--output", "json"]);
    assert!(ok, "{body}");
    assert_eq!(body["command"], "isoforms");
    assert_eq!(body["result"]["quantifier"], "bambu_lr");
    assert_eq!(
        ids(&body),
        [
            "SYNT0001.1",
            "SYNT0002.1",
            "SYNT0004.1",
            "SYNT0003.1",
            "SYNT0005.1"
        ]
    );
    let (ok, body) = run(&[
        "isoforms",
        locus,
        "--quantifier",
        "NanoCount_lr",
        "--output",
        "json",
    ]);
    assert!(ok, "{body}");
    assert_eq!(
        ids(&body),
        [
            "SYNT0002.1",
            "SYNT0001.1",
            "SYNT0004.1",
            "SYNT0003.1",
            "SYNT0005.1"
        ]
    );
}

#[test]
fn refuses_unknown_quantifiers_and_non_locus_documents() {
    let locus = fixture("locus/synthetic_locus.locus.json");
    let (ok, body) = run(&[
        "isoforms",
        locus.to_str().expect("utf-8"),
        "--quantifier",
        "nope",
        "--output",
        "json",
    ]);
    assert!(!ok);
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("NanoCount_lr") && message.contains("bambu_lr"),
        "lists the known quantifiers: {message}"
    );
    let dna = fixture("snapgene/synthetic_linear.dna");
    let (ok, body) = run(&["isoforms", dna.to_str().expect("utf-8"), "--output", "json"]);
    assert!(!ok);
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("message")
            .contains("not a locus document")
    );
}

#[test]
fn text_output_names_every_isoform_and_state() {
    let locus = fixture("locus/synthetic_minus.locus.json");
    let (ok, body) = run(&["isoforms", locus.to_str().expect("utf-8")]);
    assert!(ok);
    let text = body.as_str().expect("text");
    for id in ["SYNM0001.1", "SYNM0002.1", "SYNM0003.1"] {
        assert!(text.contains(id), "{id} missing from:\n{text}");
    }
    assert!(text.contains("discoverable"), "{text}");
}

#[test]
fn locus_bundles_open_like_any_other_document() {
    let locus = fixture("locus/synthetic_locus.locus.json");
    let (ok, body) = run(&[
        "inspect",
        locus.to_str().expect("utf-8"),
        "--output",
        "json",
    ]);
    assert!(ok, "{body}");
    assert_eq!(body["result"]["length"], 6000);
    assert_eq!(body["result"]["topology"], "linear");
}

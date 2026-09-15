use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats/snapgene")
        .join(name)
}

fn run(command: &str, file: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .arg(command)
        .arg(fixture(file))
        .args(extra)
        .output()
        .unwrap()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout must contain one JSON envelope")
}

#[test]
fn all_json_projections_expose_the_same_import_warnings() {
    let inspect = run("inspect", "synthetic_partial.dna", &["--output", "json"]);
    let expected = json(&inspect)["warnings"].clone();
    assert!(!expected.as_array().unwrap().is_empty());
    assert_eq!(expected, json(&inspect)["result"]["warnings"]);
    for command in ["inspect", "features", "sequence"] {
        let output = run(command, "synthetic_partial.dna", &["--output", "json"]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let body = json(&output);
        assert_eq!(body["schema_version"], "0.3.0");
        assert_eq!(body["warnings"], expected);
    }
}

#[test]
fn text_warnings_go_to_stderr_without_contaminating_sequence() {
    for command in ["inspect", "features", "sequence"] {
        let output = run(command, "synthetic_partial.dna", &[]);
        assert!(output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("warning [snapgene_feature_range_unsupported]")
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("warning ["));
        if command == "sequence" {
            assert_eq!(output.stdout, b"ACGTACGTACGT\n");
        }
    }
}

#[test]
fn strict_json_rejects_partial_imports_with_structured_warnings() {
    for command in ["inspect", "features", "sequence"] {
        let output = run(
            command,
            "synthetic_partial.dna",
            &["--strict", "--output", "json"],
        );
        assert!(!output.status.success());
        assert!(output.stderr.is_empty());
        let body = json(&output);
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"]["code"], "import_warnings");
        assert!(!body["warnings"].as_array().unwrap().is_empty());
        assert!(body.get("result").is_none());
    }
}

#[test]
fn strict_rejects_even_preserved_metadata_and_accepts_clean_imports() {
    let output = run("sequence", "synthetic_circular.dna", &["--strict"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("snapgene_packet_not_interpreted"));
    for command in ["inspect", "features", "sequence"] {
        let output = run(
            command,
            "synthetic_linear.dna",
            &["--strict", "--output", "json"],
        );
        assert!(output.status.success());
        assert_eq!(json(&output)["warnings"], serde_json::json!([]));
    }
}

#[test]
fn runtime_errors_keep_warnings_already_encountered() {
    let output = run(
        "sequence",
        "synthetic_partial.dna",
        &["--range", "0..999", "--output", "json"],
    );
    assert!(!output.status.success());
    assert_eq!(json(&output)["ok"], false);
    assert!(!json(&output)["warnings"].as_array().unwrap().is_empty());
    let output = run("inspect", "invalid_truncated.dna", &["--output", "json"]);
    assert!(!output.status.success());
    assert_eq!(json(&output)["warnings"], serde_json::json!([]));
}

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("dnagent-policy-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn map_reports_warnings_and_strict_rejection_does_not_write() {
    let dir = TempDir::new();
    let path = dir.0.join("map.svg");
    let out = path.to_str().unwrap();
    let output = run("map", "synthetic_partial.dna", &["--strict", "--out", out]);
    assert!(!output.status.success());
    assert_eq!(json(&output)["error"]["code"], "import_warnings");
    assert!(!path.exists());
    std::fs::write(&path, b"existing output").unwrap();
    let output = run("map", "synthetic_partial.dna", &["--strict", "--out", out]);
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&path).unwrap(), b"existing output");
    let output = run("map", "synthetic_partial.dna", &["--out", out]);
    assert!(output.status.success());
    assert!(!json(&output)["warnings"].as_array().unwrap().is_empty());
    assert!(std::fs::read_to_string(&path).unwrap().contains("<svg"));
    let output = run("map", "synthetic_linear.dna", &["--strict", "--out", out]);
    assert!(output.status.success());
    assert_eq!(json(&output)["warnings"], serde_json::json!([]));
}

#[test]
fn strict_is_supported_before_the_subcommand_but_not_by_gui() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .args(["--strict", "inspect"])
        .arg(fixture("synthetic_linear.dna"))
        .args(["--output", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .args(["gui", "--strict"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    #[cfg(feature = "gui")]
    assert!(String::from_utf8_lossy(&output.stderr).contains("not gui"));
    #[cfg(not(feature = "gui"))]
    assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
}

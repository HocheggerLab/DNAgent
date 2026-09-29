//! CLI contract for `library` and `detect-features` on hand-written GenBank files.
//! Agreement with Biopython and a brute-force scan is checked by scripts/check_feature_library.py.
use serde_json::Value;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const KAN: &str = "ATGGCTAGCAAAGGAGAAGAACTTTTCACT"; // 30 bp, not palindromic

fn reverse_complement(s: &str) -> String {
    s.chars()
        .rev()
        .map(|c| match c {
            'A' => 'T',
            'C' => 'G',
            'G' => 'C',
            _ => 'A',
        })
        .collect()
}

fn genbank(name: &str, topology: &str, sequence: &str, features: &[(&str, &str, &str)]) -> String {
    let mut text = format!(
        "LOCUS       {name} {} bp    DNA     {topology}   SYN 01-JAN-2026\nFEATURES             Location/Qualifiers\n",
        sequence.len()
    );
    for (kind, location, label) in features {
        writeln!(
            text,
            "     {kind:<16}{location}\n                     /label={label}"
        )
        .unwrap();
    }
    text + &format!("ORIGIN\n        1 {}\n//\n", sequence.to_lowercase())
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("dnagent-cli-library-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("constructs")).unwrap();
        Self(dir)
    }
    fn write(&self, name: &str, text: &[u8]) -> PathBuf {
        let path = self.0.join("constructs").join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
    fn db(&self) -> String {
        self.0.join("features.sqlite").display().to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .args(args)
        .output()
        .unwrap()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout must contain one JSON envelope")
}

fn collection(scratch: &Scratch) -> PathBuf {
    let filler = "GGGGCCCCTT";
    // a.gb: KanR forward, plus features the library must not collect.
    let a = format!("{filler}{KAN}{filler}ACGTACGTACGTACGTACGT");
    scratch.write(
        "a.gb",
        genbank(
            "a",
            "linear",
            &a,
            &[
                ("source", &format!("1..{}", a.len()), "a"),
                ("CDS", "11..40", "KanR"),
                ("misc_feature", "41..60", "Feature 1"),
                ("primer_bind", "41..60", "ACGTACGTACGTACGTACGT"),
                ("misc_feature", "1..8", "short"),
            ],
        )
        .as_bytes(),
    );
    // b.dna: GenBank text saved as .dna; the same sequence on the reverse strand, renamed.
    let b = format!("{filler}{}{filler}", reverse_complement(KAN));
    scratch.write(
        "b.dna",
        genbank(
            "b",
            "circular",
            &b,
            &[("CDS", "complement(11..40)", "NeoR")],
        )
        .as_bytes(),
    );
    // An upload that is not a sequence.
    scratch.write("notes.dna", b"%PDF-1.3 not a plasmid");
    scratch.0.join("constructs")
}

#[test]
fn import_deduplicates_by_sequence_reports_skips_and_is_idempotent() {
    let scratch = Scratch::new("import");
    let folder = collection(&scratch);
    let db = scratch.db();
    let output = run(&[
        "library",
        "--db",
        &db,
        "import",
        folder.to_str().unwrap(),
        "--output",
        "json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = json(&output);
    assert_eq!(body["command"], "library-import");
    let totals = &body["result"]["totals"];
    assert_eq!(
        (
            totals["files"].as_u64(),
            totals["imported"].as_u64(),
            totals["failed"].as_u64()
        ),
        (Some(3), Some(2), Some(1))
    );
    assert_eq!(
        (
            totals["occurrences"].as_u64(),
            totals["new_features"].as_u64()
        ),
        (Some(2), Some(1))
    );
    let skipped = &totals["skipped_features"];
    for reason in [
        "source_feature",
        "generic_name",
        "sequence_as_name",
        "too_short",
    ] {
        assert_eq!(skipped[reason].as_u64(), Some(1), "{reason}: {skipped}");
    }
    let files = body["result"]["files"].as_array().unwrap();
    let b = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("b.dna"))
        .unwrap();
    assert!(
        b["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w == "content_format_mismatch")
    );
    let pdf = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("notes.dna"))
        .unwrap();
    assert_eq!(pdf["status"], "failed");
    assert!(pdf["reason"].as_str().unwrap().contains("PDF"));

    // One feature: first-seen name on a tie, the other name as an alias.
    let list = json(&run(&["library", "--db", &db, "list", "--output", "json"]));
    let features = list["result"].as_array().unwrap();
    assert_eq!(features.len(), 1);
    assert_eq!(
        (
            features[0]["name"].as_str(),
            features[0]["occurrences"].as_u64()
        ),
        (Some("KanR"), Some(2))
    );
    assert_eq!(features[0]["aliases"], serde_json::json!(["NeoR"]));
    let search = json(&run(&[
        "library", "--db", &db, "search", "neo", "--output", "json",
    ]));
    assert_eq!(search["result"].as_array().unwrap().len(), 1);
    let id = features[0]["id"].as_i64().unwrap().to_string();
    let show = json(&run(&[
        "library", "--db", &db, "show", &id, "--output", "json",
    ]));
    assert_eq!(show["result"]["sequence"], KAN);
    assert_eq!(show["result"]["seen_in"].as_array().unwrap().len(), 2);

    let again = json(&run(&[
        "library",
        "--db",
        &db,
        "import",
        folder.to_str().unwrap(),
        "--output",
        "json",
    ]));
    assert_eq!(again["result"]["totals"]["unchanged"].as_u64(), Some(2));
    assert_eq!(again["result"]["library"]["occurrences"].as_u64(), Some(2));
}

#[test]
fn detect_finds_features_across_the_origin_and_marks_existing_annotations() {
    let scratch = Scratch::new("detect");
    let folder = collection(&scratch);
    let db = scratch.db();
    assert!(
        run(&["library", "--db", &db, "import", folder.to_str().unwrap()])
            .status
            .success()
    );
    // KanR through the origin of a 40 bp circle, unannotated; and annotated in a.gb.
    let circle = format!("{}TTTTTAAAAA{}", &KAN[15..], &KAN[..15]);
    let target = scratch.0.join("target.gb");
    std::fs::write(&target, genbank("target", "circular", &circle, &[])).unwrap();
    let body = json(&run(&[
        "detect-features",
        target.to_str().unwrap(),
        "--db",
        &db,
        "--output",
        "json",
    ]));
    assert_eq!(body["command"], "detect-features");
    let matches = body["result"]["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["name"], "KanR");
    assert_eq!(matches[0]["strand"], "forward");
    assert_eq!(
        matches[0]["location"]["parts"],
        serde_json::json!([{"kind": "circular_arc", "start": 25, "length": 30}])
    );
    assert_eq!(matches[0]["annotated_as"], serde_json::json!([]));

    let a = folder.join("a.gb");
    let annotated = json(&run(&[
        "detect-features",
        a.to_str().unwrap(),
        "--db",
        &db,
        "--output",
        "json",
    ]));
    assert_eq!(
        annotated["result"]["matches"][0]["annotated_as"],
        serde_json::json!(["feature-0002"])
    );
    let fresh = json(&run(&[
        "detect-features",
        a.to_str().unwrap(),
        "--db",
        &db,
        "--new-only",
        "--output",
        "json",
    ]));
    assert_eq!(fresh["result"]["matches"], serde_json::json!([]));

    // Hidden features are not detected.
    let id = json(&run(&["library", "--db", &db, "list", "--output", "json"]))["result"][0]["id"]
        .as_i64()
        .unwrap()
        .to_string();
    assert!(
        run(&["library", "--db", &db, "edit", &id, "--hide"])
            .status
            .success()
    );
    let hidden = json(&run(&[
        "detect-features",
        target.to_str().unwrap(),
        "--db",
        &db,
        "--output",
        "json",
    ]));
    assert_eq!(hidden["result"]["matches"], serde_json::json!([]));
}

#[test]
fn a_missing_library_is_a_structured_error() {
    let scratch = Scratch::new("missing");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats/snapgene/pUC19_M77789.dna");
    let output = run(&[
        "detect-features",
        fixture.to_str().unwrap(),
        "--db",
        &scratch.db(),
        "--output",
        "json",
    ]);
    assert!(!output.status.success());
    let body = json(&output);
    assert_eq!(body["error"]["code"], "library_failed");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("dnagent library import")
    );
    assert!(
        !Path::new(&scratch.db()).exists(),
        "querying must not create a library"
    );
}

#[test]
fn variant_families_list_once_and_can_be_split() {
    let scratch = Scratch::new("families");
    let db = scratch.db();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats/genbank/synthetic_variants.gb");
    assert!(
        run(&["library", "--db", &db, "import", fixture.to_str().unwrap()])
            .status
            .success()
    );
    let families = json(&run(&["library", "--db", &db, "list", "--output", "json"]));
    let rows = families["result"].as_array().unwrap();
    let element = rows
        .iter()
        .find(|f| f["name"] == "variant element")
        .unwrap();
    assert_eq!(
        element["variants"].as_u64(),
        Some(1),
        "the 270 bp version is a variant of the 300 bp one"
    );
    assert!(
        rows.iter().all(|f| f["name"] != "variant element short"),
        "variants are listed under their family"
    );
    assert!(
        rows.iter().any(|f| f["name"] == "nested motif"),
        "a much shorter nested part is its own family"
    );
    let all = json(&run(&[
        "library", "--db", &db, "list", "--all", "--output", "json",
    ]));
    let short = all["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "variant element short")
        .unwrap()
        .clone();
    assert_eq!(short["family_id"], element["id"]);
    let id = short["id"].as_i64().unwrap().to_string();
    let split = json(&run(&[
        "library",
        "--db",
        &db,
        "edit",
        &id,
        "--standalone",
        "--output",
        "json",
    ]));
    assert_eq!(split["result"]["grouping"], "standalone");
    assert_eq!(
        split["result"]["family_id"], short["id"],
        "split off into its own family"
    );
    let info = json(&run(&["library", "--db", &db, "info", "--output", "json"]));
    assert_eq!(
        info["result"]["families"].as_u64(),
        info["result"]["features"].as_u64()
    );
}

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/plans")
        .join(name)
}
#[test]
fn optimisation_alias_returns_canonical_envelope_and_variable_lengths() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .current_dir(std::env::temp_dir())
        .arg("gibson-optimize")
        .arg(fixture("synthetic-gibson-optimisation.json"))
        .arg("--strict")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stderr, [] as [u8; 0]);
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["command"], "gibson-optimise");
    assert_eq!(body["schema_version"], "0.10.0");
    let report = &body["result"];
    assert_eq!(report["pairs"].as_array().unwrap().len(), 2);
    assert!(report["design"]["annealing_length"].is_null());
    assert!(report["pairs"][0]["feasible_pairs"].as_u64().unwrap() > 0);
}
#[test]
fn existing_overlap_keeps_shared_provenance_and_rejects_wrong_plan_kind() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .current_dir(std::env::temp_dir())
        .arg("gibson-assemble")
        .arg(fixture("synthetic-gibson-existing.json"))
        .arg("--strict")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stderr, [] as [u8; 0]);
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    let report = &body["result"];
    assert_eq!(report["product_sequence_5to3"].as_str().unwrap().len(), 300);
    assert_eq!(report["components"][1]["product_start"], 155);
    assert_eq!(report["components"][1]["wraps_origin"], true);
    for command in ["gibson-assemble", "gibson-optimise"] {
        let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
            .arg(command)
            .arg(fixture("synthetic-gibson.json"))
            .output()
            .unwrap();
        assert!(!output.status.success());
        let body: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(body["error"]["code"], "gibson_failed");
        assert!(body.get("result").is_none());
    }
}

#[test]
fn mixed_digest_literal_and_pcr_sources_materialise_with_primers() {
    let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
        .current_dir(std::env::temp_dir())
        .arg("gibson-assemble")
        .arg(fixture("synthetic-gibson-mixed.json"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        body["result"]["product_sequence_5to3"]
            .as_str()
            .unwrap()
            .len(),
        225
    );
    assert_eq!(
        body["result"]["inputs"][2]["primers"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let warning_codes: Vec<_> = body["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|warning| warning["code"].as_str())
        .collect();
    assert!(warning_codes.contains(&"gibson_digest_fragment_projection"));
    assert!(warning_codes.contains(&"gibson_pcr_product_projection"));
}

#[test]
fn literal_sources_export_materialised_fasta_and_genbank() {
    let mut state = 91_827_u64;
    let random_dna = |length: usize, state: &mut u64| -> String {
        (0..length)
            .map(|_| {
                *state ^= *state << 13;
                *state ^= *state >> 7;
                *state ^= *state << 17;
                char::from(b"ACGT"[(*state % 4) as usize])
            })
            .collect()
    };
    let overlap = random_dna(20, &mut state);
    let first = format!("{}{overlap}", random_dna(40, &mut state));
    let second = format!("{overlap}{}", random_dna(40, &mut state));
    let plan = serde_json::json!({
        "schema_version": 1,
        "inputs": [
            {"name": "synthetic_a", "sequence": first, "topology": "linear"},
            {"name": "synthetic_b", "sequence": second, "topology": "linear"}
        ],
        "topology": "linear",
        "fragments": [
            {"input": 1, "start": 0, "length": 60, "orientation": "forward"},
            {"input": 2, "start": 0, "length": 60, "orientation": "forward"}
        ],
        "overlaps": [20]
    });
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("dnagent-literal-{nonce}.json"));
    std::fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();

    for (format, marker) in [("fasta", ">dnagent_gibson"), ("genbank", "LOCUS")] {
        let output = Command::new(env!("CARGO_BIN_EXE_dnagent"))
            .arg("gibson-assemble")
            .arg(&path)
            .arg("--output")
            .arg(format)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains(marker));
        assert!(String::from_utf8_lossy(&output.stderr).contains("derived"));
    }
    std::fs::remove_file(path).unwrap();
}

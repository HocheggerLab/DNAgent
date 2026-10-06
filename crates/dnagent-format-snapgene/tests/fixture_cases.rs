//! Hand-authored expectations, not snapshots of the importer under test.
use dnagent_domain::{LocationOperator, Region, Strand, Topology};
use dnagent_format_snapgene::import_bytes;
use dnagent_formats::{ImportError, ImportReport};
use std::path::PathBuf;

fn bytes(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/formats/snapgene")
        .join(name);
    std::fs::read(path).unwrap()
}

fn import(name: &str) -> ImportReport {
    import_bytes(&bytes(name), "synthetic").unwrap()
}

#[test]
fn linear_iupac_multipart_qualifiers_and_primer_are_retained() {
    let report = import("synthetic_linear.dna");
    assert!(report.warnings.is_empty());
    assert_eq!(report.record.topology(), Topology::Linear);
    assert_eq!(report.record.sequence().as_str(), "ACGTRYSWKMBDHVN");
    assert_eq!(report.record.features().len(), 1);
    let feature = &report.record.features()[0];
    assert_eq!(feature.location().strand(), Strand::Reverse);
    assert_eq!(feature.location().operator(), LocationOperator::Join);
    assert_eq!(
        feature.location().parts(),
        &[
            Region::linear(1, 4, 15).unwrap(),
            Region::linear(9, 14, 15).unwrap()
        ]
    );
    assert_eq!(feature.display().color.as_deref(), Some("#123456"));
    let qualifiers: Vec<_> = feature
        .qualifiers()
        .iter()
        .map(|q| (q.key.as_str(), q.value.as_deref()))
        .collect();
    assert_eq!(
        qualifiers,
        vec![
            ("note", Some("first")),
            ("note", Some("second")),
            ("pseudo", None)
        ]
    );
    let primers = report.record.primers();
    assert_eq!(primers.len(), 1);
    assert_eq!(primers[0].name, "synthetic primer");
    assert_eq!(primers[0].sequence.as_str(), "ACGTN");
    assert_eq!(
        primers[0].description.as_deref(),
        Some("retained description")
    );
    assert_eq!(
        report.preserved_metadata.interpreted_source_packets.len(),
        2
    );
}

#[test]
fn unannotated_linear_sequence_is_valid() {
    let report = import("synthetic_unannotated.dna");
    assert_eq!(report.record.sequence().as_str(), "ACGT");
    assert_eq!(report.record.topology(), Topology::Linear);
    assert!(report.record.features().is_empty());
    assert!(report.record.primers().is_empty());
    assert!(report.warnings.is_empty());
}

#[test]
fn multipart_origin_feature_keeps_part_order_and_strand() {
    let report = import("synthetic_multipart_origin.dna");
    let location = report.record.features()[0].location();
    assert_eq!(report.record.topology(), Topology::Circular);
    assert_eq!(location.strand(), Strand::Reverse);
    assert_eq!(location.operator(), LocationOperator::Join);
    assert_eq!(
        location.parts(),
        &[
            Region::circular_arc(10, 4, 12).unwrap(),
            Region::linear(4, 6, 12).unwrap(),
        ]
    );
    assert!(report.warnings.is_empty());
}

#[test]
fn overlapping_features_share_bases_in_source_order() {
    let report = import("synthetic_overlaps.dna");
    let features = report.record.features();
    let labels: Vec<_> = features
        .iter()
        .map(dnagent_domain::Feature::label)
        .collect();
    assert_eq!(
        labels,
        ["overlap forward", "overlap reverse", "overlap multipart"]
    );
    assert_eq!(report.record.topology(), Topology::Linear);
    assert_eq!(
        features[0].location().parts(),
        &[Region::linear(5, 15, 30).unwrap()]
    );
    assert_eq!(features[1].location().strand(), Strand::Reverse);
    assert_eq!(
        features[1].location().parts(),
        &[Region::linear(10, 20, 30).unwrap()]
    );
    assert_eq!(features[2].location().operator(), LocationOperator::Join);
    assert_eq!(
        features[2].location().parts(),
        &[
            Region::linear(1, 4, 30).unwrap(),
            Region::linear(11, 13, 30).unwrap(),
        ]
    );
    assert!(report.warnings.is_empty());
}

#[test]
fn puc19_transcribes_record_features_and_polylinker_sites() {
    let report = import("pUC19_M77789.dna");
    let record = &report.record;
    assert_eq!(record.sequence().len(), 2686);
    assert_eq!(record.topology(), Topology::Circular);
    let features = record.features();
    assert_eq!(features.len(), 17);
    assert_eq!(features[6].label(), "pBR322");
    assert_eq!(features[6].location().strand(), Strand::Reverse);
    assert_eq!(
        features[6].location().parts(),
        &[Region::linear(684, 2686, 2686).unwrap()]
    );
    // SmaI (CCCGGG) and KpnI (GGTACC) overlap by one base in the polylinker.
    assert_eq!(features[13].label(), "SmaI site");
    assert_eq!(
        features[13].location().parts(),
        &[Region::linear(267, 273, 2686).unwrap()]
    );
    assert_eq!(
        features[14].location().parts(),
        &[Region::linear(271, 277, 2686).unwrap()]
    );
    assert_eq!(&record.sequence().as_str()[267..277], "CCCGGGTACC");
    assert!(report.warnings.is_empty());
}

#[test]
fn partial_import_reports_losses_and_retains_source_bytes() {
    let report = import("synthetic_partial.dna");
    let codes: Vec<_> = report.warnings.iter().map(|w| w.code.as_str()).collect();
    for expected in [
        "snapgene_feature_range_unsupported",
        "snapgene_feature_skipped",
        "snapgene_primer_skipped",
        "snapgene_notes_not_interpreted",
        "snapgene_packet_not_interpreted",
    ] {
        assert!(codes.contains(&expected), "missing warning {expected}");
    }
    assert_eq!(report.record.features().len(), 1);
    assert_eq!(report.record.primers().len(), 1);
    assert_eq!(
        report.record.features()[0].location().parts(),
        &[Region::linear(0, 3, 12).unwrap()]
    );
    let metadata = &report.preserved_metadata;
    let raw_features = metadata
        .interpreted_source_packets
        .iter()
        .find(|p| p.packet_type == 10)
        .unwrap();
    assert!(
        std::str::from_utf8(&raw_features.payload)
            .unwrap()
            .contains("10-99")
    );
    let raw_primers = metadata
        .interpreted_source_packets
        .iter()
        .find(|p| p.packet_type == 5)
        .unwrap();
    assert!(
        std::str::from_utf8(&raw_primers.payload)
            .unwrap()
            .contains("AC!T")
    );
    let opaque = metadata
        .opaque_packets
        .iter()
        .find(|p| p.packet_type == 0x1c)
        .unwrap();
    assert_eq!(opaque.payload, b"\x00\xffopaque");
    assert_eq!(opaque.payload_length, opaque.payload.len());
}

#[test]
fn duplicate_sequence_is_rejected() {
    assert!(matches!(
        import_bytes(&bytes("invalid_duplicate_sequence.dna"), "bad"),
        Err(ImportError::DuplicateSequence)
    ));
}

#[test]
fn missing_sequence_is_rejected() {
    assert!(matches!(
        import_bytes(&bytes("invalid_missing_sequence.dna"), "bad"),
        Err(ImportError::MissingSequence)
    ));
}

#[test]
fn truncated_payload_is_rejected() {
    assert!(matches!(
        import_bytes(&bytes("invalid_truncated.dna"), "bad"),
        Err(ImportError::TruncatedPacket { .. })
    ));
}

#[test]
fn malformed_feature_xml_is_rejected() {
    assert!(matches!(
        import_bytes(&bytes("invalid_feature_xml.dna"), "bad"),
        Err(ImportError::InvalidXml {
            packet_type: 10,
            ..
        })
    ));
}

/// A file DNAgent only read must be written back exactly as it arrived: same cookie
/// version fields, same DNA flag bits (including the methylation bits DNAgent does not
/// model), same packet order and the annotation XML untouched.
#[test]
fn every_valid_fixture_round_trips_byte_for_byte() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/formats/snapgene");
    let mut checked = 0;
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if path.extension().is_none_or(|e| e != "dna") || name.starts_with("invalid_") {
            continue;
        }
        let original = bytes(&name);
        let report = dnagent_format_snapgene::import_bytes(&original, &name).unwrap();
        let written = dnagent_format_snapgene::export_bytes(&report).unwrap();
        assert_eq!(written, original, "{name} did not round trip byte for byte");
        checked += 1;
    }
    assert!(
        checked >= 11,
        "expected every valid fixture, checked {checked}"
    );
}

/// Writing an edited record from the file's retained annotation packets would produce a
/// file that disagrees with itself, so it is refused.
#[test]
fn an_edited_record_is_refused_rather_than_written_stale() {
    let report = import("synthetic_linear.dna");
    let mut features = report.record.features().to_vec();
    features.pop();
    let edited = dnagent_formats::ImportReport {
        record: dnagent_domain::SequenceRecord::new(
            report.record.name(),
            report.record.sequence().clone(),
            report.record.topology(),
            features,
            report.record.primers().to_vec(),
        )
        .unwrap(),
        ..report
    };
    let error = dnagent_format_snapgene::export_bytes(&edited).unwrap_err();
    assert!(matches!(
        error,
        dnagent_format_snapgene::ExportError::Modified { .. }
    ));
}

/// A record that never came from SnapGene has no packet layout to reproduce.
#[test]
fn a_record_without_a_snapgene_layout_cannot_be_written() {
    let mut report = import("synthetic_linear.dna");
    report.preserved_metadata.snapgene = None;
    assert_eq!(
        dnagent_format_snapgene::export_bytes(&report).unwrap_err(),
        dnagent_format_snapgene::ExportError::NoLayout
    );
}

/// Opt-in round-trip over real SnapGene files, which carry packets the public fixtures do
/// not (`0x03`, `0x0d`, `0x0e`, `0x11`, `0x23`, …). Private data stays outside the
/// repository: point `DNAGENT_PRIVATE_DNA` at a directory and run with `--ignored`.
#[test]
#[ignore = "needs DNAGENT_PRIVATE_DNA; private constructs are not in the repository"]
fn real_snapgene_files_round_trip_byte_for_byte() {
    let directory = std::env::var("DNAGENT_PRIVATE_DNA")
        .expect("set DNAGENT_PRIVATE_DNA to a directory of .dna files");
    let mut checked = 0;
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "dna") {
            continue;
        }
        let original = std::fs::read(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let report = dnagent_format_snapgene::import_bytes(&original, &name).unwrap();
        let written = dnagent_format_snapgene::export_bytes(&report).unwrap();
        assert_eq!(written.len(), original.len(), "{name}: length differs");
        assert!(
            written == original,
            "{name} did not round trip byte for byte"
        );
        checked += 1;
    }
    assert!(checked > 0, "no .dna files in {directory}");
}

/// Generated annotation packets must describe exactly what the record holds: write every
/// fixture from the model, read it back, and require the same features, primers, sequence
/// and topology. This is the check that matters for records DNAgent builds or edits.
#[test]
fn generated_files_preserve_the_model_for_every_fixture() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/formats/snapgene");
    let mut checked = 0;
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if path.extension().is_none_or(|e| e != "dna") || name.starts_with("invalid_") {
            continue;
        }
        let report = import_bytes(&bytes(&name), &name).unwrap();
        let (written, _) = dnagent_format_snapgene::export_record(
            &report.record,
            Some(&report.preserved_metadata),
        );
        let reread = import_bytes(&written, &name)
            .unwrap_or_else(|error| panic!("{name}: generated file does not read back: {error}"));
        assert_eq!(
            reread.record.sequence(),
            report.record.sequence(),
            "{name}: sequence"
        );
        assert_eq!(
            reread.record.topology(),
            report.record.topology(),
            "{name}: topology"
        );
        assert_eq!(
            reread.record.features(),
            report.record.features(),
            "{name}: features"
        );
        assert_eq!(
            reread.record.primers(),
            report.record.primers(),
            "{name}: primers"
        );
        checked += 1;
    }
    assert!(
        checked >= 11,
        "expected every valid fixture, checked {checked}"
    );
}

/// The same, over real SnapGene files (see the byte-for-byte test for how to point at them).
#[test]
#[ignore = "needs DNAGENT_PRIVATE_DNA; private constructs are not in the repository"]
fn generated_files_preserve_the_model_for_real_files() {
    let directory = std::env::var("DNAGENT_PRIVATE_DNA")
        .expect("set DNAGENT_PRIVATE_DNA to a directory of .dna files");
    let mut checked = 0;
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "dna") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let report = import_bytes(&std::fs::read(&path).unwrap(), &name).unwrap();
        let (written, _) = dnagent_format_snapgene::export_record(
            &report.record,
            Some(&report.preserved_metadata),
        );
        let reread = import_bytes(&written, &name)
            .unwrap_or_else(|error| panic!("{name}: generated file does not read back: {error}"));
        assert_eq!(
            reread.record.features(),
            report.record.features(),
            "{name}: features"
        );
        assert_eq!(
            reread.record.primers(),
            report.record.primers(),
            "{name}: primers"
        );
        assert_eq!(
            reread.record.sequence(),
            report.record.sequence(),
            "{name}: sequence"
        );
        checked += 1;
    }
    assert!(checked > 0, "no .dna files in {directory}");
}

/// Editing an annotation must not rewrite the sequence in a different case: DNAgent
/// upper-cases internally, but the file's own casing is what the user sees in SnapGene.
#[test]
fn generating_keeps_the_source_files_sequence_casing() {
    let report = import("synthetic_linear.dna");
    let (written, _) =
        dnagent_format_snapgene::export_record(&report.record, Some(&report.preserved_metadata));
    let original = bytes("synthetic_linear.dna");
    let sequence_of = |file: &[u8]| {
        let start = file.windows(4).position(|w| w == b"acgt" || w == b"ACGT");
        start.map(|at| String::from_utf8_lossy(&file[at..at + 15]).into_owned())
    };
    assert_eq!(sequence_of(&written), sequence_of(&original));
    assert!(sequence_of(&written).unwrap().starts_with("acgt"));
}

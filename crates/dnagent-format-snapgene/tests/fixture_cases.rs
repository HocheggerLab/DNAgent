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

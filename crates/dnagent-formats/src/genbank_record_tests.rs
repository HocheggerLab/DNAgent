//! Hand-built records and hand-written GenBank; expectations are not writer snapshots.
#![allow(clippy::too_many_arguments, clippy::too_many_lines)] // explicit fixtures read better inline
use super::*;

fn q(key: &str, value: Option<&str>) -> Qualifier {
    Qualifier {
        key: key.into(),
        value: value.map(Into::into),
    }
}

fn feature(
    id: &str,
    kind: &str,
    label: &str,
    parts: Vec<Region>,
    strand: Strand,
    operator: LocationOperator,
    qualifiers: Vec<Qualifier>,
    color: Option<&str>,
) -> Feature {
    Feature::new(
        FeatureId::new(id).unwrap(),
        kind,
        label,
        Location::new(parts, strand, operator).unwrap(),
        qualifiers,
        DisplayHints {
            color: color.map(Into::into),
        },
    )
}

fn rich_report() -> ImportReport {
    let length = 40;
    let sequence = DnaSeq::new("ATGGCCTAAACGTACGTACGTRYSWKMNACGTACGTACGT").unwrap();
    let features = vec![
        feature(
            "feature-0001",
            "CDS",
            "forward \"quoted\" CDS",
            vec![Region::linear(0, 9, length).unwrap()],
            Strand::Forward,
            LocationOperator::Contiguous,
            vec![
                q("note", Some("first")),
                q("note", Some("second")),
                q("pseudo", None),
                q("codon_start", Some("1")),
                q("label", Some("imported label qualifier")),
                q("translation", Some(&"M".repeat(150))),
                q(
                    "note",
                    Some(
                        "a long free-text note that certainly needs to wrap across several GenBank lines because it is long",
                    ),
                ),
            ],
            Some("#123456"),
        ),
        feature(
            "feature-0002",
            "misc_feature",
            "reverse join",
            vec![
                Region::linear(1, 4, length).unwrap(),
                Region::linear(9, 14, length).unwrap(),
            ],
            Strand::Reverse,
            LocationOperator::Join,
            vec![],
            None,
        ),
        feature(
            "feature-0003",
            "promoter",
            "origin arc",
            vec![Region::circular_arc(35, 10, length).unwrap()],
            Strand::Reverse,
            LocationOperator::Contiguous,
            vec![],
            Some("#abcdef"),
        ),
        feature(
            "feature-0004",
            "misc_feature",
            "unknown strand",
            vec![Region::linear(20, 25, length).unwrap()],
            Strand::Unknown,
            LocationOperator::Contiguous,
            vec![],
            None,
        ),
        feature(
            "custom-id",
            "misc_feature",
            "",
            vec![
                Region::linear(5, 6, length).unwrap(),
                Region::linear(30, 33, length).unwrap(),
            ],
            Strand::Forward,
            LocationOperator::Order,
            vec![q("note", Some(""))],
            None,
        ),
        feature(
            "feature-0006",
            "rep_origin",
            "arc then linear",
            vec![
                Region::circular_arc(38, 4, length).unwrap(),
                Region::linear(4, 6, length).unwrap(),
            ],
            Strand::Reverse,
            LocationOperator::Join,
            vec![],
            None,
        ),
    ];
    let primers = vec![
        ImportedPrimer {
            name: "p1".into(),
            sequence: DnaSeq::new("ACGTN").unwrap(),
            description: Some("retained \"description\"".into()),
        },
        ImportedPrimer {
            name: "p2".into(),
            sequence: DnaSeq::new("GGG").unwrap(),
            description: None,
        },
    ];
    let record = SequenceRecord::new(
        "my construct v2",
        sequence,
        Topology::Circular,
        features,
        primers,
    )
    .unwrap();
    ImportReport {
        record,
        warnings: vec![
            ImportWarning::new(
                "snapgene_packet_not_interpreted",
                "preserved uninterpreted packet 0x1c (4 bytes)",
            )
            .for_packet(0x1c),
        ],
        preserved_metadata: FormatExtensions {
            snapgene: None,
            opaque_packets: vec![OpaquePacket::new(0x1c, vec![0, 255, 1, 2])],
            interpreted_source_packets: vec![OpaquePacket::new(10, b"<Features/>".to_vec())],
            genbank_header: vec![],
            locus: None,
        },
    }
}

fn options() -> WriteOptions {
    WriteOptions {
        date: "29-SEP-2026".into(),
    }
}

#[test]
fn base64_round_trips_every_padding_case() {
    for bytes in [
        &b""[..],
        b"f",
        b"fo",
        b"foo",
        b"foob",
        b"fooba",
        b"foobar",
        &[0, 255, 128, 7],
    ] {
        assert_eq!(base64_decode(&base64_encode(bytes)).unwrap(), bytes);
    }
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert!(base64_decode("abc").is_none());
}

#[test]
fn dnagent_records_round_trip_losslessly() {
    let report = rich_report();
    let (text, warnings) = write(&report, &options());
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        text.lines().all(|line| line.chars().count() <= 80
            || line.contains("/translation")
            || line.trim_start().starts_with('M')),
        "long line"
    );
    let back = read(text.as_bytes(), "fallback").unwrap();
    assert_eq!(back.record, report.record);
    assert_eq!(back.warnings, report.warnings);
    assert_eq!(
        back.preserved_metadata.opaque_packets,
        report.preserved_metadata.opaque_packets
    );
    assert_eq!(
        back.preserved_metadata.interpreted_source_packets,
        report.preserved_metadata.interpreted_source_packets
    );
    // Writing what was read gives byte-identical text (header now preserved verbatim).
    assert_eq!(write(&back, &options()).0, text);
}

#[test]
fn written_locations_are_standard_genbank() {
    let (text, _) = write(&rich_report(), &options());
    assert!(text.contains("     CDS             1..9\n"));
    assert!(text.contains("     misc_feature    complement(join(2..4,10..14))\n"));
    assert!(text.contains("     promoter        complement(join(36..40,1..5))\n"));
    assert!(text.contains("/dnagent_location=\"v1;contiguous;reverse;A35+10\""));
    assert!(text.contains("/dnagent_location=\"v1;contiguous;unknown;L20-25\""));
    assert!(text.contains("     misc_feature    order(6..6,31..33)\n"));
    assert!(text.contains("/dnagent_id=\"custom-id\""));
    assert!(text.contains("/codon_start=1\n"));
    assert!(text.contains("/label=\"forward \"\"quoted\"\" CDS\""));
    assert!(text.contains("LOCUS       my_construct_v2"));
    assert!(text.ends_with("//\n"));
}

#[test]
fn control_characters_are_reported_not_silently_changed() {
    let mut report = rich_report();
    let original = report.record.features()[0].clone();
    let edited = Feature::new(
        original.id().clone(),
        original.kind(),
        "line\nbreak",
        original.location().clone(),
        vec![],
        DisplayHints::default(),
    );
    report.record = SequenceRecord::new(
        "x",
        report.record.sequence().clone(),
        Topology::Circular,
        vec![edited],
        vec![],
    )
    .unwrap();
    let (_, warnings) = write(&report, &options());
    assert_eq!(warnings[0].code, "genbank_control_characters_flattened");
}

const THIRD_PARTY: &str =
    "LOCUS       DEMO                      30 bp    DNA     circular SYN 01-JAN-2020
DEFINITION  Hand-written demo record.
ACCESSION   DEMO1
KEYWORDS    .
SOURCE      synthetic
  ORGANISM  synthetic
COMMENT     Kept verbatim.
FEATURES             Location/Qualifiers
     source          1..30
                     /organism=\"synthetic\"
     CDS             join(complement(11..13),complement(1..6))
                     /gene=\"demo\"
                     /translation=\"MAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
                     AAAAAAAA\"
     misc_feature    <3..>8
                     /note=\"partial\"
     misc_feature    J00194.1:100..202
     misc_feature    28..3
                     /note=\"wraps; and a \"\"quote\"\" inside\"
ORIGIN
        1 acgtacgtac gtacgtacgt acgtacgtac
//
";

#[test]
fn third_party_genbank_is_read_with_structured_warnings() {
    let report = read(THIRD_PARTY.as_bytes(), "fallback").unwrap();
    let record = &report.record;
    assert_eq!(
        (record.name(), record.topology(), record.sequence().len()),
        ("DEMO", Topology::Circular, 30)
    );
    let features = record.features();
    assert_eq!(features.len(), 4); // remote location skipped
    assert_eq!(
        features[0].label(),
        "synthetic".to_owned().replace("synthetic", "source")
    );
    // join(complement(B),complement(A)) == complement(join(A,B)): parts kept A, B.
    assert_eq!(features[1].label(), "demo");
    assert_eq!(features[1].location().strand(), Strand::Reverse);
    assert_eq!(
        features[1].location().parts(),
        &[
            Region::linear(0, 6, 30).unwrap(),
            Region::linear(10, 13, 30).unwrap()
        ]
    );
    let translation = features[1]
        .qualifiers()
        .iter()
        .find(|q| q.key == "translation")
        .unwrap();
    assert_eq!(translation.value.as_deref().unwrap().len(), 59);
    assert!(!translation.value.as_deref().unwrap().contains(' '));
    assert_eq!(
        features[3].location().parts(),
        &[Region::circular_arc(27, 6, 30).unwrap()]
    );
    assert_eq!(
        features[3].qualifiers()[0].value.as_deref(),
        Some("wraps; and a \"quote\" inside")
    );
    let codes: Vec<&str> = report.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(
        codes,
        ["genbank_partial_location", "genbank_feature_skipped"]
    );
    assert_eq!(
        report.preserved_metadata.genbank_header[0],
        "DEFINITION  Hand-written demo record."
    );
    assert!(
        report
            .preserved_metadata
            .genbank_header
            .iter()
            .any(|l| l.contains("Kept verbatim."))
    );
    // Header survives a DNAgent rewrite verbatim.
    let (text, _) = write(&report, &options());
    assert!(text.contains("DEFINITION  Hand-written demo record.\nACCESSION   DEMO1\n"));
}

#[test]
fn malformed_input_is_rejected() {
    assert!(read(b"no locus", "x").is_err());
    assert!(
        read(
            b"LOCUS       X 4 bp DNA linear\nFEATURES             Location/Qualifiers\n",
            "x"
        )
        .is_err()
    );
    let bad_block = THIRD_PARTY.replace("COMMENT     Kept verbatim.", "COMMENT     BEGIN-DNAAGENT-DATA (JSON; do not edit)\n            {not json\n            END-DNAAGENT-DATA");
    assert!(read(bad_block.as_bytes(), "x").is_err());
}

#[test]
fn data_block_survives_whitespace_trimming_at_chunk_boundaries() {
    let mut report = rich_report();
    // Every other character is a space, so chunk boundaries fall on spaces.
    let spaced = "a ".repeat(200);
    report.record = SequenceRecord::new(
        spaced.clone(),
        report.record.sequence().clone(),
        Topology::Circular,
        vec![],
        vec![],
    )
    .unwrap();
    let (text, _) = write(&report, &options());
    let trimmed = text.lines().fold(String::new(), |mut all, line| {
        all.push_str(line.trim_end());
        all.push('\n');
        all
    });
    assert_eq!(read(trimmed.as_bytes(), "x").unwrap().record.name(), spaced);
}

#[test]
fn files_written_before_the_rename_still_open() {
    let (text, _) = write(&rich_report(), &options());
    let legacy = text
        .replace("BEGIN-DNAGENT-DATA", "BEGIN-DNAAGENT-DATA")
        .replace("END-DNAGENT-DATA", "END-DNAAGENT-DATA");
    assert_ne!(legacy, text);
    assert_eq!(
        read(legacy.as_bytes(), "x").unwrap().record,
        rich_report().record
    );
}

#[test]
fn unquoted_values_continue_on_the_next_line() {
    // As written by ApE: an unquoted qualifier value wrapped onto a continuation line.
    let text = "LOCUS       ape_style                 12 bp ds-DNA     circular     09-FEB-2015\n\
FEATURES             Location/Qualifiers\n\
\x20    primer_bind     complement(2..7)\n\
\x20                    /label=M13-fwd\n\
\x20                    /ApEinfo_graphicformat=arrow_data {{0 1 2 0 0 -1} {} 0}\n\
\x20                    width 5 offset 0\n\
\x20    misc_feature    9..10\n\
\x20                    /label=second\n\
ORIGIN\n\
\x20       1 acgtacgtac gt\n\
//\n";
    let report = read(text.as_bytes(), "fallback").unwrap();
    let features = report.record.features();
    assert_eq!(features.len(), 2);
    let graphic = features[0]
        .qualifiers()
        .iter()
        .find(|q| q.key == "ApEinfo_graphicformat")
        .unwrap();
    assert_eq!(
        graphic.value.as_deref(),
        Some("arrow_data {{0 1 2 0 0 -1} {} 0} width 5 offset 0")
    );
    assert_eq!(features[1].label(), "second");
    // A stray line after a quoted value is still an error.
    let bad = text.replace(
        "/ApEinfo_graphicformat=arrow_data {{0 1 2 0 0 -1} {} 0}",
        "/note=\"quoted\"",
    );
    assert!(read(bad.as_bytes(), "fallback").is_err());
}

#[test]
fn origin_placeholders_keep_coordinates() {
    let text = "LOCUS       placeholder               10 bp    DNA     linear   SYN 01-JAN-2026\n\
FEATURES             Location/Qualifiers\n\
\x20    misc_feature    8..10\n\
\x20                    /label=after\n\
ORIGIN\n\
\x20       1 acg*tac gta\n\
//\n";
    let report = read(text.as_bytes(), "fallback").unwrap();
    assert_eq!(report.record.sequence().as_str(), "ACGNTACGTA");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.code == "genbank_sequence_placeholder" && w.message.contains("'*' at 3"))
    );
    assert!(
        report
            .warnings
            .iter()
            .all(|w| w.code != "genbank_length_mismatch")
    );
    let short = text.replace("10 bp", "12 bp");
    let report = read(short.as_bytes(), "fallback").unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.code == "genbank_length_mismatch")
    );
}

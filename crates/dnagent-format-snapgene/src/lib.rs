//! Read-only SnapGene `.dna` migration adapter.
//!
//! Packet framing and XML DTOs are informed by PlasCAD's MIT-licensed reader
//! at commit `717459dcd780ec4266e9a2aa15297eae31956da5`.
//! Copyright (c) 2025 David O'Connor. See the repository `NOTICE` and
//! `licenses/PlasCAD-MIT.txt` for the complete permission notice.

use dnagent_domain::{
    DisplayHints, DnaSeq, Feature, FeatureId, ImportedPrimer, Location, LocationOperator,
    Qualifier, Region, SequenceRecord, Strand, Topology,
};
use dnagent_formats::{
    FormatExtensions, ImportError, ImportReport, ImportWarning, OpaquePacket, SnapGeneLayout,
};
use quick_xml::de::from_str;
use serde::Deserialize;
use std::fmt::Write as _;
use std::str;

const COOKIE: u8 = 0x09;
const DNA: u8 = 0x00;
const PRIMERS: u8 = 0x05;
const NOTES: u8 = 0x06;
const FEATURES: u8 = 0x0a;
const COOKIE_PAYLOAD_LENGTH: usize = 14;

/// Import a SnapGene record from an in-memory byte slice.
pub fn import_bytes(
    bytes: &[u8],
    record_name: impl Into<String>,
) -> Result<ImportReport, ImportError> {
    let packets = packets(bytes)?;
    let Some(first) = packets.first() else {
        return Err(invalid_format("file contains no packets"));
    };
    if first.packet_type != COOKIE {
        return Err(invalid_format("first packet is not the SnapGene cookie"));
    }
    validate_cookie(first.payload)?;

    let mut sequence = None;
    let mut raw_sequence = None;
    let mut dna_flags = 0u8;
    let mut topology = Topology::Linear;
    let mut raw_features = Vec::new();
    let mut raw_primers = Vec::new();
    let mut warnings = Vec::new();
    let mut extensions = FormatExtensions::default();
    let packet_order = packets.iter().map(|p| p.packet_type).collect();
    let cookie = first.payload.to_vec();

    for packet in packets.into_iter().skip(1) {
        match packet.packet_type {
            DNA => {
                if sequence.is_some() {
                    return Err(ImportError::DuplicateSequence);
                }
                let (parsed_sequence, parsed_topology, flags, text) = parse_dna(packet.payload)?;
                raw_sequence = (text != parsed_sequence.as_str()).then_some(text);
                sequence = Some(parsed_sequence);
                topology = parsed_topology;
                dna_flags = flags;
            }
            FEATURES => {
                raw_features.extend(parse_features_xml(packet.payload)?);
                extensions
                    .interpreted_source_packets
                    .push(OpaquePacket::new(FEATURES, packet.payload.to_vec()));
            }
            PRIMERS => {
                raw_primers.extend(parse_primers_xml(packet.payload)?);
                extensions
                    .interpreted_source_packets
                    .push(OpaquePacket::new(PRIMERS, packet.payload.to_vec()));
            }
            NOTES => preserve_packet(
                &packet,
                "snapgene_notes_not_interpreted",
                &mut warnings,
                &mut extensions,
            ),
            _ => preserve_packet(
                &packet,
                "snapgene_packet_not_interpreted",
                &mut warnings,
                &mut extensions,
            ),
        }
    }

    let sequence = sequence.ok_or(ImportError::MissingSequence)?;
    extensions.snapgene = Some(SnapGeneLayout {
        cookie,
        dna_flags,
        raw_sequence,
        packet_order,
    });
    let sequence_length = sequence.len();
    let features = convert_features(raw_features, sequence_length, topology, &mut warnings);
    let primers = convert_primers(raw_primers, &mut warnings);

    Ok(ImportReport {
        record: SequenceRecord::new(record_name, sequence, topology, features, primers)?,
        warnings,
        preserved_metadata: extensions,
    })
}

#[derive(Debug)]
struct Packet<'a> {
    packet_type: u8,
    payload: &'a [u8],
}

fn packets(bytes: &[u8]) -> Result<Vec<Packet<'_>>, ImportError> {
    let mut packets = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let available = bytes.len() - offset;
        if available < 5 {
            return Err(ImportError::TruncatedPacket {
                offset,
                expected: 5,
                available,
            });
        }
        let packet_type = bytes[offset];
        let payload_length = u32::from_be_bytes(
            bytes[offset + 1..offset + 5]
                .try_into()
                .expect("a four-byte length header was checked"),
        ) as usize;
        let payload_offset = offset + 5;
        let available = bytes.len() - payload_offset;
        if payload_length > available {
            return Err(ImportError::TruncatedPacket {
                offset: payload_offset,
                expected: payload_length,
                available,
            });
        }
        let end = payload_offset + payload_length;
        packets.push(Packet {
            packet_type,
            payload: &bytes[payload_offset..end],
        });
        offset = end;
    }
    Ok(packets)
}

fn validate_cookie(payload: &[u8]) -> Result<(), ImportError> {
    if payload.len() != COOKIE_PAYLOAD_LENGTH {
        return Err(invalid_format(format!(
            "cookie payload has length {}, expected {COOKIE_PAYLOAD_LENGTH}",
            payload.len()
        )));
    }
    if !payload.starts_with(b"SnapGene") {
        return Err(invalid_format("cookie does not start with SnapGene"));
    }
    Ok(())
}

fn parse_dna(payload: &[u8]) -> Result<(DnaSeq, Topology, u8, String), ImportError> {
    let Some((&flags, sequence)) = payload.split_first() else {
        return Err(invalid_format("DNA packet is empty"));
    };
    let sequence = str::from_utf8(sequence).map_err(|source| ImportError::InvalidUtf8 {
        packet_type: DNA,
        source,
    })?;
    let topology = if flags & 0x01 == 0 {
        Topology::Linear
    } else {
        Topology::Circular
    };
    Ok((DnaSeq::new(sequence)?, topology, flags, sequence.to_owned()))
}

fn preserve_packet(
    packet: &Packet<'_>,
    code: &str,
    warnings: &mut Vec<ImportWarning>,
    extensions: &mut FormatExtensions,
) {
    warnings.push(
        ImportWarning::new(
            code,
            format!(
                "preserved uninterpreted packet 0x{:02x} ({} bytes)",
                packet.packet_type,
                packet.payload.len()
            ),
        )
        .for_packet(packet.packet_type),
    );
    extensions.opaque_packets.push(OpaquePacket::new(
        packet.packet_type,
        packet.payload.to_vec(),
    ));
}

#[derive(Debug, Default, Deserialize)]
struct FeaturesXml {
    #[serde(rename = "Feature", default)]
    features: Vec<FeatureXml>,
}

#[derive(Debug, Deserialize)]
struct FeatureXml {
    #[serde(rename = "@type", default)]
    kind: Option<String>,
    #[serde(rename = "@directionality", default)]
    directionality: Option<String>,
    #[serde(rename = "@name", default)]
    name: Option<String>,
    #[serde(rename = "Segment", default)]
    segments: Vec<SegmentXml>,
    #[serde(rename = "Q", default)]
    qualifiers: Vec<QualifierXml>,
}

#[derive(Debug, Deserialize)]
struct SegmentXml {
    #[serde(rename = "@range", default)]
    range: Option<String>,
    #[serde(rename = "@color", default)]
    color: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QualifierXml {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "V", default)]
    values: Vec<QualifierValueXml>,
}

#[derive(Debug, Deserialize)]
struct QualifierValueXml {
    #[serde(rename = "@text", default)]
    text: Option<String>,
    #[serde(rename = "@predef", default)]
    predefined: Option<String>,
    #[serde(rename = "@int", default)]
    integer: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct PrimersXml {
    #[serde(rename = "Primer", default)]
    primers: Vec<PrimerXml>,
}

#[derive(Debug, Deserialize)]
struct PrimerXml {
    #[serde(rename = "@sequence")]
    sequence: String,
    #[serde(rename = "@name", default)]
    name: String,
    #[serde(rename = "@description", default)]
    description: Option<String>,
}

fn parse_features_xml(payload: &[u8]) -> Result<Vec<FeatureXml>, ImportError> {
    let xml = str::from_utf8(payload).map_err(|source| ImportError::InvalidUtf8 {
        packet_type: FEATURES,
        source,
    })?;
    from_str::<FeaturesXml>(xml)
        .map(|value| value.features)
        .map_err(|error| ImportError::InvalidXml {
            packet_type: FEATURES,
            message: error.to_string(),
        })
}

fn parse_primers_xml(payload: &[u8]) -> Result<Vec<PrimerXml>, ImportError> {
    let xml = str::from_utf8(payload).map_err(|source| ImportError::InvalidUtf8 {
        packet_type: PRIMERS,
        source,
    })?;
    from_str::<PrimersXml>(xml)
        .map(|value| value.primers)
        .map_err(|error| ImportError::InvalidXml {
            packet_type: PRIMERS,
            message: error.to_string(),
        })
}

fn convert_features(
    features: Vec<FeatureXml>,
    molecule_length: usize,
    topology: Topology,
    warnings: &mut Vec<ImportWarning>,
) -> Vec<Feature> {
    let mut converted = Vec::new();
    for (feature_index, feature) in features.into_iter().enumerate() {
        let mut parts = Vec::new();
        for segment in &feature.segments {
            let Some(range) = &segment.range else {
                warnings.push(ImportWarning::new(
                    "snapgene_feature_segment_missing_range",
                    format!(
                        "feature {} contains a segment without a range",
                        feature_index + 1
                    ),
                ));
                continue;
            };
            match convert_range(range, molecule_length, topology) {
                Ok(region) => parts.push(region),
                Err(reason) => warnings.push(ImportWarning::new(
                    "snapgene_feature_range_unsupported",
                    format!("feature {} range {range:?}: {reason}", feature_index + 1),
                )),
            }
        }
        if parts.is_empty() {
            warnings.push(ImportWarning::new(
                "snapgene_feature_skipped",
                format!("feature {} has no supported segments", feature_index + 1),
            ));
            continue;
        }

        let operator = if parts.len() == 1 {
            LocationOperator::Contiguous
        } else {
            LocationOperator::Join
        };
        let strand = match feature.directionality.as_deref() {
            Some("1") => Strand::Forward,
            Some("2") => Strand::Reverse,
            _ => Strand::Unknown,
        };
        let location = Location::new(parts, strand, operator)
            .expect("converted feature locations are non-empty and have a matching operator");
        let qualifiers = feature
            .qualifiers
            .into_iter()
            .flat_map(|qualifier| {
                if qualifier.values.is_empty() {
                    vec![Qualifier {
                        key: qualifier.name,
                        value: None,
                    }]
                } else {
                    qualifier
                        .values
                        .into_iter()
                        .map(|value| Qualifier {
                            key: qualifier.name.clone(),
                            value: value
                                .text
                                .or(value.predefined)
                                .or_else(|| value.integer.map(|number| number.to_string())),
                        })
                        .collect()
                }
            })
            .collect();
        let display = DisplayHints {
            color: feature
                .segments
                .iter()
                .find_map(|segment| segment.color.clone()),
        };
        let id = FeatureId::new(format!("feature-{:04}", feature_index + 1))
            .expect("generated feature identifiers are non-empty");
        converted.push(Feature::new(
            id,
            feature.kind.unwrap_or_else(|| "misc_feature".to_owned()),
            feature.name.unwrap_or_default(),
            location,
            qualifiers,
            display,
        ));
    }
    converted
}

fn convert_range(
    range: &str,
    molecule_length: usize,
    topology: Topology,
) -> Result<Region, String> {
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| "expected one-based inclusive start-end".to_owned())?;
    let start = start
        .parse::<usize>()
        .map_err(|_| "start is not an integer".to_owned())?;
    let end = end
        .parse::<usize>()
        .map_err(|_| "end is not an integer".to_owned())?;
    if start == 0 || end == 0 {
        return Err("SnapGene coordinates must be one-based".to_owned());
    }
    if start <= end {
        Region::linear(start - 1, end, molecule_length).map_err(|error| error.to_string())
    } else if topology == Topology::Circular && start <= molecule_length && end <= molecule_length {
        let length = molecule_length - (start - 1) + end;
        Region::circular_arc(start - 1, length, molecule_length).map_err(|error| error.to_string())
    } else {
        Err("wrapping range requires circular topology and in-bounds endpoints".to_owned())
    }
}

fn convert_primers(
    primers: Vec<PrimerXml>,
    warnings: &mut Vec<ImportWarning>,
) -> Vec<ImportedPrimer> {
    primers
        .into_iter()
        .enumerate()
        .filter_map(|(index, primer)| match DnaSeq::new(&primer.sequence) {
            Ok(sequence) => Some(ImportedPrimer {
                name: primer.name,
                sequence,
                description: primer.description.filter(|value| !value.is_empty()),
            }),
            Err(error) => {
                warnings.push(ImportWarning::new(
                    "snapgene_primer_skipped",
                    format!("primer {} has an invalid sequence: {error}", index + 1),
                ));
                None
            }
        })
        .collect()
}

fn invalid_format(reason: impl Into<String>) -> ImportError {
    ImportError::InvalidFormat {
        format: "SnapGene",
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(packet_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut result = vec![packet_type];
        result.extend(u32::try_from(payload.len()).unwrap().to_be_bytes());
        result.extend(payload);
        result
    }

    fn synthetic_file(topology_flag: u8, sequence: &str, features: Option<&str>) -> Vec<u8> {
        let mut bytes = packet(COOKIE, b"SnapGene\0\0\0\0\0\0");
        let mut dna = vec![topology_flag];
        dna.extend(sequence.as_bytes());
        bytes.extend(packet(DNA, &dna));
        if let Some(features) = features {
            bytes.extend(packet(FEATURES, features.as_bytes()));
        }
        bytes
    }

    #[test]
    fn imports_sequence_topology_and_feature() {
        let xml = r##"<Features><Feature name="ampR" type="CDS" directionality="2"><Segment range="3-8" color="#ff0000"/><Q name="note"><V text="test"/></Q></Feature></Features>"##;
        let report = import_bytes(&synthetic_file(1, "acgtacgtac", Some(xml)), "fixture").unwrap();
        assert_eq!(report.record.sequence().as_str(), "ACGTACGTAC");
        assert_eq!(report.record.topology(), Topology::Circular);
        assert_eq!(report.record.features().len(), 1);
        let feature = &report.record.features()[0];
        assert_eq!(feature.label(), "ampR");
        assert_eq!(feature.location().strand(), Strand::Reverse);
        assert_eq!(feature.location().parts()[0].start().get(), 2);
        assert_eq!(feature.location().parts()[0].length().get(), 6);
    }

    #[test]
    fn preserves_origin_spanning_feature_as_one_arc() {
        let xml = r#"<Features><Feature name="wrap"><Segment range="9-2"/></Feature></Features>"#;
        let report = import_bytes(&synthetic_file(1, "acgtacgtac", Some(xml)), "fixture").unwrap();
        let region = &report.record.features()[0].location().parts()[0];
        assert!(region.is_circular_arc());
        assert_eq!(region.start().get(), 8);
        assert_eq!(region.length().get(), 4);
    }

    #[test]
    fn rejects_truncated_packet_without_panicking() {
        let error = import_bytes(&[COOKIE, 0, 0, 0, 14, b'S'], "bad").unwrap_err();
        assert!(matches!(error, ImportError::TruncatedPacket { .. }));
    }

    #[test]
    fn preserves_unknown_packets_with_warning() {
        let mut bytes = synthetic_file(0, "ACGT", None);
        bytes.extend(packet(0x1c, b"opaque"));
        let report = import_bytes(&bytes, "fixture").unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(report.preserved_metadata.opaque_packets.len(), 1);
    }
}

// ------------------------------------------------------------------------- export

/// Escape a value for an XML attribute written with double quotes.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(character),
        }
    }
    out
}

/// One `Segment` range, as SnapGene writes them: one-based and inclusive, with a wrapping
/// arc written as `start-end` where the end has come back round past the origin.
fn segment_range(region: &Region, molecule_length: usize) -> String {
    let start = region.start().get();
    let end_exclusive = start + region.length().get();
    let last = if region.is_circular_arc() {
        (end_exclusive + molecule_length - 1) % molecule_length + 1
    } else {
        end_exclusive
    };
    format!("{}-{last}", start + 1)
}

/// Build the `Features` packet payload from the record's own annotations, for a record
/// DNAgent created or edited. Only what DNAgent models is written: SnapGene's display and
/// detection attributes (`translationMW`, `maxRunOn`, `detectionMode`, …) are absent, so a
/// generated file is faithful to the biology but is not a byte copy of a SnapGene one.
fn features_xml(record: &SequenceRecord) -> String {
    let molecule_length = record.sequence().len();
    let mut xml = String::from("<?xml version=\"1.0\"?><Features>");
    for (index, feature) in record.features().iter().enumerate() {
        let _ = write!(xml, "<Feature recentID=\"{index}\"");
        let _ = write!(xml, " name=\"{}\"", escape(feature.label()));
        let _ = write!(xml, " type=\"{}\"", escape(feature.kind()));
        if let Some(directionality) = match feature.location().strand() {
            Strand::Forward => Some("1"),
            Strand::Reverse => Some("2"),
            Strand::Unknown => None,
        } {
            let _ = write!(xml, " directionality=\"{directionality}\"");
        }
        xml.push('>');
        for part in feature.location().parts() {
            let _ = write!(
                xml,
                "<Segment range=\"{}\" type=\"standard\"",
                segment_range(part, molecule_length)
            );
            if let Some(color) = &feature.display().color {
                let _ = write!(xml, " color=\"{}\"", escape(color));
            }
            xml.push_str("/>");
        }
        for qualifier in feature.qualifiers() {
            let _ = write!(xml, "<Q name=\"{}\">", escape(&qualifier.key));
            if let Some(value) = &qualifier.value {
                // SnapGene stores whole numbers as @int; our reader turns @int back into a
                // string, so write @int only where that round trip is exact.
                match value.parse::<i64>() {
                    Ok(number) if number.to_string() == *value => {
                        let _ = write!(xml, "<V int=\"{number}\"/>");
                    }
                    _ => {
                        let _ = write!(xml, "<V text=\"{}\"/>", escape(value));
                    }
                }
            }
            xml.push_str("</Q>");
        }
        xml.push_str("</Feature>");
    }
    xml.push_str("</Features>");
    xml
}

/// Build the `Primers` packet payload from the record's primer list.
fn primers_xml(record: &SequenceRecord) -> String {
    let mut xml = String::from("<?xml version=\"1.0\"?><Primers>");
    for primer in record.primers() {
        let _ = write!(
            xml,
            "<Primer name=\"{}\" sequence=\"{}\"",
            escape(&primer.name),
            escape(primer.sequence.as_str())
        );
        if let Some(description) = &primer.description {
            let _ = write!(xml, " description=\"{}\"", escape(description));
        }
        xml.push_str("/>");
    }
    xml.push_str("</Primers>");
    xml
}

/// Why a record cannot be written as SnapGene `.dna`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExportError {
    #[error(
        "this record was not read from a SnapGene file, so its packet layout is unknown; save GenBank instead"
    )]
    NoLayout,
    #[error(
        "the record no longer matches the SnapGene annotations it was read with ({reason}); writing it would produce a file that contradicts itself"
    )]
    Modified { reason: String },
}

fn packet(packet_type: u8, payload: &[u8], out: &mut Vec<u8>) {
    out.push(packet_type);
    out.extend_from_slice(
        &u32::try_from(payload.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    out.extend_from_slice(payload);
}

/// Write a record that was read from `.dna` and not edited since, reproducing the original
/// file byte for byte: the cookie and DNA flags as they arrived, every packet in its
/// original order, and the annotation packets as their untouched source XML.
///
/// Records DNAgent built or edited are refused rather than written from stale annotation
/// packets; generating SnapGene annotation XML from the model is separate work.
pub fn export_bytes(report: &ImportReport) -> Result<Vec<u8>, ExportError> {
    let extensions = &report.preserved_metadata;
    let layout = extensions.snapgene.as_ref().ok_or(ExportError::NoLayout)?;
    verify_unmodified(report)?;

    let mut out = Vec::new();
    let mut interpreted = extensions.interpreted_source_packets.iter();
    let mut opaque = extensions.opaque_packets.iter();
    let mut sequence_written = false;
    for &packet_type in &layout.packet_order {
        match packet_type {
            COOKIE => packet(COOKIE, &layout.cookie, &mut out),
            DNA if !sequence_written => {
                let text = layout
                    .raw_sequence
                    .as_deref()
                    .filter(|raw| raw.eq_ignore_ascii_case(report.record.sequence().as_str()))
                    .unwrap_or_else(|| report.record.sequence().as_str());
                let mut payload = Vec::with_capacity(text.len() + 1);
                payload.push(layout.dna_flags);
                payload.extend_from_slice(text.as_bytes());
                packet(DNA, &payload, &mut out);
                sequence_written = true;
            }
            FEATURES | PRIMERS => {
                let source = interpreted.next().ok_or_else(|| ExportError::Modified {
                    reason: format!("no retained source packet for type 0x{packet_type:02x}"),
                })?;
                packet(source.packet_type, &source.payload, &mut out);
            }
            _ => {
                let source = opaque.next().ok_or_else(|| ExportError::Modified {
                    reason: format!("no retained source packet for type 0x{packet_type:02x}"),
                })?;
                packet(source.packet_type, &source.payload, &mut out);
            }
        }
    }
    Ok(out)
}

/// The retained annotation packets must still describe the record. Re-reading them is the
/// only honest check: an edit that added, removed or moved a feature invalidates them.
fn verify_unmodified(report: &ImportReport) -> Result<(), ExportError> {
    let mut raw_features = Vec::new();
    let mut raw_primers = Vec::new();
    for source in &report.preserved_metadata.interpreted_source_packets {
        match source.packet_type {
            FEATURES => raw_features.extend(parse_features_xml(&source.payload).map_err(|e| {
                ExportError::Modified {
                    reason: e.to_string(),
                }
            })?),
            PRIMERS => raw_primers.extend(parse_primers_xml(&source.payload).map_err(|e| {
                ExportError::Modified {
                    reason: e.to_string(),
                }
            })?),
            _ => {}
        }
    }
    let mut ignored = Vec::new();
    let record = &report.record;
    let features = convert_features(
        raw_features,
        record.sequence().len(),
        record.topology(),
        &mut ignored,
    );
    let primers = convert_primers(raw_primers, &mut ignored);
    if features != record.features() {
        return Err(ExportError::Modified {
            reason: format!(
                "{} annotations in the file, {} in the record",
                features.len(),
                record.features().len()
            ),
        });
    }
    if primers != record.primers() {
        return Err(ExportError::Modified {
            reason: "the primer list differs from the file's".into(),
        });
    }
    Ok(())
}

/// The cookie SnapGene writes for a DNA file: the marker, file type 1, and export/import
/// versions. Used when a record has no SnapGene ancestry to copy one from.
const CANONICAL_COOKIE: [u8; COOKIE_PAYLOAD_LENGTH] = *b"SnapGene\x00\x01\x00\x01\x00\x01";

/// Write any record as `.dna`, generating the annotation packets from the model.
///
/// The result holds only what DNAgent models: cookie, sequence and the `Features` and
/// `Primers` packets. Uninterpreted packets of a source file are **not** carried over,
/// because their meaning is unknown and they may describe annotations the record no longer
/// has; each is reported instead. Prefer [`export_bytes`] when the record is unchanged.
#[must_use]
pub fn export_record(
    record: &SequenceRecord,
    source: Option<&FormatExtensions>,
) -> (Vec<u8>, Vec<ImportWarning>) {
    let layout = source.and_then(|extensions| extensions.snapgene.as_ref());
    let mut warnings = Vec::new();

    let cookie = layout.map_or_else(|| CANONICAL_COOKIE.to_vec(), |l| l.cookie.clone());
    // Bit 0 is topology, which the record owns; any other bit (methylation and the like) is
    // kept only when this record came from a SnapGene file that set it.
    let circular = u8::from(record.topology() == Topology::Circular);
    let flags = layout.map_or(circular, |l| (l.dna_flags & !1) | circular);

    if let Some(extensions) = source {
        for packet in &extensions.opaque_packets {
            warnings.push(
                ImportWarning::new(
                    "snapgene_packet_not_written",
                    format!(
                        "uninterpreted packet 0x{:02x} ({} bytes) is not carried into a generated file; its meaning is unknown and it may not describe this record",
                        packet.packet_type, packet.payload_length
                    ),
                )
                .for_packet(packet.packet_type),
            );
        }
    }
    if layout.is_some_and(|l| l.dna_flags & !1 != 0) {
        warnings.push(ImportWarning::new(
            "snapgene_dna_flags_retained",
            format!(
                "sequence flag bits 0x{:02x} (methylation and similar settings DNAgent does not model) are copied from the source file unchanged",
                flags & !1
            ),
        ));
    }
    warnings.push(ImportWarning::new(
        "snapgene_annotations_generated",
        "annotations were written from DNAgent's model; SnapGene's display and detection attributes are absent",
    ));

    let mut out = Vec::new();
    packet(COOKIE, &cookie, &mut out);
    let mut dna = Vec::with_capacity(record.sequence().len() + 1);
    dna.push(flags);
    dna.extend_from_slice(record.sequence().as_str().as_bytes());
    packet(DNA, &dna, &mut out);
    if !record.features().is_empty() {
        packet(FEATURES, features_xml(record).as_bytes(), &mut out);
    }
    if !record.primers().is_empty() {
        packet(PRIMERS, primers_xml(record).as_bytes(), &mut out);
    }
    (out, warnings)
}

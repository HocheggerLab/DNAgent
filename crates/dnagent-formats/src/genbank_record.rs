//! Lossless GenBank records for DNAgent (read and write).
//!
//! The body is ordinary GenBank that other tools read normally. Where GenBank has no
//! standard form for something DNAgent models, namespaced additions carry it:
//!
//! - `/label` holds the feature label (written first, so an imported `label`
//!   qualifier survives as the second one);
//! - `/dnagent_color` holds the display colour;
//! - `/dnagent_location` holds the exact location when the standard location cannot
//!   (origin-spanning single parts, unknown strand);
//! - `/dnagent_id` holds a feature id that differs from its default position-based id;
//! - a `BEGIN-DNAGENT-DATA` … `END-DNAGENT-DATA` (earlier `…DNAAGENT…` markers are still read) COMMENT block (deliberately not NCBI structured-comment syntax) holds JSON with
//!   the original record name, unplaced primers, retained SnapGene packets (base64) and
//!   the source's import warnings, so they are re-reported on reopening.
//!
//! Third-party header sections (DEFINITION … COMMENT) are preserved verbatim.

use crate::{FormatExtensions, ImportError, ImportReport, ImportWarning, OpaquePacket};
use dnagent_domain::{
    DisplayHints, DnaSeq, Feature, FeatureId, ImportedPrimer, Location, LocationOperator,
    Qualifier, Region, SequenceRecord, Strand, Topology,
};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

const FORMAT: &str = "genbank";
const DATA_START: &str = "BEGIN-DNAGENT-DATA (JSON; do not edit)";
const DATA_END: &str = "END-DNAGENT-DATA";
/// Markers written before the product was renamed DNAgent; still read.
const LEGACY_DATA_START: &str = "BEGIN-DNAAGENT-DATA";
const LEGACY_DATA_END: &str = "END-DNAAGENT-DATA";
const QUALIFIER_INDENT: &str = "                     ";
const WRAP: usize = 58;

/// Options that must be explicit for deterministic output.
#[derive(Debug, Clone)]
pub struct WriteOptions {
    /// LOCUS date, e.g. `29-SEP-2026`.
    pub date: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DataBlock {
    format: String,
    version: u32,
    name: String,
    #[serde(default)]
    primers: Vec<PrimerData>,
    #[serde(default)]
    snapgene_packets: Vec<PacketData>,
    #[serde(default)]
    source_warnings: Vec<WarningData>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PrimerData {
    name: String,
    sequence: String,
    description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PacketData {
    packet_type: u8,
    /// `opaque` (never interpreted) or `interpreted_source` (raw copy at original import).
    role: String,
    base64: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WarningData {
    code: String,
    message: String,
    packet_type: Option<u8>,
}

// ---------------------------------------------------------------- base64 (RFC 4648)

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| ALPHABET.iter().position(|&a| a == c).map(|p| p as u32);
    let bytes: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().rev().take_while(|&&b| b == b'=').count();
        let mut n = 0u32;
        for &b in &chunk[..4 - pad] {
            n = (n << 6) | value(b)?;
        }
        n <<= 6 * pad as u32;
        let decoded = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&decoded[..3 - pad]);
    }
    Some(out)
}

// ---------------------------------------------------------------- writing

fn part_ranges(region: &Region, length: usize) -> Vec<(usize, usize)> {
    let start = region.start().get();
    let span = region.length().get();
    if start + span <= length {
        vec![(start + 1, start + span)]
    } else {
        vec![(start + 1, length), (1, start + span - length)]
    }
}

fn standard_location(location: &Location, length: usize) -> String {
    let ranges: Vec<String> = location
        .parts()
        .iter()
        .flat_map(|part| part_ranges(part, length))
        .map(|(a, b)| format!("{a}..{b}"))
        .collect();
    let body = if ranges.len() == 1 {
        ranges[0].clone()
    } else {
        let operator = if location.operator() == LocationOperator::Order {
            "order"
        } else {
            "join"
        };
        format!("{operator}({})", ranges.join(","))
    };
    if location.strand() == Strand::Reverse {
        format!("complement({body})")
    } else {
        body
    }
}

/// Exact location in DNAgent notation, e.g. `v1;join;reverse;A10+4,L4-6`.
fn exact_location(location: &Location) -> String {
    let operator = match location.operator() {
        LocationOperator::Contiguous => "contiguous",
        LocationOperator::Join => "join",
        LocationOperator::Order => "order",
    };
    let strand = match location.strand() {
        Strand::Forward => "forward",
        Strand::Reverse => "reverse",
        Strand::Unknown => "unknown",
    };
    let parts: Vec<String> = location
        .parts()
        .iter()
        .map(|part| {
            let (start, span) = (part.start().get(), part.length().get());
            if part.is_circular_arc() {
                format!("A{start}+{span}")
            } else {
                format!("L{start}-{}", start + span)
            }
        })
        .collect();
    format!("v1;{operator};{strand};{}", parts.join(","))
}

fn needs_exact(location: &Location) -> bool {
    location.strand() == Strand::Unknown || location.parts().iter().any(Region::is_circular_arc)
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Qualifier lines, wrapped only at single spaces (readers rejoin with one space),
/// except `translation`, which wraps anywhere and is rejoined without spaces.
fn write_qualifier(out: &mut String, key: &str, value: Option<&str>) {
    let Some(value) = value else {
        let _ = writeln!(out, "{QUALIFIER_INDENT}/{key}");
        return;
    };
    let text = if matches!(key, "codon_start" | "transl_table")
        && value.chars().all(|c| c.is_ascii_digit())
        && !value.is_empty()
    {
        value.to_owned()
    } else {
        quote(value)
    };
    let first = format!("/{key}={text}");
    let mut lines: Vec<String> = Vec::new();
    if key == "translation" {
        let chars: Vec<char> = first.chars().collect();
        for chunk in chars.chunks(WRAP) {
            lines.push(chunk.iter().collect());
        }
    } else {
        // Break at the last single space before the width; the space is dropped and
        // readers rejoin continuation lines with one space. Longer words overflow.
        let chars: Vec<char> = first.chars().collect();
        let mut begin = 0;
        while chars.len() - begin > WRAP {
            let window = &chars[begin..(begin + WRAP + 1).min(chars.len())];
            let cut = (1..window.len().saturating_sub(1)).rev().find(|&i| {
                window[i] == ' '
                    && window[i - 1] != ' '
                    && chars.get(begin + i + 1).is_some_and(|n| *n != ' ')
            });
            let Some(cut) = cut else {
                // No break point in the window: find the next one after it, if any.
                match (begin + WRAP..chars.len()).find(|&j| {
                    chars[j] == ' '
                        && chars[j - 1] != ' '
                        && chars.get(j + 1).is_some_and(|n| *n != ' ')
                }) {
                    Some(j) => {
                        lines.push(chars[begin..j].iter().collect());
                        begin = j + 1;
                        continue;
                    }
                    None => break,
                }
            };
            lines.push(chars[begin..begin + cut].iter().collect());
            begin += cut + 1;
        }
        lines.push(chars[begin..].iter().collect());
    }
    for line in lines {
        let _ = writeln!(out, "{QUALIFIER_INDENT}{line}");
    }
}

fn clean_value(value: &str, warnings: &mut Vec<ImportWarning>, context: &str) -> String {
    if value.chars().any(char::is_control) {
        warnings.push(ImportWarning::new(
            "genbank_control_characters_flattened",
            format!("{context}: line breaks/control characters replaced by spaces (GenBank cannot store them)"),
        ));
        value
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect()
    } else {
        value.to_owned()
    }
}

fn write_header(out: &mut String, report: &ImportReport, options: &WriteOptions) {
    let record = &report.record;
    let length = record.sequence().len();
    let locus: String = record
        .name()
        .chars()
        .map(|c| if c.is_ascii_graphic() { c } else { '_' })
        .collect();
    let locus = if locus.is_empty() {
        "untitled".to_owned()
    } else {
        locus
    };
    let topology = if record.topology() == Topology::Circular {
        "circular"
    } else {
        "linear"
    };
    let _ = writeln!(
        *out,
        "LOCUS       {locus:<16} {length:>11} bp    DNA     {topology:<8} SYN {}",
        options.date
    );
    let header = &report.preserved_metadata.genbank_header;
    if header.is_empty() {
        let _ = writeln!(
            out,
            "DEFINITION  {}.",
            record
                .name()
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect::<String>()
        );
        out.push_str("ACCESSION   .\nVERSION     .\nKEYWORDS    .\nSOURCE      synthetic DNA construct\n  ORGANISM  synthetic DNA construct\n");
    } else {
        for line in header {
            out.push_str(line);
            out.push('\n');
        }
    }
}

fn data_block(report: &ImportReport) -> DataBlock {
    let record = &report.record;
    DataBlock {
        format: "dnagent-genbank-extension".into(),
        version: 1,
        name: record.name().to_owned(),
        primers: record
            .primers()
            .iter()
            .map(|p| PrimerData {
                name: p.name.clone(),
                sequence: p.sequence.as_str().to_owned(),
                description: p.description.clone(),
            })
            .collect(),
        snapgene_packets: report
            .preserved_metadata
            .opaque_packets
            .iter()
            .map(|p| ("opaque", p))
            .chain(
                report
                    .preserved_metadata
                    .interpreted_source_packets
                    .iter()
                    .map(|p| ("interpreted_source", p)),
            )
            .map(|(role, p)| PacketData {
                packet_type: p.packet_type,
                role: role.into(),
                base64: base64_encode(&p.payload),
            })
            .collect(),
        source_warnings: report
            .warnings
            .iter()
            .map(|w| WarningData {
                code: w.code.clone(),
                message: w.message.clone(),
                packet_type: w.packet_type,
            })
            .collect(),
    }
}

fn write_data_block(out: &mut String, block: &DataBlock) {
    // Compact JSON only has spaces inside strings; escaping them makes the chunked
    // lines immune to whitespace trimming by readers and editors.
    let json = serde_json::to_string(block)
        .expect("serialisable block")
        .replace(' ', "\\u0020");
    out.push_str("COMMENT     ");
    out.push_str(DATA_START);
    out.push('\n');
    let chars: Vec<char> = json.chars().collect();
    for chunk in chars.chunks(66) {
        let _ = writeln!(*out, "            {}", chunk.iter().collect::<String>());
    }
    let _ = writeln!(*out, "            {DATA_END}");
}

fn write_features(out: &mut String, record: &SequenceRecord, warnings: &mut Vec<ImportWarning>) {
    let length = record.sequence().len();
    out.push_str("FEATURES             Location/Qualifiers\n");
    for (index, feature) in record.features().iter().enumerate() {
        let kind = if feature.kind().is_empty() {
            "misc_feature"
        } else {
            feature.kind()
        };
        let kind: String = kind
            .chars()
            .map(|c| if c.is_whitespace() { '_' } else { c })
            .collect();
        let _ = writeln!(
            out,
            "     {kind:<16}{}",
            standard_location(feature.location(), length)
        );
        let context = format!("feature {}", feature.id().as_str());
        write_qualifier(
            out,
            "label",
            Some(&clean_value(feature.label(), warnings, &context)),
        );
        if feature.id().as_str() != format!("feature-{:04}", index + 1) {
            write_qualifier(out, "dnagent_id", Some(feature.id().as_str()));
        }
        if let Some(color) = &feature.display().color {
            write_qualifier(out, "dnagent_color", Some(color));
        }
        if needs_exact(feature.location()) {
            write_qualifier(
                out,
                "dnagent_location",
                Some(&exact_location(feature.location())),
            );
        }
        for qualifier in feature.qualifiers() {
            let value = qualifier
                .value
                .as_deref()
                .map(|v| clean_value(v, warnings, &context));
            write_qualifier(out, &qualifier.key, value.as_deref());
        }
    }
}

fn write_origin(out: &mut String, record: &SequenceRecord) {
    out.push_str("ORIGIN\n");
    for (line, bases) in record.sequence().as_str().as_bytes().chunks(60).enumerate() {
        let _ = write!(out, "{:>9}", line * 60 + 1);
        for group in bases.chunks(10) {
            out.push(' ');
            out.push_str(&String::from_utf8_lossy(group).to_ascii_lowercase());
        }
        out.push('\n');
    }
    out.push_str("//\n");
}

/// Write a record, its retained format metadata and the source's import warnings.
/// Returns the text and any limitation of this write (never silent).
#[must_use]
pub fn write(report: &ImportReport, options: &WriteOptions) -> (String, Vec<ImportWarning>) {
    let mut warnings = Vec::new();
    let mut out = String::new();
    write_header(&mut out, report, options);
    write_data_block(&mut out, &data_block(report));
    write_features(&mut out, &report.record, &mut warnings);
    write_origin(&mut out, &report.record);
    (out, warnings)
}

// ---------------------------------------------------------------- reading

fn invalid(reason: impl Into<String>) -> ImportError {
    ImportError::InvalidFormat {
        format: FORMAT,
        reason: reason.into(),
    }
}

#[derive(Debug)]
struct RawRange {
    start: usize,
    end: usize,
    complement: bool,
    partial: bool,
}

/// Parse the supported location grammar into ranges and an operator.
fn parse_location(text: &str) -> Result<(Vec<RawRange>, LocationOperator), String> {
    fn inner(
        text: &str,
        complement: bool,
        out: &mut Vec<RawRange>,
        operator: &mut Option<LocationOperator>,
    ) -> Result<(), String> {
        let text = text.trim();
        if let Some(rest) = text
            .strip_prefix("complement(")
            .and_then(|r| r.strip_suffix(')'))
        {
            return inner(rest, !complement, out, operator);
        }
        for (name, op) in [
            ("join(", LocationOperator::Join),
            ("order(", LocationOperator::Order),
        ] {
            if let Some(rest) = text.strip_prefix(name).and_then(|r| r.strip_suffix(')')) {
                if operator.is_some_and(|existing| existing != op) {
                    return Err("mixed join/order operators".into());
                }
                *operator = Some(op);
                let mut depth = 0usize;
                let mut start = 0usize;
                for (i, c) in rest.char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => depth = depth.saturating_sub(1),
                        ',' if depth == 0 => {
                            inner(&rest[start..i], complement, out, operator)?;
                            start = i + 1;
                        }
                        _ => {}
                    }
                }
                return inner(&rest[start..], complement, out, operator);
            }
        }
        if text.contains(':') {
            return Err(format!("remote location {text:?} is not supported"));
        }
        if text.contains('^') {
            return Err(format!("between-base location {text:?} is not supported"));
        }
        let partial = text.contains('<') || text.contains('>');
        let clean: String = text.chars().filter(|c| !matches!(c, '<' | '>')).collect();
        let (a, b) = clean
            .split_once("..")
            .unwrap_or((clean.as_str(), clean.as_str()));
        let start = a
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("unsupported location {text:?}"))?;
        let end = b
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("unsupported location {text:?}"))?;
        if start == 0 || end == 0 {
            return Err(format!("GenBank locations are one-based: {text:?}"));
        }
        out.push(RawRange {
            start,
            end,
            complement,
            partial,
        });
        Ok(())
    }
    let mut ranges = Vec::new();
    let mut operator = None;
    inner(text, false, &mut ranges, &mut operator)?;
    Ok((ranges, operator.unwrap_or(LocationOperator::Contiguous)))
}

fn build_location(
    ranges: &[RawRange],
    operator: LocationOperator,
    length: usize,
    circular: bool,
) -> Result<Location, String> {
    let complemented = ranges.iter().filter(|r| r.complement).count();
    let strand = if complemented == 0 {
        Strand::Forward
    } else if complemented == ranges.len() {
        Strand::Reverse
    } else {
        return Err("mixed-strand locations are not supported".into());
    };
    let mut regions = Vec::with_capacity(ranges.len());
    for range in ranges {
        let region = if range.start <= range.end {
            Region::linear(range.start - 1, range.end, length)
        } else if circular {
            Region::circular_arc(
                range.start - 1,
                length - range.start + 1 + range.end,
                length,
            )
        } else {
            return Err(format!(
                "range {}..{} runs backwards on a linear molecule",
                range.start, range.end
            ));
        };
        regions.push(region.map_err(|e| e.to_string())?);
    }
    let operator = if regions.len() == 1 {
        LocationOperator::Contiguous
    } else if operator == LocationOperator::Contiguous {
        LocationOperator::Join
    } else {
        operator
    };
    Location::new(regions, strand, operator).map_err(|e| e.to_string())
}

fn parse_exact(text: &str, length: usize) -> Result<Location, String> {
    let fields: Vec<&str> = text.split(';').collect();
    let [version, operator, strand, parts] = fields.as_slice() else {
        return Err("expected 4 fields".into());
    };
    if *version != "v1" {
        return Err(format!("unknown version {version}"));
    }
    let operator = match *operator {
        "contiguous" => LocationOperator::Contiguous,
        "join" => LocationOperator::Join,
        "order" => LocationOperator::Order,
        other => return Err(format!("unknown operator {other}")),
    };
    let strand = match *strand {
        "forward" => Strand::Forward,
        "reverse" => Strand::Reverse,
        "unknown" => Strand::Unknown,
        other => return Err(format!("unknown strand {other}")),
    };
    let mut regions = Vec::new();
    for part in parts.split(',') {
        let region = if let Some(rest) = part.strip_prefix('A') {
            let (s, l) = rest.split_once('+').ok_or("bad arc")?;
            Region::circular_arc(
                s.parse().map_err(|_| "bad arc start")?,
                l.parse().map_err(|_| "bad arc length")?,
                length,
            )
        } else if let Some(rest) = part.strip_prefix('L') {
            let (s, e) = rest.split_once('-').ok_or("bad range")?;
            Region::linear(
                s.parse().map_err(|_| "bad start")?,
                e.parse().map_err(|_| "bad end")?,
                length,
            )
        } else {
            return Err(format!("bad part {part}"));
        };
        regions.push(region.map_err(|e| e.to_string())?);
    }
    Location::new(regions, strand, operator).map_err(|e| e.to_string())
}

/// Forward-reference positions covered by a location, in source order (for consistency checks).
fn covered(location: &Location, length: usize) -> Vec<usize> {
    location
        .parts()
        .iter()
        .flat_map(|p| (0..p.length().get()).map(move |i| (p.start().get() + i) % length))
        .collect()
}

struct RawFeature {
    key: String,
    location: String,
    qualifiers: Vec<(String, Option<String>)>,
}

fn parse_features(lines: &[&str]) -> Result<Vec<RawFeature>, ImportError> {
    let mut features: Vec<RawFeature> = Vec::new();
    let mut in_location = false;
    let mut open_value: Option<(String, String)> = None; // key, accumulated raw (with opening quote)
    let finish = |feature: &mut RawFeature, key: String, raw: String| {
        let value = raw.trim();
        let value = if value.starts_with('"') {
            let inner = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or_else(|| value.strip_prefix('"').unwrap_or(value));
            inner.replace("\"\"", "\"")
        } else {
            value.to_owned()
        };
        feature.qualifiers.push((key, Some(value)));
    };
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let is_key_line =
            line.len() > 5 && line.starts_with("     ") && !line[5..].starts_with(' ');
        if is_key_line && open_value.is_none() {
            let rest = &line[5..];
            let (key, location) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            features.push(RawFeature {
                key: key.to_owned(),
                location: location.trim().to_owned(),
                qualifiers: Vec::new(),
            });
            in_location = true;
            continue;
        }
        let Some(feature) = features.last_mut() else {
            return Err(invalid("qualifier before any feature"));
        };
        let content = line.trim_start();
        if let Some((key, raw)) = open_value.take() {
            let joiner = if key == "translation" { "" } else { " " };
            let raw = format!("{raw}{joiner}{content}");
            if quotes_closed(&raw) {
                finish(feature, key, raw);
            } else {
                open_value = Some((key, raw));
            }
            continue;
        }
        if let Some(qualifier) = content.strip_prefix('/') {
            in_location = false;
            match qualifier.split_once('=') {
                None => feature
                    .qualifiers
                    .push((qualifier.trim_end().to_owned(), None)),
                Some((key, raw)) => {
                    if raw.starts_with('"') && !quotes_closed(raw) {
                        open_value = Some((key.to_owned(), raw.to_owned()));
                    } else {
                        finish(feature, key.to_owned(), raw.to_owned());
                    }
                }
            }
        } else if in_location {
            feature.location.push_str(content.trim());
        } else {
            return Err(invalid(format!(
                "unexpected feature-table line {content:?}"
            )));
        }
    }
    if open_value.is_some() {
        return Err(invalid("unterminated qualifier value"));
    }
    Ok(features)
}

/// A value starting with `"` is closed when its quotes (with `""` escapes) balance.
fn quotes_closed(raw: &str) -> bool {
    let body = raw.trim_end();
    body.len() >= 2 && body.ends_with('"') && body.matches('"').count().is_multiple_of(2)
}

/// Header lines after LOCUS, verbatim, and the DNAgent data block if present.
fn parse_header(lines: &[&str]) -> Result<(Vec<String>, Option<DataBlock>), ImportError> {
    let header_end = lines.len();
    // Header: keep sections verbatim, except the DNAgent data block.
    let mut header: Vec<String> = Vec::new();
    let mut data: Option<DataBlock> = None;
    let mut index = 0;
    while index < header_end {
        let line = lines[index];
        let marker = line.get(7..).map(str::trim_start).unwrap_or_default();
        if line.starts_with("COMMENT")
            && (marker.starts_with(DATA_START) || marker.starts_with(LEGACY_DATA_START))
        {
            let mut json = String::new();
            index += 1;
            while index < header_end
                && !lines[index].trim().starts_with(DATA_END)
                && !lines[index].trim().starts_with(LEGACY_DATA_END)
            {
                json.push_str(lines[index].trim());
                index += 1;
            }
            index += 1;
            let block: DataBlock = serde_json::from_str(&json)
                .map_err(|e| invalid(format!("invalid DNAgent data block: {e}")))?;
            if block.version != 1 {
                return Err(invalid(format!(
                    "unsupported DNAgent data block version {}",
                    block.version
                )));
            }
            data = Some(block);
            continue;
        }
        header.push(line.to_owned());
        index += 1;
    }

    Ok((header, data))
}

fn convert_feature(
    position: usize,
    raw: RawFeature,
    taken: usize,
    length: usize,
    circular: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Result<Option<Feature>, ImportError> {
    let context = format!("feature {} ({} {})", position + 1, raw.key, raw.location);
    let parsed = parse_location(&raw.location).and_then(|(ranges, operator)| {
        if ranges.iter().any(|r| r.partial) {
            warnings.push(ImportWarning::new(
                "genbank_partial_location",
                format!("{context}: partial-end markers < > are not modelled and were dropped"),
            ));
        }
        // `join(complement(B),complement(A))` lists parts in transcript order; DNAgent
        // keeps `complement(join(A,B))` source order, so individually complemented parts reverse.
        let mut ranges = ranges;
        let individually = ranges.len() > 1
            && ranges.iter().all(|r| r.complement)
            && !raw.location.trim_start().starts_with("complement(");
        if individually {
            ranges.reverse();
        }
        build_location(&ranges, operator, length, circular)
    });
    let mut location = match parsed {
        Ok(location) => location,
        Err(reason) => {
            warnings.push(ImportWarning::new(
                "genbank_feature_skipped",
                format!("{context}: {reason}"),
            ));
            return Ok(None);
        }
    };
    let mut label = None;
    let mut color = None;
    let mut id = None;
    let mut qualifiers = Vec::new();
    for (key, value) in raw.qualifiers {
        match key.as_str() {
            "label" if label.is_none() => label = Some(value.unwrap_or_default()),
            "dnagent_color" if color.is_none() => color = value,
            "dnagent_id" if id.is_none() => id = value,
            "dnagent_location" => match value.as_deref().map(|v| parse_exact(v, length)) {
                // Only accepted when it describes the same bases, in the same order, as
                // the standard location (and the same strand, or unknown on a forward one).
                Some(Ok(exact))
                    if covered(&exact, length) == covered(&location, length)
                        && (exact.strand() == location.strand()
                            || (exact.strand() == Strand::Unknown && location.strand() == Strand::Forward)) =>
                {
                    location = exact;
                }
                _ => warnings.push(ImportWarning::new("genbank_dnagent_location_ignored", format!("{context}: /dnagent_location disagrees with the standard location; the standard location was used"))),
            },
            _ => qualifiers.push(Qualifier { key, value }),
        }
    }
    let label = label.unwrap_or_else(|| {
        ["gene", "product", "locus_tag", "standard_name", "note"]
            .iter()
            .find_map(|k| {
                qualifiers
                    .iter()
                    .find(|q| q.key == *k)
                    .and_then(|q| q.value.clone())
            })
            .unwrap_or_else(|| raw.key.clone())
    });
    let id = FeatureId::new(id.unwrap_or_else(|| format!("feature-{:04}", taken + 1)))?;
    Ok(Some(Feature::new(
        id,
        raw.key,
        label,
        location,
        qualifiers,
        DisplayHints { color },
    )))
}

fn apply_data_block(
    block: DataBlock,
    preserved: &mut FormatExtensions,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(String, Vec<ImportedPrimer>), ImportError> {
    for packet in block.snapgene_packets {
        let payload = base64_decode(&packet.base64)
            .ok_or_else(|| invalid("invalid base64 in DNAgent data block"))?;
        let restored = OpaquePacket::new(packet.packet_type, payload);
        match packet.role.as_str() {
            "opaque" => preserved.opaque_packets.push(restored),
            "interpreted_source" => preserved.interpreted_source_packets.push(restored),
            other => {
                return Err(invalid(format!("unknown SnapGene packet role {other:?}")));
            }
        }
    }
    warnings.splice(
        0..0,
        block.source_warnings.into_iter().map(|w| ImportWarning {
            code: w.code,
            message: w.message,
            packet_type: w.packet_type,
        }),
    );
    let primers = block
        .primers
        .into_iter()
        .map(|p| {
            Ok(ImportedPrimer {
                name: p.name,
                sequence: DnaSeq::new(&p.sequence)?,
                description: p.description,
            })
        })
        .collect::<Result<Vec<_>, ImportError>>()?;
    Ok((block.name, primers))
}

/// Read a GenBank record (DNAgent-written or third-party).
pub fn read(bytes: &[u8], fallback_name: &str) -> Result<ImportReport, ImportError> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("file is not UTF-8"))?;
    let lines: Vec<&str> = text.lines().collect();
    let locus = lines
        .iter()
        .position(|l| l.starts_with("LOCUS"))
        .ok_or_else(|| invalid("no LOCUS line"))?;
    if lines.iter().filter(|l| l.starts_with("LOCUS")).count() > 1 {
        return Err(invalid("exactly one GenBank record is supported"));
    }
    let tokens: Vec<&str> = lines[locus].split_whitespace().collect();
    let locus_name = tokens.get(1).copied().unwrap_or(fallback_name);
    let mut warnings = Vec::new();
    let topology = if tokens.iter().any(|t| t.eq_ignore_ascii_case("circular")) {
        Topology::Circular
    } else {
        if !tokens.iter().any(|t| t.eq_ignore_ascii_case("linear")) {
            warnings.push(ImportWarning::new(
                "genbank_topology_missing",
                "LOCUS line declares no topology; imported as linear",
            ));
        }
        Topology::Linear
    };
    let features_at = lines.iter().position(|l| l.starts_with("FEATURES"));
    let origin_at = lines
        .iter()
        .position(|l| l.starts_with("ORIGIN"))
        .ok_or_else(|| invalid("no ORIGIN section"))?;
    let header_end = features_at.unwrap_or(origin_at);

    let (header, data) = parse_header(&lines[locus + 1..header_end])?;
    let mut sequence = String::new();
    for line in &lines[origin_at + 1..] {
        if line.starts_with("//") {
            break;
        }
        sequence.extend(line.chars().filter(char::is_ascii_alphabetic));
    }
    let sequence = DnaSeq::new(&sequence)?;
    let length = sequence.len();

    let raw_features = match features_at {
        Some(start) => parse_features(&lines[start + 1..origin_at])?,
        None => Vec::new(),
    };
    let mut features = Vec::new();
    for (position, raw) in raw_features.into_iter().enumerate() {
        if let Some(feature) = convert_feature(
            position,
            raw,
            features.len(),
            length,
            topology == Topology::Circular,
            &mut warnings,
        )? {
            features.push(feature);
        }
    }
    let mut preserved = FormatExtensions {
        genbank_header: header,
        ..FormatExtensions::default()
    };
    let (name, primers) = match data {
        Some(block) => apply_data_block(block, &mut preserved, &mut warnings)?,
        None => (locus_name.to_owned(), Vec::new()),
    };
    let record = SequenceRecord::new(name, sequence, topology, features, primers)?;
    Ok(ImportReport {
        record,
        warnings,
        preserved_metadata: preserved,
    })
}

#[cfg(test)]
#[path = "genbank_record_tests.rs"]
mod tests;

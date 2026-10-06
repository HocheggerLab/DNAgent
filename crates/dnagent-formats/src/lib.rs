//! Shared format-adapter contracts.

pub mod assembly;
pub mod genbank;
pub mod genbank_record;
pub mod locus;

use dnagent_domain::{DomainError, SequenceRecord};
use serde::Serialize;
use thiserror::Error;

/// A non-fatal limitation encountered while importing a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportWarning {
    pub code: String,
    pub message: String,
    pub packet_type: Option<u8>,
}

impl ImportWarning {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            packet_type: None,
        }
    }

    #[must_use]
    pub const fn for_packet(mut self, packet_type: u8) -> Self {
        self.packet_type = Some(packet_type);
        self
    }
}

/// An opaque SnapGene packet retained for future interpretation or export reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpaquePacket {
    pub packet_type: u8,
    #[serde(skip_serializing)]
    pub payload: Vec<u8>,
    pub payload_length: usize,
}

impl OpaquePacket {
    #[must_use]
    pub fn new(packet_type: u8, payload: Vec<u8>) -> Self {
        let payload_length = payload.len();
        Self {
            packet_type,
            payload,
            payload_length,
        }
    }
}

/// What a SnapGene file carries beside the sequence and its annotations, so a record read
/// from `.dna` can be written back exactly as it arrived. The cookie's version fields, the
/// DNA flag bits beyond "circular" (methylation and friends, which DNAgent does not model)
/// and the packet order all vary between real files, so none can be regenerated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapGeneLayout {
    /// The 14-byte cookie payload, verbatim.
    pub cookie: Vec<u8>,
    /// First byte of the DNA packet. Bit 0 is topology; the rest are preserved, not modelled.
    pub dna_flags: u8,
    /// The sequence exactly as the file stored it, when that differs from the record's
    /// (DNAgent upper-cases; SnapGene files may hold lower or mixed case).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_sequence: Option<String>,
    /// Packet types in file order, including the cookie and the DNA packet.
    pub packet_order: Vec<u8>,
}

/// Adapter-owned source data retained outside the biological domain.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FormatExtensions {
    /// Present when the record was read from SnapGene `.dna`; required to write one back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapgene: Option<SnapGeneLayout>,
    /// Packets whose semantics are not yet interpreted.
    pub opaque_packets: Vec<OpaquePacket>,
    /// Raw copies of interpreted packets, preserving unsupported nested metadata.
    pub interpreted_source_packets: Vec<OpaquePacket>,
    /// GenBank header lines after LOCUS (DEFINITION … COMMENT), kept verbatim for re-export.
    pub genbank_header: Vec<String>,
    /// Isoform, evidence and expression metadata of a locus bundle (see [`locus`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locus: Option<serde_json::Value>,
}

/// Successful import plus explicit fidelity information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub record: SequenceRecord,
    pub warnings: Vec<ImportWarning>,
    pub preserved_metadata: FormatExtensions,
}

/// Typed failures shared by migration adapters.
#[derive(Debug, Error)]
pub enum ImportError {
    #[error("input is not a supported {format} file: {reason}")]
    InvalidFormat {
        format: &'static str,
        reason: String,
    },
    #[error("truncated packet at byte {offset}: expected {expected} bytes, found {available}")]
    TruncatedPacket {
        offset: usize,
        expected: usize,
        available: usize,
    },
    #[error("invalid UTF-8 in packet type 0x{packet_type:02x}: {source}")]
    InvalidUtf8 {
        packet_type: u8,
        #[source]
        source: std::str::Utf8Error,
    },
    #[error("invalid XML in packet type 0x{packet_type:02x}: {message}")]
    InvalidXml { packet_type: u8, message: String },
    #[error("domain validation failed: {0}")]
    Domain(#[from] DomainError),
    #[error("no DNA sequence packet was present")]
    MissingSequence,
    #[error("more than one DNA sequence packet was present")]
    DuplicateSequence,
}

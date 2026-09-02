//! Shared format-adapter contracts.

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

/// Adapter-owned source data retained outside the biological domain.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FormatExtensions {
    /// Packets whose semantics are not yet interpreted.
    pub opaque_packets: Vec<OpaquePacket>,
    /// Raw copies of interpreted packets, preserving unsupported nested metadata.
    pub interpreted_source_packets: Vec<OpaquePacket>,
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

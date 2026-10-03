//! Typed application use cases shared by CLI and GUI adapters.

pub mod amplification;
pub mod editing;
pub mod enzymes;
pub mod gibson;
pub mod gibson_extensions;
pub mod gibson_product;
pub mod library;
pub mod ligation;
pub mod locus;
pub mod translation;

use dnagent_domain::compatibility::{self, CompatibilityError, CompatibilityReport};
use dnagent_domain::digest::{self, Digest, DigestError};
use dnagent_domain::fragment_annotations::{self, AnnotatedDigest, AnnotationError};
use dnagent_domain::restriction::{self, RestrictionError, RestrictionScan};
use dnagent_domain::{
    DomainError, ImportedPrimer, Location, Qualifier, SequenceRecord, Strand, Topology,
};
pub use dnagent_formats::genbank::ExportStrand;
use dnagent_formats::{ImportError, ImportReport, ImportWarning};
use serde::Serialize;
use std::path::Path;
use thiserror::Error;

/// Errors at the application boundary.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "unsupported input extension for {0}; accepted sequence files are .dna, .gb, .gbk, .genbank, .fa, .fasta, .fna and DNAgent locus bundles (.json)"
    )]
    UnsupportedExtension(String),
    #[error("{path} is not a sequence file: it contains {content}")]
    NotSequenceFile { path: String, content: &'static str },
    #[error(
        "{0} has a .dna extension but no SnapGene header; it may be an older or different binary format"
    )]
    NotSnapGene(String),
    #[error(
        "{0} is not a locus document (open a `degron-db locus` bundle, or GenBank saved from one)"
    )]
    NotLocus(String),
    #[error("invalid FASTA input: {0}")]
    Fasta(&'static str),
    #[error(transparent)]
    Import(#[from] ImportError),
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Library(#[from] dnagent_library::LibraryError),
    #[error(
        "strict mode rejected {count} warning(s); inspect the warnings or retry without --strict"
    )]
    ImportWarnings { count: usize },
    #[error(transparent)]
    Restriction(#[from] RestrictionError),
    #[error(transparent)]
    Digest(#[from] DigestError),
    #[error(transparent)]
    Compatibility(#[from] CompatibilityError),
    #[error(transparent)]
    Annotation(#[from] AnnotationError),
    #[error(transparent)]
    Genbank(#[from] dnagent_formats::genbank::GenbankError),
    #[error(transparent)]
    AssemblyExport(#[from] dnagent_formats::assembly::AssemblyExportError),
    #[error(transparent)]
    Ligation(#[from] dnagent_domain::ligation::LigationError),
    #[error("invalid ligation plan JSON: {0}")]
    LigationPlan(#[from] serde_json::Error),
    #[error(transparent)]
    Gibson(#[from] dnagent_domain::gibson::GibsonError),
    #[error("invalid Gibson plan JSON: {0}")]
    GibsonPlan(serde_json::Error),
    #[error(transparent)]
    Amplification(#[from] dnagent_domain::amplification::DesignError),
    #[error("invalid amplification plan JSON: {0}")]
    AmplificationPlan(serde_json::Error),
    #[error(transparent)]
    Translation(#[from] dnagent_domain::translation::TranslationError),
    #[error("no feature with id {0:?}; list ids with `dnagent features`")]
    FeatureNotFound(String),
    #[error(transparent)]
    Edit(#[from] editing::EditError),
    #[error("failed to write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("unsupported output {0}; DNAgent saves GenBank (.gb, .gbk or .genbank)")]
    UnsupportedOutput(String),
    #[error("invalid sequence range [{start}, {end}) for length {length}")]
    InvalidRange {
        start: usize,
        end: usize,
        length: usize,
    },
}

/// Open a supported sequence file and normalize it into the domain model.
/// Single-record FASTA imports are deliberately sequence-only and linear because
/// FASTA has no standard topology or feature representation.
pub fn open_path(path: &Path) -> Result<ImportReport, AppError> {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase);
    if extension
        .as_deref()
        .and_then(SequenceFormat::from_extension)
        .is_none()
    {
        return Err(AppError::UnsupportedExtension(path.display().to_string()));
    }
    let bytes = std::fs::read(path).map_err(|source| AppError::Read {
        path: path.display().to_string(),
        source,
    })?;
    import_path_bytes(path, &bytes)
}

/// Extensions DNAgent opens as sequence files.
pub const SEQUENCE_EXTENSIONS: [&str; 7] = ["dna", "gb", "gbk", "genbank", "fa", "fasta", "fna"];

/// A sequence format recognised from file content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceFormat {
    SnapGene,
    GenBank,
    Fasta,
    /// A DNAgent locus bundle (`degron-db locus`): a gene locus with its isoforms.
    Locus,
}

impl SequenceFormat {
    fn describe(self) -> &'static str {
        match self {
            Self::SnapGene => "SnapGene",
            Self::GenBank => "GenBank",
            Self::Fasta => "FASTA",
            Self::Locus => "a DNAgent locus bundle",
        }
    }

    fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "dna" => Some(Self::SnapGene),
            "fa" | "fasta" | "fna" => Some(Self::Fasta),
            "gb" | "gbk" | "genbank" => Some(Self::GenBank),
            "json" => Some(Self::Locus),
            _ => None,
        }
    }
}

/// What the first bytes of a file say it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sniffed {
    Sequence(SequenceFormat),
    /// Recognisably not sequence data (office document, PDF, image…).
    Other(&'static str),
    Unknown,
}

/// Magic numbers of common non-sequence uploads.
const MAGIC: [(&[u8], &str); 6] = [
    (
        b"PK\x03\x04",
        "a ZIP archive or Office document (e.g. Word .docx)",
    ),
    (b"%PDF", "a PDF document"),
    (b"\x89PNG", "a PNG image"),
    (b"\xFF\xD8\xFF", "a JPEG image"),
    (
        b"\xD0\xCF\x11\xE0",
        "a legacy Office document (e.g. Word .doc)",
    ),
    (b"{\\rtf", "an RTF document"),
];

/// Recognise SnapGene, GenBank and FASTA by content, and common non-sequence uploads
/// by their magic numbers. Content that fits none of these is `Unknown`.
#[must_use]
pub fn sniff(bytes: &[u8]) -> Sniffed {
    if bytes.first() == Some(&0x09) && bytes.get(5..13) == Some(b"SnapGene".as_slice()) {
        return Sniffed::Sequence(SequenceFormat::SnapGene);
    }
    let text = bytes
        .strip_prefix(b"\xEF\xBB\xBF".as_slice())
        .unwrap_or(bytes);
    let start = text
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(text.len());
    let text = &text[start..];
    if text.starts_with(b"LOCUS") {
        return Sniffed::Sequence(SequenceFormat::GenBank);
    }
    if text.starts_with(b">") {
        return Sniffed::Sequence(SequenceFormat::Fasta);
    }
    if text.starts_with(b"{") && dnagent_formats::locus::is_locus(bytes) {
        return Sniffed::Sequence(SequenceFormat::Locus);
    }
    MAGIC
        .iter()
        .find(|(magic, _)| bytes.starts_with(magic))
        .map_or(Sniffed::Unknown, |(_, what)| Sniffed::Other(what))
}

/// Import the exact byte snapshot supplied by the caller (e.g. for hashed provenance).
///
/// The format comes from the content when it is recognisable; otherwise from the
/// extension. A file whose content and extension disagree (e.g. GenBank text saved as
/// `.dna`) is read by content, with a `content_format_mismatch` warning.
pub fn import_path_bytes(path: &Path, bytes: &[u8]) -> Result<ImportReport, AppError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    let Some(named) = extension
        .as_deref()
        .and_then(SequenceFormat::from_extension)
    else {
        return Err(AppError::UnsupportedExtension(path.display().to_string()));
    };
    let fallback_name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("untitled");
    let format = match sniff(bytes) {
        Sniffed::Sequence(format) => format,
        Sniffed::Other(content) => {
            return Err(AppError::NotSequenceFile {
                path: path.display().to_string(),
                content,
            });
        }
        // Every SnapGene file starts with its cookie packet (type 9).
        Sniffed::Unknown if named == SequenceFormat::SnapGene && bytes.first() != Some(&0x09) => {
            return Err(AppError::NotSnapGene(path.display().to_string()));
        }
        Sniffed::Unknown => named,
    };
    let mut report = match format {
        SequenceFormat::SnapGene => dnagent_format_snapgene::import_bytes(bytes, fallback_name)?,
        SequenceFormat::Fasta => import_single_fasta(bytes, fallback_name)?,
        SequenceFormat::GenBank => dnagent_formats::genbank_record::read(bytes, fallback_name)?,
        SequenceFormat::Locus => dnagent_formats::locus::read(bytes, fallback_name)?,
    };
    if format != named {
        report.warnings.insert(
            0,
            ImportWarning::new(
                "content_format_mismatch",
                format!(
                    "file has a .{} extension but contains {}; it was read as {}",
                    extension.unwrap_or_default(),
                    format.describe(),
                    format.describe()
                ),
            ),
        );
    }
    Ok(report)
}

fn import_single_fasta(bytes: &[u8], fallback_name: &str) -> Result<ImportReport, AppError> {
    let text = std::str::from_utf8(bytes).map_err(|_| AppError::Fasta("file is not UTF-8"))?;
    let mut name = None;
    let mut sequence = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix('>') {
            if name.is_some() {
                return Err(AppError::Fasta("exactly one FASTA record is required"));
            }
            name = Some(
                header
                    .split_whitespace()
                    .next()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(fallback_name)
                    .to_owned(),
            );
        } else {
            if name.is_none() {
                return Err(AppError::Fasta("sequence appeared before the FASTA header"));
            }
            if line.bytes().any(|base| base.is_ascii_whitespace()) {
                return Err(AppError::Fasta(
                    "whitespace inside a sequence line is not supported",
                ));
            }
            sequence.push_str(line);
        }
    }
    let name = name.ok_or(AppError::Fasta("missing FASTA header"))?;
    if sequence.is_empty() {
        return Err(AppError::Fasta("FASTA sequence is empty"));
    }
    Ok(ImportReport {
        record: SequenceRecord::new(
            name,
            dnagent_domain::DnaSeq::new(sequence)?,
            Topology::Linear,
            vec![],
            vec![],
        )?,
        warnings: vec![ImportWarning::new(
            "fasta_sequence_only_linear",
            "FASTA carries no standard topology, features or primers; imported as a sequence-only linear record",
        )],
        preserved_metadata: dnagent_formats::FormatExtensions::default(),
    })
}

/// Reject any reported import limitation, including preserved uninterpreted metadata.
/// No claim of complete format support is made for warning-free imports.
pub fn require_warning_free_import(report: &ImportReport) -> Result<(), AppError> {
    if report.warnings.is_empty() {
        Ok(())
    } else {
        Err(AppError::ImportWarnings {
            count: report.warnings.len(),
        })
    }
}

/// Compact record projection used by terminal and GUI headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InspectView {
    pub name: String,
    pub length: usize,
    pub topology: Topology,
    pub feature_count: usize,
    pub primer_count: usize,
    pub warnings: Vec<ImportWarning>,
}

impl InspectView {
    /// Project an imported report without exposing opaque payload bytes.
    #[must_use]
    pub fn from_report(report: &ImportReport) -> Self {
        let record = &report.record;
        Self {
            name: record.name().to_owned(),
            length: record.sequence().len(),
            topology: record.topology(),
            feature_count: record.features().len(),
            primer_count: record.primers().len(),
            warnings: report.warnings.clone(),
        }
    }
}

/// Stable feature-list row shared by adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureView {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub strand: Strand,
    pub location: Location,
    pub color: Option<String>,
    pub qualifiers: Vec<Qualifier>,
}

/// Build deterministic feature-list rows in source order.
#[must_use]
pub fn feature_views(record: &SequenceRecord) -> Vec<FeatureView> {
    record
        .features()
        .iter()
        .map(|feature| FeatureView {
            id: feature.id().as_str().to_owned(),
            label: feature.label().to_owned(),
            kind: feature.kind().to_owned(),
            strand: feature.location().strand(),
            location: feature.location().clone(),
            color: feature.display().color.clone(),
            qualifiers: feature.qualifiers().to_vec(),
        })
        .collect()
}

/// Retained primer sequences in source order, not inferred binding sites or new designs.
#[must_use]
pub fn primer_views(record: &SequenceRecord) -> &[ImportedPrimer] {
    record.primers()
}

/// A restriction result plus new analysis warnings (import warnings remain on the input report).
pub struct RestrictionView {
    pub result: RestrictionScan,
    pub warnings: Vec<ImportWarning>,
}

pub fn restriction_sites(
    record: &SequenceRecord,
    names: &[String],
) -> Result<RestrictionView, AppError> {
    let result = restriction::find_sites(record.sequence(), record.topology(), names)?;
    let unavailable = result
        .sites
        .iter()
        .filter(|s| !s.cleavage_available)
        .count();
    let warnings = if unavailable == 0 {
        vec![]
    } else {
        vec![ImportWarning::new(
            "restriction_cut_out_of_bounds",
            format!(
                "{unavailable} recognition site(s) have cuts outside the linear molecule; no cleavage is inferred for those sites"
            ),
        )]
    };
    Ok(RestrictionView { result, warnings })
}

/// Complete sequence-only digest; fragment annotations are not propagated yet.
pub fn simulate_digest(record: &SequenceRecord, names: &[String]) -> Result<Digest, AppError> {
    Ok(digest::simulate_digest(
        record.sequence(),
        record.topology(),
        names,
    )?)
}

#[derive(Debug, Serialize)]
pub struct CompatibilityInput {
    pub name: String,
    pub digest: Digest,
}

#[derive(Debug, Serialize)]
pub struct CompatibilityView {
    pub inputs: Vec<CompatibilityInput>,
    pub analysis: CompatibilityReport,
}

/// Digest one or two records, then compare all distinct physical ends.
pub fn end_compatibility(
    inputs: &[(&SequenceRecord, &[String])],
) -> Result<CompatibilityView, AppError> {
    if !(1..=2).contains(&inputs.len()) {
        return Err(CompatibilityError::InputCount.into());
    }
    let digests = inputs
        .iter()
        .map(|(record, names)| simulate_digest(record, names))
        .collect::<Result<Vec<_>, _>>()?;
    let analysis = compatibility::compatible_ends(&digests)?;
    let inputs = inputs
        .iter()
        .zip(digests)
        .map(|((record, _), digest)| CompatibilityInput {
            name: record.name().to_owned(),
            digest,
        })
        .collect();
    Ok(CompatibilityView { inputs, analysis })
}

/// Feature projections on each product strand; original metadata is retained as provenance.
pub fn annotated_fragments(
    record: &SequenceRecord,
    names: &[String],
) -> Result<AnnotatedDigest, AppError> {
    Ok(fragment_annotations::annotated_digest(record, names)?)
}

/// Conservative GenBank selected-strand views; full provenance remains in JSON.
pub fn fragment_genbank(
    report: &AnnotatedDigest,
    strand: ExportStrand,
) -> Result<String, AppError> {
    Ok(dnagent_formats::genbank::export(report, strand)?)
}

/// Explicit strand FASTA, not an annotated or duplex-preserving exchange format.
#[must_use]
pub fn fragment_fasta(report: &AnnotatedDigest) -> String {
    use std::fmt::Write;
    let mut text = String::new();
    for fragment in &report.digest.fragments {
        for (label, strand) in [("top", &fragment.top), ("bottom", &fragment.bottom)] {
            writeln!(
                text,
                ">{}|{label} length={} source_start={} strand_sequence=5to3",
                fragment.id, strand.length, strand.source_start
            )
            .expect("writing to String cannot fail");
            for (i, base) in strand.sequence_5to3.chars().enumerate() {
                if i > 0 && i % 80 == 0 {
                    text.push('\n');
                }
                text.push(base);
            }
            text.push('\n');
        }
    }
    text
}

/// Sequence-only FASTA for a materialised exact-overlap assembly.
pub fn assembly_fasta(
    assembly: &dnagent_domain::existing_overlaps::ExistingAssembly,
    name: &str,
) -> Result<String, AppError> {
    Ok(dnagent_formats::assembly::fasta(assembly, name)?)
}

/// Conservative GenBank product view with component provenance features.
pub fn assembly_genbank(
    assembly: &dnagent_domain::existing_overlaps::ExistingAssembly,
    name: &str,
) -> Result<String, AppError> {
    Ok(dnagent_formats::assembly::genbank(assembly, name)?)
}

/// Current UTC time as RFC 3339, e.g. `2026-09-29T18:04:05Z`.
#[must_use]
pub fn utc_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, seconds) = (now / 86_400, now % 86_400);
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

/// Today's date in GenBank LOCUS form (UTC), e.g. `29-SEP-2026`.
#[must_use]
pub fn genbank_date_today() -> String {
    const MONTHS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400);
    // Civil-from-days (Howard Hinnant), proleptic Gregorian.
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{day:02}-{}-{year}",
        MONTHS[usize::try_from(month - 1).unwrap_or(0)]
    )
}

/// Serialise a report as DNAgent GenBank. Returned warnings describe limitations of
/// this write; they are never silent.
#[must_use]
pub fn genbank_text(report: &ImportReport, date: &str) -> (String, Vec<ImportWarning>) {
    dnagent_formats::genbank_record::write(
        report,
        &dnagent_formats::genbank_record::WriteOptions {
            date: date.to_owned(),
        },
    )
}

/// Reject output paths that are not GenBank.
pub fn require_genbank_path(path: &Path) -> Result<(), AppError> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if matches!(extension.as_deref(), Some("gb" | "gbk" | "genbank")) {
        Ok(())
    } else {
        Err(AppError::UnsupportedOutput(path.display().to_string()))
    }
}

/// Unix socket where a running DNAgent desktop app serves agents (MCP), shared by the app
/// and `dnagent mcp`: `$DNAGENT_AGENT_SOCKET`, else `~/.dnagent/agent.sock`.
#[must_use]
pub fn agent_socket_path() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("DNAGENT_AGENT_SOCKET").filter(|p| !p.is_empty()) {
        return path.into();
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| ".".into());
    Path::new(&home).join(".dnagent").join("agent.sock")
}

/// Write text atomically: a sibling temporary file, then rename over the target.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), AppError> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".dnagent-tmp");
    let temporary = std::path::PathBuf::from(temporary);
    std::fs::write(&temporary, text).map_err(|source| AppError::Write {
        path: temporary.display().to_string(),
        source,
    })?;
    std::fs::rename(&temporary, path).map_err(|source| AppError::Write {
        path: path.display().to_string(),
        source,
    })
}

/// Serialise and save GenBank; returns the write warnings (limitations of this save).
pub fn save_genbank(
    report: &ImportReport,
    path: &Path,
    date: &str,
) -> Result<Vec<ImportWarning>, AppError> {
    require_genbank_path(path)?;
    let (text, warnings) = genbank_text(report, date);
    write_atomic(path, &text)?;
    Ok(warnings)
}

/// Checked sequence-range projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SequenceRangeView {
    pub start: usize,
    pub end: usize,
    pub sequence: String,
}

/// Extract a zero-based, half-open sequence range.
pub fn sequence_range(
    record: &SequenceRecord,
    start: usize,
    end: usize,
) -> Result<SequenceRangeView, AppError> {
    let sequence = record
        .sequence()
        .slice(start, end)
        .ok_or(AppError::InvalidRange {
            start,
            end,
            length: record.sequence().len(),
        })?;
    Ok(SequenceRangeView {
        start,
        end,
        sequence: sequence.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dnagent_domain::DnaSeq;

    const GENBANK: &[u8] = b"LOCUS       misnamed                   8 bp    DNA     linear   SYN 01-JAN-2026\nFEATURES             Location/Qualifiers\nORIGIN\n        1 acgtacgt\n//\n";

    #[test]
    fn content_decides_the_format_and_a_mismatch_is_reported() {
        let report = import_path_bytes(Path::new("plasmid.dna"), GENBANK).unwrap();
        assert_eq!(report.record.sequence().as_str(), "ACGTACGT");
        assert_eq!(report.warnings[0].code, "content_format_mismatch");
        let named = import_path_bytes(Path::new("plasmid.gb"), GENBANK).unwrap();
        assert!(
            named
                .warnings
                .iter()
                .all(|w| w.code != "content_format_mismatch")
        );
        let fasta = import_path_bytes(Path::new("seq.gb"), b">seq\nACGT\n").unwrap();
        assert_eq!(fasta.warnings[0].code, "content_format_mismatch");
    }

    #[test]
    fn uploads_that_are_not_sequences_are_named() {
        for (bytes, what) in [
            (&b"PK\x03\x04rest"[..], "Office"),
            (b"%PDF-1.3", "PDF"),
            (b"\x89PNG\r\n", "PNG"),
        ] {
            let error = import_path_bytes(Path::new("upload.dna"), bytes).unwrap_err();
            assert!(matches!(error, AppError::NotSequenceFile { .. }), "{error}");
            assert!(error.to_string().contains(what), "{error}");
        }
        let old = import_path_bytes(Path::new("old.dna"), b"\x00\x01\x00\x01binary").unwrap_err();
        assert!(matches!(old, AppError::NotSnapGene(_)), "{old}");
    }

    #[test]
    fn warning_free_policy_rejects_even_preserved_metadata() {
        let mut report = ImportReport {
            record: SequenceRecord::new(
                "test",
                DnaSeq::new("ACGT").unwrap(),
                Topology::Linear,
                vec![],
                vec![],
            )
            .unwrap(),
            warnings: vec![],
            preserved_metadata: dnagent_formats::FormatExtensions::default(),
        };
        assert!(require_warning_free_import(&report).is_ok());
        report.warnings.push(ImportWarning::new(
            "snapgene_packet_not_interpreted",
            "preserved opaque packet",
        ));
        assert!(matches!(
            require_warning_free_import(&report),
            Err(AppError::ImportWarnings { count: 1 })
        ));
    }

    #[test]
    fn sequence_ranges_are_half_open() {
        let record = SequenceRecord::new(
            "test",
            DnaSeq::new("ACGT").unwrap(),
            Topology::Linear,
            vec![],
            vec![],
        )
        .unwrap();
        let view = sequence_range(&record, 1, 3).unwrap();
        assert_eq!(view.sequence, "CG");
        assert!(sequence_range(&record, 2, 5).is_err());
    }

    #[test]
    fn single_record_fasta_is_canonical_linear_and_warned() {
        let report =
            import_single_fasta(b">example description\nacgt\nTGCA\n", "fallback").unwrap();
        assert_eq!(report.record.name(), "example");
        assert_eq!(report.record.sequence().as_str(), "ACGTTGCA");
        assert_eq!(report.record.topology(), Topology::Linear);
        assert_eq!(report.warnings[0].code, "fasta_sequence_only_linear");
        assert!(import_single_fasta(b">one\nACGT\n>two\nTGCA\n", "fallback").is_err());
        assert!(import_single_fasta(b"ACGT\n", "fallback").is_err());
    }
}

//! Restriction sites and digests for the desktop, from the shared engine and the active
//! enzyme catalogue. The frontend only chooses enzyme names and displays results.
use crate::{Diagnostic, Direction};
use dnagent_domain::restriction::{self, OverhangPolarity};
use dnagent_domain::{SequenceRecord, Strand};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Serialize, TS)]
pub struct EnzymeInfo {
    pub name: String,
    /// IUPAC recognition sequence (may be degenerate or interrupted).
    pub site: String,
    pub top_cut_offset: i32,
    pub bottom_cut_offset: i32,
    /// `blunt`, `five_prime` or `three_prime`.
    pub overhang: String,
    pub overhang_length: u32,
}

#[derive(Debug, Serialize, TS)]
pub struct EnzymeCatalogueInfo {
    pub source: String,
    pub version: String,
    pub enzymes: Vec<EnzymeInfo>,
    /// Enzymes the source lists but DNAgent cannot model yet (e.g. two-sided cutters).
    pub unsupported: u32,
}

/// Sites of one enzyme in a document: all recognition sites, and those whose cuts fall
/// inside the molecule.
#[derive(Debug, Serialize, TS)]
pub struct EnzymeCount {
    pub name: String,
    pub sites: u32,
    pub cuts: u32,
}

#[derive(Debug, Serialize, TS)]
pub struct Site {
    pub enzyme: String,
    /// Recognition start and length (may wrap on circular molecules).
    pub start: u32,
    pub length: u32,
    pub strand: Direction,
    /// Top/bottom strand cut boundaries; null when outside a linear molecule.
    pub top_cut: Option<u32>,
    pub bottom_cut: Option<u32>,
    pub cleavage_available: bool,
}

#[derive(Debug, Serialize, TS)]
pub struct FragmentEndInfo {
    pub enzymes: Vec<String>,
    /// `blunt`, `five_prime` or `three_prime`.
    pub overhang: String,
    pub overhang_sequence: String,
    /// The molecule's original end (linear input), not a cut.
    pub original_terminus: bool,
}

#[derive(Debug, Serialize, TS)]
pub struct Fragment {
    pub id: String,
    /// Top-strand start and length (may wrap on circular molecules).
    pub start: u32,
    pub length: u32,
    pub left: Option<FragmentEndInfo>,
    pub right: Option<FragmentEndInfo>,
}

fn u32_of(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn polarity(value: OverhangPolarity) -> String {
    match value {
        OverhangPolarity::Blunt => "blunt",
        OverhangPolarity::FivePrime => "five_prime",
        OverhangPolarity::ThreePrime => "three_prime",
    }
    .into()
}

#[must_use]
pub fn catalogue_info() -> EnzymeCatalogueInfo {
    let catalogue = restriction::active_catalogue();
    EnzymeCatalogueInfo {
        source: catalogue.source.name.clone(),
        version: catalogue.source.version.clone(),
        enzymes: catalogue
            .enzymes
            .iter()
            .map(|e| {
                let delta = e.bottom_cut_offset - e.top_cut_offset;
                EnzymeInfo {
                    name: e.name.into(),
                    site: e.recognition_sequence.into(),
                    top_cut_offset: e.top_cut_offset,
                    bottom_cut_offset: e.bottom_cut_offset,
                    overhang: polarity(match delta.cmp(&0) {
                        std::cmp::Ordering::Equal => OverhangPolarity::Blunt,
                        std::cmp::Ordering::Greater => OverhangPolarity::FivePrime,
                        std::cmp::Ordering::Less => OverhangPolarity::ThreePrime,
                    }),
                    overhang_length: delta.unsigned_abs(),
                }
            })
            .collect(),
        unsupported: u32_of(catalogue.unsupported.len()),
    }
}

/// Site counts for every catalogue enzyme (enzymes that cannot be scanned count zero).
#[must_use]
pub fn counts(record: &SequenceRecord) -> Vec<EnzymeCount> {
    restriction::active_catalogue()
        .enzymes
        .iter()
        .map(|enzyme| {
            let scan = restriction::find_sites(
                record.sequence(),
                record.topology(),
                &[enzyme.name.to_owned()],
            );
            let (sites, cuts) = scan.map_or((0, 0), |s| {
                (
                    s.sites.len(),
                    s.sites.iter().filter(|x| x.cleavage_available).count(),
                )
            });
            EnzymeCount {
                name: enzyme.name.into(),
                sites: u32_of(sites),
                cuts: u32_of(cuts),
            }
        })
        .collect()
}

pub fn sites(record: &SequenceRecord, names: &[String]) -> Result<Vec<Site>, Diagnostic> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let scan =
        restriction::find_sites(record.sequence(), record.topology(), names).map_err(|e| {
            Diagnostic {
                code: "restriction_scan_failed".into(),
                message: e.to_string(),
            }
        })?;
    Ok(scan
        .sites
        .iter()
        .map(|s| Site {
            enzyme: s.enzyme.into(),
            start: u32_of(s.recognition.start().get()),
            length: u32_of(s.recognition.length().get()),
            strand: if s.strand == Strand::Reverse {
                Direction::Reverse
            } else {
                Direction::Forward
            },
            top_cut: s.top_cut.map(u32_of),
            bottom_cut: s.bottom_cut.map(u32_of),
            cleavage_available: s.cleavage_available,
        })
        .collect())
}

pub fn digest(record: &SequenceRecord, names: &[String]) -> Result<Vec<Fragment>, Diagnostic> {
    let digest = dnagent_app::simulate_digest(record, names).map_err(|e| Diagnostic {
        code: "digest_failed".into(),
        message: e.to_string(),
    })?;
    let end = |end: &dnagent_domain::digest::FragmentEnd| FragmentEndInfo {
        enzymes: end.enzymes.iter().map(|e| (*e).to_owned()).collect(),
        overhang: polarity(end.polarity),
        overhang_sequence: end.overhang_sequence.clone(),
        original_terminus: end.original_terminus,
    };
    Ok(digest
        .fragments
        .iter()
        .map(|f| Fragment {
            id: f.id.clone(),
            start: u32_of(f.top.source_start),
            length: u32_of(f.top.length),
            left: f.left_end.as_ref().map(end),
            right: f.right_end.as_ref().map(end),
        })
        .collect())
}

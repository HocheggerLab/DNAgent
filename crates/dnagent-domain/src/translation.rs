//! Translation of DNA to protein and open-reading-frame discovery.
//!
//! Genetic codes are the pinned NCBI tables in [`crate::genetic_codes`]. Translation is
//! IUPAC-aware: a codon translates to an amino acid only when every expansion of its
//! ambiguity codes agrees (otherwise `X`); a stop requires every expansion to be a stop;
//! starts must be exact. Coordinates are zero-based reference positions on the forward
//! strand. Multipart features are spliced in source order; reverse-strand features are
//! the reverse complement of that splice (GenBank `complement(join(...))`). Nothing here
//! infers genes, splicing, frames beyond the declared inputs, or experimental validity.

use crate::genetic_codes::{GeneticCodeData, TABLES};
use crate::{DnaSeq, Feature, Strand, Topology};
use serde::Serialize;
use thiserror::Error;

pub use crate::genetic_codes::{GC_PRT_SHA256, GC_PRT_VERSION};

/// Errors raised by translation requests.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TranslationError {
    #[error("unknown NCBI genetic code {0}; valid tables are listed by `dnagent translate --help`")]
    UnknownTable(u32),
    #[error("feature {0:?} has unknown strand; translation needs a declared strand")]
    UnknownStrand(String),
    #[error("codon_start must be 1, 2 or 3, got {0:?}")]
    InvalidCodonStart(String),
    #[error("transl_table qualifier {0:?} is not an NCBI table number")]
    InvalidTableQualifier(String),
    #[error("frame offset must be 0, 1 or 2, got {0}")]
    InvalidFrame(usize),
    #[error("invalid range {start}..{end} for a {topology:?} molecule of length {length}")]
    InvalidRange {
        start: usize,
        end: usize,
        length: usize,
        topology: Topology,
    },
    #[error("nothing to translate: fewer than three coding bases")]
    TooShort,
    #[error("minimum ORF length must be at least 1 codon")]
    InvalidMinimum,
}

/// Which codons may open an ORF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartPolicy {
    /// Only ATG.
    AtgOnly,
    /// Every start codon of the selected table (e.g. GTG/TTG in table 11).
    TableStarts,
}

/// Coding direction relative to the forward reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodingStrand {
    Forward,
    Reverse,
}

/// A pinned NCBI translation table.
#[derive(Debug, Clone, Copy)]
pub struct GeneticCode(&'static GeneticCodeData);

fn base_index(base: u8) -> usize {
    match base {
        b'T' => 0,
        b'C' => 1,
        b'A' => 2,
        b'G' => 3,
        _ => unreachable!("expansions are ACGT"),
    }
}

fn expansions(base: u8) -> &'static [u8] {
    match base {
        b'A' => b"A",
        b'C' => b"C",
        b'G' => b"G",
        b'T' => b"T",
        b'R' => b"AG",
        b'Y' => b"CT",
        b'S' => b"CG",
        b'W' => b"AT",
        b'K' => b"GT",
        b'M' => b"AC",
        b'B' => b"CGT",
        b'D' => b"AGT",
        b'H' => b"ACT",
        b'V' => b"ACG",
        b'N' => b"ACGT",
        _ => unreachable!("DnaSeq validates IUPAC DNA"),
    }
}

fn complement(base: u8) -> u8 {
    match base {
        b'A' => b'T',
        b'T' => b'A',
        b'C' => b'G',
        b'G' => b'C',
        b'R' => b'Y',
        b'Y' => b'R',
        b'S' => b'S',
        b'W' => b'W',
        b'K' => b'M',
        b'M' => b'K',
        b'B' => b'V',
        b'V' => b'B',
        b'D' => b'H',
        b'H' => b'D',
        b'N' => b'N',
        _ => unreachable!("DnaSeq validates IUPAC DNA"),
    }
}

impl GeneticCode {
    /// Look up an NCBI table by number.
    pub fn ncbi(id: u32) -> Result<Self, TranslationError> {
        TABLES
            .iter()
            .find(|table| u32::from(table.id) == id)
            .map(Self)
            .ok_or(TranslationError::UnknownTable(id))
    }

    /// NCBI table 1.
    #[must_use]
    pub fn standard() -> Self {
        Self(&TABLES[0])
    }

    #[must_use]
    pub fn id(self) -> u8 {
        self.0.id
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        self.0.name
    }

    /// Every available table as `(id, name)`.
    pub fn all() -> impl Iterator<Item = (u8, &'static str)> {
        TABLES.iter().map(|table| (table.id, table.name))
    }

    fn each_expansion(codon: [u8; 3], mut visit: impl FnMut(usize)) {
        for &a in expansions(codon[0]) {
            for &b in expansions(codon[1]) {
                for &c in expansions(codon[2]) {
                    visit(16 * base_index(a) + 4 * base_index(b) + base_index(c));
                }
            }
        }
    }

    /// Amino acid for an IUPAC codon: agreed by every expansion, else `X`. Stops are `*`.
    #[must_use]
    pub fn translate_codon(self, codon: [u8; 3]) -> char {
        let mut agreed: Option<u8> = None;
        let mut conflict = false;
        Self::each_expansion(codon, |index| {
            let amino = self.0.amino_acids[index];
            match agreed {
                None => agreed = Some(amino),
                Some(previous) if previous != amino => conflict = true,
                Some(_) => {}
            }
        });
        if conflict {
            'X'
        } else {
            char::from(agreed.expect("at least one expansion"))
        }
    }

    /// True when every expansion is a stop codon.
    #[must_use]
    pub fn is_stop(self, codon: [u8; 3]) -> bool {
        self.translate_codon(codon) == '*'
    }

    /// True for an exact (unambiguous) start codon under `policy`.
    #[must_use]
    pub fn is_start(self, codon: [u8; 3], policy: StartPolicy) -> bool {
        if !codon.iter().all(|base| b"ACGT".contains(base)) {
            return false;
        }
        match policy {
            StartPolicy::AtgOnly => &codon == b"ATG",
            StartPolicy::TableStarts => {
                let index =
                    16 * base_index(codon[0]) + 4 * base_index(codon[1]) + base_index(codon[2]);
                self.0.starts[index] == b'M'
            }
        }
    }
}

/// One translated codon with the reference positions of its three coding bases
/// (in coding order, so they decrease on the reverse strand and may jump at joins).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Codon {
    pub amino_acid: char,
    /// Codon on the coding strand, 5′→3′.
    pub codon: String,
    pub positions: [usize; 3],
}

/// Translation of an explicit coding sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Translation {
    pub table: u8,
    pub table_name: &'static str,
    pub strand: CodingStrand,
    /// 1-based offset into the coding sequence of the first translated base.
    pub codon_start: u8,
    /// One letter per codon; stops are `*` wherever they occur.
    pub protein: String,
    pub codons: Vec<Codon>,
    /// Codon indices of stops before the final codon.
    pub internal_stops: Vec<usize>,
    pub terminal_stop: bool,
    /// Coding bases left over after the last complete codon (0–2).
    pub trailing_bases: usize,
    /// Codons translated as `X` because their ambiguity codes disagree.
    pub ambiguous_codons: usize,
    pub starts_with_atg: bool,
    /// A complete feature CDS (codon_start 1, table start codon, terminal stop) whose
    /// first codon is an alternative start is translated with an initiator M
    /// (NCBI/Biopython `cds=True` convention). Never applied to ranges.
    pub initiator_as_methionine: bool,
}

type Coding = Vec<(u8, usize)>;

fn translate_coding(
    coding: &[(u8, usize)],
    code: GeneticCode,
    strand: CodingStrand,
    codon_start: u8,
) -> Result<Translation, TranslationError> {
    let offset = usize::from(codon_start - 1);
    let body = coding.get(offset..).unwrap_or_default();
    if body.len() < 3 {
        return Err(TranslationError::TooShort);
    }
    let codons: Vec<Codon> = body
        .as_chunks::<3>()
        .0
        .iter()
        .map(|chunk| {
            let bases = [chunk[0].0, chunk[1].0, chunk[2].0];
            Codon {
                amino_acid: code.translate_codon(bases),
                codon: String::from_utf8(bases.to_vec()).expect("ASCII bases"),
                positions: [chunk[0].1, chunk[1].1, chunk[2].1],
            }
        })
        .collect();
    let protein: String = codons.iter().map(|codon| codon.amino_acid).collect();
    let last = codons.len() - 1;
    Ok(Translation {
        table: code.id(),
        table_name: code.name(),
        strand,
        codon_start,
        internal_stops: codons
            .iter()
            .enumerate()
            .filter(|(i, c)| *i < last && c.amino_acid == '*')
            .map(|(i, _)| i)
            .collect(),
        terminal_stop: codons[last].amino_acid == '*',
        trailing_bases: body.len() % 3,
        ambiguous_codons: codons
            .iter()
            .filter(|codon| codon.amino_acid == 'X')
            .count(),
        starts_with_atg: codons[0].codon == "ATG",
        initiator_as_methionine: false,
        protein,
        codons,
    })
}

fn reverse_complement_coding(mut coding: Coding) -> Coding {
    coding.reverse();
    for (base, _) in &mut coding {
        *base = complement(*base);
    }
    coding
}

/// Bases of `[start, start + length)` with reference positions, wrapping on circular molecules.
fn forward_bases(sequence: &DnaSeq, start: usize, length: usize) -> Coding {
    let bytes = sequence.as_str().as_bytes();
    let total = bytes.len();
    (0..length)
        .map(|offset| {
            let position = (start + offset) % total;
            (bytes[position], position)
        })
        .collect()
}

/// Parsed `codon_start` (default 1) and `transl_table` (default: none) qualifiers.
pub fn feature_translation_qualifiers(
    feature: &Feature,
) -> Result<(u8, Option<u32>), TranslationError> {
    let mut codon_start = 1;
    let mut table = None;
    for qualifier in feature.qualifiers() {
        let value = qualifier.value.as_deref().unwrap_or("").trim();
        match qualifier.key.as_str() {
            "codon_start" => {
                codon_start = match value {
                    "1" => 1,
                    "2" => 2,
                    "3" => 3,
                    _ => return Err(TranslationError::InvalidCodonStart(value.to_owned())),
                }
            }
            "transl_table" => {
                table = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| TranslationError::InvalidTableQualifier(value.to_owned()))?,
                );
            }
            _ => {}
        }
    }
    Ok((codon_start, table))
}

/// Translate a feature's spliced coding sequence. `table` overrides `transl_table`.
pub fn translate_feature(
    sequence: &DnaSeq,
    feature: &Feature,
    table: Option<u32>,
) -> Result<Translation, TranslationError> {
    let (codon_start, qualifier_table) = feature_translation_qualifiers(feature)?;
    let code = GeneticCode::ncbi(table.or(qualifier_table).unwrap_or(1))?;
    let mut coding: Coding = feature
        .location()
        .parts()
        .iter()
        .flat_map(|part| forward_bases(sequence, part.start().get(), part.length().get()))
        .collect();
    let strand = match feature.location().strand() {
        Strand::Forward => CodingStrand::Forward,
        Strand::Reverse => {
            coding = reverse_complement_coding(coding);
            CodingStrand::Reverse
        }
        Strand::Unknown => {
            return Err(TranslationError::UnknownStrand(
                feature.id().as_str().to_owned(),
            ));
        }
    };
    let mut translation = translate_coding(&coding, code, strand, codon_start)?;
    let first = &translation.codons[0];
    let bases: [u8; 3] = first.codon.as_bytes().try_into().expect("three bases");
    if codon_start == 1
        && translation.terminal_stop
        && first.amino_acid != 'M'
        && code.is_start(bases, StartPolicy::TableStarts)
    {
        translation.codons[0].amino_acid = 'M';
        translation.protein.replace_range(0..1, "M");
        translation.initiator_as_methionine = true;
    }
    Ok(translation)
}

/// Translate the forward-coordinate range `[start, end)` (on circular molecules `end <
/// start` wraps through the origin) on either strand, skipping `frame` (0–2) bases
/// from the 5′ end of the coding strand.
pub fn translate_range(
    sequence: &DnaSeq,
    topology: Topology,
    start: usize,
    end: usize,
    strand: CodingStrand,
    frame: usize,
    table: u32,
) -> Result<Translation, TranslationError> {
    let length = sequence.len();
    let invalid = || TranslationError::InvalidRange {
        start,
        end,
        length,
        topology,
    };
    if frame > 2 {
        return Err(TranslationError::InvalidFrame(frame));
    }
    if start >= length || end > length || end == start {
        return Err(invalid());
    }
    let span = if end > start {
        end - start
    } else if topology == Topology::Circular {
        length - start + end
    } else {
        return Err(invalid());
    };
    let code = GeneticCode::ncbi(table)?;
    let mut coding = forward_bases(sequence, start, span);
    if strand == CodingStrand::Reverse {
        coding = reverse_complement_coding(coding);
    }
    translate_coding(
        &coding,
        code,
        strand,
        u8::try_from(frame + 1).expect("frame checked"),
    )
}

/// Imported `translation` qualifier with SnapGene's per-part commas and whitespace removed.
#[must_use]
pub fn imported_translation(feature: &Feature) -> Option<String> {
    feature
        .qualifiers()
        .iter()
        .find(|qualifier| qualifier.key == "translation")
        .and_then(|qualifier| qualifier.value.as_deref())
        .map(|value| {
            value
                .chars()
                .filter(|c| !c.is_whitespace() && *c != ',')
                .collect()
        })
}

/// Comparison between the computed protein and an imported translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportedComparison {
    pub imported_length: usize,
    pub matches: bool,
    /// First differing residue index (0-based), if any.
    pub first_difference: Option<usize>,
}

/// Compare with an imported translation, ignoring one terminal stop on either side.
#[must_use]
pub fn compare_imported(translation: &Translation, imported: &str) -> ImportedComparison {
    let computed = translation
        .protein
        .strip_suffix('*')
        .unwrap_or(&translation.protein);
    let imported = imported.strip_suffix('*').unwrap_or(imported);
    let first_difference = computed
        .chars()
        .zip(imported.chars())
        .position(|(a, b)| a != b)
        .or_else(|| (computed.len() != imported.len()).then(|| computed.len().min(imported.len())));
    ImportedComparison {
        imported_length: imported.chars().count(),
        matches: first_difference.is_none(),
        first_difference,
    }
}

/// One complete open reading frame (start codon through stop codon).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Orf {
    pub id: String,
    pub strand: CodingStrand,
    /// Lowest forward-reference position covered; with `length` it may wrap the origin.
    pub start: usize,
    /// Bases including the stop codon.
    pub length: usize,
    /// Offset of the 5′ codon base from the 5′ end of its strand, modulo 3
    /// (`start % 3` forward), matching [`six_frames`] offsets.
    pub frame: usize,
    /// Amino acids, excluding the stop.
    pub codons: usize,
    pub protein: String,
    pub start_codon: String,
    pub stop_codon: String,
    pub wraps_origin: bool,
}

/// Explicit ORF search parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OrfOptions {
    pub table: u8,
    pub min_codons: usize,
    pub starts: StartPolicy,
}

/// ORF search result with the effective parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrfScan {
    pub options: OrfOptions,
    pub table_name: &'static str,
    pub topology: Topology,
    pub orfs: Vec<Orf>,
}

/// Six-frame search for complete ORFs: the longest ORF per stop codon (first start after
/// the previous in-frame stop), at least `min_codons` amino acids. On circular molecules
/// ORFs may wrap the origin but never exceed one molecule length; frames without any
/// stop are not reported.
pub fn find_orfs(
    sequence: &DnaSeq,
    topology: Topology,
    table: u32,
    min_codons: usize,
    starts: StartPolicy,
) -> Result<OrfScan, TranslationError> {
    if min_codons == 0 {
        return Err(TranslationError::InvalidMinimum);
    }
    let code = GeneticCode::ncbi(table)?;
    let length = sequence.len();
    let circular = topology == Topology::Circular;
    let forward: Coding = forward_bases(sequence, 0, length);
    let reverse = reverse_complement_coding(forward.clone());
    let mut found: Vec<Orf> = Vec::new();
    for (strand, coding) in [
        (CodingStrand::Forward, forward),
        (CodingStrand::Reverse, reverse),
    ] {
        // Circular molecules are scanned over two turns so wrapping ORFs are complete.
        let scan: Coding = if circular {
            coding.iter().chain(coding.iter()).copied().collect()
        } else {
            coding
        };
        let mut best_per_stop: std::collections::BTreeMap<usize, Orf> =
            std::collections::BTreeMap::new();
        for frame in 0..3 {
            let mut open: Option<usize> = None;
            let mut index = frame;
            while index + 3 <= scan.len() {
                let codon = [scan[index].0, scan[index + 1].0, scan[index + 2].0];
                if code.is_stop(codon) {
                    if let Some(begin) = open.take() {
                        let bases = index + 3 - begin;
                        let amino_acids = bases / 3 - 1;
                        if begin < length && bases <= length && amino_acids >= min_codons {
                            let window = &scan[begin..index + 3];
                            let protein: String = window
                                .as_chunks::<3>()
                                .0
                                .iter()
                                .take(amino_acids)
                                .map(|c| code.translate_codon([c[0].0, c[1].0, c[2].0]))
                                .collect();
                            let positions: Vec<usize> = window.iter().map(|(_, p)| *p).collect();
                            let start = match strand {
                                CodingStrand::Forward => positions[0],
                                CodingStrand::Reverse => *positions.last().expect("non-empty"),
                            };
                            let orf = Orf {
                                id: String::new(),
                                strand,
                                start,
                                length: bases,
                                // Same offsets as `six_frames`: from the 5′ end of each strand.
                                frame: match strand {
                                    CodingStrand::Forward => start % 3,
                                    CodingStrand::Reverse => {
                                        (length - 1 - (start + bases - 1) % length) % 3
                                    }
                                },
                                codons: amino_acids,
                                protein,
                                start_codon: window[..3]
                                    .iter()
                                    .map(|(b, _)| char::from(*b))
                                    .collect(),
                                stop_codon: window[bases - 3..]
                                    .iter()
                                    .map(|(b, _)| char::from(*b))
                                    .collect(),
                                wraps_origin: start + bases > length,
                            };
                            // Key by the stop's first reference base: one (longest) ORF per stop.
                            let stop_key = window[bases - 3].1;
                            let keep = best_per_stop
                                .get(&stop_key)
                                .is_none_or(|existing| existing.length < orf.length);
                            if keep {
                                best_per_stop.insert(stop_key, orf);
                            }
                        }
                    }
                } else if open.is_none() && code.is_start(codon, starts) {
                    open = Some(index);
                }
                index += 3;
            }
        }
        found.extend(best_per_stop.into_values());
    }
    found.sort_by_key(|orf| (orf.start, orf.strand == CodingStrand::Reverse, orf.length));
    for (index, orf) in found.iter_mut().enumerate() {
        orf.id = format!("orf-{:04}", index + 1);
    }
    Ok(OrfScan {
        options: OrfOptions {
            table: code.id(),
            min_codons,
            starts,
        },
        table_name: code.name(),
        topology,
        orfs: found,
    })
}

/// One reading frame of the whole molecule for display: codon `i` covers forward
/// positions `first + 3i .. first + 3i + 3` (forward) or `first - 3i - 2 ..= first - 3i`
/// (reverse; `first` is the 5′ base on the reverse strand). Codons that would cross the
/// origin are omitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameTranslation {
    pub strand: CodingStrand,
    pub offset: usize,
    pub first: usize,
    pub protein: String,
}

/// Six-frame translation for display, frames +1..+3 then -1..-3.
pub fn six_frames(
    sequence: &DnaSeq,
    table: u32,
) -> Result<Vec<FrameTranslation>, TranslationError> {
    let code = GeneticCode::ncbi(table)?;
    let bytes = sequence.as_str().as_bytes();
    let length = bytes.len();
    let mut frames = Vec::with_capacity(6);
    for offset in 0..3 {
        let protein = bytes
            .get(offset..)
            .unwrap_or_default()
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| code.translate_codon(*c))
            .collect();
        frames.push(FrameTranslation {
            strand: CodingStrand::Forward,
            offset,
            first: offset,
            protein,
        });
    }
    for offset in 0..3 {
        let top = length.saturating_sub(offset);
        let protein = bytes[..top]
            .rchunks_exact(3)
            .map(|c| code.translate_codon([complement(c[2]), complement(c[1]), complement(c[0])]))
            .collect();
        frames.push(FrameTranslation {
            strand: CodingStrand::Reverse,
            offset,
            first: top.saturating_sub(1),
            protein,
        });
    }
    Ok(frames)
}

#[cfg(test)]
#[path = "translation_tests.rs"]
mod tests;

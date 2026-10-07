//! Small, explicit restriction-enzyme catalogue and sequence-only cleavage geometry.
//! Constants verified against Biopython 1.85 Bio.Restriction; see docs/restriction.md.

use crate::{DnaSeq, DomainError, Region, Strand, Topology};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Enzyme {
    pub name: &'static str,
    pub recognition_sequence: &'static str,
    /// Boundary offsets from recognition start on the recognition-oriented strand.
    pub top_cut_offset: i32,
    pub bottom_cut_offset: i32,
}

/// Built-in catalogue: widely used commercial Type II enzymes, hand-listed from
/// published recognition sequences and cut positions (scientific facts) and verified
/// against Biopython 1.85 `Bio.Restriction` by `scripts/check_enzymes.py`. REBASE itself
/// is not redistributed (see docs/restriction.md); it can be installed locally.
pub const BUILTIN: &[Enzyme] = &[
    Enzyme {
        name: "AatII",
        recognition_sequence: "GACGTC",
        top_cut_offset: 5,
        bottom_cut_offset: 1,
    },
    Enzyme {
        name: "Acc65I",
        recognition_sequence: "GGTACC",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "AccI",
        recognition_sequence: "GTMKAC",
        top_cut_offset: 2,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "AflII",
        recognition_sequence: "CTTAAG",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "AgeI",
        recognition_sequence: "ACCGGT",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "AluI",
        recognition_sequence: "AGCT",
        top_cut_offset: 2,
        bottom_cut_offset: 2,
    },
    Enzyme {
        name: "AlwNI",
        recognition_sequence: "CAGNNNCTG",
        top_cut_offset: 6,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "ApaI",
        recognition_sequence: "GGGCCC",
        top_cut_offset: 5,
        bottom_cut_offset: 1,
    },
    Enzyme {
        name: "ApaLI",
        recognition_sequence: "GTGCAC",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "AscI",
        recognition_sequence: "GGCGCGCC",
        top_cut_offset: 2,
        bottom_cut_offset: 6,
    },
    Enzyme {
        name: "AseI",
        recognition_sequence: "ATTAAT",
        top_cut_offset: 2,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "AsiSI",
        recognition_sequence: "GCGATCGC",
        top_cut_offset: 5,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "AvaI",
        recognition_sequence: "CYCGRG",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "AvrII",
        recognition_sequence: "CCTAGG",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BamHI",
        recognition_sequence: "GGATCC",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BbsI",
        recognition_sequence: "GAAGAC",
        top_cut_offset: 8,
        bottom_cut_offset: 12,
    },
    Enzyme {
        name: "BclI",
        recognition_sequence: "TGATCA",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BfaI",
        recognition_sequence: "CTAG",
        top_cut_offset: 1,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "BglI",
        recognition_sequence: "GCCNNNNNGGC",
        top_cut_offset: 7,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "BglII",
        recognition_sequence: "AGATCT",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BmtI",
        recognition_sequence: "GCTAGC",
        top_cut_offset: 5,
        bottom_cut_offset: 1,
    },
    Enzyme {
        name: "BsaAI",
        recognition_sequence: "YACGTR",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "BsaI",
        recognition_sequence: "GGTCTC",
        top_cut_offset: 7,
        bottom_cut_offset: 11,
    },
    Enzyme {
        name: "BsaWI",
        recognition_sequence: "WCCGGW",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BseRI",
        recognition_sequence: "GAGGAG",
        top_cut_offset: 16,
        bottom_cut_offset: 14,
    },
    Enzyme {
        name: "BsiWI",
        recognition_sequence: "CGTACG",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BsmBI",
        recognition_sequence: "CGTCTC",
        top_cut_offset: 7,
        bottom_cut_offset: 11,
    },
    Enzyme {
        name: "BsmI",
        recognition_sequence: "GAATGC",
        top_cut_offset: 7,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BspEI",
        recognition_sequence: "TCCGGA",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BspHI",
        recognition_sequence: "TCATGA",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BsrGI",
        recognition_sequence: "TGTACA",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BssHII",
        recognition_sequence: "GCGCGC",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "BstBI",
        recognition_sequence: "TTCGAA",
        top_cut_offset: 2,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "BstEII",
        recognition_sequence: "GGTNACC",
        top_cut_offset: 1,
        bottom_cut_offset: 6,
    },
    Enzyme {
        name: "BstXI",
        recognition_sequence: "CCANNNNNNTGG",
        top_cut_offset: 8,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "BtgZI",
        recognition_sequence: "GCGATG",
        top_cut_offset: 16,
        bottom_cut_offset: 20,
    },
    Enzyme {
        name: "ClaI",
        recognition_sequence: "ATCGAT",
        top_cut_offset: 2,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "DdeI",
        recognition_sequence: "CTNAG",
        top_cut_offset: 1,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "DraI",
        recognition_sequence: "TTTAAA",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "DraIII",
        recognition_sequence: "CACNNNGTG",
        top_cut_offset: 6,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "EagI",
        recognition_sequence: "CGGCCG",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "EcoNI",
        recognition_sequence: "CCTNNNNNAGG",
        top_cut_offset: 5,
        bottom_cut_offset: 6,
    },
    Enzyme {
        name: "EcoRI",
        recognition_sequence: "GAATTC",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "EcoRV",
        recognition_sequence: "GATATC",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "Esp3I",
        recognition_sequence: "CGTCTC",
        top_cut_offset: 7,
        bottom_cut_offset: 11,
    },
    Enzyme {
        name: "FseI",
        recognition_sequence: "GGCCGGCC",
        top_cut_offset: 6,
        bottom_cut_offset: 2,
    },
    Enzyme {
        name: "FspI",
        recognition_sequence: "TGCGCA",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "HaeIII",
        recognition_sequence: "GGCC",
        top_cut_offset: 2,
        bottom_cut_offset: 2,
    },
    Enzyme {
        name: "HhaI",
        recognition_sequence: "GCGC",
        top_cut_offset: 3,
        bottom_cut_offset: 1,
    },
    Enzyme {
        name: "HincII",
        recognition_sequence: "GTYRAC",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "HindIII",
        recognition_sequence: "AAGCTT",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "HinfI",
        recognition_sequence: "GANTC",
        top_cut_offset: 1,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "HpaI",
        recognition_sequence: "GTTAAC",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "KasI",
        recognition_sequence: "GGCGCC",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "KpnI",
        recognition_sequence: "GGTACC",
        top_cut_offset: 5,
        bottom_cut_offset: 1,
    },
    Enzyme {
        name: "MfeI",
        recognition_sequence: "CAATTG",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "MluI",
        recognition_sequence: "ACGCGT",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "MseI",
        recognition_sequence: "TTAA",
        top_cut_offset: 1,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "MspI",
        recognition_sequence: "CCGG",
        top_cut_offset: 1,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "NarI",
        recognition_sequence: "GGCGCC",
        top_cut_offset: 2,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "NcoI",
        recognition_sequence: "CCATGG",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "NdeI",
        recognition_sequence: "CATATG",
        top_cut_offset: 2,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "NheI",
        recognition_sequence: "GCTAGC",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "NlaIII",
        recognition_sequence: "CATG",
        top_cut_offset: 4,
        bottom_cut_offset: 0,
    },
    Enzyme {
        name: "NotI",
        recognition_sequence: "GCGGCCGC",
        top_cut_offset: 2,
        bottom_cut_offset: 6,
    },
    Enzyme {
        name: "NruI",
        recognition_sequence: "TCGCGA",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "NsiI",
        recognition_sequence: "ATGCAT",
        top_cut_offset: 5,
        bottom_cut_offset: 1,
    },
    Enzyme {
        name: "PacI",
        recognition_sequence: "TTAATTAA",
        top_cut_offset: 5,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "PaqCI",
        recognition_sequence: "CACCTGC",
        top_cut_offset: 11,
        bottom_cut_offset: 15,
    },
    Enzyme {
        name: "PciI",
        recognition_sequence: "ACATGT",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "PmeI",
        recognition_sequence: "GTTTAAAC",
        top_cut_offset: 4,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "PmlI",
        recognition_sequence: "CACGTG",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "PsiI",
        recognition_sequence: "TTATAA",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "PspOMI",
        recognition_sequence: "GGGCCC",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "PstI",
        recognition_sequence: "CTGCAG",
        top_cut_offset: 5,
        bottom_cut_offset: 1,
    },
    Enzyme {
        name: "PvuI",
        recognition_sequence: "CGATCG",
        top_cut_offset: 4,
        bottom_cut_offset: 2,
    },
    Enzyme {
        name: "PvuII",
        recognition_sequence: "CAGCTG",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "RsaI",
        recognition_sequence: "GTAC",
        top_cut_offset: 2,
        bottom_cut_offset: 2,
    },
    Enzyme {
        name: "SacI",
        recognition_sequence: "GAGCTC",
        top_cut_offset: 5,
        bottom_cut_offset: 1,
    },
    Enzyme {
        name: "SacII",
        recognition_sequence: "CCGCGG",
        top_cut_offset: 4,
        bottom_cut_offset: 2,
    },
    Enzyme {
        name: "SalI",
        recognition_sequence: "GTCGAC",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "SapI",
        recognition_sequence: "GCTCTTC",
        top_cut_offset: 8,
        bottom_cut_offset: 11,
    },
    Enzyme {
        name: "Sau3AI",
        recognition_sequence: "GATC",
        top_cut_offset: 0,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "SbfI",
        recognition_sequence: "CCTGCAGG",
        top_cut_offset: 6,
        bottom_cut_offset: 2,
    },
    Enzyme {
        name: "ScaI",
        recognition_sequence: "AGTACT",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "SfiI",
        recognition_sequence: "GGCCNNNNNGGCC",
        top_cut_offset: 8,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "SgrAI",
        recognition_sequence: "CRCCGGYG",
        top_cut_offset: 2,
        bottom_cut_offset: 6,
    },
    Enzyme {
        name: "SmaI",
        recognition_sequence: "CCCGGG",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "SnaBI",
        recognition_sequence: "TACGTA",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "SpeI",
        recognition_sequence: "ACTAGT",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "SphI",
        recognition_sequence: "GCATGC",
        top_cut_offset: 5,
        bottom_cut_offset: 1,
    },
    Enzyme {
        name: "SspI",
        recognition_sequence: "AATATT",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "StuI",
        recognition_sequence: "AGGCCT",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "SwaI",
        recognition_sequence: "ATTTAAAT",
        top_cut_offset: 4,
        bottom_cut_offset: 4,
    },
    Enzyme {
        name: "TaqI",
        recognition_sequence: "TCGA",
        top_cut_offset: 1,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "XbaI",
        recognition_sequence: "TCTAGA",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "XhoI",
        recognition_sequence: "CTCGAG",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "XmaI",
        recognition_sequence: "CCCGGG",
        top_cut_offset: 1,
        bottom_cut_offset: 5,
    },
    Enzyme {
        name: "ZraI",
        recognition_sequence: "GACGTC",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
];

/// Where the active catalogue came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogueSource {
    pub name: String,
    pub version: String,
    /// SHA-256 of the source files, when loaded from an installed database.
    pub sha256: Option<String>,
}

/// An enzyme a source database lists but DNAgent cannot model yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnsupportedEnzyme {
    pub name: String,
    pub reason: String,
}

/// The enzymes available to scans and digests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnzymeCatalogue {
    pub source: CatalogueSource,
    pub enzymes: Vec<Enzyme>,
    pub unsupported: Vec<UnsupportedEnzyme>,
}

static ACTIVE: std::sync::OnceLock<EnzymeCatalogue> = std::sync::OnceLock::new();

impl EnzymeCatalogue {
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            source: CatalogueSource {
                name: "DNAgent built-in".into(),
                version: "1".into(),
                sha256: None,
            },
            enzymes: BUILTIN.to_vec(),
            unsupported: Vec::new(),
        }
    }

    /// Parse REBASE EMBOSS files (`emboss_e.NNN`, `emboss_r.NNN`). Only commercially
    /// available enzymes (at least one supplier) are included; two-sided cutters, unknown
    /// cut positions and invalid patterns are listed as unsupported. Names and patterns
    /// are leaked once so enzymes keep the `'static` API (load a catalogue once per process).
    pub fn from_emboss(
        e_text: &str,
        r_text: &str,
        version: &str,
        sha256: Option<String>,
    ) -> Result<Self, RestrictionError> {
        let suppliers = emboss_suppliers(r_text);
        let mut enzymes = Vec::new();
        let mut unsupported = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for line in e_text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [name, pattern, length, ncuts, _blunt, c1, c2, c3, c4] = fields.as_slice() else {
                return Err(RestrictionError::Catalogue(format!(
                    "malformed EMBOSS line {line:?}"
                )));
            };
            if !seen.insert((*name).to_owned()) || suppliers.get(*name).is_none_or(String::is_empty)
            {
                continue;
            }
            let number = |text: &str| {
                text.parse::<i32>()
                    .map_err(|_| RestrictionError::Catalogue(format!("bad number in {line:?}")))
            };
            let (length, ncuts, c1, c2, c3, c4) = (
                number(length)?,
                number(ncuts)?,
                number(c1)?,
                number(c2)?,
                number(c3)?,
                number(c4)?,
            );
            let pattern = pattern.to_ascii_uppercase();
            let reason = if !pattern.bytes().all(|b| b"ACGTRYSWKMBDHVN".contains(&b))
                || usize::try_from(length).ok() != Some(pattern.len())
            {
                Some("recognition sequence is not plain IUPAC DNA".to_owned())
            } else if ncuts == 0 {
                Some("cut positions unknown".to_owned())
            } else if ncuts > 2 || c3 != 0 || c4 != 0 {
                Some("cuts on both sides of the site (not modelled yet)".to_owned())
            } else {
                None
            };
            if let Some(reason) = reason {
                unsupported.push(UnsupportedEnzyme {
                    name: (*name).to_owned(),
                    reason,
                });
                continue;
            }
            // EMBOSS numbers residues ...,-2,-1,1,2,... and cuts after the given residue.
            let boundary = |c: i32| if c < 0 { c + 1 } else { c };
            enzymes.push(Enzyme {
                name: Box::leak((*name).to_owned().into_boxed_str()),
                recognition_sequence: Box::leak(pattern.into_boxed_str()),
                top_cut_offset: boundary(c1),
                bottom_cut_offset: boundary(c2),
            });
        }
        if enzymes.is_empty() {
            return Err(RestrictionError::Catalogue(
                "no commercially available enzymes found".into(),
            ));
        }
        enzymes.sort_by(|a, b| a.name.cmp(b.name));
        Ok(Self {
            source: CatalogueSource {
                name: "REBASE".into(),
                version: version.to_owned(),
                sha256,
            },
            enzymes,
            unsupported,
        })
    }

    /// Case-insensitive lookup.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Enzyme> {
        self.enzymes
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }
}

/// Supplier codes per enzyme from `emboss_r` (line 6 of each entry).
fn emboss_suppliers(r_text: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let body: Vec<&str> = r_text.lines().skip_while(|l| l.starts_with('#')).collect();
    for entry in body.split(|l| l.trim() == "//") {
        let entry: Vec<&str> = entry
            .iter()
            .copied()
            .skip_while(|l| l.trim().is_empty())
            .collect();
        if entry.len() >= 6 {
            map.insert(entry[0].trim().to_owned(), entry[5].trim().to_owned());
        }
    }
    map
}

/// Install the process-wide catalogue (once, before the first scan). Returns false if a
/// catalogue was already active.
pub fn install_catalogue(catalogue: EnzymeCatalogue) -> bool {
    ACTIVE.set(catalogue).is_ok()
}

/// The active catalogue: an installed one, otherwise the built-in set.
pub fn active_catalogue() -> &'static EnzymeCatalogue {
    ACTIVE.get_or_init(EnzymeCatalogue::builtin)
}

/// IUPAC code → bases it stands for.
fn iupac(base: u8) -> &'static [u8] {
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
        _ => b"",
    }
}

fn iupac_complement(base: u8) -> u8 {
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
        _ => b'N',
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RestrictionError {
    #[error("select at least one restriction enzyme")]
    EmptySelection,
    #[error("unsupported restriction enzyme {0:?}; list supported enzymes with `enzymes`")]
    UnknownEnzyme(String),
    #[error(
        "restriction scanning requires unambiguous A/C/G/T; found {symbol} at zero-based position {position}"
    )]
    AmbiguousSequence { symbol: char, position: usize },
    #[error("invalid recognition coordinates: {0}")]
    Coordinates(#[from] DomainError),
    #[error("enzyme catalogue: {0}")]
    Catalogue(String),
    #[error(
        "circular molecule length {length} is too short for {enzyme} recognition/cleavage span {required}"
    )]
    ShortCircle {
        enzyme: &'static str,
        length: usize,
        required: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverhangPolarity {
    Blunt,
    FivePrime,
    ThreePrime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestrictionSite {
    pub enzyme: &'static str,
    pub recognition: Region,
    /// Relative to the stored sequence; palindromic sites use canonical Forward.
    pub strand: Strand,
    /// Boundaries along the stored forward sequence, even for reverse sites.
    /// Null denotes a cut outside a linear molecule. Circular boundaries are modulo length.
    pub top_cut: Option<usize>,
    pub bottom_cut: Option<usize>,
    pub cleavage_available: bool,
    /// Nominal enzyme geometry, not proof of cleavage or a fragment-end sequence.
    pub overhang_polarity: OverhangPolarity,
    pub overhang_length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestrictionScan {
    pub length: usize,
    pub topology: Topology,
    pub enzymes: Vec<Enzyme>,
    pub sites: Vec<RestrictionSite>,
    pub assumptions: Vec<&'static str>,
}

/// Find exact sites and nominal cuts. Does not model methylation, reaction conditions,
/// star activity, accessibility, minimum flanking DNA or digest fragments.
/// Ambiguous input is rejected rather than reported as a misleading negative scan.
pub fn find_sites(
    sequence: &DnaSeq,
    topology: Topology,
    names: &[String],
) -> Result<RestrictionScan, RestrictionError> {
    if names.is_empty() {
        return Err(RestrictionError::EmptySelection);
    }
    let catalogue = active_catalogue();
    for name in names {
        if catalogue.get(name).is_none() {
            return Err(RestrictionError::UnknownEnzyme(name.clone()));
        }
    }
    let enzymes: Vec<_> = catalogue
        .enzymes
        .iter()
        .copied()
        .filter(|e| names.iter().any(|n| e.name.eq_ignore_ascii_case(n)))
        .collect();
    let bases = sequence.as_str().as_bytes();
    let n = bases.len();
    for (position, base) in bases.iter().enumerate() {
        if !b"ACGT".contains(base) {
            return Err(RestrictionError::AmbiguousSequence {
                symbol: char::from(*base),
                position,
            });
        }
    }
    let mut sites = Vec::new();
    for enzyme in &enzymes {
        let motif = enzyme.recognition_sequence.as_bytes();
        let m = motif.len();
        // Recognition plus any cuts outside it, on either side.
        let right = i64::from(enzyme.top_cut_offset.max(enzyme.bottom_cut_offset))
            .max(i64::try_from(m).unwrap_or(i64::MAX));
        let left = i64::from(enzyme.top_cut_offset.min(enzyme.bottom_cut_offset)).min(0);
        let span = usize::try_from(right - left).unwrap_or(usize::MAX);
        if topology == Topology::Circular && n < span {
            return Err(RestrictionError::ShortCircle {
                enzyme: enzyme.name,
                length: n,
                required: span,
            });
        }
        if n < m {
            continue;
        }
        let reverse: Vec<u8> = motif.iter().rev().map(|b| iupac_complement(*b)).collect();
        let starts = if topology == Topology::Circular {
            n
        } else {
            n - m + 1
        };
        for start in 0..starts {
            for (pattern, strand) in [
                (motif, Strand::Forward),
                (reverse.as_slice(), Strand::Reverse),
            ] {
                if strand == Strand::Reverse && reverse == motif {
                    continue;
                }
                if !pattern
                    .iter()
                    .enumerate()
                    .all(|(i, b)| iupac(*b).contains(&bases[(start + i) % n]))
                {
                    continue;
                }
                sites.push(site_geometry(enzyme, start, strand, n, topology)?);
            }
        }
    }
    sites.sort_by_key(|s| {
        (
            s.recognition.start().get(),
            s.enzyme,
            s.strand == Strand::Reverse,
        )
    });
    Ok(RestrictionScan {
        length: n,
        topology,
        enzymes,
        sites,
        assumptions: vec![
            "ACGT input only (ambiguous input rejected); recognition sequences may be degenerate or interrupted (IUPAC)",
            "Nominal cleavage geometry only; methylation, star activity, reaction conditions and flanking-DNA requirements not modelled",
            "Palindromic sites reported once in forward orientation",
            "No digest fragments or fragment-end sequences are inferred",
        ],
    })
}

fn site_geometry(
    enzyme: &Enzyme,
    start: usize,
    strand: Strand,
    n: usize,
    topology: Topology,
) -> Result<RestrictionSite, DomainError> {
    let m = enzyme.recognition_sequence.len();
    // find_sites bounds start and motif length before calling us. Still propagate
    // checked-region failures rather than panic if those invariants ever change.
    let recognition = if topology == Topology::Linear || start + m <= n {
        Region::linear(start, start + m, n)?
    } else {
        Region::circular_arc(start, m, n)?
    };
    let (top_offset, bottom_offset) = if strand == Strand::Forward {
        (
            i128::from(enzyme.top_cut_offset),
            i128::from(enzyme.bottom_cut_offset),
        )
    } else {
        (
            m as i128 - i128::from(enzyme.bottom_cut_offset),
            m as i128 - i128::from(enzyme.top_cut_offset),
        )
    };
    let boundary = |offset: i128| {
        let position = start as i128 + offset;
        if topology == Topology::Circular {
            Some(position.rem_euclid(n as i128) as usize)
        } else {
            usize::try_from(position).ok().filter(|p| *p <= n)
        }
    };
    let top_cut = boundary(top_offset);
    let bottom_cut = boundary(bottom_offset);
    let delta = enzyme.bottom_cut_offset - enzyme.top_cut_offset;
    Ok(RestrictionSite {
        enzyme: enzyme.name,
        recognition,
        strand,
        top_cut,
        bottom_cut,
        cleavage_available: top_cut.is_some() && bottom_cut.is_some(),
        overhang_polarity: match delta.cmp(&0) {
            std::cmp::Ordering::Equal => OverhangPolarity::Blunt,
            std::cmp::Ordering::Greater => OverhangPolarity::FivePrime,
            std::cmp::Ordering::Less => OverhangPolarity::ThreePrime,
        },
        overhang_length: delta.unsigned_abs() as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_site_geometry_returns_checked_coordinate_error() {
        for topology in [Topology::Linear, Topology::Circular] {
            assert!(site_geometry(&BUILTIN[42], 0, Strand::Forward, 3, topology).is_err());
            assert!(site_geometry(&BUILTIN[42], 0, Strand::Forward, 0, topology).is_err());
        }
    }
    fn scan(sequence: &str, topology: Topology, enzyme: &str) -> RestrictionScan {
        find_sites(
            &DnaSeq::new(sequence).unwrap(),
            topology,
            &[enzyme.to_owned()],
        )
        .unwrap()
    }
    #[test]
    fn catalogue_geometries_and_palindrome_deduplication() {
        for (name, motif, top, bottom, polarity) in [
            ("EcoRI", "GAATTC", 1, 5, OverhangPolarity::FivePrime),
            ("BamHI", "GGATCC", 1, 5, OverhangPolarity::FivePrime),
            ("EcoRV", "GATATC", 3, 3, OverhangPolarity::Blunt),
            ("KpnI", "GGTACC", 5, 1, OverhangPolarity::ThreePrime),
            ("BsaI", "GGTCTCAAAAAA", 7, 11, OverhangPolarity::FivePrime),
            ("BsmBI", "CGTCTCAAAAAA", 7, 11, OverhangPolarity::FivePrime),
        ] {
            let result = scan(motif, Topology::Linear, name);
            assert_eq!(result.sites.len(), 1);
            let site = &result.sites[0];
            assert_eq!((site.top_cut, site.bottom_cut), (Some(top), Some(bottom)));
            assert_eq!(site.overhang_polarity, polarity);
        }
    }
    #[test]
    fn reverse_type_iis_cuts_upstream_and_swaps_strands() {
        let result = scan("AAAAAGAGACCAAAAA", Topology::Linear, "BsaI");
        let site = &result.sites[0];
        assert_eq!(site.strand, Strand::Reverse);
        assert_eq!((site.top_cut, site.bottom_cut), (Some(0), Some(4)));
        assert_eq!(site.overhang_polarity, OverhangPolarity::FivePrime);
    }
    #[test]
    fn circular_origin_and_cut_wrapping() {
        let result = scan("TTCGAA", Topology::Circular, "EcoRI");
        assert_eq!(result.sites.len(), 1);
        let site = &result.sites[0];
        assert!(site.recognition.is_circular_arc());
        assert_eq!((site.top_cut, site.bottom_cut), (Some(4), Some(2)));
        let result = scan("GAGACCAAAAAA", Topology::Circular, "BsaI");
        assert_eq!(
            (result.sites[0].top_cut, result.sites[0].bottom_cut),
            (Some(7), Some(11))
        );
    }
    #[test]
    fn linear_end_sites_are_not_silently_dropped() {
        let result = scan("GGTCTC", Topology::Linear, "BsaI");
        assert_eq!(result.sites.len(), 1);
        assert!(!result.sites[0].cleavage_available);
        assert_eq!(
            (result.sites[0].top_cut, result.sites[0].bottom_cut),
            (None, None)
        );
        assert_eq!(scan("TTCGAA", Topology::Linear, "EcoRI").sites, []);
        assert_eq!(scan("ACG", Topology::Linear, "EcoRI").sites, []);
    }
    #[test]
    fn explicit_errors_and_deterministic_selection() {
        let seq = DnaSeq::new("GAATTC").unwrap();
        assert!(matches!(
            find_sites(&seq, Topology::Linear, &[]),
            Err(RestrictionError::EmptySelection)
        ));
        assert!(matches!(
            find_sites(&seq, Topology::Linear, &["unknown".into()]),
            Err(RestrictionError::UnknownEnzyme(_))
        ));
        assert!(matches!(
            find_sites(
                &DnaSeq::new("GAANTC").unwrap(),
                Topology::Linear,
                &["EcoRI".into()]
            ),
            Err(RestrictionError::AmbiguousSequence { .. })
        ));
        assert!(matches!(
            find_sites(&seq, Topology::Circular, &["BsaI".into()]),
            Err(RestrictionError::ShortCircle { .. })
        ));
        let result = find_sites(&seq, Topology::Linear, &["ecori".into(), "EcoRI".into()]).unwrap();
        assert_eq!(result.enzymes.len(), 1);
        assert_eq!(result.sites.len(), 1);
    }

    #[test]
    fn degenerate_and_interrupted_sites_match_iupac() {
        // BsaWI WCCGGW is a degenerate palindrome: reported once per site.
        let result = scan("AACCGGTT", Topology::Linear, "BsaWI");
        assert_eq!(result.sites.len(), 1);
        assert_eq!(result.sites[0].top_cut, Some(2));
        // BglI GCCNNNNNGGC (cut GCCNNNN^NGGC) with arbitrary interruption.
        let result = scan("TTGCCATGCAGGCTT", Topology::Linear, "BglI");
        assert_eq!(result.sites.len(), 1);
        assert_eq!(
            (result.sites[0].top_cut, result.sites[0].bottom_cut),
            (Some(9), Some(6))
        );
        assert_eq!(
            result.sites[0].overhang_polarity,
            OverhangPolarity::ThreePrime
        );
        assert_eq!(scan("TTGCCATGCAGGGTT", Topology::Linear, "BglI").sites, []);
        // HinfI GANTC, non-palindromic only through N: reported once.
        assert_eq!(scan("AAGATTCAA", Topology::Linear, "HinfI").sites.len(), 1);
    }

    #[test]
    fn non_palindromic_degenerate_sites_are_found_on_both_strands() {
        // BsmI GAATGC(1/-1): forward at 0, reverse (GCATTC) at 10.
        let result = scan("GAATGCAAAAGCATTCAAAA", Topology::Linear, "BsmI");
        let strands: Vec<Strand> = result.sites.iter().map(|s| s.strand).collect();
        assert_eq!(strands, vec![Strand::Forward, Strand::Reverse]);
        assert_eq!(result.sites[0].top_cut, Some(7));
        // Reverse site: top cut = start + len - bottom offset = 10 + 6 - 5.
        assert_eq!(result.sites[1].top_cut, Some(11));
    }

    #[test]
    fn rebase_emboss_parsing_converts_cut_numbering_and_filters() {
        let e = "# comment\nEcoRI\tGAATTC\t6\t2\t0\t1\t5\t0\t0\nTspRI\tCASTG\t5\t2\t0\t7\t-3\t0\t0\n\
                 AloI\tGAACNNNNNNTCC\t13\t4\t0\t-8\t-13\t20\t15\nNoSupply\tGGGG\t4\t2\t0\t1\t3\t0\t0\nUnknownCut\tACGT\t4\t0\t0\t0\t0\t0\t0\n";
        let r = "# header\nEcoRI\nEscherichia coli\n\n\n\nBCFN\n0\n//\nTspRI\nThermus\n\n\n\nN\n0\n//\nAloI\nAcinetobacter\n\n\n\nB\n0\n//\nNoSupply\nX\n\n\n\n\n0\n//\nUnknownCut\nY\n\n\n\nN\n0\n//\n";
        let catalogue = EnzymeCatalogue::from_emboss(e, r, "999", None).unwrap();
        let names: Vec<&str> = catalogue.enzymes.iter().map(|x| x.name).collect();
        assert_eq!(names, vec!["EcoRI", "TspRI"]);
        let tspri = catalogue.get("tspri").unwrap();
        // "Cut after residue -3" (no residue 0) is the boundary two bases upstream.
        assert_eq!((tspri.top_cut_offset, tspri.bottom_cut_offset), (7, -2));
        let reasons: Vec<(&str, &str)> = catalogue
            .unsupported
            .iter()
            .map(|u| (u.name.as_str(), u.reason.as_str()))
            .collect();
        assert_eq!(
            reasons,
            vec![
                ("AloI", "cuts on both sides of the site (not modelled yet)"),
                ("UnknownCut", "cut positions unknown")
            ]
        );
        assert!(EnzymeCatalogue::from_emboss("Bad line", r, "1", None).is_err());
    }
}

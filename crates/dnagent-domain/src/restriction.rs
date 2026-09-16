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

pub const ENZYMES: [Enzyme; 6] = [
    Enzyme {
        name: "EcoRI",
        recognition_sequence: "GAATTC",
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
        name: "EcoRV",
        recognition_sequence: "GATATC",
        top_cut_offset: 3,
        bottom_cut_offset: 3,
    },
    Enzyme {
        name: "KpnI",
        recognition_sequence: "GGTACC",
        top_cut_offset: 5,
        bottom_cut_offset: 1,
    },
    Enzyme {
        name: "BsaI",
        recognition_sequence: "GGTCTC",
        top_cut_offset: 7,
        bottom_cut_offset: 11,
    },
    Enzyme {
        name: "BsmBI",
        recognition_sequence: "CGTCTC",
        top_cut_offset: 7,
        bottom_cut_offset: 11,
    },
];

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
    for name in names {
        if !ENZYMES.iter().any(|e| e.name.eq_ignore_ascii_case(name)) {
            return Err(RestrictionError::UnknownEnzyme(name.clone()));
        }
    }
    let enzymes: Vec<_> = ENZYMES
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
        let span = m
            .max(enzyme.top_cut_offset as usize)
            .max(enzyme.bottom_cut_offset as usize);
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
        let reverse: Vec<_> = motif
            .iter()
            .rev()
            .map(|base| match base {
                b'A' => b'T',
                b'C' => b'G',
                b'G' => b'C',
                b'T' => b'A',
                _ => unreachable!("catalogue uses ACGT only"),
            })
            .collect();
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
                    .all(|(i, b)| bases[(start + i) % n] == *b)
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
            "Exact ACGT sequence matching only; ambiguous input rejected",
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
            assert!(site_geometry(&ENZYMES[0], 0, Strand::Forward, 3, topology).is_err());
            assert!(site_geometry(&ENZYMES[0], 0, Strand::Forward, 0, topology).is_err());
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
        assert!(scan("TTCGAA", Topology::Linear, "EcoRI").sites.is_empty());
        assert!(scan("ACG", Topology::Linear, "EcoRI").sites.is_empty());
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
}

//! Conservative complete restriction digests with explicit strand and end geometry.

use crate::restriction::{self, Enzyme, OverhangPolarity, RestrictionError};
use crate::{DnaSeq, Strand, Topology};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DigestError {
    #[error(transparent)]
    Restriction(#[from] RestrictionError),
    #[error(
        "complete digest refused: {enzyme} site at {start} has a cut outside the linear molecule; no partial products returned (use sites to inspect uncleavable sites)"
    )]
    OutOfBounds { enzyme: &'static str, start: usize },
    #[error(
        "complete digest refused: {enzyme} cuts at a linear terminus; terminal single-stranded products are not modelled and no partial products are returned"
    )]
    TerminalCut { enzyme: &'static str },
    #[error(
        "cannot digest overlapping/touching cleavage regions; products without a paired core are not modelled"
    )]
    OverlappingCuts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DigestCut {
    pub top_cut: usize,
    pub bottom_cut: usize,
    /// Enzymes producing the same physical cut pair are grouped, not cut twice.
    pub enzymes: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FragmentEnd {
    /// True only for an original linear end; input ends are assumed blunt.
    pub original_terminus: bool,
    pub polarity: OverhangPolarity,
    pub protruding_strand: Option<Strand>,
    /// Exposed oligo written 5′ to 3′ on its own strand; empty for blunt ends.
    pub overhang_sequence: String,
    pub enzymes: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FragmentStrand {
    /// Start of the source interval along the input FORWARD coordinate axis.
    /// For the bottom strand this is not the oligo's 5′ boundary.
    pub source_start: usize,
    pub length: usize,
    pub sequence_5to3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DigestFragment {
    pub id: String,
    pub topology: Topology,
    pub top: FragmentStrand,
    pub bottom: FragmentStrand,
    pub paired_length: usize,
    /// Null only for an uncut circular molecule.
    pub left_end: Option<FragmentEnd>,
    pub right_end: Option<FragmentEnd>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Digest {
    pub input_length: usize,
    pub input_topology: Topology,
    pub enzymes: Vec<Enzyme>,
    pub cuts: Vec<DigestCut>,
    pub fragments: Vec<DigestFragment>,
    pub assumptions: Vec<&'static str>,
}

/// Internal unwrapped cut boundaries. Bottom may be negative on a circle.
#[derive(Debug, Clone)]
struct Cut {
    top: i128,
    bottom: i128,
    enzymes: Vec<&'static str>,
}

/// Simulate complete cleavage. Ambiguous input, unavailable/terminal cuts and
/// overlapping staggered regions are rejected rather than approximated.
pub fn simulate_digest(
    sequence: &DnaSeq,
    topology: Topology,
    names: &[String],
) -> Result<Digest, DigestError> {
    let scan = restriction::find_sites(sequence, topology, names)?;
    let n = sequence.len();
    let mut cuts: Vec<Cut> = Vec::new();
    for site in &scan.sites {
        let (Some(top), Some(bottom)) = (site.top_cut, site.bottom_cut) else {
            return Err(DigestError::OutOfBounds {
                enzyme: site.enzyme,
                start: site.recognition.start().get(),
            });
        };
        if topology == Topology::Linear && (top == 0 || top == n || bottom == 0 || bottom == n) {
            return Err(DigestError::TerminalCut {
                enzyme: site.enzyme,
            });
        }
        // Deliberately do NOT reuse site.bottom_cut below: circular site cuts
        // are modulo n, whereas partition needs a locally unwrapped cut pair.
        // Reconstructing bottom = top + signed stagger preserves a 4-base end
        // across the origin (including negative bottom coordinates for 3′ ends).
        // Substituting the wrapped bottom would corrupt adjacent fragment windows.
        let delta = match site.overhang_polarity {
            OverhangPolarity::Blunt => 0,
            OverhangPolarity::FivePrime => site.overhang_length as i128,
            OverhangPolarity::ThreePrime => -(site.overhang_length as i128),
        };
        cuts.push(Cut {
            top: top as i128,
            bottom: top as i128 + delta,
            enzymes: vec![site.enzyme],
        });
    }
    let cuts = merge_cuts(cuts);
    let fragments = if cuts.is_empty() {
        vec![uncut_fragment(sequence, topology)]
    } else {
        partition(sequence, topology, &cuts)?
    };
    let normalise = |p: i128| {
        if topology == Topology::Circular {
            p.rem_euclid(n as i128) as usize
        } else {
            p as usize
        }
    };
    Ok(Digest {
        input_length: n,
        input_topology: topology,
        enzymes: scan.enzymes,
        cuts: cuts
            .into_iter()
            .map(|cut| DigestCut {
                top_cut: normalise(cut.top),
                bottom_cut: normalise(cut.bottom),
                enzymes: cut.enzymes,
            })
            .collect(),
        fragments,
        assumptions: vec![
            "Complete cleavage at all selected recognition sites; double-stranded DNA assumed",
            "Sequence-only prediction: methylation, star activity, reaction conditions, accessibility and flanking-DNA requirements not modelled",
            "Original linear ends assumed blunt; terminal and overlapping cleavage regions rejected",
            "Top and bottom sequences are both written 5-prime to 3-prime; source_start uses the stored forward coordinate axis",
            "Fragments ordered along the stored top strand, not by size; no gel or experimental validation",
            "Sequence-only fragments: feature annotations, primers and retained metadata are not propagated; source record is unchanged",
        ],
    })
}

fn merge_cuts(mut cuts: Vec<Cut>) -> Vec<Cut> {
    cuts.sort_by_key(|c| (c.top, c.bottom));
    let mut unique: Vec<Cut> = Vec::new();
    for cut in cuts {
        if let Some(previous) = unique
            .last_mut()
            .filter(|c| c.top == cut.top && c.bottom == cut.bottom)
        {
            previous.enzymes.extend(cut.enzymes);
            previous.enzymes.sort_unstable();
            previous.enzymes.dedup();
        } else {
            unique.push(cut);
        }
    }
    unique
}

fn partition(
    sequence: &DnaSeq,
    topology: Topology,
    cuts: &[Cut],
) -> Result<Vec<DigestFragment>, DigestError> {
    let n = sequence.len() as i128;
    let mut boundaries = cuts.to_vec();
    if topology == Topology::Linear {
        boundaries.insert(
            0,
            Cut {
                top: 0,
                bottom: 0,
                enzymes: vec![],
            },
        );
        boundaries.push(Cut {
            top: n,
            bottom: n,
            enzymes: vec![],
        });
    } else {
        let first = &cuts[0];
        boundaries.push(Cut {
            top: first.top + n,
            bottom: first.bottom + n,
            enzymes: first.enzymes.clone(),
        });
    }
    boundaries
        .windows(2)
        .enumerate()
        .map(|(index, pair)| {
            let (left, right) = (&pair[0], &pair[1]);
            let paired_start = left.top.max(left.bottom);
            let paired_end = right.top.min(right.bottom);
            if paired_start >= paired_end {
                return Err(DigestError::OverlappingCuts);
            }
            Ok(DigestFragment {
                id: format!("fragment-{:04}", index + 1),
                topology: Topology::Linear,
                top: strand_interval(sequence, left.top, right.top, false),
                bottom: strand_interval(sequence, left.bottom, right.bottom, true),
                paired_length: (paired_end - paired_start) as usize,
                left_end: Some(fragment_end(sequence, left, true)),
                right_end: Some(fragment_end(sequence, right, false)),
            })
        })
        .collect()
}

fn strand_interval(sequence: &DnaSeq, start: i128, end: i128, reverse: bool) -> FragmentStrand {
    let bases = sequence.as_str().as_bytes();
    let n = bases.len() as i128;
    let interval: String = (start..end)
        .map(|p| char::from(bases[p.rem_euclid(n) as usize]))
        .collect();
    FragmentStrand {
        source_start: start.rem_euclid(n) as usize,
        length: interval.len(),
        sequence_5to3: if reverse {
            reverse_complement(&interval)
        } else {
            interval
        },
    }
}

/// Internal ACGT-only helper for validated digest/ligation strand sequences.
pub(crate) fn reverse_complement(sequence: &str) -> String {
    sequence
        .bytes()
        .rev()
        .map(|base| match base {
            b'A' => 'T',
            b'C' => 'G',
            b'G' => 'C',
            b'T' => 'A',
            _ => unreachable!("restriction scan validated ACGT"),
        })
        .collect()
}

fn fragment_end(sequence: &DnaSeq, cut: &Cut, left: bool) -> FragmentEnd {
    let delta = cut.bottom - cut.top;
    let polarity = match delta.cmp(&0) {
        std::cmp::Ordering::Equal => OverhangPolarity::Blunt,
        std::cmp::Ordering::Greater => OverhangPolarity::FivePrime,
        std::cmp::Ordering::Less => OverhangPolarity::ThreePrime,
    };
    let protruding_strand = if delta == 0 {
        None
    } else if (delta > 0) == left {
        Some(Strand::Forward)
    } else {
        Some(Strand::Reverse)
    };
    let overhang_sequence = strand_interval(
        sequence,
        cut.top.min(cut.bottom),
        cut.top.max(cut.bottom),
        protruding_strand == Some(Strand::Reverse),
    )
    .sequence_5to3;
    FragmentEnd {
        original_terminus: cut.enzymes.is_empty(),
        polarity,
        protruding_strand,
        overhang_sequence,
        enzymes: cut.enzymes.clone(),
    }
}

fn uncut_fragment(sequence: &DnaSeq, topology: Topology) -> DigestFragment {
    let natural = FragmentEnd {
        original_terminus: true,
        polarity: OverhangPolarity::Blunt,
        protruding_strand: None,
        overhang_sequence: String::new(),
        enzymes: vec![],
    };
    DigestFragment {
        id: "fragment-0001".into(),
        topology,
        top: strand_interval(sequence, 0, sequence.len() as i128, false),
        bottom: strand_interval(sequence, 0, sequence.len() as i128, true),
        paired_length: sequence.len(),
        left_end: (topology == Topology::Linear).then(|| natural.clone()),
        right_end: (topology == Topology::Linear).then_some(natural),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn digest(seq: &str, topology: Topology, names: &[&str]) -> Result<Digest, DigestError> {
        simulate_digest(
            &DnaSeq::new(seq).unwrap(),
            topology,
            &names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>(),
        )
    }
    fn conserved(result: &Digest) {
        assert_eq!(
            result.fragments.iter().map(|f| f.top.length).sum::<usize>(),
            result.input_length
        );
        assert_eq!(
            result
                .fragments
                .iter()
                .map(|f| f.bottom.length)
                .sum::<usize>(),
            result.input_length
        );
        for f in &result.fragments {
            assert_eq!(f.top.length, f.top.sequence_5to3.len());
            assert_eq!(f.bottom.length, f.bottom.sequence_5to3.len());
            assert!(f.paired_length > 0);
        }
    }
    #[test]
    fn ecori_linear_has_asymmetric_strands_and_complementary_ends() {
        let d = digest("AAAGAATTCTTT", Topology::Linear, &["EcoRI"]).unwrap();
        conserved(&d);
        assert_eq!(d.fragments.len(), 2);
        assert_eq!(d.fragments[0].top.sequence_5to3, "AAAG");
        assert_eq!(d.fragments[0].bottom.sequence_5to3, "AATTCTTT");
        assert_eq!(d.fragments[1].top.sequence_5to3, "AATTCTTT");
        assert_eq!(d.fragments[1].bottom.sequence_5to3, "AAAG");
        let right = d.fragments[0].right_end.as_ref().unwrap();
        let left = d.fragments[1].left_end.as_ref().unwrap();
        assert_eq!(right.protruding_strand, Some(Strand::Reverse));
        assert_eq!(left.protruding_strand, Some(Strand::Forward));
        assert_eq!(right.overhang_sequence, "AATT");
        assert_eq!(left.overhang_sequence, "AATT");
        assert_eq!(d.fragments[0].paired_length, 4);
    }
    #[test]
    fn blunt_and_three_prime_geometry() {
        for (enzyme, seq, polarity) in [
            ("EcoRV", "AAAGATATCTTT", OverhangPolarity::Blunt),
            ("KpnI", "AAAGGTACCTTT", OverhangPolarity::ThreePrime),
        ] {
            let d = digest(seq, Topology::Linear, &[enzyme]).unwrap();
            conserved(&d);
            let end = d.fragments[0].right_end.as_ref().unwrap();
            assert_eq!(end.polarity, polarity);
            if enzyme == "KpnI" {
                assert_eq!(end.protruding_strand, Some(Strand::Forward));
                assert_eq!(end.overhang_sequence, "GTAC");
            } else {
                assert_eq!(end.protruding_strand, None);
                assert_eq!(d.fragments[0].top.length, 6);
            }
        }
    }
    #[test]
    fn nonpalindromic_type_iis_overhang_is_oriented_per_fragment() {
        let d = digest("AAAGGTCTCAACGATTT", Topology::Linear, &["BsaI"]).unwrap();
        conserved(&d);
        assert_eq!(
            d.fragments[0].right_end.as_ref().unwrap().overhang_sequence,
            "TCGT"
        );
        assert_eq!(
            d.fragments[1].left_end.as_ref().unwrap().overhang_sequence,
            "ACGA"
        );
        let reverse = digest(
            &reverse_complement("AAAGGTCTCAACGATTT"),
            Topology::Linear,
            &["BsaI"],
        )
        .unwrap();
        conserved(&reverse);
        assert_eq!(
            reverse.fragments[0]
                .right_end
                .as_ref()
                .unwrap()
                .overhang_sequence,
            "ACGA"
        );
    }
    #[test]
    fn circular_single_cut_wraps_both_strands() {
        let d = digest("TTCGAA", Topology::Circular, &["EcoRI"]).unwrap();
        conserved(&d);
        assert_eq!(d.fragments.len(), 1);
        assert_eq!(d.fragments[0].topology, Topology::Linear);
        assert_eq!(d.fragments[0].top.sequence_5to3, "AATTCG");
        assert_eq!(d.fragments[0].bottom.sequence_5to3, "AATTCG");
        assert_eq!(d.fragments[0].paired_length, 2);
        let kpn = digest("ACCAAAGGT", Topology::Circular, &["KpnI"]).unwrap();
        conserved(&kpn);
        assert_eq!(
            kpn.fragments[0].left_end.as_ref().unwrap().polarity,
            OverhangPolarity::ThreePrime
        );
    }
    #[test]
    fn two_cut_circle_keeps_bottom_unwrapped_across_origin() {
        for (prefix, suffix, enzyme, expected_cuts, expected_lengths) in [
            (
                "TTC",
                "GAA",
                "EcoRI",
                [(14, 14), (26, 2)],
                [(12, 16), (16, 12)],
            ),
            (
                "ACC",
                "GGT",
                "KpnI",
                [(2, 26), (14, 14)],
                [(12, 16), (16, 12)],
            ),
        ] {
            let seq = format!("{prefix}{}GATATC{}{suffix}", "A".repeat(8), "A".repeat(8));
            let d = digest(&seq, Topology::Circular, &[enzyme, "EcoRV"]).unwrap();
            conserved(&d);
            assert_eq!(d.input_length, 28);
            assert_eq!(
                d.cuts
                    .iter()
                    .map(|c| (c.top_cut, c.bottom_cut))
                    .collect::<Vec<_>>(),
                expected_cuts
            );
            assert_eq!(
                d.fragments
                    .iter()
                    .map(|f| (f.top.length, f.bottom.length))
                    .collect::<Vec<_>>(),
                expected_lengths
            );
            assert!(d.fragments.iter().all(|f| f.paired_length == 12));
            // Independent reconstruction of each entire source strand in forward
            // coordinates, starting at the first reported boundary on that strand.
            for bottom in [false, true] {
                let start = if bottom {
                    d.cuts[0].bottom_cut
                } else {
                    d.cuts[0].top_cut
                };
                let reconstructed: String = d
                    .fragments
                    .iter()
                    .map(|f| {
                        if bottom {
                            reverse_complement(&f.bottom.sequence_5to3)
                        } else {
                            f.top.sequence_5to3.clone()
                        }
                    })
                    .collect();
                assert_eq!(reconstructed, format!("{}{}", &seq[start..], &seq[..start]));
            }
        }
    }

    #[test]
    fn no_cut_retains_topology_and_multiple_cuts_conserve_strands() {
        for topology in [Topology::Linear, Topology::Circular] {
            let no_cut = digest("ACGTACGTACGT", topology, &["EcoRI"]).unwrap();
            conserved(&no_cut);
            assert!(no_cut.cuts.is_empty());
            assert_eq!(no_cut.fragments[0].topology, topology);
            assert_eq!(
                no_cut.fragments[0].left_end.is_none(),
                topology == Topology::Circular
            );
            let d = digest(
                "AAAGAATTCAAAAAAGGATCCTTT",
                topology,
                &["BamHI", "EcoRI", "ecori"],
            )
            .unwrap();
            conserved(&d);
            assert_eq!(d.cuts.len(), 2);
            assert_eq!(
                d.fragments.len(),
                if topology == Topology::Circular { 2 } else { 3 }
            );
        }
    }
    #[test]
    fn convergent_type_iis_sites_merge_only_identical_physical_cuts() {
        let d = digest("GGTCTCAAAAAAGAGACG", Topology::Linear, &["BsaI", "BsmBI"]).unwrap();
        conserved(&d);
        assert_eq!(d.cuts.len(), 1);
        assert_eq!(d.cuts[0].enzymes, ["BsaI", "BsmBI"]);
        assert_eq!(d.fragments.len(), 2);
        for seq in ["GGTCTCAAAAAGAGACC", "GGTCTCAAAAAAAAAAGAGACC"] {
            assert!(matches!(
                digest(seq, Topology::Linear, &["BsaI"]),
                Err(DigestError::OverlappingCuts)
            ));
        }
    }

    #[test]
    fn unsafe_geometries_are_rejected_and_duplicate_pairs_merged() {
        assert!(matches!(
            digest("GGTCTC", Topology::Linear, &["BsaI"]),
            Err(DigestError::OutOfBounds { .. })
        ));
        assert!(matches!(
            digest("AAAAAGAGACCAAA", Topology::Linear, &["BsaI"]),
            Err(DigestError::TerminalCut { .. })
        ));
        let seq = DnaSeq::new("ACGTACGTACGT").unwrap();
        let cuts = vec![
            Cut {
                top: 2,
                bottom: 6,
                enzymes: vec!["EcoRI"],
            },
            Cut {
                top: 5,
                bottom: 9,
                enzymes: vec!["BamHI"],
            },
        ];
        assert!(matches!(
            partition(&seq, Topology::Linear, &cuts),
            Err(DigestError::OverlappingCuts)
        ));
        let seam = vec![
            Cut {
                top: 1,
                bottom: 5,
                enzymes: vec!["EcoRI"],
            },
            Cut {
                top: 9,
                bottom: 13,
                enzymes: vec!["BamHI"],
            },
        ];
        assert!(matches!(
            partition(&seq, Topology::Circular, &seam),
            Err(DigestError::OverlappingCuts)
        ));
        let same = merge_cuts(vec![cuts[0].clone(), cuts[0].clone()]);
        assert_eq!(same.len(), 1);
        assert_eq!(same[0].enzymes, ["EcoRI"]);
    }
}

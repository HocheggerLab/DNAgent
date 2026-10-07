//! Sequence-only restriction-end compatibility; no ligation product prediction.
use crate::Strand;
use crate::digest::{Digest, FragmentEnd};
use crate::restriction::OverhangPolarity;
use serde::Serialize;
use thiserror::Error;

/// Output/resource guard: unordered distinct pairs scale as n*(n-1)/2,
/// so 128 endpoints cap the matrix at 8,128 comparisons (not a biological limit).
pub const MAX_ENDPOINTS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityReason {
    BluntEnds,
    ComplementaryOverhangs,
    PolarityMismatch,
    OverhangLengthMismatch,
    OverhangSequenceMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompatibilityError {
    #[error("invalid fragment end: {0}")]
    InvalidEnd(&'static str),
    #[error("invalid digest endpoint structure: {0}")]
    InvalidDigest(&'static str),
    #[error("expected one or two digests")]
    InputCount,
    #[error(
        "{count} endpoints exceeds the explicit comparison limit of {MAX_ENDPOINTS}; select fewer enzymes"
    )]
    TooManyEnds { count: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EndCompatibility {
    pub compatible: bool,
    pub reason: CompatibilityReason,
    /// Relative to the first fragment, which stays in its stored orientation.
    /// This is a proposed orientation, not an applied sequence transformation.
    pub second_fragment_orientation: Strand,
    /// Position along the first fragment's fixed forward axis.
    pub second_placement: Placement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    BeforeFirst,
    AfterFirst,
}

/// Compare physical oligos (each written 5′→3′). Enzyme names are provenance,
/// not compatibility criteria. Same-side ends require reversing fragment two.
pub fn compare_ends(
    first: &FragmentEnd,
    first_side: EndSide,
    second: &FragmentEnd,
    second_side: EndSide,
) -> Result<EndCompatibility, CompatibilityError> {
    validate_end(first, first_side)?;
    validate_end(second, second_side)?;
    let reason = if first.polarity != second.polarity {
        CompatibilityReason::PolarityMismatch
    } else if first.polarity == OverhangPolarity::Blunt {
        CompatibilityReason::BluntEnds
    } else if first.overhang_sequence.len() != second.overhang_sequence.len() {
        CompatibilityReason::OverhangLengthMismatch
    } else if first.overhang_sequence.bytes().eq(second
        .overhang_sequence
        .bytes()
        .rev()
        .map(complement))
    {
        CompatibilityReason::ComplementaryOverhangs
    } else {
        CompatibilityReason::OverhangSequenceMismatch
    };
    Ok(EndCompatibility {
        compatible: matches!(
            reason,
            CompatibilityReason::BluntEnds | CompatibilityReason::ComplementaryOverhangs
        ),
        reason,
        second_placement: if first_side == EndSide::Left {
            Placement::BeforeFirst
        } else {
            Placement::AfterFirst
        },
        second_fragment_orientation: if first_side == second_side {
            Strand::Reverse
        } else {
            Strand::Forward
        },
    })
}

fn complement(base: u8) -> u8 {
    match base {
        b'A' => b'T',
        b'T' => b'A',
        b'C' => b'G',
        b'G' => b'C',
        _ => unreachable!("end validated"),
    }
}

fn validate_end(end: &FragmentEnd, side: EndSide) -> Result<(), CompatibilityError> {
    if end.polarity == OverhangPolarity::Blunt {
        if !end.overhang_sequence.is_empty() || end.protruding_strand.is_some() {
            return Err(CompatibilityError::InvalidEnd(
                "blunt ends cannot have a protruding oligo",
            ));
        }
    } else {
        if end.original_terminus {
            return Err(CompatibilityError::InvalidEnd(
                "original termini are assumed blunt in this model",
            ));
        }
        if end.overhang_sequence.is_empty()
            || !end
                .overhang_sequence
                .bytes()
                .all(|b| matches!(b, b'A' | b'C' | b'G' | b'T'))
        {
            return Err(CompatibilityError::InvalidEnd(
                "cohesive ends require a nonempty uppercase ACGT oligo",
            ));
        }
        let top = (end.polarity == OverhangPolarity::FivePrime) == (side == EndSide::Left);
        if end.protruding_strand
            != Some(if top {
                Strand::Forward
            } else {
                Strand::Reverse
            })
        {
            return Err(CompatibilityError::InvalidEnd(
                "strand, polarity and fragment side disagree",
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Endpoint {
    pub id: String,
    /// One-based input number; fragment IDs are local to this input's digest.
    pub input: usize,
    pub fragment_id: String,
    pub side: EndSide,
    pub end: FragmentEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EndPair {
    pub first: String,
    pub second: String,
    /// True only for opposite ends of the same fragment, not a product prediction.
    pub same_fragment: bool,
    pub assessment: EndCompatibility,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatibilityReport {
    pub endpoints: Vec<Endpoint>,
    pub pairs: Vec<EndPair>,
    pub assumptions: Vec<&'static str>,
}

/// All unordered pairs of distinct physical ends in one or two digests.
/// Uncut circles contribute no ends. Reuse of the same end/copies is not inferred.
pub fn compatible_ends(digests: &[Digest]) -> Result<CompatibilityReport, CompatibilityError> {
    if !(1..=2).contains(&digests.len()) {
        return Err(CompatibilityError::InputCount);
    }
    let mut endpoints = Vec::new();
    for (index, digest) in digests.iter().enumerate() {
        let mut ids = std::collections::HashSet::new();
        for fragment in &digest.fragments {
            if fragment.id.is_empty() || !ids.insert(&fragment.id) {
                return Err(CompatibilityError::InvalidDigest(
                    "empty or duplicate fragment ID",
                ));
            }
            let has_ends = fragment.topology == crate::Topology::Linear;
            if fragment.left_end.is_some() != has_ends || fragment.right_end.is_some() != has_ends {
                return Err(CompatibilityError::InvalidDigest(
                    "fragment topology and free ends disagree",
                ));
            }
            for (side, end) in [
                (EndSide::Left, &fragment.left_end),
                (EndSide::Right, &fragment.right_end),
            ] {
                if let Some(end) = end {
                    validate_end(end, side)?;
                    let label = if side == EndSide::Left {
                        "left"
                    } else {
                        "right"
                    };
                    endpoints.push(Endpoint {
                        id: format!("input-{}:{}:{label}", index + 1, fragment.id),
                        input: index + 1,
                        fragment_id: fragment.id.clone(),
                        side,
                        end: end.clone(),
                    });
                }
            }
        }
    }
    if endpoints.len() > MAX_ENDPOINTS {
        return Err(CompatibilityError::TooManyEnds {
            count: endpoints.len(),
        });
    }
    let mut pairs = Vec::new();
    for (i, first) in endpoints.iter().enumerate() {
        for second in &endpoints[i + 1..] {
            pairs.push(EndPair {
                first: first.id.clone(),
                second: second.id.clone(),
                same_fragment: first.input == second.input
                    && first.fragment_id == second.fragment_id,
                assessment: compare_ends(&first.end, first.side, &second.end, second.side)?,
            });
        }
    }
    Ok(CompatibilityReport {
        endpoints,
        pairs,
        assumptions: vec![
            "Sequence-only end compatibility, not ligation efficiency, an assembled product or experimental validation",
            "Matching polarity and reverse-complementary 5-prime-to-3-prime oligos required; blunt ends are sequence-compatible",
            "First fragment orientation is fixed; the reported second orientation is a proposal, not an applied transformation",
            "Phosphorylation, damage, ligase, reaction conditions and circularisation geometry are not modelled; original linear ends are assumed blunt",
            "All unordered distinct-end pairs, including within-input and same-fragment pairs; no end reuse, molecule copies or global assembly inference",
            "Uncut circles have no free ends; identical enzyme names do not imply compatibility and different names do not imply incompatibility",
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn end(seq: &str, polarity: OverhangPolarity, side: EndSide) -> FragmentEnd {
        FragmentEnd {
            original_terminus: false,
            polarity,
            protruding_strand: if polarity == OverhangPolarity::Blunt {
                None
            } else {
                Some(
                    if (polarity == OverhangPolarity::FivePrime) == (side == EndSide::Left) {
                        Strand::Forward
                    } else {
                        Strand::Reverse
                    },
                )
            },
            overhang_sequence: seq.into(),
            enzymes: vec!["BsaI"],
        }
    }
    #[test]
    fn reasons_and_orientation_are_explicit() {
        use OverhangPolarity::{Blunt, FivePrime, ThreePrime};
        let first = end("ACGA", FivePrime, EndSide::Right);
        for (seq, polarity, expected) in [
            (
                "TCGT",
                FivePrime,
                CompatibilityReason::ComplementaryOverhangs,
            ),
            (
                "ACGA",
                FivePrime,
                CompatibilityReason::OverhangSequenceMismatch,
            ),
            (
                "TCG",
                FivePrime,
                CompatibilityReason::OverhangLengthMismatch,
            ),
            ("TCGT", ThreePrime, CompatibilityReason::PolarityMismatch),
            ("", Blunt, CompatibilityReason::PolarityMismatch),
        ] {
            for side in [EndSide::Left, EndSide::Right] {
                let result =
                    compare_ends(&first, EndSide::Right, &end(seq, polarity, side), side).unwrap();
                assert_eq!(result.reason, expected);
                assert_eq!(
                    result.compatible,
                    expected == CompatibilityReason::ComplementaryOverhangs
                );
                assert_eq!(
                    result.second_fragment_orientation,
                    if side == EndSide::Right {
                        Strand::Reverse
                    } else {
                        Strand::Forward
                    }
                );
            }
        }
        assert!(
            compare_ends(
                &end("", Blunt, EndSide::Left),
                EndSide::Left,
                &end("", Blunt, EndSide::Right),
                EndSide::Right
            )
            .unwrap()
            .compatible
        );
    }
    #[test]
    fn three_prime_and_cross_enzyme_compatibility() {
        for polarity in [OverhangPolarity::FivePrime, OverhangPolarity::ThreePrime] {
            let a = end("ACGA", polarity, EndSide::Left);
            let mut b = end("TCGT", polarity, EndSide::Right);
            b.enzymes = vec!["BsmBI"];
            assert!(
                compare_ends(&a, EndSide::Left, &b, EndSide::Right)
                    .unwrap()
                    .compatible
            );
        }
    }
    #[test]
    fn malformed_public_end_values_are_rejected() {
        let good = end("ACGA", OverhangPolarity::FivePrime, EndSide::Left);
        for bad in [
            end("N", OverhangPolarity::FivePrime, EndSide::Left),
            end("", OverhangPolarity::FivePrime, EndSide::Left),
            end("A", OverhangPolarity::Blunt, EndSide::Left),
            end("ACGA", OverhangPolarity::FivePrime, EndSide::Right),
        ] {
            assert!(compare_ends(&bad, EndSide::Left, &good, EndSide::Left).is_err());
        }
    }
    #[test]
    fn circle_self_closure_and_no_free_ends() {
        let seq = crate::DnaSeq::new("AAAGAATTCTTT").unwrap();
        let digest =
            crate::digest::simulate_digest(&seq, crate::Topology::Circular, &["EcoRI".into()])
                .unwrap();
        let report = compatible_ends(&[digest]).unwrap();
        assert_eq!(report.pairs.len(), 1);
        assert!(report.pairs[0].same_fragment && report.pairs[0].assessment.compatible);
        let uncut =
            crate::digest::simulate_digest(&seq, crate::Topology::Circular, &["BamHI".into()])
                .unwrap();
        assert_eq!(compatible_ends(&[uncut]).unwrap().endpoints, []);
    }
    #[test]
    fn public_digest_endpoint_structure_is_checked() {
        assert!(matches!(
            compatible_ends(&[]),
            Err(CompatibilityError::InputCount)
        ));
        let seq = crate::DnaSeq::new("AAAGAATTCTTT").unwrap();
        let mut digest =
            crate::digest::simulate_digest(&seq, crate::Topology::Linear, &["EcoRI".into()])
                .unwrap();
        digest.fragments[1].id = digest.fragments[0].id.clone();
        assert!(matches!(
            compatible_ends(&[digest.clone()]),
            Err(CompatibilityError::InvalidDigest(_))
        ));
        digest.fragments[1].id = "unique".into();
        digest.fragments[0].topology = crate::Topology::Circular;
        assert!(matches!(
            compatible_ends(&[digest]),
            Err(CompatibilityError::InvalidDigest(_))
        ));
        let mut original = end("ACGA", OverhangPolarity::FivePrime, EndSide::Left);
        original.original_terminus = true;
        assert!(compare_ends(&original, EndSide::Left, &original, EndSide::Left).is_err());
    }

    #[test]
    fn bounded_and_deterministic_pair_matrix() {
        let seq = crate::DnaSeq::new("AAAGAATTCTTT").unwrap();
        let mut digest =
            crate::digest::simulate_digest(&seq, crate::Topology::Linear, &["EcoRI".into()])
                .unwrap();
        let report = compatible_ends(&[digest.clone(), digest.clone()]).unwrap();
        assert_eq!(report.endpoints.len(), 8);
        assert_eq!(report.pairs.len(), 28);
        assert_eq!(
            report,
            compatible_ends(&[digest.clone(), digest.clone()]).unwrap()
        );
        let fragment = digest.fragments[0].clone();
        digest.fragments = (0..MAX_ENDPOINTS)
            .map(|i| {
                let mut f = fragment.clone();
                f.id = format!("f{i}");
                f
            })
            .collect();
        assert!(matches!(
            compatible_ends(&[digest]),
            Err(CompatibilityError::TooManyEnds { .. })
        ));
    }
}

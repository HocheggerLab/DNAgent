//! Explicit complete restriction/ligation products; no implicit copies or end reuse.
use crate::compatibility::{CompatibilityError, EndCompatibility, EndSide, compare_ends};
use crate::digest::{DigestFragment, FragmentEnd, reverse_complement};
use crate::fragment_annotations::{AnnotatedDigest, AnnotationError, annotated_digest};
use crate::restriction::OverhangPolarity;
use crate::{SequenceRecord, Strand, Topology};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

pub const MAX_INPUTS: usize = 16;
pub const MAX_COMPONENTS: usize = 128;
pub const MAX_STRAND_BASES: usize = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    Forward,
    Reverse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FragmentSelection {
    /// One-based digest-instance number, not an implicit molecule-copy count.
    pub input: usize,
    pub fragment_id: String,
    pub orientation: Orientation,
}

#[derive(Debug, Error)]
pub enum LigationError {
    #[error("invalid ligation plan: {0}")]
    InvalidPlan(&'static str),
    #[error(transparent)]
    Annotation(#[from] AnnotationError),
    #[error(transparent)]
    Compatibility(#[from] CompatibilityError),
    #[error(
        "incompatible junction after component {after} and before component {before}: {reason:?}"
    )]
    Incompatible {
        after: usize,
        before: usize,
        reason: crate::compatibility::CompatibilityReason,
    },
    #[error("ligation duplex conservation check failed: {0}")]
    Conservation(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductSpan {
    /// Zero-based offset on this product strand's own 5′→3′ sequence.
    pub start: usize,
    pub length: usize,
    /// Which original digest strand supplies these bases and its annotations.
    pub source_strand: Strand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentPlacement {
    pub selection: FragmentSelection,
    pub top: ProductSpan,
    pub bottom: ProductSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Junction {
    pub after_component: usize,
    pub before_component: usize,
    pub closure: bool,
    /// Nick boundary on each product strand's own 5′→3′ axis; circles modulo length.
    pub top_boundary: usize,
    pub bottom_boundary: usize,
    pub assessment: EndCompatibility,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnusedFragment {
    pub input: usize,
    pub fragment_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LigationProduct {
    pub topology: Topology,
    pub top_sequence_5to3: String,
    pub bottom_sequence_5to3: String,
    /// Start of bottom's forward-coordinate interval relative to top base zero.
    /// Signed for linear products; normalised modulo product length for circles.
    pub bottom_forward_start: i128,
    pub paired_length: usize,
    pub left_end: Option<FragmentEnd>,
    pub right_end: Option<FragmentEnd>,
    pub components: Vec<ComponentPlacement>,
    pub junctions: Vec<Junction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LigationReport {
    /// Original component annotations are linked through product placements.
    pub inputs: Vec<AnnotatedDigest>,
    pub product: LigationProduct,
    pub unused_fragments: Vec<UnusedFragment>,
    pub assumptions: Vec<&'static str>,
}

/// Validate bounded request shape before expensive imports or digestion.
pub fn validate_request(
    input_count: usize,
    selected: &[FragmentSelection],
) -> Result<(), LigationError> {
    if !(1..=MAX_INPUTS).contains(&input_count) {
        return Err(LigationError::InvalidPlan(
            "expected 1..=16 input digest instances",
        ));
    }
    if selected.is_empty() || selected.len() > MAX_COMPONENTS {
        return Err(LigationError::InvalidPlan(
            "expected 1..=128 selected components",
        ));
    }
    let mut used = HashSet::new();
    for selection in selected {
        if selection.input == 0 || selection.input > input_count || selection.fragment_id.is_empty()
        {
            return Err(LigationError::InvalidPlan(
                "invalid input number or empty fragment ID",
            ));
        }
        if !used.insert((selection.input, &selection.fragment_id)) {
            return Err(LigationError::InvalidPlan(
                "fragment reused; declare a separate input digest instance for another copy",
            ));
        }
    }
    Ok(())
}

pub fn simulate_ligation(
    inputs: &[(&SequenceRecord, &[String])],
    selected: &[FragmentSelection],
    topology: Topology,
) -> Result<LigationReport, LigationError> {
    validate_request(inputs.len(), selected)?;
    // Accept validated source records, not caller-forged public digest structs.
    let inputs = inputs
        .iter()
        .map(|(r, names)| annotated_digest(r, names))
        .collect::<Result<Vec<_>, _>>()?;
    let components = selected
        .iter()
        .map(|selection| {
            let fragment = inputs[selection.input - 1]
                .digest
                .fragments
                .iter()
                .find(|f| f.id == selection.fragment_id)
                .ok_or(LigationError::InvalidPlan(
                    "selected fragment does not exist",
                ))?;
            orient(fragment, selection.orientation)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let product = assemble(&components, selected, topology)?;
    let unused_fragments = inputs
        .iter()
        .enumerate()
        .flat_map(|(index, input)| {
            input
                .digest
                .fragments
                .iter()
                .filter(move |f| {
                    !selected
                        .iter()
                        .any(|s| s.input == index + 1 && s.fragment_id == f.id)
                })
                .map(move |f| UnusedFragment {
                    input: index + 1,
                    fragment_id: f.id.clone(),
                })
        })
        .collect();
    Ok(LigationReport {
        inputs,
        product,
        unused_fragments,
        assumptions: vec![
            "Explicit digest instances and component order; no implicit fragment copies or endpoint reuse",
            "Assumes selected sequence-compatible junctions seal completely; phosphorylation, nicks, reaction conditions and experimental yield are not predicted",
            "Reverse orientation exchanges whole top/bottom 5-prime-to-3-prime sequences and ends; exposed physical oligos are not reverse-complemented",
            "Both selected strands are conserved without trimming, fill-in or overlap deduplication; unused fragments are reported separately",
            "Component-local source annotations remain in inputs and map through product spans; gene repair, fusion, joined feature reconstruction and functional integrity are not inferred",
            "For circular products bottom_forward_start records the phase between strand coordinate origins; bottom sequence is not silently rotated",
            "Linear single-component plans are pass-through products with no ligation junctions",
        ],
    })
}

struct Oriented {
    top: String,
    bottom: String,
    left: FragmentEnd,
    right: FragmentEnd,
}

fn flip(strand: Strand) -> Strand {
    match strand {
        Strand::Forward => Strand::Reverse,
        Strand::Reverse => Strand::Forward,
        Strand::Unknown => Strand::Unknown,
    }
}

fn orient(fragment: &DigestFragment, orientation: Orientation) -> Result<Oriented, LigationError> {
    let (Some(left), Some(right)) = (&fragment.left_end, &fragment.right_end) else {
        return Err(LigationError::InvalidPlan(
            "uncut circular fragments have no free ends",
        ));
    };
    if orientation == Orientation::Forward {
        return Ok(Oriented {
            top: fragment.top.sequence_5to3.clone(),
            bottom: fragment.bottom.sequence_5to3.clone(),
            left: left.clone(),
            right: right.clone(),
        });
    }
    let mut left = right.clone();
    let mut right = fragment
        .left_end
        .clone()
        .ok_or(LigationError::InvalidPlan("missing end"))?;
    left.protruding_strand = left.protruding_strand.map(flip);
    right.protruding_strand = right.protruding_strand.map(flip);
    Ok(Oriented {
        top: fragment.bottom.sequence_5to3.clone(),
        bottom: fragment.top.sequence_5to3.clone(),
        left,
        right,
    })
}

fn total_length(components: &[Oriented], bottom: bool) -> Result<usize, LigationError> {
    components.iter().try_fold(0usize, |total, component| {
        total
            .checked_add(if bottom {
                component.bottom.len()
            } else {
                component.top.len()
            })
            .filter(|n| *n <= MAX_STRAND_BASES)
            .ok_or(LigationError::InvalidPlan(
                "product exceeds 10,000,000 bases per strand",
            ))
    })
}

fn assemble(
    oriented: &[Oriented],
    selected: &[FragmentSelection],
    topology: Topology,
) -> Result<LigationProduct, LigationError> {
    let nt = total_length(oriented, false)?;
    let nb = total_length(oriented, true)?;
    let mut components = Vec::new();
    let mut junctions = Vec::new();
    let (mut top, mut bottom) = (0, 0);
    for (index, (component, selection)) in oriented.iter().zip(selected).enumerate() {
        let source_strand = if selection.orientation == Orientation::Forward {
            Strand::Forward
        } else {
            Strand::Reverse
        };
        components.push(ComponentPlacement {
            selection: selection.clone(),
            top: ProductSpan {
                start: top,
                length: component.top.len(),
                source_strand,
            },
            bottom: ProductSpan {
                start: nb - bottom - component.bottom.len(),
                length: component.bottom.len(),
                source_strand: flip(source_strand),
            },
        });
        top += component.top.len();
        bottom += component.bottom.len();
        let closure = index + 1 == oriented.len();
        if !closure || topology == Topology::Circular {
            let next = (index + 1) % oriented.len();
            let assessment = compare_ends(
                &component.right,
                EndSide::Right,
                &oriented[next].left,
                EndSide::Left,
            )?;
            if !assessment.compatible {
                return Err(LigationError::Incompatible {
                    after: index + 1,
                    before: next + 1,
                    reason: assessment.reason,
                });
            }
            junctions.push(Junction {
                after_component: index + 1,
                before_component: next + 1,
                closure,
                top_boundary: if closure { 0 } else { top },
                bottom_boundary: nb - bottom,
                assessment,
            });
        }
    }
    let first = &oriented[0];
    let last = &oriented[oriented.len() - 1];
    let top_sequence_5to3: String = oriented.iter().map(|c| c.top.as_str()).collect();
    let bottom_sequence_5to3: String = oriented.iter().rev().map(|c| c.bottom.as_str()).collect();
    if top_sequence_5to3.len() != nt || bottom_sequence_5to3.len() != nb {
        return Err(LigationError::Conservation(
            "selected strand lengths changed",
        ));
    }
    let delta = stagger(&first.left);
    let (bottom_forward_start, paired_length) = verify_duplex(
        &top_sequence_5to3,
        &bottom_sequence_5to3,
        delta,
        stagger(&last.right),
        topology,
    )?;
    Ok(LigationProduct {
        topology,
        top_sequence_5to3,
        bottom_sequence_5to3,
        bottom_forward_start,
        paired_length,
        left_end: (topology == Topology::Linear).then(|| first.left.clone()),
        right_end: (topology == Topology::Linear).then(|| last.right.clone()),
        components,
        junctions,
    })
}

fn stagger(end: &FragmentEnd) -> i128 {
    let n = end.overhang_sequence.len() as i128;
    match end.polarity {
        OverhangPolarity::Blunt => 0,
        OverhangPolarity::FivePrime => n,
        OverhangPolarity::ThreePrime => -n,
    }
}

fn verify_duplex(
    top: &str,
    bottom: &str,
    delta: i128,
    right_delta: i128,
    topology: Topology,
) -> Result<(i128, usize), LigationError> {
    let bottom_forward = reverse_complement(bottom);
    if topology == Topology::Circular {
        if top.len() != bottom.len() {
            return Err(LigationError::Conservation(
                "unequal circular strand lengths",
            ));
        }
        let offset = delta.rem_euclid(top.len() as i128) as usize;
        if bottom_forward != format!("{}{}", &top[offset..], &top[..offset]) {
            return Err(LigationError::Conservation(
                "circular strand phase mismatch",
            ));
        }
        return Ok((offset as i128, top.len()));
    }
    let end = delta + bottom.len() as i128;
    if end - top.len() as i128 != right_delta {
        return Err(LigationError::Conservation("terminal stagger mismatch"));
    }
    let lo = delta.max(0);
    let hi = end.min(top.len() as i128);
    if lo >= hi
        || top[lo as usize..hi as usize]
            != bottom_forward[(lo - delta) as usize..(hi - delta) as usize]
    {
        return Err(LigationError::Conservation("linear paired-core mismatch"));
    }
    Ok((delta, (hi - lo) as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DnaSeq;
    fn record(seq: &str, topology: Topology) -> SequenceRecord {
        SequenceRecord::new(
            "synthetic",
            DnaSeq::new(seq).unwrap(),
            topology,
            vec![],
            vec![],
        )
        .unwrap()
    }
    fn selection(input: usize, number: usize, orientation: Orientation) -> FragmentSelection {
        FragmentSelection {
            input,
            fragment_id: format!("fragment-{number:04}"),
            orientation,
        }
    }
    #[test]
    fn rejoining_linear_digest_conserves_both_strands() {
        for (seq, enzyme) in [
            ("AAAGAATTCTTT", "EcoRI"),
            ("AAAGGTACCTTT", "KpnI"),
            ("AAAGATATCTTT", "EcoRV"),
        ] {
            let r = record(seq, Topology::Linear);
            let names = vec![enzyme.into()];
            let selected = [
                selection(1, 1, Orientation::Forward),
                selection(1, 2, Orientation::Forward),
            ];
            let report = simulate_ligation(&[(&r, &names)], &selected, Topology::Linear).unwrap();
            assert_eq!(report.product.top_sequence_5to3, seq);
            assert_eq!(report.product.bottom_sequence_5to3, reverse_complement(seq));
            assert_eq!(report.product.junctions.len(), 1);
            assert_eq!(report.product.paired_length, seq.len());
            assert!(report.unused_fragments.is_empty());
        }
    }
    #[test]
    fn circle_closure_keeps_explicit_bottom_phase() {
        for (seq, enzyme, phase) in [
            ("AAAGAATTCTTT", "EcoRI", 4),
            ("AAAGGTACCTTT", "KpnI", 8),
            ("AAAGATATCTTT", "EcoRV", 0),
        ] {
            let r = record(seq, Topology::Circular);
            let names = vec![enzyme.into()];
            for orientation in [Orientation::Forward, Orientation::Reverse] {
                let report = simulate_ligation(
                    &[(&r, &names)],
                    &[selection(1, 1, orientation)],
                    Topology::Circular,
                )
                .unwrap();
                assert_eq!(report.product.bottom_forward_start, phase);
                assert!(report.product.left_end.is_none());
                assert!(report.product.junctions[0].closure);
                assert_eq!(report.product.paired_length, 12);
            }
        }
    }
    #[test]
    fn reverse_order_and_orientation_reverse_the_whole_duplex() {
        let r = record("AAAGAATTCTGC", Topology::Linear);
        let names = vec!["EcoRI".into()];
        let report = simulate_ligation(
            &[(&r, &names)],
            &[
                selection(1, 2, Orientation::Reverse),
                selection(1, 1, Orientation::Reverse),
            ],
            Topology::Linear,
        )
        .unwrap();
        assert_eq!(report.product.top_sequence_5to3, "GCAGAATTCTTT");
        assert_eq!(report.product.bottom_sequence_5to3, r.sequence().as_str());
        assert_eq!(
            report.product.components[0].top.source_strand,
            Strand::Reverse
        );
    }
    #[test]
    fn pass_through_preserves_sticky_termini_and_paired_core() {
        for (seq, enzyme) in [("AAAGAATTCTGC", "EcoRI"), ("AAAGGTACCTGC", "KpnI")] {
            let r = record(seq, Topology::Linear);
            let names = vec![enzyme.into()];
            for number in [1, 2] {
                for orientation in [Orientation::Forward, Orientation::Reverse] {
                    let report = simulate_ligation(
                        &[(&r, &names)],
                        &[selection(1, number, orientation)],
                        Topology::Linear,
                    )
                    .unwrap();
                    let source = &report.inputs[0].digest.fragments[number - 1];
                    assert_eq!(report.product.paired_length, source.paired_length);
                    assert!(report.product.junctions.is_empty());
                    assert_eq!(report.unused_fragments.len(), 1);
                }
            }
        }
    }

    #[test]
    fn resource_limits_are_checked_at_the_boundary() {
        let s = selection(1, 1, Orientation::Forward);
        assert!(validate_request(1, &vec![s; MAX_COMPONENTS + 1]).is_err());
        // The length helper's threshold is also tested at the exact boundary.
        let base = record("AAAA", Topology::Linear);
        let names = vec!["EcoRI".into()];
        let fragment = annotated_digest(&base, &names)
            .unwrap()
            .digest
            .fragments
            .remove(0);
        let mut component = orient(&fragment, Orientation::Forward).unwrap();
        component.top = "A".repeat(MAX_STRAND_BASES);
        assert_eq!(
            total_length(std::slice::from_ref(&component), false).unwrap(),
            MAX_STRAND_BASES
        );
        component.top.push('A');
        assert!(total_length(&[component], false).is_err());
    }

    #[test]
    fn refuses_reuse_missing_fragments_and_uncut_circles() {
        let r = record("AAAGAATTCTTT", Topology::Circular);
        let names = vec!["BamHI".into()];
        let s = selection(1, 1, Orientation::Forward);
        assert!(
            simulate_ligation(
                &[(&r, &names)],
                std::slice::from_ref(&s),
                Topology::Circular
            )
            .is_err()
        );
        assert!(validate_request(1, &[s.clone(), s]).is_err());
        assert!(
            simulate_ligation(
                &[(&r, &names)],
                &[selection(1, 99, Orientation::Forward)],
                Topology::Linear
            )
            .is_err()
        );
        assert!(validate_request(17, &[selection(1, 1, Orientation::Forward)]).is_err());
    }
    #[test]
    fn incompatible_junctions_fail_and_distinct_instances_allow_explicit_copies() {
        let a = record("AAAGAATTCTTT", Topology::Linear);
        let b = record("AAAGGTACCTTT", Topology::Linear);
        let ea = vec!["EcoRI".into()];
        let eb = vec!["KpnI".into()];
        let selected = [
            selection(1, 1, Orientation::Forward),
            selection(2, 2, Orientation::Forward),
        ];
        assert!(matches!(
            simulate_ligation(&[(&a, &ea), (&b, &eb)], &selected, Topology::Linear),
            Err(LigationError::Incompatible { .. })
        ));
        let report =
            simulate_ligation(&[(&a, &ea), (&a, &ea)], &selected, Topology::Linear).unwrap();
        assert_eq!(report.unused_fragments.len(), 2);
        assert_eq!(report.product.top_sequence_5to3, a.sequence().as_str());
    }
}

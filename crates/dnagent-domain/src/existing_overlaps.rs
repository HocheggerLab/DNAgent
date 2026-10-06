//! Assembly of declared linear fragment views with explicit exact overlaps.
use crate::fragment_annotations::FeatureMapping;
use crate::gibson::{
    CoreSelection, GibsonError, GibsonJunction, MAX_BASES, core, core_annotations,
    unique_duplex_site, validate_templates,
};
use crate::{SequenceRecord, Topology};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ExistingComponent {
    pub selection: CoreSelection,
    pub sequence_5to3: String,
    pub product_start: usize,
    pub wraps_origin: bool,
    pub annotations: Vec<FeatureMapping>,
}

#[derive(Debug, Serialize)]
pub struct ExistingAssembly {
    pub inputs: Vec<SequenceRecord>,
    pub topology: Topology,
    pub product_sequence_5to3: String,
    pub components: Vec<ExistingComponent>,
    pub junctions: Vec<GibsonJunction>,
    pub assumptions: Vec<&'static str>,
}

pub fn validate_request(
    count: usize,
    selected: &[CoreSelection],
    topology: Topology,
    overlaps: &[usize],
) -> Result<(), GibsonError> {
    if !(1..=16).contains(&count) || selected.is_empty() || selected.len() > 128 {
        return Err(GibsonError::Invalid(
            "expected 1..=16 sources and 1..=128 existing fragments",
        ));
    }
    let junction_count = selected.len() - usize::from(topology == Topology::Linear);
    if overlaps.len() != junction_count || overlaps.iter().any(|n| !(20..=60).contains(n)) {
        return Err(GibsonError::Invalid(
            "supply exactly one 20..=60-base overlap per junction, including circular closure",
        ));
    }
    let mut total = 0usize;
    for (i, s) in selected.iter().enumerate() {
        let incoming = if i > 0 {
            overlaps[i - 1]
        } else if topology == Topology::Circular {
            overlaps[selected.len() - 1]
        } else {
            0
        };
        let outgoing = overlaps.get(i).copied().unwrap_or(0);
        if s.input == 0 || s.input > count || s.length <= incoming + outgoing {
            return Err(GibsonError::Invalid(
                "invalid source index or fragment without a non-overlapping interior",
            ));
        }
        total = total
            .checked_add(s.length)
            .filter(|n| *n <= MAX_BASES)
            .ok_or(GibsonError::Invalid(
                "total supplied fragment bases exceed 1,000,000",
            ))?;
    }
    Ok(())
}

pub fn assemble(
    records: &[SequenceRecord],
    selected: &[CoreSelection],
    topology: Topology,
    overlaps: &[usize],
) -> Result<ExistingAssembly, GibsonError> {
    validate_request(records.len(), selected, topology, overlaps)?;
    validate_templates(records)?;
    let sequences = selected
        .iter()
        .map(|s| core(&records[s.input - 1], s))
        .collect::<Result<Vec<_>, _>>()?;
    let mut product = sequences[0].clone();
    let mut starts = vec![0];
    for (i, next) in sequences.iter().enumerate().skip(1) {
        let overlap = overlaps[i - 1];
        check_overlap(&product, next, overlap, i)?;
        starts.push(product.len() - overlap);
        product.push_str(&next[overlap..]);
    }
    if topology == Topology::Circular {
        let overlap = overlaps[overlaps.len() - 1];
        check_overlap(&product, &sequences[0], overlap, selected.len())?;
        product.truncate(product.len() - overlap);
    }
    let mut junctions = Vec::new();
    for (i, &length) in overlaps.iter().enumerate() {
        let next = (i + 1) % selected.len();
        let motif = &sequences[next][..length];
        if !unique_duplex_site(&product, motif, topology) {
            return Err(GibsonError::Component {
                component: i + 1,
                reason: "existing overlap is repeated or reverse-complement ambiguous in the merged product",
            });
        }
        junctions.push(GibsonJunction {
            after_component: i + 1,
            before_component: next + 1,
            closure: next == 0,
            product_start: starts[next],
            overlap_sequence_5to3: motif.into(),
            added_by: None,
        });
    }
    let mut components = Vec::new();
    for (i, (s, sequence)) in selected.iter().zip(sequences).enumerate() {
        // Conservation includes both source associations at merged homology bases.
        if !sequence
            .bytes()
            .enumerate()
            .all(|(j, b)| product.as_bytes()[(starts[i] + j) % product.len()] == b)
        {
            return Err(GibsonError::Invalid(
                "existing-fragment source-to-product conservation failed",
            ));
        }
        components.push(ExistingComponent {
            selection: s.clone(),
            product_start: starts[i],
            wraps_origin: starts[i] + sequence.len() > product.len(),
            annotations: core_annotations(&records[s.input - 1], s, &sequence)?,
            sequence_5to3: sequence,
        });
    }
    Ok(ExistingAssembly {
        inputs: records.to_vec(),
        topology,
        product_sequence_5to3: product,
        components,
        junctions,
        assumptions: vec![
            "Selections declare existing linear fragment views, even when source coordinate topology is circular; no PCR or digestion preparation is inferred",
            "Only explicitly declared exact overlaps are merged; no ordering, orientation, polishing or overlap-length search is performed",
            "Each overlap must be unique across both orientations of the merged product; partial/mismatched homology and reaction kinetics are not assessed",
            "Component-local annotations map by product_start plus local coordinate, modulo product length for circles; both source associations at shared homology are retained",
            "Merged homology has one sequence copy but may have multiple source annotations; genes, translations and biological function are not reconstructed",
            "Assumes ideal fully paired assembly; no experimental validation, yield prediction, raw XML or opaque metadata export",
        ],
    })
}

fn check_overlap(
    first: &str,
    second: &str,
    length: usize,
    component: usize,
) -> Result<(), GibsonError> {
    if first.len() < length
        || second.len() < length
        || first[first.len() - length..] != second[..length]
    {
        return Err(GibsonError::Component {
            component,
            reason: "declared existing suffix/prefix overlap does not match",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DnaSeq, ligation::Orientation};
    fn source() -> SequenceRecord {
        let mut state = 1_234_567_u64;
        let sequence: String = (0..240)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                char::from(b"ACGT"[(state % 4) as usize])
            })
            .collect();
        SequenceRecord::new(
            "synthetic",
            DnaSeq::new(sequence).unwrap(),
            Topology::Circular,
            vec![],
            vec![],
        )
        .unwrap()
    }
    fn selections() -> Vec<CoreSelection> {
        vec![
            CoreSelection {
                input: 1,
                start: 0,
                length: 140,
                orientation: Orientation::Forward,
            },
            CoreSelection {
                input: 1,
                start: 115,
                length: 150,
                orientation: Orientation::Forward,
            },
        ]
    }
    #[test]
    fn merges_existing_homology_and_preserves_circular_source_associations() {
        let r = source();
        let out = assemble(
            std::slice::from_ref(&r),
            &selections(),
            Topology::Circular,
            &[25, 25],
        )
        .unwrap();
        assert_eq!(out.product_sequence_5to3, r.sequence().as_str());
        assert_eq!(out.components[1].product_start, 115);
        assert!(out.components[1].wraps_origin);
        assert_eq!(out.junctions[1].product_start, 0);
        let mut reverse = selections();
        reverse.reverse();
        for s in &mut reverse {
            s.orientation = Orientation::Reverse;
        }
        let out = assemble(
            std::slice::from_ref(&r),
            &reverse,
            Topology::Circular,
            &[25, 25],
        )
        .unwrap();
        let expected = crate::digest::reverse_complement(r.sequence().as_str());
        assert_eq!(out.product_sequence_5to3.len(), 240);
        assert!(expected.repeat(2).contains(&out.product_sequence_5to3));
    }
    #[test]
    fn single_linear_view_needs_no_overlap_or_pcr_length() {
        let r = source();
        let selected = [CoreSelection {
            input: 1,
            start: 0,
            length: 1,
            orientation: Orientation::Forward,
        }];
        let out = assemble(std::slice::from_ref(&r), &selected, Topology::Linear, &[]).unwrap();
        assert_eq!(out.product_sequence_5to3, &r.sequence().as_str()[..1]);
        assert!(out.junctions.is_empty());
        assert!(!out.components[0].wraps_origin);
    }

    #[test]
    fn rejects_mismatch_bad_counts_exhausted_interiors_and_repeated_homology() {
        assert!(assemble(&[source()], &selections(), Topology::Circular, &[24, 25]).is_err());
        assert!(validate_request(1, &selections(), Topology::Circular, &[25]).is_err());
        assert!(validate_request(0, &selections(), Topology::Circular, &[25, 25]).is_err());
        let mut s = selections();
        s[0].length = 50;
        assert!(validate_request(1, &s, Topology::Circular, &[25, 25]).is_err());
        s[0].length = usize::MAX;
        assert!(validate_request(1, &s, Topology::Circular, &[25, 25]).is_err());
        let repeated = SequenceRecord::new(
            "synthetic",
            DnaSeq::new("A".repeat(240)).unwrap(),
            Topology::Circular,
            vec![],
            vec![],
        )
        .unwrap();
        assert!(assemble(&[repeated], &selections(), Topology::Circular, &[25, 25]).is_err());
    }
}

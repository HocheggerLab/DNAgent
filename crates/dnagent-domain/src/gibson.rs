//! Explicit PCR-tail Gibson candidates, not thermodynamically optimised primers.
use crate::digest::{FragmentStrand, reverse_complement};
use crate::fragment_annotations::{FeatureMapping, map_feature};
use crate::ligation::Orientation;
use crate::{DomainError, SequenceRecord, Topology};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_BASES: usize = 1_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSelection {
    pub input: usize,
    pub start: usize,
    pub length: usize,
    pub orientation: Orientation,
}

#[derive(Debug, Error)]
pub enum GibsonError {
    #[error("invalid Gibson design: {0}")]
    Invalid(&'static str),
    #[error("component {component}: {reason}")]
    Component {
        component: usize,
        reason: &'static str,
    },
    #[error(transparent)]
    Coordinates(#[from] DomainError),
}

#[derive(Debug, Clone, Serialize)]
pub struct PrimerCandidate {
    pub sequence_5to3: String,
    pub annealing_sequence_5to3: String,
    pub tail_sequence_5to3: String,
    pub annealing_gc_bases: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GibsonComponent {
    pub selection: CoreSelection,
    pub product_start: usize,
    pub core_sequence_5to3: String,
    pub pcr_product_sequence_5to3: String,
    pub forward_primer: PrimerCandidate,
    pub reverse_primer: PrimerCandidate,
    /// Core-local, not PCR-tail or inferred fused-product annotations.
    pub annotations: Vec<FeatureMapping>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GibsonJunction {
    pub after_component: usize,
    pub before_component: usize,
    pub closure: bool,
    pub product_start: usize,
    pub overlap_sequence_5to3: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GibsonReport {
    pub inputs: Vec<SequenceRecord>,
    pub topology: Topology,
    pub overlap_length: usize,
    pub annealing_length: Option<usize>,
    pub product_sequence_5to3: String,
    pub components: Vec<GibsonComponent>,
    pub junctions: Vec<GibsonJunction>,
    pub assumptions: Vec<&'static str>,
}

pub fn validate_request(
    input_count: usize,
    selected: &[CoreSelection],
    overlap: usize,
    annealing: usize,
) -> Result<(), GibsonError> {
    if !(1..=16).contains(&input_count) || selected.is_empty() || selected.len() > 128 {
        return Err(GibsonError::Invalid(
            "expected 1..=16 templates and 1..=128 cores",
        ));
    }
    if !(20..=60).contains(&overlap) || !(18..=40).contains(&annealing) {
        return Err(GibsonError::Invalid(
            "overlap must be 20..=60 and annealing length 18..=40 bases",
        ));
    }
    let mut total = 0usize;
    for selection in selected {
        if selection.input == 0
            || selection.input > input_count
            || selection.length < overlap.max(2 * annealing)
        {
            return Err(GibsonError::Invalid(
                "invalid template number or core shorter than overlap/two annealing regions",
            ));
        }
        total = total
            .checked_add(selection.length)
            .filter(|n| *n <= MAX_BASES)
            .ok_or(GibsonError::Invalid("product exceeds 1,000,000 bases"))?;
    }
    Ok(())
}

pub fn design(
    records: &[SequenceRecord],
    selected: &[CoreSelection],
    topology: Topology,
    overlap: usize,
    annealing: usize,
) -> Result<GibsonReport, GibsonError> {
    validate_request(records.len(), selected, overlap, annealing)?;
    let mut report = design_with_lengths(
        records,
        selected,
        topology,
        overlap,
        &vec![(annealing, annealing); selected.len()],
    )?;
    report.assumptions.extend([
        "Primer candidates use explicit fixed annealing lengths; GC counts are descriptive, not optimisation or thermodynamic validation",
        "Exact primer-site uniqueness is checked on each full template in both orientations; mismatched/off-template binding, dimers, hairpins and melting temperatures are not evaluated",
    ]);
    Ok(report)
}

pub(crate) fn validate_templates(records: &[SequenceRecord]) -> Result<(), GibsonError> {
    if records.iter().any(|r| {
        r.sequence().len() > MAX_BASES
            || !r.sequence().as_str().bytes().all(|b| b"ACGT".contains(&b))
    }) {
        return Err(GibsonError::Invalid(
            "templates must be unambiguous ACGT and at most 1,000,000 bases",
        ));
    }
    Ok(())
}

pub(crate) fn design_with_lengths(
    records: &[SequenceRecord],
    selected: &[CoreSelection],
    topology: Topology,
    overlap: usize,
    lengths: &[(usize, usize)],
) -> Result<GibsonReport, GibsonError> {
    validate_lengths(records.len(), selected, overlap, lengths)?;
    validate_templates(records)?;
    let cores = selected
        .iter()
        .map(|s| core(&records[s.input - 1], s))
        .collect::<Result<Vec<_>, _>>()?;
    let product_sequence_5to3 = cores.concat();
    let mut components = Vec::new();
    let mut junctions = Vec::new();
    let mut product_start = 0;
    for (i, (selection, sequence)) in selected.iter().zip(&cores).enumerate() {
        let template = &records[selection.input - 1];
        let forward_anneal = &sequence[..lengths[i].0];
        let reverse_anneal = reverse_complement(&sequence[sequence.len() - lengths[i].1..]);
        for primer in [forward_anneal, &reverse_anneal] {
            if !unique_duplex_site(template.sequence().as_str(), primer, template.topology()) {
                return Err(GibsonError::Component {
                    component: i + 1,
                    reason: "primer annealing sequence lacks a unique exact site on its full template",
                });
            }
        }
        let closure = i + 1 == selected.len();
        let has_junction = !closure || topology == Topology::Circular;
        let next = (i + 1) % selected.len();
        let tail = if has_junction {
            &cores[next][..overlap]
        } else {
            ""
        };
        if has_junction {
            if !unique_duplex_site(&product_sequence_5to3, tail, topology) {
                return Err(GibsonError::Component {
                    component: i + 1,
                    reason: "overlap is repeated or reverse-complement ambiguous in the intended product",
                });
            }
            junctions.push(GibsonJunction {
                after_component: i + 1,
                before_component: next + 1,
                closure,
                product_start: if closure {
                    0
                } else {
                    product_start + sequence.len()
                },
                overlap_sequence_5to3: tail.to_owned(),
            });
        }
        let annotations = core_annotations(template, selection, sequence)?;
        components.push(GibsonComponent {
            selection: selection.clone(),
            product_start,
            core_sequence_5to3: sequence.clone(),
            pcr_product_sequence_5to3: format!("{sequence}{tail}"),
            forward_primer: primer(forward_anneal, ""),
            reverse_primer: primer(&reverse_anneal, &reverse_complement(tail)),
            annotations,
        });
        product_start += sequence.len();
    }
    verify_assembly(&components, &junctions, &product_sequence_5to3)?;
    Ok(GibsonReport {
        inputs: records.to_vec(),
        topology,
        overlap_length: overlap,
        annealing_length: lengths
            .iter()
            .all(|&(f, r)| f == lengths[0].0 && r == f)
            .then_some(lengths[0].0),
        product_sequence_5to3,
        components,
        junctions,
        assumptions: vec![
            "PCR-tail design mode: requested cores concatenate without deduplicating endogenous sequence; this is not intake of pre-existing overlapping fragments",
            "Only reverse primers receive 5-prime tails, derived from the next oriented core prefix; forward primers have no synthetic tails",
            "Overlaps must occur exactly once in the intended product across both orientations, including circular-origin matches",
            "Assumes clean sequence-faithful PCR and ideal overlap-directed assembly; reaction yield, enzyme conditions and experimental validity are not predicted",
            "Core-local annotations map through product_start; feature reunion/fusion, translations and biological function are not inferred",
            "Synthetic overlap copies are removed exactly once at each planned junction; final core bases remain unchanged",
            "Source primers are provenance only, not reused binding-site predictions; opaque SnapGene metadata is not exported",
            "Linear single-core designs are PCR-only, without an assembly junction; circular single-core designs model PCR-assisted reclosure",
        ],
    })
}

fn validate_lengths(
    input_count: usize,
    selected: &[CoreSelection],
    overlap: usize,
    lengths: &[(usize, usize)],
) -> Result<(), GibsonError> {
    validate_request(input_count, selected, overlap, 18)?;
    if lengths.len() != selected.len()
        || lengths.iter().zip(selected).any(|(&(f, r), s)| {
            !(18..=40).contains(&f) || !(18..=40).contains(&r) || f + r > s.length
        })
    {
        return Err(GibsonError::Invalid(
            "invalid or overlapping primer annealing lengths",
        ));
    }
    Ok(())
}

pub(crate) fn core_annotations(
    template: &SequenceRecord,
    selection: &CoreSelection,
    sequence: &str,
) -> Result<Vec<FeatureMapping>, DomainError> {
    let interval = FragmentStrand {
        source_start: selection.start,
        length: selection.length,
        sequence_5to3: sequence.into(),
    };
    template
        .features()
        .iter()
        .map(|f| {
            map_feature(
                f,
                &interval,
                Topology::Linear,
                template.sequence().len(),
                selection.orientation == Orientation::Reverse,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|m| m.into_iter().filter(|m| !m.parts.is_empty()).collect())
}

pub fn core(record: &SequenceRecord, selection: &CoreSelection) -> Result<String, GibsonError> {
    let n = record.sequence().len();
    let end = selection
        .start
        .checked_add(selection.length)
        .ok_or(GibsonError::Invalid("source interval overflow"))?;
    if selection.start >= n
        || selection.length > n
        || (record.topology() == Topology::Linear && end > n)
    {
        return Err(GibsonError::Invalid(
            "source interval outside template bounds/topology",
        ));
    }
    let sequence = record.sequence().as_str();
    let core = if end <= n {
        sequence[selection.start..end].to_owned()
    } else {
        format!("{}{}", &sequence[selection.start..], &sequence[..end - n])
    };
    Ok(if selection.orientation == Orientation::Reverse {
        reverse_complement(&core)
    } else {
        core
    })
}

pub(crate) fn primer(annealing: &str, tail: &str) -> PrimerCandidate {
    PrimerCandidate {
        sequence_5to3: format!("{tail}{annealing}"),
        annealing_sequence_5to3: annealing.into(),
        tail_sequence_5to3: tail.into(),
        annealing_gc_bases: annealing
            .bytes()
            .filter(|b| matches!(b, b'G' | b'C'))
            .count(),
    }
}

#[must_use]
pub fn unique_duplex_site(sequence: &str, motif: &str, topology: Topology) -> bool {
    let rc = reverse_complement(motif);
    if rc == motif || motif.len() > sequence.len() {
        return false;
    }
    let extended = if topology == Topology::Circular {
        format!("{sequence}{}", &sequence[..motif.len() - 1])
    } else {
        sequence.into()
    };
    extended
        .as_bytes()
        .windows(motif.len())
        .filter(|w| *w == motif.as_bytes() || *w == rc.as_bytes())
        .take(2)
        .count()
        == 1
}

fn verify_assembly(
    components: &[GibsonComponent],
    junctions: &[GibsonJunction],
    expected: &str,
) -> Result<(), GibsonError> {
    let mut assembled = components[0].pcr_product_sequence_5to3.clone();
    for junction in junctions {
        let overlap = &junction.overlap_sequence_5to3;
        let next = &components[junction.before_component - 1].pcr_product_sequence_5to3;
        if !assembled.ends_with(overlap) || !next.starts_with(overlap) {
            return Err(GibsonError::Invalid("PCR-product junction mismatch"));
        }
        if junction.closure {
            assembled.truncate(assembled.len() - overlap.len());
        } else {
            assembled.push_str(&next[overlap.len()..]);
        }
    }
    if assembled != expected {
        return Err(GibsonError::Invalid(
            "PCR-product assembly does not conserve requested cores",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DnaSeq;
    fn record() -> SequenceRecord {
        // Deterministic synthetic original, not a biological reference sequence.
        let mut state = 1_234_567_u64;
        let sequence: String = (0..300)
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
                start: 270,
                length: 90,
                orientation: Orientation::Forward,
            },
            CoreSelection {
                input: 1,
                start: 90,
                length: 100,
                orientation: Orientation::Reverse,
            },
        ]
    }
    #[test]
    fn circular_origin_reverse_core_and_pcr_tails_reconstruct_product() {
        let report = design(&[record()], &selections(), Topology::Circular, 25, 22).unwrap();
        assert_eq!(report.product_sequence_5to3.len(), 190);
        assert_eq!(report.junctions.len(), 2);
        assert!(report.junctions[1].closure);
        assert!(
            report.components[0]
                .forward_primer
                .tail_sequence_5to3
                .is_empty()
        );
        assert_eq!(
            report.components[0].reverse_primer.tail_sequence_5to3,
            reverse_complement(&report.components[1].core_sequence_5to3[..25])
        );
    }
    #[test]
    fn linear_last_primer_has_no_tail_and_source_errors_fail() {
        let r = record();
        let report = design(
            std::slice::from_ref(&r),
            &selections(),
            Topology::Linear,
            25,
            22,
        )
        .unwrap();
        assert!(
            report.components[1]
                .reverse_primer
                .tail_sequence_5to3
                .is_empty()
        );
        assert_eq!(report.junctions.len(), 1);
        let mut bad = selections();
        bad[0].start = usize::MAX;
        assert!(design(std::slice::from_ref(&r), &bad, Topology::Linear, 25, 22).is_err());
        assert!(design(&[r], &selections(), Topology::Linear, 19, 22).is_err());
    }
    #[test]
    fn resource_and_template_bounds_are_explicit() {
        let mut selected = selections();
        assert!(validate_request(17, &selected, 25, 22).is_err());
        assert!(validate_request(0, &selected, 25, 22).is_err());
        assert!(validate_request(1, &vec![selected[0].clone(); 129], 25, 22).is_err());
        selected.truncate(1);
        selected[0].length = MAX_BASES;
        assert!(validate_request(1, &selected, 60, 40).is_ok());
        selected[0].length += 1;
        assert!(validate_request(1, &selected, 60, 40).is_err());
        selected[0].length = usize::MAX;
        assert!(validate_request(1, &selected, 60, 40).is_err());
        let mut ambiguous = record().sequence().as_str().to_owned();
        ambiguous.replace_range(0..1, "N");
        let r = SequenceRecord::new(
            "synthetic",
            DnaSeq::new(ambiguous).unwrap(),
            Topology::Circular,
            vec![],
            vec![],
        )
        .unwrap();
        assert!(design(&[r], &selections(), Topology::Circular, 25, 22).is_err());
    }

    #[test]
    fn exact_uniqueness_includes_opposite_strand_and_origin_crossings() {
        assert!(!unique_duplex_site(
            "AACGTGGACGTT",
            "AACGT",
            Topology::Linear
        ));
        assert!(unique_duplex_site("ACGTTCGGA", "GGAAC", Topology::Circular));
        assert!(!unique_duplex_site("ACGTTCGGA", "GGAAC", Topology::Linear));
    }

    #[test]
    fn repeated_primers_overlaps_and_palindromes_are_rejected() {
        assert!(!unique_duplex_site("AAAAAA", "AAA", Topology::Linear));
        assert!(!unique_duplex_site("AAGCTT", "AAGCTT", Topology::Linear));
        assert!(unique_duplex_site("GACCTAGT", "TGA", Topology::Circular));
        let s = selections()[0].clone();
        assert!(design(&[record()], &[s.clone(), s], Topology::Circular, 25, 22).is_err());
    }
}

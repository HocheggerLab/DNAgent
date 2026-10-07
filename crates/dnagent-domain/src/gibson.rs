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

/// How a fragment reaches the reaction. A `Provided` fragment (restriction digest,
/// synthesis, a stock linear DNA) is used as it is: it gets no primers, and the homology
/// at its junctions is written into the neighbouring PCR fragments' primers instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preparation {
    Pcr,
    Provided,
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
    pub preparation: Preparation,
    pub product_start: usize,
    pub core_sequence_5to3: String,
    /// The fragment as it enters the reaction: the core plus any primer tails for a PCR
    /// fragment, the core itself for a provided one.
    pub fragment_sequence_5to3: String,
    /// Absent for provided fragments, which are not amplified.
    pub forward_primer: Option<PrimerCandidate>,
    pub reverse_primer: Option<PrimerCandidate>,
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
    /// Which primer carries this overlap as a 5-prime tail: the upstream fragment's
    /// reverse primer, or the downstream fragment's forward primer when the upstream
    /// fragment is provided and so cannot be extended. `None` when the fragments already
    /// overlap and no primer adds anything.
    pub added_by: Option<TailCarrier>,
}

/// The primer that writes a junction's overlap into its fragment (1-based component).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "primer", content = "component", rename_all = "snake_case")]
pub enum TailCarrier {
    ReverseOf(usize),
    ForwardOf(usize),
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
    preparation: &[Preparation],
    topology: Topology,
    overlap: usize,
    annealing: usize,
) -> Result<GibsonReport, GibsonError> {
    validate_request(records.len(), selected, overlap, annealing)?;
    let mut report = design_with_lengths(
        records,
        selected,
        preparation,
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

/// Plan each junction: which sequence is the shared overlap, and which primer writes it.
/// A fragment's forward tail can be decided while visiting the fragment before it, so this
/// runs before any primer is built.
fn plan_junctions(
    cores: &[String],
    preparation: &[Preparation],
    starts: &[usize],
    product_sequence_5to3: &str,
    topology: Topology,
    overlap: usize,
) -> Result<Vec<GibsonJunction>, GibsonError> {
    let mut junctions: Vec<GibsonJunction> = Vec::new();
    for i in 0..cores.len() {
        let closure = i + 1 == cores.len();
        if closure && topology != Topology::Circular {
            continue;
        }
        let next = (i + 1) % cores.len();
        let (overlap_sequence_5to3, added_by, product_start) = match (
            preparation[i],
            preparation[next],
        ) {
            // A provided fragment cannot be extended, so its neighbour's forward
            // primer copies the provided fragment's own 3-prime end.
            (Preparation::Provided, Preparation::Pcr) => (
                cores[i][cores[i].len() - overlap..].to_owned(),
                TailCarrier::ForwardOf(next + 1),
                starts[i] + cores[i].len() - overlap,
            ),
            (Preparation::Provided, Preparation::Provided) => {
                return Err(GibsonError::Component {
                    component: i + 1,
                    reason: "both fragments at this junction are provided, so no primer can add the overlap; give them existing overlaps instead",
                });
            }
            // Otherwise keep the overlap in the downstream fragment's 5-prime end.
            (Preparation::Pcr, _) => (
                cores[next][..overlap].to_owned(),
                TailCarrier::ReverseOf(i + 1),
                if closure { 0 } else { starts[next] },
            ),
        };
        if !unique_duplex_site(product_sequence_5to3, &overlap_sequence_5to3, topology) {
            return Err(GibsonError::Component {
                component: i + 1,
                reason: "overlap is repeated or reverse-complement ambiguous in the intended product",
            });
        }
        junctions.push(GibsonJunction {
            after_component: i + 1,
            before_component: next + 1,
            closure,
            product_start,
            overlap_sequence_5to3,
            added_by: Some(added_by),
        });
    }
    Ok(junctions)
}

pub(crate) fn design_with_lengths(
    records: &[SequenceRecord],
    selected: &[CoreSelection],
    preparation: &[Preparation],
    topology: Topology,
    overlap: usize,
    lengths: &[(usize, usize)],
) -> Result<GibsonReport, GibsonError> {
    validate_preparation(selected, preparation, overlap)?;
    validate_lengths(records.len(), selected, overlap, lengths, preparation)?;
    validate_templates(records)?;
    let cores = selected
        .iter()
        .map(|s| core(&records[s.input - 1], s))
        .collect::<Result<Vec<_>, _>>()?;
    let product_sequence_5to3 = cores.concat();
    let mut starts = Vec::with_capacity(cores.len());
    let mut at = 0usize;
    for sequence in &cores {
        starts.push(at);
        at += sequence.len();
    }

    for (i, (selection, sequence)) in selected.iter().zip(&cores).enumerate() {
        if preparation[i] == Preparation::Provided {
            continue;
        }
        let template = &records[selection.input - 1];
        let reverse_anneal = reverse_complement(&sequence[sequence.len() - lengths[i].1..]);
        for annealing in [&sequence[..lengths[i].0], reverse_anneal.as_str()] {
            if !unique_duplex_site(template.sequence().as_str(), annealing, template.topology()) {
                return Err(GibsonError::Component {
                    component: i + 1,
                    reason: "primer annealing sequence lacks a unique exact site on its full template",
                });
            }
        }
    }

    let junctions = plan_junctions(
        &cores,
        preparation,
        &starts,
        &product_sequence_5to3,
        topology,
        overlap,
    )?;

    let mut components = Vec::new();
    for (i, (selection, sequence)) in selected.iter().zip(&cores).enumerate() {
        let template = &records[selection.input - 1];
        let tail_of = |want: TailCarrier| -> &str {
            junctions
                .iter()
                .find(|j| match (j.added_by, Some(want)) {
                    (Some(TailCarrier::ReverseOf(a)), Some(TailCarrier::ReverseOf(b)))
                    | (Some(TailCarrier::ForwardOf(a)), Some(TailCarrier::ForwardOf(b))) => a == b,
                    _ => false,
                })
                .map_or("", |j| j.overlap_sequence_5to3.as_str())
        };
        let forward_tail = tail_of(TailCarrier::ForwardOf(i + 1));
        let reverse_tail = tail_of(TailCarrier::ReverseOf(i + 1));
        let annotations = core_annotations(template, selection, sequence)?;
        let (fragment_sequence_5to3, forward_primer, reverse_primer) = match preparation[i] {
            Preparation::Provided => (sequence.clone(), None, None),
            Preparation::Pcr => {
                let forward_anneal = &sequence[..lengths[i].0];
                let reverse_anneal = reverse_complement(&sequence[sequence.len() - lengths[i].1..]);
                (
                    format!("{forward_tail}{sequence}{reverse_tail}"),
                    Some(primer(forward_anneal, forward_tail)),
                    Some(primer(&reverse_anneal, &reverse_complement(reverse_tail))),
                )
            }
        };
        components.push(GibsonComponent {
            selection: selection.clone(),
            preparation: preparation[i],
            product_start: starts[i],
            core_sequence_5to3: sequence.clone(),
            fragment_sequence_5to3,
            forward_primer,
            reverse_primer,
            annotations,
        });
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
            "Each junction's overlap is written by one primer: the upstream fragment's reverse primer, or the downstream fragment's forward primer where the upstream fragment is provided rather than amplified",
            "Provided fragments (restriction digest, synthesis, stock linear DNA) receive no primers; their ends are used exactly as given and are not verified against any digest here",
            "Overlaps must occur exactly once in the intended product across both orientations, including circular-origin matches",
            "Assumes clean sequence-faithful PCR and ideal overlap-directed assembly; reaction yield, enzyme conditions and experimental validity are not predicted",
            "Core-local annotations map through product_start; feature reunion/fusion, translations and biological function are not inferred",
            "Synthetic overlap copies are removed exactly once at each planned junction; final core bases remain unchanged",
            "Source primers are provenance only, not reused binding-site predictions; opaque SnapGene metadata is not exported",
            "Linear single-core designs are PCR-only, without an assembly junction; circular single-core designs model PCR-assisted reclosure",
        ],
    })
}

/// A provided fragment is not amplified, so it must already carry any overlap taken from
/// its own ends: up to one at each junction.
fn validate_preparation(
    selected: &[CoreSelection],
    preparation: &[Preparation],
    overlap: usize,
) -> Result<(), GibsonError> {
    if preparation.len() != selected.len() {
        return Err(GibsonError::Invalid(
            "every core needs an explicit preparation",
        ));
    }
    for (i, selection) in selected.iter().enumerate() {
        if preparation[i] == Preparation::Provided && selection.length < 2 * overlap {
            return Err(GibsonError::Component {
                component: i + 1,
                reason: "a provided fragment must be at least twice the overlap length",
            });
        }
    }
    Ok(())
}

fn validate_lengths(
    input_count: usize,
    selected: &[CoreSelection],
    overlap: usize,
    lengths: &[(usize, usize)],
    preparation: &[Preparation],
) -> Result<(), GibsonError> {
    validate_request(input_count, selected, overlap, 18)?;
    if lengths.len() != selected.len()
        || lengths
            .iter()
            .zip(selected)
            .zip(preparation)
            .filter(|(_, p)| **p == Preparation::Pcr)
            .any(|((&(f, r), s), _)| {
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
    let mut assembled = components[0].fragment_sequence_5to3.clone();
    for junction in junctions {
        let overlap = &junction.overlap_sequence_5to3;
        let next = &components[junction.before_component - 1].fragment_sequence_5to3;
        if !assembled.ends_with(overlap) || !next.starts_with(overlap) {
            return Err(GibsonError::Invalid("fragment junction mismatch"));
        }
        if junction.closure {
            assembled.truncate(assembled.len() - overlap.len());
        } else {
            assembled.push_str(&next[overlap.len()..]);
        }
    }
    if assembled != expected {
        return Err(GibsonError::Invalid(
            "fragment assembly does not conserve requested cores",
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
        let report = design(
            &[record()],
            &selections(),
            &[Preparation::Pcr; 2],
            Topology::Circular,
            25,
            22,
        )
        .unwrap();
        assert_eq!(report.product_sequence_5to3.len(), 190);
        assert_eq!(report.junctions.len(), 2);
        assert!(report.junctions[1].closure);
        assert_eq!(
            report.components[0]
                .forward_primer
                .as_ref()
                .unwrap()
                .tail_sequence_5to3,
            ""
        );
        assert_eq!(
            report.components[0]
                .reverse_primer
                .as_ref()
                .unwrap()
                .tail_sequence_5to3,
            reverse_complement(&report.components[1].core_sequence_5to3[..25])
        );
    }
    #[test]
    fn linear_last_primer_has_no_tail_and_source_errors_fail() {
        let r = record();
        let report = design(
            std::slice::from_ref(&r),
            &selections(),
            &[Preparation::Pcr; 2],
            Topology::Linear,
            25,
            22,
        )
        .unwrap();
        assert_eq!(
            report.components[1]
                .reverse_primer
                .as_ref()
                .unwrap()
                .tail_sequence_5to3,
            ""
        );
        assert_eq!(report.junctions.len(), 1);
        let mut bad = selections();
        bad[0].start = usize::MAX;
        assert!(
            design(
                std::slice::from_ref(&r),
                &bad,
                &[Preparation::Pcr; 2],
                Topology::Linear,
                25,
                22
            )
            .is_err()
        );
        assert!(
            design(
                &[r],
                &selections(),
                &[Preparation::Pcr; 2],
                Topology::Linear,
                19,
                22
            )
            .is_err()
        );
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
        assert!(
            design(
                &[r],
                &selections(),
                &[Preparation::Pcr; 2],
                Topology::Circular,
                25,
                22
            )
            .is_err()
        );
    }

    /// The usual bench design: the vector is cut and used as it is, so both overlaps are
    /// written into the insert's primers and the vector gets none.
    #[test]
    fn a_provided_vector_puts_both_tails_on_the_amplified_insert() {
        let report = design(
            &[record()],
            &selections(),
            &[Preparation::Provided, Preparation::Pcr],
            Topology::Circular,
            25,
            22,
        )
        .unwrap();
        let (vector, insert) = (&report.components[0], &report.components[1]);
        assert!(vector.forward_primer.is_none() && vector.reverse_primer.is_none());
        assert_eq!(vector.fragment_sequence_5to3, vector.core_sequence_5to3);

        let forward = insert.forward_primer.as_ref().unwrap();
        let reverse = insert.reverse_primer.as_ref().unwrap();
        assert_eq!(forward.tail_sequence_5to3.len(), 25);
        assert_eq!(reverse.tail_sequence_5to3.len(), 25);
        // The insert copies the provided vector's own ends: its 3-prime end before the
        // insert, and its 5-prime start after it.
        assert_eq!(
            forward.tail_sequence_5to3,
            vector.core_sequence_5to3[vector.core_sequence_5to3.len() - 25..]
        );
        assert_eq!(
            reverse.tail_sequence_5to3,
            reverse_complement(&vector.core_sequence_5to3[..25])
        );
        assert!(matches!(
            report.junctions[0].added_by,
            Some(TailCarrier::ForwardOf(2))
        ));
        assert!(matches!(
            report.junctions[1].added_by,
            Some(TailCarrier::ReverseOf(2))
        ));
    }

    #[test]
    fn two_provided_fragments_cannot_form_a_junction() {
        let error = design(
            &[record()],
            &selections(),
            &[Preparation::Provided; 2],
            Topology::Circular,
            25,
            22,
        )
        .unwrap_err();
        assert!(format!("{error}").contains("both fragments at this junction are provided"));
    }

    #[test]
    fn preparation_must_cover_every_core() {
        assert!(
            design(
                &[record()],
                &selections(),
                &[Preparation::Pcr],
                Topology::Circular,
                25,
                22
            )
            .is_err()
        );
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
        assert!(
            design(
                &[record()],
                &[s.clone(), s],
                &[Preparation::Pcr; 2],
                Topology::Circular,
                25,
                22
            )
            .is_err()
        );
    }
}

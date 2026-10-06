//! Deterministic bounded optimisation: NN annealing Tm and sequence-only structure screens.
//! Numerical parameter provenance and scope: docs/primer-optimisation.md.
use crate::digest::reverse_complement;
use crate::gibson::{
    CoreSelection, GibsonError, GibsonReport, Preparation, PrimerCandidate, core,
    design_with_lengths, primer, unique_duplex_site, validate_request, validate_templates,
};
use crate::{SequenceRecord, Topology};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Solution {
    pub sodium_mm: f64,
    pub potassium_mm: f64,
    pub tris_mm: f64,
    pub magnesium_mm: f64,
    pub dntp_mm: f64,
    pub primer_nm: f64,
}
impl Solution {
    fn equivalent_sodium(&self) -> f64 {
        self.sodium_mm
            + self.potassium_mm
            + self.tris_mm / 2.0
            + 120.0 * (self.magnesium_mm - self.dntp_mm).max(0.0).sqrt()
    }
    fn validate(&self) -> bool {
        [
            (self.sodium_mm, 500.0),
            (self.potassium_mm, 500.0),
            (self.tris_mm, 200.0),
            (self.magnesium_mm, 10.0),
            (self.dntp_mm, 10.0),
        ]
        .iter()
        .all(|(v, max)| v.is_finite() && (0.0..=*max).contains(v))
            && self.primer_nm.is_finite()
            && (1.0..=5000.0).contains(&self.primer_nm)
            && self.equivalent_sodium() > 0.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimerConstraints {
    pub min_length: usize,
    pub max_length: usize,
    pub min_tm_c: f64,
    pub max_tm_c: f64,
    pub target_tm_c: f64,
    pub max_pair_tm_difference_c: f64,
    pub min_gc_fraction: f64,
    pub max_gc_fraction: f64,
    pub max_hairpin_stem: usize,
    pub max_dimer_run: usize,
    pub max_three_prime_run: usize,
    pub solution: Solution,
}
impl PrimerConstraints {
    pub fn validate(&self) -> Result<(), GibsonError> {
        if !(18..=40).contains(&self.min_length)
            || !(self.min_length..=40).contains(&self.max_length)
            || !self.solution.validate()
            || ![
                self.min_tm_c,
                self.max_tm_c,
                self.target_tm_c,
                self.max_pair_tm_difference_c,
                self.min_gc_fraction,
                self.max_gc_fraction,
            ]
            .iter()
            .all(|x| x.is_finite())
            || !(0.0..=100.0).contains(&self.min_tm_c)
            || !(self.min_tm_c..=100.0).contains(&self.max_tm_c)
            || !(self.min_tm_c..=self.max_tm_c).contains(&self.target_tm_c)
            || !(0.0..=20.0).contains(&self.max_pair_tm_difference_c)
            || !(0.0..=1.0).contains(&self.min_gc_fraction)
            || !(self.min_gc_fraction..=1.0).contains(&self.max_gc_fraction)
            || self.max_hairpin_stem > 20
            || self.max_dimer_run > 40
            || self.max_three_prime_run > 20
        {
            return Err(GibsonError::Invalid(
                "invalid primer optimisation constraints or solution concentrations",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DimerScreen {
    pub longest_run: usize,
    pub longest_three_prime_run: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct AssessedPrimer {
    pub primer: PrimerCandidate,
    pub annealing_tm_c: f64,
    pub hairpin_stem: usize,
    pub self_dimer: DimerScreen,
}
#[derive(Debug, Serialize)]
pub struct OptimisedPair {
    /// 1-based component this pair amplifies; provided fragments have no pair.
    pub component: usize,
    pub forward: AssessedPrimer,
    pub reverse: AssessedPrimer,
    pub heterodimer: DimerScreen,
    pub score: f64,
    pub candidate_pairs_examined: usize,
    pub feasible_pairs: usize,
}
#[derive(Debug, Serialize)]
pub struct OptimisedGibson {
    pub design: GibsonReport,
    pub constraints: PrimerConstraints,
    pub pairs: Vec<OptimisedPair>,
    pub assumptions: Vec<&'static str>,
}

pub fn optimise(
    records: &[SequenceRecord],
    selected: &[CoreSelection],
    preparation: &[Preparation],
    topology: Topology,
    overlap: usize,
    constraints: &PrimerConstraints,
) -> Result<OptimisedGibson, GibsonError> {
    constraints.validate()?;
    validate_request(records.len(), selected, overlap, constraints.min_length)?;
    validate_templates(records)?;
    let cores = selected
        .iter()
        .map(|s| core(&records[s.input - 1], s))
        .collect::<Result<Vec<_>, _>>()?;
    // Tails follow the same rule as the design: the upstream fragment's reverse primer
    // carries the overlap, unless that fragment is provided and so cannot be extended,
    // in which case the downstream fragment's forward primer does.
    let joined = |i: usize| i + 1 < selected.len() || topology == Topology::Circular;
    let mut forward_tails = vec![String::new(); selected.len()];
    let mut reverse_tails = vec![String::new(); selected.len()];
    for i in 0..selected.len() {
        if !joined(i) {
            continue;
        }
        let next = (i + 1) % selected.len();
        match (preparation[i], preparation[next]) {
            (Preparation::Provided, Preparation::Pcr) => {
                forward_tails[next].clone_from(&cores[i][cores[i].len() - overlap..].to_owned());
            }
            (Preparation::Provided, Preparation::Provided) => {
                return Err(GibsonError::Component {
                    component: i + 1,
                    reason: "both fragments at this junction are provided, so no primer can add the overlap; give them existing overlaps instead",
                });
            }
            (Preparation::Pcr, _) => {
                reverse_tails[i] = reverse_complement(&cores[next][..overlap]);
            }
        }
    }
    let mut pairs = Vec::new();
    let mut lengths = Vec::with_capacity(selected.len());
    for (i, (selection, sequence)) in selected.iter().zip(&cores).enumerate() {
        if preparation[i] == Preparation::Provided {
            // Not amplified: no primers to choose, and no annealing lengths to report.
            lengths.push((0, 0));
            continue;
        }
        let template = &records[selection.input - 1];
        let forward = candidates(sequence, &forward_tails[i], false, template, constraints)?;
        let reverse = candidates(sequence, &reverse_tails[i], true, template, constraints)?;
        let pair = choose_pair(&forward, &reverse, sequence.len(), constraints).ok_or(GibsonError::Component { component: i + 1, reason: "no primer pair satisfies exact-site, Tm, GC and sequence-structure constraints; no fallback design returned" })?;
        lengths.push((
            pair.forward.primer.annealing_sequence_5to3.len(),
            pair.reverse.primer.annealing_sequence_5to3.len(),
        ));
        pairs.push(OptimisedPair {
            component: i + 1,
            ..pair
        });
    }
    let design = design_with_lengths(records, selected, preparation, topology, overlap, &lengths)?;
    Ok(OptimisedGibson {
        design,
        constraints: constraints.clone(),
        pairs,
        assumptions: vec![
            "Annealing Tm: Allawi/SantaLucia 1997 DNA NN parameters, SantaLucia 1998 entropy salt correction, von Ahsen 2001 sodium-equivalent Mg/dNTP approximation; primer concentration in excess, complementary template concentration neglected",
            "Tm describes the template-matching annealing segment, not the full tailed oligo; no recommended PCR annealing temperature is inferred",
            "Hairpin and dimer checks use full oligos but only contiguous Watson-Crick sequence complementarity, not structure energies, bulges, mismatches or temperature-dependent folding",
            "Hairpin stems require at least a three-base unpaired loop; dimer screens report all contiguous runs and runs reaching either oligo's 3-prime end",
            "Each component primer pair is optimised separately for separate PCR reactions; cross-component multiplex primer interactions are not evaluated",
            "Among feasible pairs minimise summed absolute deviations from target Tm; ties prefer shorter total annealing length, then shorter forward length, then reverse length",
            "Exact annealing-site uniqueness is tested on the full source in both orientations; off-template and mismatched binding are not evaluated",
            "Thermodynamic estimates and sequence screens do not establish ordering readiness or experimental validation; bounds are implementation constraints, not calibration guarantees",
        ],
    })
}

fn candidates(
    sequence: &str,
    tail: &str,
    reverse: bool,
    template: &SequenceRecord,
    c: &PrimerConstraints,
) -> Result<Vec<AssessedPrimer>, GibsonError> {
    let mut result = Vec::new();
    for length in c.min_length..=c.max_length.min(sequence.len()) {
        let anneal = if reverse {
            reverse_complement(&sequence[sequence.len() - length..])
        } else {
            sequence[..length].into()
        };
        if !unique_duplex_site(template.sequence().as_str(), &anneal, template.topology()) {
            continue;
        }
        let p = primer(&anneal, tail);
        let tm = annealing_tm(&anneal, &c.solution)?;
        let gc = f64::from(u32::try_from(p.annealing_gc_bases).expect("bounded oligo"))
            / f64::from(u32::try_from(length).expect("bounded oligo"));
        let hairpin = hairpin_stem(&p.sequence_5to3);
        let dimer = dimer_screen(&p.sequence_5to3, &p.sequence_5to3);
        if (c.min_tm_c..=c.max_tm_c).contains(&tm)
            && (c.min_gc_fraction..=c.max_gc_fraction).contains(&gc)
            && hairpin <= c.max_hairpin_stem
            && passes(dimer, c)
        {
            result.push(AssessedPrimer {
                primer: p,
                annealing_tm_c: tm,
                hairpin_stem: hairpin,
                self_dimer: dimer,
            });
        }
    }
    Ok(result)
}
fn passes(d: DimerScreen, c: &PrimerConstraints) -> bool {
    d.longest_run <= c.max_dimer_run && d.longest_three_prime_run <= c.max_three_prime_run
}

fn choose_pair(
    forward: &[AssessedPrimer],
    reverse: &[AssessedPrimer],
    length: usize,
    c: &PrimerConstraints,
) -> Option<OptimisedPair> {
    let mut best: Option<OptimisedPair> = None;
    let mut examined = 0;
    let mut feasible = 0;
    for f in forward {
        for r in reverse {
            examined += 1;
            let fl = f.primer.annealing_sequence_5to3.len();
            let rl = r.primer.annealing_sequence_5to3.len();
            let dimer = dimer_screen(&f.primer.sequence_5to3, &r.primer.sequence_5to3);
            if fl + rl > length
                || (f.annealing_tm_c - r.annealing_tm_c).abs() > c.max_pair_tm_difference_c
                || !passes(dimer, c)
            {
                continue;
            }
            feasible += 1;
            let score =
                (f.annealing_tm_c - c.target_tm_c).abs() + (r.annealing_tm_c - c.target_tm_c).abs();
            let key = (score, fl + rl, fl, rl);
            if best.as_ref().is_none_or(|b| {
                let bf = b.forward.primer.annealing_sequence_5to3.len();
                let br = b.reverse.primer.annealing_sequence_5to3.len();
                key < (b.score, bf + br, bf, br)
            }) {
                best = Some(OptimisedPair {
                    component: 0, // set by the caller, which knows the component
                    forward: f.clone(),
                    reverse: r.clone(),
                    heterodimer: dimer,
                    score,
                    candidate_pairs_examined: 0,
                    feasible_pairs: 0,
                });
            }
        }
    }
    best.map(|mut b| {
        b.candidate_pairs_examined = examined;
        b.feasible_pairs = feasible;
        b
    })
}

/// Exact-match, non-self-complementary annealing duplex model. Concentrations are explicit.
pub fn annealing_tm(sequence: &str, solution: &Solution) -> Result<f64, GibsonError> {
    if !solution.validate()
        || !(18..=40).contains(&sequence.len())
        || !sequence.bytes().all(|b| b"ACGT".contains(&b))
        || reverse_complement(sequence) == sequence
    {
        return Err(GibsonError::Invalid(
            "Tm requires 18..=40 unambiguous non-self-complementary bases and valid solution conditions",
        ));
    }
    let (mut h, mut s) = (0.0, 0.0);
    for b in [
        sequence.as_bytes()[0],
        sequence.as_bytes()[sequence.len() - 1],
    ] {
        let (dh, ds) = if matches!(b, b'A' | b'T') {
            (2.3, 4.1)
        } else {
            (0.1, -2.8)
        };
        h += dh;
        s += ds;
    }
    for pair in sequence.as_bytes().windows(2) {
        let (dh, ds) = match pair {
            b"AA" | b"TT" => (-7.9, -22.2),
            b"AT" => (-7.2, -20.4),
            b"TA" => (-7.2, -21.3),
            b"CA" | b"TG" => (-8.5, -22.7),
            b"GT" | b"AC" => (-8.4, -22.4),
            b"CT" | b"AG" => (-7.8, -21.0),
            b"GA" | b"TC" => (-8.2, -22.2),
            b"CG" => (-10.6, -27.2),
            b"GC" => (-9.8, -24.4),
            b"GG" | b"CC" => (-8.0, -19.9),
            _ => unreachable!("validated ACGT"),
        };
        h += dh;
        s += ds;
    }
    s += 0.368
        * f64::from(u32::try_from(sequence.len() - 1).expect("bounded oligo"))
        * (solution.equivalent_sodium() * 1e-3).ln();
    Ok(1000.0 * h / (s + 1.987 * (solution.primer_nm * 1e-9).ln()) - 273.15)
}

// Private screens only receive bounded, validated ACGT oligos.
pub(crate) fn dimer_screen(a: &str, b: &str) -> DimerScreen {
    let b = reverse_complement(b);
    let mut previous = vec![0; b.len() + 1];
    let mut result = DimerScreen {
        longest_run: 0,
        longest_three_prime_run: 0,
    };
    for (i, base) in a.bytes().enumerate() {
        let mut row = vec![0; b.len() + 1];
        for (j, other) in b.bytes().enumerate() {
            if base == other {
                let run = previous[j] + 1;
                row[j + 1] = run;
                result.longest_run = result.longest_run.max(run);
                if i + 1 == a.len() || j + 1 == run {
                    result.longest_three_prime_run = result.longest_three_prime_run.max(run);
                }
            }
        }
        previous = row;
    }
    result
}
pub(crate) fn hairpin_stem(s: &str) -> usize {
    let rc = reverse_complement(s);
    let mut best = 0;
    for left in 0..s.len() {
        for right in left + 4..s.len() {
            let mut k = 0;
            while left + k + 3 < right - k
                && s.as_bytes()[left + k] == rc.as_bytes()[s.len() - 1 - right + k]
            {
                k += 1;
            }
            best = best.max(k);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    fn solution() -> Solution {
        Solution {
            sodium_mm: 50.0,
            potassium_mm: 0.0,
            tris_mm: 0.0,
            magnesium_mm: 1.5,
            dntp_mm: 0.2,
            primer_nm: 250.0,
        }
    }
    #[test]
    fn nn_tm_matches_pinned_biopython_reference_and_rc() {
        let seq = "ACGTTGCAATGCCGTAGCTAGC";
        let tm = annealing_tm(seq, &solution()).unwrap();
        assert!((tm - 68.544_083_450_990_97).abs() < 1e-10);
        assert!((tm - annealing_tm(&reverse_complement(seq), &solution()).unwrap()).abs() < 1e-10);
        assert!(annealing_tm("ACGT", &solution()).is_err());
        assert!(annealing_tm("ACGTTGCAATGCCGTAGCTAGN", &solution()).is_err());
        let mut invalid = solution();
        invalid.primer_nm = f64::NAN;
        assert!(annealing_tm(seq, &invalid).is_err());
        invalid = solution();
        invalid.sodium_mm = 0.0;
        invalid.magnesium_mm = 0.0;
        assert!(annealing_tm(seq, &invalid).is_err());
    }
    #[test]
    fn a_longer_primer_can_resolve_a_repeated_minimum_length_site() {
        use crate::{DnaSeq, ligation::Orientation};
        let mut state = 1_234_567_u64;
        let mut bases: Vec<_> = (0..180)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                b"ACGT"[(state % 4) as usize]
            })
            .collect();
        bases.copy_within(0..18, 120);
        bases[138] = if bases[18] == b'A' { b'C' } else { b'A' };
        let record = SequenceRecord::new(
            "synthetic",
            DnaSeq::new(String::from_utf8(bases).unwrap()).unwrap(),
            Topology::Linear,
            vec![],
            vec![],
        )
        .unwrap();
        let c = PrimerConstraints {
            min_length: 18,
            max_length: 19,
            min_tm_c: 0.0,
            max_tm_c: 100.0,
            target_tm_c: 60.0,
            max_pair_tm_difference_c: 20.0,
            min_gc_fraction: 0.0,
            max_gc_fraction: 1.0,
            max_hairpin_stem: 20,
            max_dimer_run: 40,
            max_three_prime_run: 20,
            solution: solution(),
        };
        let selected = [CoreSelection {
            input: 1,
            start: 0,
            length: 90,
            orientation: Orientation::Forward,
        }];
        assert!(!unique_duplex_site(
            record.sequence().as_str(),
            &record.sequence().as_str()[..18],
            Topology::Linear
        ));
        let result = optimise(
            &[record],
            &selected,
            &[Preparation::Pcr],
            Topology::Linear,
            25,
            &c,
        )
        .unwrap();
        assert_eq!(
            result.pairs[0].forward.primer.annealing_sequence_5to3.len(),
            19
        );
    }

    #[test]
    fn structure_screens_distinguish_internal_and_three_prime_runs() {
        assert_eq!(
            dimer_screen("AAAAA", "TTTTT"),
            DimerScreen {
                longest_run: 5,
                longest_three_prime_run: 5
            }
        );
        assert_eq!(
            dimer_screen("GAAAAC", "ATTTTG"),
            DimerScreen {
                longest_run: 4,
                longest_three_prime_run: 1
            }
        );
        assert_eq!(
            dimer_screen("GAAAAC", "ATTTTG"),
            dimer_screen("ATTTTG", "GAAAAC")
        );
        assert_eq!(hairpin_stem("GCGAAACGC"), 3);
        assert_eq!(hairpin_stem("GCGCGC"), 1);
        assert_eq!(hairpin_stem("AAAAAAAAAA"), 0);
    }
}

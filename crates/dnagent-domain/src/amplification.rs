//! Bounded offline amplification design. Reference coordinates are zero-based;
//! input identities are one-based. No filesystem, external tools or inferred assays.
#[cfg(test)]
#[path = "amplification_tests.rs"]
mod tests;
use crate::digest::reverse_complement;
use crate::primer_optimisation::{
    DimerScreen, PrimerConstraints, annealing_tm, dimer_screen, hairpin_stem,
};
use crate::{SequenceRecord, Topology};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DesignError {
    #[error("invalid amplification request: {0}")]
    Invalid(&'static str),
    #[error(
        "amplification search exceeds explicit implementation budget; narrow the window/length range or reduce templates (no partial design returned)"
    )]
    Budget,
    #[error(
        "no pair satisfies the declared constraints and supplied-template screening; no fallback returned"
    )]
    NoPairs,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub reference_input: usize,
    pub window_start: usize,
    pub window_length: usize,
    /// Boundary relative to the unrolled window. A primer must straddle it.
    pub junction_offset: Option<usize>,
    pub junction_min_bases: usize,
    pub positive_inputs: Vec<usize>,
    pub negative_inputs: Vec<usize>,
    pub min_product_length: usize,
    pub max_product_length: usize,
    pub screen_max_product_length: usize,
    pub max_mismatches: usize,
    pub exact_three_prime_bases: usize,
    pub max_results: usize,
    pub constraints: PrimerConstraints,
}
#[derive(Debug, Clone, Serialize)]
pub struct Oligo {
    pub sequence_5to3: String,
    pub reference_start: usize,
    pub length: usize,
    pub reverse: bool,
    pub tm_c: f64,
    pub gc_fraction: f64,
    pub hairpin_stem: usize,
    pub self_dimer: DimerScreen,
}
#[derive(Debug, Clone, Serialize)]
pub struct Site {
    pub primer: usize,
    pub start: usize,
    pub length: usize,
    /// Reverse means extension towards decreasing forward-reference coordinates.
    pub reverse: bool,
    pub mismatches: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct Product {
    pub start: usize,
    pub length: usize,
    pub plus_primer: usize,
    pub minus_primer: usize,
    pub mismatches: usize,
    /// Template interval, not an inferred mismatch-corrected PCR product.
    pub template_sequence_forward: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Screen {
    pub input: usize,
    pub sites: Vec<Site>,
    pub products: Vec<Product>,
}
#[derive(Debug, Serialize)]
pub struct Pair {
    pub forward: Oligo,
    pub reverse: Oligo,
    pub heterodimer: DimerScreen,
    pub score: f64,
    pub screens: Vec<Screen>,
}
#[derive(Debug, Serialize)]
pub struct Design {
    pub request: Request,
    pub pairs: Vec<Pair>,
    pub candidate_pairs_examined: usize,
    pub feasible_pairs: usize,
    pub model: &'static str,
    pub limitations: Vec<&'static str>,
}

impl Request {
    pub fn validate(&self, records: &[SequenceRecord]) -> Result<(), DesignError> {
        let invalid =
            || DesignError::Invalid("invalid bounds, solution, roles, reference or junction");
        self.constraints.validate().map_err(|_| invalid())?;
        if records.is_empty()
            || records.len() > 8
            || !(1..=records.len()).contains(&self.reference_input)
            || self.window_length > 500
            || self.window_length < 36
            || self.min_product_length < 36
            || self.max_product_length < self.min_product_length
            || self.max_product_length > self.window_length
            || self.screen_max_product_length < self.max_product_length
            || self.screen_max_product_length > 5000
            || self.max_mismatches > 2
            || !(4..=self.constraints.min_length).contains(&self.exact_three_prime_bases)
            || !(1..=20).contains(&self.max_results)
            || !(1..=10).contains(&self.junction_min_bases)
            || self.junction_offset.is_some_and(|j| {
                j < self.junction_min_bases
                    || j > self.window_length.saturating_sub(self.junction_min_bases)
            })
        {
            return Err(invalid());
        }
        let mut roles = self.positive_inputs.clone();
        roles.extend(&self.negative_inputs);
        roles.sort_unstable();
        if roles != (1..=records.len()).collect::<Vec<_>>()
            || !self.positive_inputs.contains(&self.reference_input)
        {
            return Err(invalid());
        }
        let reference = &records[self.reference_input - 1];
        if self.window_start >= reference.sequence().len()
            || self.window_length > reference.sequence().len()
            || (reference.topology() == Topology::Linear
                && self.window_start + self.window_length > reference.sequence().len())
        {
            return Err(invalid());
        }
        if records.iter().any(|r| {
            r.sequence().len() > 100_000
                || !r.sequence().as_str().bytes().all(|b| b"ACGT".contains(&b))
        }) {
            return Err(DesignError::Invalid(
                "templates must be unambiguous ACGT, at most 100,000 bases each",
            ));
        }
        Ok(())
    }
}
fn interval(record: &SequenceRecord, start: usize, length: usize) -> String {
    let seq = record.sequence().as_str().as_bytes();
    (0..length)
        .map(|i| char::from(seq[(start + i) % seq.len()]))
        .collect()
}
fn gc(sequence: &str) -> f64 {
    let n = u32::try_from(sequence.len()).expect("bounded primer");
    let count = u32::try_from(
        sequence
            .bytes()
            .filter(|b| matches!(b, b'G' | b'C'))
            .count(),
    )
    .expect("bounded primer");
    f64::from(count) / f64::from(n)
}
fn dimer_ok(screen: DimerScreen, c: &PrimerConstraints) -> bool {
    screen.longest_run <= c.max_dimer_run && screen.longest_three_prime_run <= c.max_three_prime_run
}
struct Candidate {
    oligo: Oligo,
    offset: usize,
    sites: Vec<Vec<Site>>,
}

/// Exhaustive full-oligo Hamming search, with an exact 3′ anchor. Not a seed-only
/// alignment search: bulges, indels and mismatches within the anchor are excluded.
fn sites(
    record: &SequenceRecord,
    oligo: &str,
    request: &Request,
    budget: &mut usize,
) -> Result<Vec<Site>, DesignError> {
    let n = record.sequence().len();
    if oligo.len() > n {
        return Ok(Vec::new());
    }
    let stops = if record.topology() == Topology::Circular {
        n
    } else {
        n - oligo.len() + 1
    };
    let rc = reverse_complement(oligo);
    let mut result = Vec::new();
    for (reverse, motif) in [(false, oligo), (true, rc.as_str())] {
        for start in 0..stops {
            let mut mismatches = 0;
            let mut valid = true;
            for (i, base) in motif.bytes().enumerate() {
                *budget += 1;
                if *budget > 200_000_000 {
                    return Err(DesignError::Budget);
                }
                if base != record.sequence().as_str().as_bytes()[(start + i) % n] {
                    mismatches += 1;
                    let anchored = if reverse {
                        i < request.exact_three_prime_bases
                    } else {
                        i >= oligo.len() - request.exact_three_prime_bases
                    };
                    if anchored || mismatches > request.max_mismatches {
                        valid = false;
                        break;
                    }
                }
            }
            if valid {
                result.push(Site {
                    primer: 0,
                    start,
                    length: oligo.len(),
                    reverse,
                    mismatches,
                });
            }
            if result.len() > 200 {
                return Err(DesignError::Budget);
            }
        }
    }
    Ok(result)
}
fn candidates(
    record: &SequenceRecord,
    records: &[SequenceRecord],
    r: &Request,
    reverse: bool,
    budget: &mut usize,
) -> Result<Vec<Candidate>, DesignError> {
    let mut result = Vec::new();
    let c = &r.constraints;
    for offset in 0..r.window_length {
        for length in c.min_length..=c.max_length {
            if offset + length > r.window_length {
                continue;
            }
            let start = (r.window_start + offset) % record.sequence().len();
            let mut sequence = interval(record, start, length);
            if reverse {
                sequence = reverse_complement(&sequence);
            }
            let fraction = gc(&sequence);
            if !(c.min_gc_fraction..=c.max_gc_fraction).contains(&fraction) {
                continue;
            }
            let Ok(tm) = annealing_tm(&sequence, &c.solution) else {
                continue;
            };
            if !(c.min_tm_c..=c.max_tm_c).contains(&tm) {
                continue;
            }
            let hairpin = hairpin_stem(&sequence);
            let self_dimer = dimer_screen(&sequence, &sequence);
            if hairpin > c.max_hairpin_stem || !dimer_ok(self_dimer, c) {
                continue;
            }
            let found = records
                .iter()
                .map(|template| sites(template, &sequence, r, budget))
                .collect::<Result<Vec<_>, _>>()?;
            // Necessary condition for an exact product on every positive template.
            if r.positive_inputs
                .iter()
                .any(|i| !found[i - 1].iter().any(|s| s.mismatches == 0))
            {
                continue;
            }
            result.push(Candidate {
                offset,
                oligo: Oligo {
                    sequence_5to3: sequence,
                    reference_start: start,
                    length,
                    reverse,
                    tm_c: tm,
                    gc_fraction: fraction,
                    hairpin_stem: hairpin,
                    self_dimer,
                },
                sites: found,
            });
        }
    }
    Ok(result)
}
fn products(
    record: &SequenceRecord,
    sites: &[Site],
    max_length: usize,
    budget: &mut usize,
) -> Result<Vec<Product>, DesignError> {
    let n = record.sequence().len();
    let mut result = Vec::new();
    for plus in sites.iter().filter(|s| !s.reverse) {
        for minus in sites.iter().filter(|s| s.reverse) {
            *budget += 1;
            if *budget > 20_000_000 {
                return Err(DesignError::Budget);
            }
            let distance = if record.topology() == Topology::Circular {
                (minus.start + n - plus.start) % n
            } else if minus.start >= plus.start {
                minus.start - plus.start
            } else {
                continue;
            };
            let length = distance + minus.length;
            // Nonoverlapping sites, one traversal at most. Includes self-primer products.
            if distance < plus.length || length > n || length > max_length {
                continue;
            }
            result.push(Product {
                start: plus.start,
                length,
                plus_primer: plus.primer,
                minus_primer: minus.primer,
                mismatches: plus.mismatches + minus.mismatches,
                template_sequence_forward: interval(record, plus.start, length),
            });
        }
    }
    Ok(result)
}

fn screen_pair(
    records: &[SequenceRecord],
    f: &Candidate,
    r: &Candidate,
    request: &Request,
    budget: &mut usize,
) -> Result<Option<Vec<Screen>>, DesignError> {
    let mut screens = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let mut binding = f.sites[index].clone();
        for s in &mut binding {
            s.primer = 1;
        }
        binding.extend(r.sites[index].iter().cloned().map(|mut s| {
            s.primer = 2;
            s
        }));
        let products = products(record, &binding, request.screen_max_product_length, budget)?;
        let accepted = if request.positive_inputs.contains(&(index + 1)) {
            products.len() == 1
                && products[0].mismatches == 0
                && products[0].plus_primer != products[0].minus_primer
                && (request.min_product_length..=request.max_product_length)
                    .contains(&products[0].length)
        } else {
            products.is_empty()
        };
        if !accepted {
            return Ok(None);
        }
        screens.push(Screen {
            input: index + 1,
            sites: binding,
            products,
        });
    }
    Ok(Some(screens))
}

pub fn design(records: &[SequenceRecord], request: &Request) -> Result<Design, DesignError> {
    request.validate(records)?;
    let reference = &records[request.reference_input - 1];
    let mut budget = 0;
    let forward = candidates(reference, records, request, false, &mut budget)?;
    let reverse = candidates(reference, records, request, true, &mut budget)?;
    if forward.len().saturating_mul(reverse.len()) > 1_000_000 {
        return Err(DesignError::Budget);
    }
    let mut pairs = Vec::new();
    let mut examined = 0;
    let mut feasible = 0;
    let mut product_budget = 0;
    for f in &forward {
        for r in &reverse {
            examined += 1;
            if r.offset < f.offset + f.oligo.length {
                continue;
            }
            let length = r.offset + r.oligo.length - f.offset;
            if !(request.min_product_length..=request.max_product_length).contains(&length) {
                continue;
            }
            if let Some(j) = request.junction_offset {
                let spans = |p: &Candidate| {
                    p.offset + request.junction_min_bases <= j
                        && j + request.junction_min_bases <= p.offset + p.oligo.length
                };
                if !spans(f) && !spans(r) {
                    continue;
                }
            }
            if (f.oligo.tm_c - r.oligo.tm_c).abs() > request.constraints.max_pair_tm_difference_c {
                continue;
            }
            let heterodimer = dimer_screen(&f.oligo.sequence_5to3, &r.oligo.sequence_5to3);
            if !dimer_ok(heterodimer, &request.constraints) {
                continue;
            }
            let Some(screens) = screen_pair(records, f, r, request, &mut product_budget)? else {
                continue;
            };
            feasible += 1;
            pairs.push(Pair {
                forward: f.oligo.clone(),
                reverse: r.oligo.clone(),
                heterodimer,
                score: (f.oligo.tm_c - request.constraints.target_tm_c).abs()
                    + (r.oligo.tm_c - request.constraints.target_tm_c).abs(),
                screens,
            });
            pairs.sort_by(|a, b| {
                a.score
                    .total_cmp(&b.score)
                    .then_with(|| {
                        (a.forward.length + a.reverse.length)
                            .cmp(&(b.forward.length + b.reverse.length))
                    })
                    .then_with(|| a.forward.reference_start.cmp(&b.forward.reference_start))
                    .then_with(|| a.reverse.reference_start.cmp(&b.reverse.reference_start))
                    .then_with(|| a.forward.length.cmp(&b.forward.length))
            });
            pairs.truncate(request.max_results);
        }
    }
    if pairs.is_empty() {
        return Err(DesignError::NoPairs);
    }
    Ok(Design {
        request: request.clone(),
        pairs,
        candidate_pairs_examined: examined,
        feasible_pairs: feasible,
        model: "dnagent-nn-v1: Allawi/SantaLucia 1997; SantaLucia 1998 entropy salt correction; von Ahsen 2001 sodium-equivalent Mg/dNTP; primer in excess",
        limitations: vec![
            "Not Primer3: deterministic bounded search using DNAgent's existing NN model and contiguous-complement structure screens, not folding energies or experimental validation",
            "Specificity covers only supplied templates, full-oligo Hamming mismatches with an exact 3-prime anchor; no indels, seed-only matches, BLAST or genome/transcriptome search",
            "Predicted products require nonoverlapping inward-facing sites, at most one template traversal and length <= screen_max_product_length; no products is conditional on these bounds",
            "Positive templates require exactly one exact heteroprimer product in the requested size range; any additional screened product rejects a pair. Negative templates require zero screened products",
            "All candidate oligos originate within the declared reference window; shared-block discovery outside it is not performed",
            "Junction constraint requires a primer to straddle the explicit boundary; this alone does not establish transcript or construct specificity",
            "No annealing temperature, assay validity, primer ordering readiness, multiplex suitability or DNA/RNA origin is inferred",
        ],
    })
}

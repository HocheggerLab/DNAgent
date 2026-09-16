# Bounded Gibson primer optimisation

```bash
dnagent gibson-optimise fixtures/plans/synthetic-gibson-optimisation.json
# US spelling is an alias; envelopes retain the canonical command name:
dnagent gibson-optimize fixtures/plans/synthetic-gibson-optimisation.json
```

Output is JSON only, envelope **0.9.0**. The input uses a separate
[version-1 optimisation plan](../schemas/gibson-optimisation-plan-1.schema.json).
See the public synthetic example for all required fields. There are **no implicit
reaction-condition or screening defaults**. Its values are a software example,
not a recommended PCR protocol.

## What changes, and what does not

The optimiser keeps the declared source intervals, orientations, product order,
overlap lengths and one-sided PCR-tail scheme unchanged. It enumerates forward
and reverse **annealing lengths** within the explicitly requested 18–40-base bounds.
A longer annealing segment can resolve an exact-site ambiguity at a shorter length;
the operation does not prematurely reject the design at its minimum length.

Candidates must pass:

- Exact annealing-site uniqueness on the whole source, both orientations, including
  circular seams. Self-reverse-complementary annealing segments are refused.
- Inclusive annealing-Tm and GC-fraction bounds.
- Inclusive full-oligo hairpin/self-dimer sequence-screen limits.
- Pair Tm-difference and full-oligo heterodimer limits, and non-overlapping annealing
  intervals on the selected core.

Among feasible pairs, minimise `abs(Tm_forward-target) + abs(Tm_reverse-target)`.
Ties prefer shorter total annealing length, then shorter forward, then reverse
length. Each component is optimised independently for **separate PCR reactions**.
There is no multiplex cross-component primer screen. At most 23 candidates per
orientation and 529 pairs per component are considered before filtering.

Failure to find a feasible pair returns `gibson_failed`, identifying the component,
with **no relaxed-constraint fallback or partial design**. In particular, the fixed
one-sided tail scheme can make single-core circularisation infeasible because the
primer pair contains complementary sequence. No alternative tail scheme or source
boundary shift is silently substituted.

## Tm model and explicit units

Tm applies to the **template-matching annealing segment**, not the full tailed
primer. It is not a recommended PCR annealing temperature.

- DNA nearest-neighbour parameters: Allawi & SantaLucia (1997), *Biochemistry*
  **36**, 10581–10594; equivalent to Biopython's `DNA_NN3` table.
- Entropy salt correction: SantaLucia (1998), *PNAS* **95**, 1460–1465:
  `ΔS += 0.368 × (length−1) × ln([Na_equivalent] in M)`.
- Sodium equivalence: von Ahsen et al. (2001), *Clinical Chemistry* **47**,
  1956–1961: concentrations in mM,
  `Na_equivalent = Na + K + Tris/2 + 120 × sqrt(max(Mg−total_dNTP, 0))`.
- `Tm = 1000×ΔH / (ΔS + 1.987×ln(primer concentration in M)) − 273.15`.
  The primer is assumed in excess; complementary template concentration is
  neglected. Self-complementarity symmetry corrections are unnecessary because
  such annealing sequences are rejected before scoring.

Plan concentrations `sodium_mm`, `potassium_mm`, `tris_mm`, `magnesium_mm` and
`dntp_mm` are **millimolar**; dNTP means **total**, not per nucleotide. `primer_nm`
is **nanomolar**. All must be finite. Accepted bounds are Na/K 0–500 mM, Tris
0–200 mM, Mg/total-dNTP 0–10 mM, primer 1–5000 nM, with positive sodium-equivalent
concentration. These are input guards, not empirical calibration guarantees.
The Mg/dNTP treatment is an approximation, not a chemical-equilibrium solver.

The Rust equations and numeric lookup are independently implemented from the
referenced model; pinned Biopython 1.85 is the numerical comparison, not a required
runtime engine. No Primer3 or other folding engine is silently invoked.

## Structure screens are not structure thermodynamics

Screens examine the **complete oligos, including tails**:

- Hairpin: longest contiguous Watson–Crick stem with at least three unpaired loop
  bases. `max_hairpin_stem` is an inclusive allowed stem length.
- Dimer: longest contiguous complementary run over all antiparallel alignments.
- 3′ dimer: longest such run reaching either oligo's 3′ terminus.

`max_dimer_run` and `max_three_prime_run` apply to both self- and within-pair
heterodimers. Stem/run lengths are **not ΔG or structure Tm**. Bulges, mismatches,
non-contiguous stems, temperature-dependent folding and kinetic effects are not
modelled. A passing screen is not proof that primers will work or are ordering-ready.

## Report, provenance and validation

The result includes unchanged constraints/solution values, selected full oligos,
annealing Tm, full-oligo screens, score, and candidate-pair counts after single-primer
filtering. `design` is the shared PCR-tail product/provenance report. Its
`annealing_length` is `null` when selected lengths vary; the actual annealing
sequences are always explicit. Fixed-length `gibson` results retain their integer
length. Original templates, annotations and import warnings remain available.

The independent checker enumerates the whole candidate space, compares Tm with
`Bio.SeqUtils.MeltingTemp.Tm_NN(DNA_NN3, saltcorr=5, dnac2=0)`, uses separate brute-force
sequence screens, verifies the winning pair and feasible counts, and reconstructs
PCR products from the selected oligos. It covers different salts, Mg below/equal/
above total dNTP, orientations, varied length bounds, and impossible constraints.
This is **software-model validation**, not wet-lab validation or a second empirical
primer-design engine. Real-template optimisation coverage and full folding-energy
assessment remain follow-ups.

```bash
uv run scripts/check_gibson_extensions.py --binary target/debug/dnagent
```

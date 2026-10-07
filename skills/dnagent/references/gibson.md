# Three distinct Gibson workflows

Decide **before writing a plan**:

| Starting material / requested operation | Choose |
|---|---|
| Template intervals; explicit fixed annealing length | `gibson` |
| Template intervals; search annealing lengths under supplied constraints | `gibson-optimise` |
| Already-overlapping linear fragment views | `gibson-assemble` |

For the complete insert-into-vector recipe (choosing cores from a handoff, running,
writing and checking the product), load the **`gibson-cloning`** skill.

Do not run PCR-tail mode on already-overlapping fragments expecting endogenous
homology to be removed. Do not use existing-overlap mode to invent PCR tails.
All plans below are schema examples, **not validated experimental designs**.

## Known synthetic examples (software practice only)

When a checkout is available, these are passing public examples with matching
source fixtures; paths inside each plan resolve automatically:

```bash
"$DNA" gibson "$DNAGENT_REPO/fixtures/plans/synthetic-gibson.json" --output json
"$DNA" gibson-optimise "$DNAGENT_REPO/fixtures/plans/synthetic-gibson-optimisation.json"
"$DNA" gibson-assemble "$DNAGENT_REPO/fixtures/plans/synthetic-gibson-existing.json"
```

The two PCR examples each predict 190 bases but select different second-core
intervals. Existing-overlap mode recovers the 300-base synthetic circle. They are
separate specified designs, not interchangeable plans or substitutes for user data.

## Shared intake and orientation

Inspect templates first. Clarify source intervals, desired final component order,
orientation, topology and method. Select sources by **1-based input number**.
`start` is on the original forward axis; `length` counts bases. Extract first,
then apply the explicitly declared `reverse` orientation. Circular sources allow
arcs crossing the origin, at most one lap; linear sources cannot wrap.

All source DNA must be ACGT, even outside the chosen intervals. Bounds: 1–16
sources, 1–128 selections, each source at most 1,000,000 bases. PCR core totals and
existing-fragment totals are each capped at 1,000,000 bases. Unknown fields, missing
orientations or wrong plan kinds fail; all plan versions here are **1**, not 0.9.0.

## A. Fixed-length PCR tails

```json
{
  "schema_version": 2,
  "inputs": [{"path": "template.dna"}],
  "cores": [
    {"input": 1, "start": 270, "length": 90, "orientation": "forward", "preparation": "provided"},
    {"input": 1, "start": 90, "length": 100, "orientation": "reverse", "preparation": "pcr"}
  ],
  "topology": "circular",
  "overlap_length": 25,
  "annealing_length": 22
}
```

```bash
"$DNA" gibson plan.json --output json
```

These example coordinates require an appropriate circular source of at least
300 bases; do not transplant them to a user's construct.

Every core states its `preparation`, and it is **required**: `pcr` (amplified here,
so it can take primer tails) or `provided` (restriction fragment, synthesis or stock
linear DNA, used exactly as given, no primers). For insert-into-vector the recipient
vector is normally `provided`: see the `gibson-cloning` skill.

For oriented core `C_i`, length `A`, overlap `L`, each junction's overlap is written
by exactly one primer:

- Upstream core is `pcr`: overlap `C_next[:L]` on the **reverse** primer of `C_i`.
- Upstream core is `provided`: overlap `C_i[-L:]` on the **forward** primer of
  `C_next`, because a provided fragment cannot be extended.
- Both provided: refused.

So forward primer of `C_i` is `(forward tail) + C_i[:A]`, reverse is
`RC(reverse tail) + RC(C_i[-A:])`, and the PCR top strand is
`(forward tail) + C_i + (reverse tail)`. The last linear core has no downstream
junction; circular closure uses the first core as next. A tail is never split
between two pairs. Provided cores report `forward_primer`/`reverse_primer` as null
and `fragment_sequence_5to3` equal to the core.
The final product is **exactly concatenated oriented cores**. Endogenous shared
sequence remains in each core; only added tail copies are merged.

Overlap length is 20–60 bases, annealing length 18–40, each core at least
`max(overlap_length, 2*annealing_length)`. Each primer needs one exact locus across
both orientations of its **whole template**; each overlap needs one locus across
both orientations of the **whole intended product**, including circular seams.
Self-RC motifs and repeated/RC-ambiguous homology are conservatively refused.

Fixed mode does **not** evaluate Tm, dimers, hairpins or off-template/mismatched
binding. GC counts are descriptive. Single-core linear mode predicts PCR only;
single-core circular mode predicts PCR-assisted reclosure, not established primer
compatibility or experimental circularisation.

## B. Constrained annealing-length optimisation

Use a separate plan: same `inputs`, `cores`, `topology`, `overlap_length`, but
**replace `annealing_length` with `constraints`**. No constraints are implicit.
The following block illustrates syntax/units, not recommended reaction settings:

```json
{
  "min_length": 18,
  "max_length": 32,
  "min_tm_c": 58,
  "max_tm_c": 66,
  "target_tm_c": 62,
  "max_pair_tm_difference_c": 3,
  "min_gc_fraction": 0.3,
  "max_gc_fraction": 0.7,
  "max_hairpin_stem": 6,
  "max_dimer_run": 8,
  "max_three_prime_run": 4,
  "solution": {
    "sodium_mm": 50,
    "potassium_mm": 0,
    "tris_mm": 0,
    "magnesium_mm": 1.5,
    "dntp_mm": 0.2,
    "primer_nm": 250
  }
}
```

```bash
"$DNA" gibson-optimise optimisation-plan.json
```

`gibson-optimize` is a spelling alias; the JSON command remains `gibson-optimise`.
Do **not** append `--output json`. Ask the user for actual conditions or explicitly
label an agreed exploratory scenario. Never silently adopt this example as their
buffer composition. `dntp_mm` is **total dNTP**, not per nucleotide; primer is **nM**,
all other solution concentrations **mM**.

The optimiser varies only annealing lengths (18–40). It does not move source
boundaries, change tails, optimise overlaps or search orders. Cores must fit the
minimum lengths, and selected forward+reverse annealing lengths must fit together.
Longer annealing regions may resolve an exact-site ambiguity of shorter candidates.

Filters: exact-site uniqueness, inclusive annealing Tm/GC bounds, full-oligo
hairpin/self-dimer screens, pair Tm difference and heterodimer screens. Among feasible
pairs minimise summed absolute target-Tm deviations; ties prefer shorter total
annealing length, then forward, then reverse length. Separate components assume
**separate PCRs**, not a multiplex interaction screen. At most 23 candidates per
orientation and 529 pairs per component before filtering.

Tm is for the **annealing segment**, not full tailed oligo or recommended PCR
annealing temperature. Model: Allawi/SantaLucia DNA NN3, SantaLucia entropy salt
correction, von Ahsen sodium-equivalent Mg/total-dNTP approximation; primer in excess,
complementary template concentration neglected. This is not a Primer3 invocation.

Hairpin/dimer checks use **whole oligos including tails**, but only contiguous
Watson–Crick sequence matches. Hairpin loop minimum is three unpaired bases; dimer
3′ runs reach either oligo's 3′ end. Limits are inclusive stem/run lengths, **not
ΔG or structure Tm**. No bulges, mismatches, temperature-dependent folding, genomic
specificity, GC-clamp or homopolymer optimisation is inferred.

Read `result.pairs` for selected oligos, Tm, screens, score and feasible counts;
read `result.design` for product/provenance. Its `annealing_length` is null if lengths
vary; actual matching segments are always explicit. Refusal has no relaxed fallback
or partial product. Single-core circular tails can enforce severe within-pair
complementarity; do not raise dimer limits merely to force a result.

## C. Supplied existing overlaps

```json
{
  "schema_version": 1,
  "inputs": [{"path": "template.dna"}],
  "fragments": [
    {"input": 1, "start": 0, "length": 180, "orientation": "forward"},
    {"input": 1, "start": 155, "length": 170, "orientation": "forward"}
  ],
  "topology": "circular",
  "overlaps": [25, 25]
}
```

```bash
"$DNA" gibson-assemble existing-plan.json
```

This shape is the public 300-base circle example, not a promise of matching
homology in another source. **Selections declare existing linear fragment views**,
including when their source record is circular. Establish that actual physical
fragments/termini match the declarations: no PCR/digest preparation or circle
opening is inferred. A complete supplied linear fragment can be selected at zero
with its full length.

One explicit 20–60-base overlap per junction; circular plans include closure.
Lengths may vary. Each oriented suffix must exactly equal the next prefix. Each
fragment must remain positive in length after both incoming and outgoing overlaps
are subtracted. Repeated/RC-ambiguous overlaps in the merged product are refused.

Only declared homology copies are merged. Product length is
`sum(fragment lengths) − sum(overlap lengths)`, with the first selected fragment
defining base zero. A single linear fragment uses `overlaps: []`; a single circular
product needs duplicated terminal homology in its supplied linear fragment.
No overlap Tm, mismatched homology, overlap search, kinetics or yield is assessed.

## Provenance and handoff for all modes

PCR components carry core-local annotations; add `product_start`. Orientation is
already applied: **do not reverse a second time**. PCR tail copies are removed,
leaving downstream-core provenance once. No gene reunion or translation updates.

Existing-overlap components preserve **both source associations** in shared homology.
For circular products, component-local `x` maps to
`(product_start + x) % product_length`; inspect `wraps_origin`. Association mapping
need not be one-to-one. `complete` is source coverage, not biological integrity or
a distinct-base count in the merged product.

Report predicted product topology, sequence/length and junctions; for PCR modes
also the full 5′→3′ oligos, annealing regions and tails. Keep original template
metadata and warnings; imported primers are provenance only. Never describe the
result as experimentally validated or ordering-ready. Real-template design coverage,
full folding-energy assessment, product annotation reconstruction/native exports
and an independent assembly-engine comparison remain follow-ups.

Canonical app references: `docs/gibson.md`, `docs/primer-optimisation.md`,
`docs/existing-overlaps.md`; input schemas `gibson-plan-1.schema.json`,
`gibson-optimisation-plan-1.schema.json`, `gibson-existing-plan-1.schema.json`.

## Annotated product GenBank (`--out`)

```bash
"$DNA" gibson plan.json --out ~/DNAgent/product.gb --output json
"$DNA" gibson-optimise plan.json --out ~/DNAgent/product.gb --name "pUC19-ORF"
```

Writes the predicted product as DNAgent GenBank (path must end `.gb`/`.gbk`/`.genbank`;
the JSON result gains `product_genbank` with path, length and counts):

- source features lying **wholly** inside a core, at their product positions (strand
  flipped for reverse cores), keeping type, name, colour and qualifiers, with a
  provenance `/note`;
- features only partly inside a core are **left out**, one `gibson_feature_clipped`
  warning per core (report these; e.g. an MCS-spanning feature cut by opening the vector);
- `primer_bind` "Gibson F<n>/R<n>" over each whole oligo as it reads on the product (the
  optimiser's chosen primers for `gibson-optimise`), and the oligos in the file's primer
  list, marked as candidates;
- "Gibson overlap a-b" over each junction overlap.

`--strict` refuses before writing if any warning would result. Re-check the product:
`inspect`, `features`, and `translate --feature` for every CDS that must stay in frame.


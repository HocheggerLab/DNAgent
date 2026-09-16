# Assembly of declared existing overlaps

```bash
dnagent gibson-assemble fixtures/plans/synthetic-gibson-existing.json
dnagent gibson-assemble plan.json --output fasta > product.fasta
dnagent gibson-assemble plan.json --output genbank > product.gb
```

JSON output uses envelope **0.9.0**. FASTA is sequence-only; conservative GenBank
uses component `misc_feature` records and does not infer genes, CDSs or translations.
The input is a separate
[version-1 existing-overlap plan](../schemas/gibson-existing-plan-1.schema.json).
It uses `fragments`, not PCR `cores`, and an `overlaps` array, not a single PCR-tail
length. Wrong plan kinds and unknown fields are rejected.

## Input meaning

Each selection identifies `input` (1-based), `start` (zero-based source forward
axis), `length` and explicit `orientation`. Extraction precedes reverse
complementation. Paths resolve relative to the plan. A supplied full fragment is a
selection starting at zero with its full sequence length.

Every Gibson plan now accepts four source forms:

```json
{"path": "fragment.fasta"}
{"name": "ordered_insert", "sequence": "ACGT...", "topology": "linear"}
{"path": "vector.dna", "enzymes": ["EcoRI"], "fragment_id": "fragment-0002", "strand": "top"}
{"path": "template.dna", "start": 5535, "length": 654,
 "orientation": "forward", "left_tail": "ACGT...", "right_tail": "TGCA...",
 "forward_annealing_length": 26, "reverse_annealing_length": 18}
```

- Plain paths accept SnapGene `.dna` and single-record `.fa`, `.fasta` or `.fna`.
  FASTA is imported as sequence-only linear DNA and emits an explicit warning because
  topology/features are not represented by the format.
- Literals require a name, validated IUPAC DNA and explicit topology. Exact Gibson
  operations subsequently require unambiguous ACGT.
- Digest sources run a complete digest and expose the selected top or bottom 5′→3′
  strand as a linear source. Its selection can trim terminal restriction-site bases
  when the declared assembly overlap lies inside the physical digest fragment.
- PCR sources extract an oriented source interval, add product-oriented left/right
  tails, check exact annealing-site uniqueness and expose the resulting ideal PCR
  product. Use either one shared `annealing_length` or both asymmetric lengths.
  Designed 5′→3′ primer candidates are retained in that derived input record's
  `primers` array. Thermodynamic optimisation is not performed by this source form.

Digest/PCR projections emit warnings because their JSON source record is a selected
assembly strand/product rather than the complete original duplex/annotation model.
Use the separate digest/fragment output for end geometry and source annotations.
`--strict` therefore rejects these deliberately lossy projections.

Selections declare **existing linear fragment views**, even when the source record
is circular. Source topology controls interval extraction only; preparation by PCR,
digestion or synthesis is not inferred. This operation neither designs primers nor
pretends to open an uncut circular molecule. The user must establish that the
physical fragments and termini match the declared views.

- Explicit order and orientations; no order/orientation search.
- One 20–60-base overlap for each adjacent junction. Circular products also require
  the last-to-first overlap. Lengths may differ at different junctions.
- Each oriented suffix must exactly equal the following oriented prefix.
- Each fragment must have a positive interior **after incoming and outgoing overlap
  lengths are subtracted**. Nested/overlap-only fragments are refused.
- At most 16 sources, 128 fragments and 1,000,000 total supplied fragment bases;
  each source is also limited to 1,000,000 unambiguous ACGT bases.
- Circular source intervals may cross the origin but cannot exceed one lap;
  linear sources cannot wrap.

A linear one-fragment plan has `overlaps: []` and is a pass-through. A circular
one-fragment plan has one overlap and requires matching duplicated terminal
homology already present in its supplied linear sequence. It is not implicit
circularisation of a source circle.

## Product and ambiguity policy

The predicted top-strand sequence merges **only the declared overlap copies**.
The first selected fragment defines product origin; a circular closing duplicate
is removed from the final suffix. Length is exactly
`sum(fragment lengths) − sum(declared overlap lengths)`.

Every overlap must occur exactly once in the merged product when both orientations
are considered, with circular seam matches included. Repeated or self-reverse-
complementary homology is rejected conservatively. There is no mismatched/partial
homology, best-effort trimming, length search or ambiguous alternative output.
All source-fragment bases are checked against their reported product positions.

This is an ideal exact-overlap **sequence prediction**, not digestion-end geometry,
assembly kinetics or experimental validation. Existing fragment overlaps are not
assessed for overlap Tm or folding; the separate primer optimiser evaluates PCR
annealing segments, not Gibson-reaction conditions.

## Annotation provenance in shared homology

Components retain source selections, their entire supplied sequence and source-
linked component-local annotation projections. Overlapping source associations
are **both retained**, even though the product carries one copy of the sequence.
No gene reunion, fusion, translation or functional repair is inferred.

For component-local coordinate `x`:

- Linear product coordinate: `component.product_start + x`.
- Circular: `(component.product_start + x) % product_length`.

Orientation is already applied to component-local annotations; do not reverse them
again. `wraps_origin` reports whether this component's span exceeds the stored
product end. It is particularly important for the closing component and single-
fragment reclosure. In shared homology this mapping need not be one-to-one across
components (or across the duplicated termini of a reclosed fragment).

An annotation's `complete` flag concerns retained source-base **associations in the
component view**, not a distinct-base count or biological integrity of a product
feature. JSON remains the source of truth. Direct FASTA and conservative GenBank
exports are available, but they intentionally carry less provenance: FASTA is
sequence-only and GenBank records component associations as `misc_feature` entries.

## Validation

Domain/CLI tests and `scripts/check_gibson_extensions.py` independently extract
source intervals with Biopython, compare exact merged products and lengths, and
verify per-base source-to-product associations including both homology copies.
Cases cover both orientations/topologies, variable overlap lengths, origin arcs,
single-fragment reclosure, mismatches, exhausted interiors, repeats, malformed
plans, strict imports and warning retention on later failures. Inputs are synthetic
originals; a separate assembly-engine and experimental comparison remain pending.

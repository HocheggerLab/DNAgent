# Gibson cloning: explicit PCR-tail candidates

```bash
dnagent gibson fixtures/plans/synthetic-gibson.json --output json
# Human-readable product and primer candidates:
dnagent gibson fixtures/plans/synthetic-gibson.json --output text
```

This first Gibson operation designs **PCR-tailed fragments** and predicts their
ideal overlap-directed product. It is **not** an assembler for supplied fragments
with pre-existing overlaps, and does not optimise primer thermodynamics. Generated
oligos are inspectable **candidates**, not ordering-ready or experimentally validated
recommendations. Separate commands now provide
[bounded primer optimisation](primer-optimisation.md) and
[existing-overlap assembly](existing-overlaps.md); neither changes this fixed-length
plan contract. No GUI integration is included in this milestone.

## Plan and coordinate contract

```json
{
  "schema_version": 1,
  "inputs": [{"path": "template.dna"}],
  "cores": [
    {"input": 1, "start": 270, "length": 90, "orientation": "forward"},
    {"input": 1, "start": 90, "length": 100, "orientation": "reverse"}
  ],
  "topology": "circular",
  "overlap_length": 25,
  "annealing_length": 22
}
```

This is the shape of the synthetic example, not a design for an unspecified real
300-base template. The [version-1 plan schema](../schemas/gibson-plan-1.schema.json)
is independent of the current **0.9.0** CLI output-envelope schema. Relative paths
resolve against the plan directory. Unknown fields, missing orientations and
unsupported versions fail explicitly.

- Templates are SnapGene `.dna` records; all retain source names, full sequences,
  topology, original features/qualifiers and imported primers in the JSON result.
- `input` is **one-based**. `start` is zero-based on the original **forward** axis;
  `length` is the count of selected source bases. Linear ranges cannot wrap.
  Circular ranges may cross the origin, but cannot exceed one full source lap.
- Extract the interval first, then reverse-complement it only if `orientation` is
  `reverse`. The `cores` array explicitly fixes final product order and base zero.
- The predicted product is **exactly the concatenation of these oriented cores**.
  If the selected cores already contain shared sequence, that sequence remains in
  each core: endogenous overlaps are not silently deduplicated.
- Reusing a template for multiple PCRs is allowed. Product overlaps still must pass
  uniqueness checks; intended repeat-containing architectures may be refused.

## Primer, PCR and junction construction

Let oriented core `i` be `C_i`, the annealing length be `A`, and overlap length `L`.
For every component with a downstream neighbour:

- Forward primer: `C_i[:A]`, no synthetic tail.
- Reverse primer: `RC(C_next[:L]) + RC(C_i[-A:])`, all written **5′→3′**.
- Predicted PCR top strand: `C_i + C_next[:L]`.

For the final linear component, omit the tail; for circular closure, its neighbour
is the first core. Reverse primers therefore carry the entire synthetic overlap;
tails are not divided between primer pairs. Each primer reports its complete oligo,
3′ annealing segment, 5′ tail and annealing GC base count. GC count is descriptive,
not a quality score or melting-temperature estimate.

The simulator also reassembles the predicted PCR products: verify each
suffix/prefix overlap, retain one copy, then remove the duplicated closing overlap
for a circle. The result must equal the requested core concatenation exactly.
Unlike restriction ligation, **same-strand synthetic homology copies are merged**.
The product is an ideal fully paired duplex; `product_sequence_5to3` is its chosen
reference strand and the other strand is its reverse complement. Intermediate
single strands, gaps, nicks and reaction kinetics are not represented.

`junctions` report one-based neighbouring component numbers, the closure flag,
overlap sequence, and its zero-based start in the final product. Closing overlap
starts at product zero. Product topology is always explicit; single-core linear
plans predict PCR only, while single-core circular plans model PCR-assisted reclosure.

## Exact-match checks and conservative limits

- Every primer annealing sequence must have exactly one exact binding-site locus
  across both orientations of its **full source template**, including circular seam
  matches. Self-reverse-complementary annealing sequences are rejected.
- Every junction overlap must occur exactly once across both orientations of the
  **whole intended product**, including component junctions and circular seams.
  Repeated or self-reverse-complementary homology is rejected, not silently selected.
- Templates must be unambiguous uppercase ACGT after normal import canonicalisation.
  Ambiguity anywhere on a template prevents an exact-site uniqueness claim.
- Bounds: 1–16 templates; 1–128 cores; 20–60-base overlaps; 18–40-base annealing
  segments; each core at least `max(L, 2*A)`; at most **1,000,000 bases per source
  template and final product**. These are implementation limits, not universal
  experimental thresholds.

Uniqueness here is an **exact-match sequence check**, not proof of specific PCR or
exclusive assembly. Partial/mismatched homology, genomic/off-template targets,
melting temperatures, salt/concentration effects, hairpins, primer dimers, polymerase
behaviour and reaction conditions are not evaluated. Separate template-specific
PCR reactions and sequence-faithful products are assumed; multiplex PCR is not
modelled. Single-fragment closure can create interacting primer sequences, which
especially requires secondary-structure review. No particular reaction yield is
predicted, and primer ordering requires further review.

## Annotation and import provenance

Each component retains **core-local** source feature mappings. Match the selection's
input number, then the mapping's source feature ID. Add `product_start` to its local
coordinates to obtain final reference-strand positions. Reverse-oriented cores
already have reversed local coordinates and flipped known feature strands; do not
reverse them a second time. Multipart ordering, coverage, source offsets and split
flags use the shared [annotation-projection rules](fragment-annotations.md).

Synthetic tails are copies of downstream core prefixes, not new functional features.
The final product retains that downstream core's annotation association once. No
feature reunion, gene fusion, coding-frame repair or translation update is inferred;
clipping/split flags describe the individual cores, not a reconstructed gene.
Product-specific FASTA/GenBank export and biological feature reconstruction remain
follow-ups. JSON is the authoritative design/provenance report for now.

Imported primers are historical metadata, not reused primer-design predictions.
Opaque packets and raw XML are not exported. Warnings accumulate in input-loading
order, even if a later input fails; `--strict` rejects any import warning before
simulation. Design/plan failures use `gibson_failed`, source I/O uses `command_failed`,
and strict rejection uses `import_warnings`. There is no partial product on failure.
The program does not write product or primer-order files.

## Reproducible validation and next steps

```bash
cargo test --workspace --locked
uv run scripts/check_cli_schema.py --binary target/debug/dnagent
uv run scripts/check_gibson.py --binary target/debug/dnagent
```

The independent synthetic checker uses Biopython 1.85 to recover source sequences
and reverse complements, reconstructs PCR products from the **whole primer oligos**
and template interiors, then merges overlaps to recover the expected core product.
It covers both orientations of each component, linear/circular products, circular
source arcs, minimum/maximum configured lengths, single-core cases, GC accounting,
and per-base source-feature associations. Negative controls cover repeats, opposite-
strand overlap ambiguity, invalid plans/ranges/limits, ambiguous templates and strict
warning retention. These are software-model checks, **not** wet-lab validation or
an independent thermodynamic/experimental assembly oracle. The new Gibson checker
uses synthetic originals only; private-template design coverage remains a gap.

Bounded NN-Tm primer optimisation and explicit existing-overlap intake are now
available as separate operations. Next work: full folding-energy assessment, richer
product annotation/export, real-template designs and a separate assembly-engine
comparison. GUI adapters remain deferred while these scope boundaries are reviewed.

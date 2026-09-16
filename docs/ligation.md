# Explicit restriction/ligation simulation

```bash
dnagent ligate plan.json --output json
# Public synthetic examples:
dnagent ligate fixtures/plans/synthetic-religation.json
dnagent ligate fixtures/plans/synthetic-closure.json
```

JSON is the default; `--output text` gives a human-readable sequence/assumption
summary. No product files are written by the program. Plans use
[`ligation-plan-1.schema.json`](../schemas/ligation-plan-1.schema.json); output
envelopes are **0.9.0** (ligation was introduced in 0.7.0). Plan version 1 is independent of envelope versioning.

## Explicit selection, never inferred copies

```json
{
  "schema_version": 1,
  "inputs": [
    {"path": "vector.dna", "enzymes": ["EcoRI", "BamHI"]},
    {"path": "insert.dna", "enzymes": ["EcoRI", "BamHI"]}
  ],
  "fragments": [
    {"input": 1, "fragment_id": "fragment-0002", "orientation": "forward"},
    {"input": 2, "fragment_id": "fragment-0001", "orientation": "reverse"}
  ],
  "topology": "circular"
}
```

This is a plan-shape example, **not a verified construct design**. Run `digest`
or `fragments` on the intended sources to inspect IDs, ends and annotations;
select suitable fragments deliberately. Relative input paths resolve against the
plan directory, not the caller's working directory. Unknown plan fields and
orientation values fail rather than being ignored.

- `inputs` declares distinct complete-digest instances, numbered from **1**.
- `fragments` declares product left-to-right component order and explicit
  `forward`/`reverse` orientation. Nothing searches for a favourable ordering.
- A particular `(input, fragment_id)` can appear only once. To model a second
  molecule copy, explicitly declare another input instance (the same path is
  allowed). This is a declared model copy, not an inferred experimental quantity.
- Each internal junction uses the previous component's right end and the next
  component's left end. Circular topology also joins the final right end to the
  first left end. Ends are never reused.
- Uncut circles have no free ends and cannot be selected, including as pass-through
  products. A linear single-component plan is an explicit zero-junction pass-through.
- Unselected fragments remain in `unused_fragments`, linked to full input reports.

All sources use the conservative complete-digest rules: ambiguous bases and
unavailable/terminal/overlapping cuts fail. A plan does not silently skip cuts in
unused inputs or select only a convenient subset of a failed digestion.

## Product geometry and conservation

Reversing a duplex exchanges its original top/bottom **5′→3′ sequences** and
left/right ends; protruding-strand labels flip. The physical overhang oligos do
**not** themselves get reverse-complemented during that orientation change.

The product top sequence concatenates oriented component tops in plan order.
The bottom 5′→3′ sequence concatenates oriented component bottoms in reverse plan
order. No overlap sequence is deleted: cohesive overhangs are complementary
physical strands, not duplicated same-strand bases as in a homology assembly.

Every selected base on both strands is retained exactly once. Each proposed
junction must pass the existing end-compatibility rules. Product-wide duplex
checks additionally require:

- Linear paired cores to be complementary and both terminal staggers to agree
  with the retained free ends. Unequal strand lengths remain explicit.
- Circular strand lengths to be equal and complementary at the reported phase.

`bottom_forward_start` locates the start of bottom's forward-coordinate interval
relative to top base zero. It can be negative for a linear 3′ stagger; on a circle
it is normalised modulo product length. The bottom sequence itself is **not**
rotated to force it to equal the direct reverse complement of the top string.

`components[].top` and `.bottom` give zero-based start/length on each product
strand's own 5′→3′ sequence. `source_strand` specifies which original digest
strand supplied that span (`forward` = original top, `reverse` = original bottom).
`junctions` give one-based adjacent component numbers, a closure flag, compatibility
assessment and each strand's own-axis nick boundary. The assessment compares
already oriented components: its relative `forward` result does not override the
explicit selection orientation or instruct a second transformation. Circular closure boundaries
are zero in those respective frames, which can have different forward-axis origins.

## Annotation provenance, not inferred gene repair

`inputs` retains each complete annotated digest, including original features,
qualifiers, operators, primers and component-local projections. Match a product
component by input number and fragment ID, choose its `source_strand` projection,
then add the component's product-span start to its local coordinates. Local 5′→3′
sequences are used unchanged, so no additional per-piece reversal is needed.

This preserves an inspectable source-to-product mapping. It does **not** merge
clipped features across junctions, infer repaired/fused genes, update translations,
or validate functional integrity. Existing clipping/split flags describe source
components, not a reconstructed biological feature in the final product. Opaque
SnapGene metadata is still outside these exports; import warnings remain relevant.

## Failures, bounds and biological assumptions

Plan and ligation-analysis failures use `ligation_failed`, with no partial product.
Incompatible junction errors identify the adjoining component numbers and reason.
Source-read failures retain `command_failed`; strict warning failures retain
`import_warnings`. Top-level warnings accumulate in source-loading order, including
when a later source fails. `--strict` rejects any import warning before simulation;
it is not experimental validation or a statement of complete export fidelity.

The operation bounds plans to **16 inputs**, **128 selected components** and
**10,000,000 bases per product strand**. These are explicit resource limits, not
biological rules. It does not construct a quadratic all-end matrix.

The prediction assumes compatible selected junctions seal completely. It does not
model phosphorylation, unsealed nicks, reaction conditions, yields, competing
products, gel isolation, methylation, star activity or experimental sequence QC.
There is no trimming, fill-in, polishing or implicit end repair. [Gibson PCR-tail
design](gibson.md) uses separate homology rules.

## Validation

```bash
cargo test --workspace --locked
uv run scripts/check_cli_schema.py --binary target/debug/dnagent
uv run scripts/check_ligation.py --binary target/debug/dnagent
# Optional existing authorised private corpus (kept outside Git):
uv run scripts/check_ligation.py --binary target/debug/dnagent \
  --manifest "$PRIVATE_CORPUS/manifest.json"
```

The checker verifies literal designed Type IIS fusion sequences for all 256
four-base oligos, including reversed asymmetric components (512 fusions), complete
source recovery after linear re-ligation, every rotation of three circular enzyme
substrates, placement/annotation offsets, nick boundaries, duplex phase and unused
fragment accounting. Private checks recover each of eight real plasmids in both
orientations against Biopython 1.85 source sequences. Source hashes are checked
before/after; no private inputs or reports are committed.

Rust/CLI checks also cover plan parsing, cwd-independent relative paths, copy/reuse
rules, no-end circles, malformed/missing plans, resource limits, sticky-ended
pass-through products, warning retention and failure without partial products.
This is an independently checked software model, not an independent experimental
ligation oracle. Native product annotation reconstruction and product-specific
FASTA/GenBank export remain follow-up work; current product sequences are in JSON
or text. A first [Gibson PCR-tail design slice](gibson.md) is now implemented;
Gibson hardening and scope review remain ahead of GUI work.

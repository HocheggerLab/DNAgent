# Restriction-end compatibility

```bash
cargo build -p dnagent-cli --locked
# Compare all distinct ends within a digest:
target/debug/dnagent compatible-ends construct.dna --enzymes EcoRI,BamHI --output json
# Include a second, independently selected digest:
target/debug/dnagent compatible-ends vector.dna --enzymes BsaI \
  --other insert.dna --other-enzymes BsmBI --output json
```

The application layer digests each record using the [complete-digest model](digest.md)
and passes those digests to a filesystem-free domain operation. No source sequence
is changed, and no ligation product is constructed. The existing digest rejection
policy remains in force: ambiguous DNA, unavailable/terminal cuts and unsupported
overlapping cleavage regions fail before compatibility is assessed.

## What compatible means

**Sequence compatibility only**, not a prediction of experimental ligation.
Phosphorylation, damage, ligase, reaction conditions, molecule concentration and
circularisation geometry are not modelled. Original linear ends are assumed blunt;
their chemistry is unknown. Pairwise matches do not establish a complete assembly.

Rules, in priority order:

| Reason | Compatible? | Meaning |
| --- | --- | --- |
| `polarity_mismatch` | No | Blunt, 5′ and 3′ ends cannot be interchanged |
| `blunt_ends` | Yes | No protruding oligos to match |
| `overhang_length_mismatch` | No | Same polarity, unequal exposed lengths |
| `complementary_overhangs` | Yes | Equal-length oligos are reverse complements |
| `overhang_sequence_mismatch` | No | Same polarity/length, non-complementary oligos |

Overhangs are compared in their own **5′→3′** orientation. Thus `ACGA` pairs with
`TCGT`, not another `ACGA`; palindromic `AATT` pairs with `AATT`. Enzyme names are
provenance, not a matching criterion: different Type IIS enzymes can generate
compatible ends, while the same enzyme can produce incompatible variable ends.
Five- and three-prime ends remain incompatible even if their oligos complement.

## Orientation and placement

The first fragment stays in its stored orientation. The report proposes:

- Opposite-side ends: `second_fragment_orientation: forward`.
- Same-side ends: `second_fragment_orientation: reverse`.
- First end on the left: `second_placement: before_first`.
- First end on the right: `second_placement: after_first`.

Reversing a double-stranded fragment exchanges its top/bottom strands and left/right
sides; it does **not** rewrite an exposed physical oligo's own 5′→3′ sequence for
the compatibility comparison. These fields describe the local alignment considered,
even for incompatible pairs. They do not apply a transformation or create a product.
For same-fragment pairs they describe local end alignment, not a second molecule
copy; a sequence-compatible pair is only a potential closure candidate.

The public domain comparator validates side/polarity/protruding-strand consistency,
uppercase ACGT oligos and blunt-end shape. Malformed public `FragmentEnd` values
produce an error, not a plausible compatibility assessment.

## JSON contract 0.5.0

`schemas/cli-envelope-0.5.0.schema.json` adds `compatible-ends`; historical schemas
remain unchanged. Existing command payload shapes are unchanged, with current
envelopes now labelled 0.8.0, adding [Gibson candidates](gibson.md) after
0.7.0's [ligation products](ligation.md) and
0.6.0's [annotation projections](fragment-annotations.md).
The 0.5.0 schema is retired and retained for archived responses only.

- `inputs`: one or two named, complete digest results, in command-line input order.
  This retains source intervals, strand sequences, enzyme/cut provenance and model
  assumptions so endpoint references can be independently checked.
- `analysis.endpoints`: deterministic IDs such as
  `input-1:fragment-0001:left`, one-based input number, local fragment ID, side and
  full end geometry. Uncut circles contribute no endpoints.
- `analysis.pairs`: every unordered pair of **distinct** endpoints, including
  within-input and cross-input comparisons. Each has endpoint IDs, `same_fragment`
  and an `assessment` with compatibility, reason, orientation and placement.
- `analysis.assumptions`: explicit scope and experimental limitations.

Endpoints are ordered by input, digest fragment order, then left/right side. Pairs
follow that order. No endpoint is paired with itself; identical molecule copies
are not inferred. An uncut linear molecule still contributes its two assumed-blunt
natural ends. Two uncut circles produce an empty endpoint/pair report.

The matrix is limited to **128 endpoints** (8,128 pairs). Larger requests fail
with `compatibility_failed`; output is never silently truncated. Select fewer
enzymes to reduce fragment count. Endpoint structure errors use the same code;
upstream digest failures retain `digest_failed`.

Both imports' warnings are retained in input order, including when loading the
second file fails. Strict mode rejects warnings before producing an analysis.
CLI argument errors (for example `--other` without `--other-enzymes`) still use
Clap diagnostics, not the runtime JSON envelope.

## Validation

```bash
cargo test --workspace --locked
uv run scripts/check_cli_schema.py --binary target/debug/dnagent
uv run scripts/check_compatibility.py --binary target/debug/dnagent
```

Domain and CLI tests cover all reasons, both cohesive polarities, same/opposite
sides, cross-enzyme matches, malformed end values, closure candidates, uncut
circles, deterministic bounded matrices and two-input warning retention.

The independent checker creates all 256 four-base BsaI oligos and a second BsmBI
substrate in temporary synthetic files. It verifies the designed oligos against
the extracted ends, uses **Biopython 1.85** to calculate reference reverse
complements, then checks every pair's reason, orientation, placement and provenance.
Mixed-polarity fixtures and explicit oversized-request rejection bring the total
to 260 cases. All responses are schema-validated. These are software/model checks,
not experimental ligation measurements; no private inputs are needed.

[Source-linked strand projections and JSON/FASTA exports](fragment-annotations.md)
and [conservative GenBank views](genbank-export.md) are now implemented.
[Explicit restriction/ligation products](ligation.md) are now implemented;
a first [Gibson PCR-tail design slice](gibson.md) is also available.
[Gibson hardening and scope review precede GUI integration](roadmap.md).

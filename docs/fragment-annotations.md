# Annotation-aware digest strands and exports

```bash
# Default output is JSON; includes duplex geometry and annotation provenance.
dnagent fragments construct.dna --enzymes EcoRI,BamHI --output json > fragments.json
# Two explicitly labelled 5′→3′ strand records per fragment, not one duplex record.
dnagent fragments construct.dna --enzymes EcoRI,BamHI --output fasta > strands.fasta
```

`fragments` uses the same conservative complete-digest simulation as `digest`.
It does not skip unavailable, terminal or overlapping cuts. Failed calculations
return no partial products. Global `--strict` rejects import warnings before
emitting either export. Runtime annotation/digest failures use `annotation_failed`;
import failures retain the usual import error codes. Current JSON envelopes are **0.7.0** (fragment projections were introduced in 0.6.0).

## Why separate strand projections?

A cohesive-ended fragment does not have a single sequence length or identical
strand boundaries. An EcoRI digest can retain two annotated bases on the top
strand but six on the bottom strand of the same product. Applying top-strand
coordinates to both would silently lose or invent annotation coverage.

Each output has:

- `source_name`, full unmodified `source_features`, and `source_primers`.
- `digest`: the existing sequence-only duplex report, including topology, both
  strand sequences, paired length, cohesive ends and source intervals. Its
  sequence-only assumptions describe this nested payload, not the wrapper.
- `annotations`: one item per digest fragment with separate `top` and `bottom`
  mapping arrays. Features with no retained bases on a strand are omitted from
  that array, but remain in `source_features`.
- Explicit assumptions and limitations; ordinary import warnings remain in the
  top-level envelope.

Each mapping references a source feature ID. `fragment_strand` is relative to
that output strand's own **5′→3′ sequence**: bottom projections flip forward and
reverse, retaining unknown strand as unknown. Original strand and location
operator remain in the source feature. This is an explicit coordinate transform,
not a claim that a feature resides biologically on both physical strands.

`parts` preserve original source-part order, then forward offset within each part:

- `source_part`: zero-based index into the original ordered location parts.
- `source_offset`: offset along that original part's forward-coordinate traversal.
- `source_region`: retained region on the original forward source axis.
- `fragment_region`: linear, half-open region on this exported strand's sequence.

Circular regions are intersected using unwrapped coordinates. A source part may
be represented by multiple pieces; its identity and ordering are not flattened.
Do not sort the mapping parts by local coordinate or interpret their count as a
new `join`/`order` operator. A bottom-frame transform does not change original
source-part order.

## Coverage is not biological integrity

`source_bases` and `retained_bases` count annotation bases **with part multiplicity**
(overlapping original parts remain distinct). `complete` means all those bases
are represented, not that a gene is contiguous, in frame, expressed or functional.
False indicates clipping. `split_source_parts` identifies original contiguous
parts represented across opposite ends of a **linear** fragment; an uncut
circular display seam is not marked as a physical disruption.

A single-cut circle can retain every annotated base while splitting a contiguous
feature between the product ends: `complete: true` and a nonempty
`split_source_parts` deliberately coexist. Disruption between multipart regions,
splicing/frame changes and functional integrity are **not inferred**. Source
qualifiers, including any translation or position-valued text, are unchanged
provenance and must not be reused as validated product annotations.

## Export boundaries

FASTA contains both strands in their physical 5′→3′ orientation, with distinct
`fragment-0001|top` and `fragment-0001|bottom` identifiers. Its `source_start`
header field is the start of the strand interval on the **source forward axis**,
not necessarily the coordinate of the first emitted base on a bottom strand.
Sequences wrap at 80 characters. Names and imported labels are not inserted into
headers. An advisory goes to stderr; stdout contains only FASTA.

FASTA does not preserve feature mappings, complete provenance, topology or duplex
end geometry; keep the JSON alongside it. JSON preserves source metadata and
projections but is not a lossless SnapGene round-trip: opaque packets/XML and
uninterpreted metadata are not exported. Source primers are retained without
inventing binding-site mappings.

[Conservative GenBank export](genbank-export.md) is now available with
`--output genbank --strand top` (or `bottom`). It explicitly selects one strand
and represents each mapped piece as `misc_feature`, with source-link and clipping
notes rather than stale CDS qualifiers. It is not a duplex product or lossless
annotation round trip; keep the JSON report alongside it.

## Validation

```bash
cargo test --workspace --locked
uv run scripts/check_cli_schema.py --binary target/debug/dnagent
uv run scripts/check_fragment_annotations.py --binary target/debug/dnagent
# Optional private corpus; inputs remain external and hash-checked:
uv run scripts/check_fragment_annotations.py --binary target/debug/dnagent \
  --manifest "$PRIVATE_CORPUS/manifest.json"
```

The independent oracle enumerates per-base source/local associations from source
XML rather than reusing the Rust interval-intersection algorithm. It checks all
nonempty intervals/arcs and all three feature strands across every rotation of
synthetic EcoRI/KpnI/EcoRV circles, linear substrates, an uncut circle and a
multipart/origin fixture. It validates retained coverage, split markers, strand
orientation, source-region reconstruction, schema, FASTA content and both
GenBank strand views (including parsed feature extraction and topology). Additional
Rust tests cover `order`, unknown strand and qualifier preservation; CLI tests
cover strict mode and failure-without-partial-output.

Biopython 1.85 is used only for sequence and FASTA verification: its annotation
parser treats some full-circle arcs (e.g. one-based `2-1`) as empty. For sequence
verification the checker builds an in-memory header/DNA-packet view; original
annotation XML and files are unchanged. This workaround is explicit and does not
remove full-circle arc cases from annotation validation.

# Conservative GenBank strand-view export

```bash
# Keep the corresponding full provenance/duplex report alongside the export.
dnagent fragments construct.dna --enzymes EcoRI,BamHI --output json > fragments.json
dnagent fragments construct.dna --enzymes EcoRI,BamHI \
  --output genbank --strand top > top.gb
dnagent fragments construct.dna --enzymes EcoRI,BamHI \
  --output genbank --strand bottom > bottom.gb
```

`--strand` is mandatory for GenBank and rejected for JSON/FASTA. Each export
contains one GenBank record per fragment, in digest order. It is a **selected
5′→3′ strand view, not a fully paired duplex product**. Unequal strand lengths,
cohesive ends and paired cores remain in the JSON report. No sequences are padded
or silently circularised. An uncut circle remains circular; cut products are linear.

## Deliberately conservative annotation policy

Every projected piece becomes a separate `misc_feature`. Its zero-based half-open
coordinates are converted to one-based inclusive GenBank coordinates. Reverse
mapped orientation uses `complement(...)`. Unknown orientation uses an ordinary
location plus an explicit `source strand unknown` note: GenBank consumers may
parse that ordinary location as forward, so consult the note/JSON rather than
infer biological orientation.

Pieces are **not** combined into an inferred `join` or `order`, nor promoted into
functional CDS/gene annotations. Each carries notes recording:

- Zero-based source-feature index into JSON `source_features`.
- Zero-based original part index and forward offset within that part.
- Complete or clipped source-base coverage (not inferred biological function).
- A split-source-part flag when applicable to linear product ends.

Original types, IDs, ordered qualifiers, location operators, primers and display
metadata retained by the domain remain in JSON. Opaque SnapGene packets and raw
XML are not exported in either format; import warnings and report assumptions
remain relevant. In particular, stale `translation`, `codon_start` and
`protein_id` qualifiers are not attached to derived regions. Complete coverage
alone is insufficient to assert an intact CDS or safe qualifier propagation.

Short, safe ASCII labels (at most 40 characters; letters, digits, spaces, dots,
underscores and hyphens, without leading/trailing whitespace) are copied as display
labels. Other labels receive a generated display label and a note pointing to the
unaltered original in JSON. Quotes, newlines, Unicode and long labels cannot inject
GenBank syntax. This is an explicit display fallback, not lossless metadata export.

## Determinism and scope

- Record names are generated (`frag1_top`, `frag1_bottom`, etc.); imported filenames
  and source names are not interpolated into native GenBank record headers.
- LOCUS dates use `01-JAN-1980` as a documented fixed placeholder, not an experiment
  or source-file date. Accessions and organisms are not invented.
- Coordinates, source forward-axis interval start and limitations are inspectable.
  Keep the JSON produced from the same input and enzyme selection for full linkage.
- Global `--strict` retains its import-warning meaning. It can reject the operation
  before any output, but it does not certify a lossless or submission-ready export.
- The CLI renders the complete text before writing stdout, so a digest/format error
  does not leave partial records on stdout. Shell redirection may still create an
  empty destination file before the program runs.
- A fidelity advisory goes to stderr. This is not a native SnapGene round trip,
  a GenBank import implementation, a database submission file or experimental QC.

The format serializer lives in `dnagent-formats`, behind a typed application
operation. Coordinate biology remains in the domain's existing projection logic;
CLI and format adapters do not recompute biological intersections. JSON payloads
were unchanged by GenBank export (introduced alongside **0.6.0**); current
**0.7.0** envelopes additionally support [ligation products](ligation.md).

## Validation

`check_fragment_annotations.py` parses both exports with pinned Biopython 1.85,
treating GenBank parser warnings as failures. It compares sequence, topology,
feature bounds/orientation, source-link notes, split/unknown flags and extracted
feature sequences against independently checked projections. The suite covers
41 synthetic cases and the optional eight-file private corpus. Rust/CLI tests
also cover hostile labels, malformed projection rejection, deterministic output,
missing/inapplicable strand flags, strict mode and empty stdout on failed exports.

Remaining work: richer biological product-feature reconstruction can follow
validated assembly semantics. This initial export intentionally favours explicit
provenance over false biological precision. [Restriction/ligation product simulation](ligation.md) is now implemented with
component annotation placements. Gibson cloning remains required before GUI work.

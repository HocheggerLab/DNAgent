# Inspection, coordinates and exports

Commands below assume `$DNA` identifies the verified executable and `source.dna`
is an authorised local file. File names are placeholders, not supplied constructs.

```bash
"$DNA" inspect source.dna --output json
"$DNA" features source.dna --output json
"$DNA" primers source.dna --output json
"$DNA" sequence source.dna --output json
"$DNA" sequence source.dna --range 100..300 --output json
"$DNA" map source.dna --out fresh-map.svg
```

`inspect` is the entry point: verify the name, length and topology against the
intended source, and read its warning envelope before designing. Record names may
come from the file stem; a familiar filename is not sequence identity. For exact
projection fields inspect the current output schema instead of inventing JSON keys.

## Coordinate and annotation interpretation

- `100..300` selects 200 bases on the stored forward reference; zero-based end is
  exclusive. A one-based inclusive interval 101–300 becomes `[100,300)`.
- `sequence --range` is a bounded stored-axis slice, **not** a circular-arc or
  reverse-complement option. For a requested origin-spanning view, retrieve the
  two stored intervals separately and label them; do not pretend `start > end`
  wraps. Gibson plans separately support explicit circular source arcs using
  `start` plus `length`.
- Feature locations preserve multipart operators, source order, strand and origin
  spans. Do not turn multipart parts into a min/max interval or assume all strands
  are known. Qualifiers are ordered `{key,value}` arrays: repeats and null values
  are meaningful, not a dictionary to deduplicate.
- `primers` reports retained name/sequence/nullable description in source order.
  They are historical metadata, **not** binding-site predictions or reusable
  experimentally validated oligos.
- Some opaque packets/raw XML are retained internally but are absent from JSON
  and sequence exports. Absence of a visible field is not evidence the source
  lacked it. Warning-free import still does not guarantee complete fidelity.

## Annotated digest views

```bash
"$DNA" fragments source.dna --enzymes EcoRI,BamHI --output json
"$DNA" fragments source.dna --enzymes EcoRI,BamHI --output fasta
"$DNA" fragments source.dna --enzymes EcoRI,BamHI --output genbank --strand top
"$DNA" fragments source.dna --enzymes EcoRI,BamHI --output genbank --strand bottom
```

Inspect the JSON projection before deciding which export serves the task.

- Each fragment has **two physical strand sequences**, each stored 5′→3′. Strand
  lengths and starts can differ because of sticky ends. Source starts are on the
  original forward axis; a bottom-strand view already accounts for reversal.
- Source-linked feature projections retain original annotations and qualifiers as
  provenance, source offsets, coverage and clipping/split-part flags. `complete`
  describes retained bases/associations, **not** biological function or intact CDSs.
- FASTA writes separately labelled top and bottom strand records. Do not count them
  as two independent double-stranded constructs, or call it a native plasmid export.
- GenBank requires explicit `--strand top|bottom`; `--strand` is rejected for other
  formats. It exports conservative selected-strand views as source-linked
  `misc_feature` pieces. It does not invent joins, gene reunions, translations,
  `codon_start` or protein IDs. Unknown feature orientation stays explicit in notes.
- Full provenance and duplex information stay in JSON. These exports are **digest
  views**, not direct ligation/Gibson product exports or faithful SnapGene round trips.
- FASTA/GenBank are raw export streams, not JSON envelopes; warnings/errors use
  stderr. Capture exit status before accepting an export. Do not feed them to a
  JSON parser or discard stderr because the output looks like sequence text.

For an annotated **assembly product**, use the product exports rather than digest
views: `gibson`/`gibson-optimise --out product.gb` (carried source features, primer
sites, overlaps; clipped features left out with warnings; see
[Gibson workflows](gibson.md)) or `gibson-assemble --output genbank` (conservative).
Never hand-build a product GenBank with apparently intact genes.

## Viewing

`map` writes deterministic SVG and emits JSON status/provenance; it modifies no source.

The main desktop app is the separate DNAgent (Tauri) application; exchange files with
it through its workspace ([Desktop handoff](handoff.md)). It is not launched from the CLI.

The optional `gui` command exists only in a CLI built with `--features gui`:

```bash
cargo build --manifest-path "$DNAGENT_REPO/Cargo.toml" -p dnagent-cli --features gui --locked
"$DNA" gui source.dna
```

Only launch interactively when requested and a display is available. It is the
older basic egui viewer, not the desktop app and not the validated cloning workflow. GUI does not support `--strict`. Prefer headless
commands for analysis, and do not automate desktop clicks to bypass CLI refusals.

Canonical app references: `README.md`, `docs/fragment-annotations.md`,
`docs/genbank-export.md`, and the current `schemas/cli-envelope-*.schema.json`.

## Gene loci and isoforms

A `GENE.locus.json` bundle (exported by `degron-db locus <SYMBOL>`) opens as a linear
record: one `mRNA` (or `misc_RNA`) feature per transcript joining its exons, and a `CDS`
feature labelled `<transcript> CDS` that includes the stop codon. `features`, `sequence`
and `translate --feature` work as usual.

`dnagent isoforms GENE.locus.json --output json` returns every transcript ordered by mean
expression across the cell-line panel for the chosen quantifier (default `bambu_lr`;
`--quantifier` switches, an unknown name fails and lists the known ones), with exons, CDS,
start/stop codon positions, evidence (`quantified`, `discoverable`, `invisible`), long-read
mappings and per-cell-line means. `display` is the colour state the desktop draws
(`expressed`, `not_detected`, `discoverable`, `invisible`, `no_data`). Positions are
zero-based on the locus; genomic (1-based) = `genomic_start` + position. Real-gene bundles
contain unpublished lab data: keep them out of Git and shared outputs. In the desktop app,
the **Isoforms** tab shows the same view; a region the user marks there arrives in a handoff
as an ordinary selection.

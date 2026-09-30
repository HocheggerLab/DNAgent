# Isoform viewer

A gene locus opens as an ordinary DNAgent document: the genomic slice (plus strand, with
flanks) is the sequence, and every transcript is an `mRNA` feature (`misc_RNA` if
non-coding) joining its exons, plus a `CDS` feature that includes the stop codon. The
desktop app adds an **Isoforms** tab for such documents; `dnagent isoforms` reports the same
view for agents.

## Getting a locus

degron-db exports one gene at a time (Python → Rust handover through a file; the
database never enters DNAgent):

```bash
# in ~/code/degron-db
uv run degron-db locus HAPSTR1            # → data/export/HAPSTR1.locus.json
```

Open the `.locus.json` with **Browse…**, the path field, or by saving it into the
workspace (`~/DNAgent`), where it is offered as a new tab. `.locus.json` is the only JSON
the workspace watcher offers.

## What the view shows

- One row per transcript: exons as boxes (thick = CDS, thin = UTR), introns as lines,
  start and stop codons as red marks (split codons are drawn on both exons).
- Colour is the long-read evidence for the chosen quantifier:
  - **orange**: quantified (matched to a long-read transcript model) and expressed;
  - **blue**: quantified but zero across the panel, or *discoverable* (it has a novel
    junction, so long reads could have found it) but not found;
  - **grey, hatched**: not distinguishable in long reads (no matching model and no
    junction that sets it apart), or no values from this quantifier.
- Rows are ordered by the **mean expression across the cell-line panel** for the quantifier
  chosen in **Order by** (default `bambu_lr`), then by evidence, MANE Select first, then id.
  Dense quantifiers (which report zeros) average the cell lines they measured; sparse ones
  (NanoCount) count a missing cell line as 0. A transcript with no rows at all has no data
  (null mean), not zero.
- **True scale** by default, with genomic coordinates on the axis; **Compress introns**
  draws every stretch not covered by any exon as a fixed short gap (marked ⫽). Loci over
  100 kb start compressed (at true scale an exon would be narrower than a pixel).
- Zoom with **+ / − / Fit**, ⌘ + scroll, or **Zoom to selection**; scroll sideways to pan.

## Interaction

- **Click an isoform**: selects its mRNA feature, highlights its exons as bands across all
  rows (to compare structures), and opens the expression panel below the drawing: a bar per
  panel cell line (mean, with the min–max range), library counts, the panel mean, the
  evidence in words and the long-read model it matched.
- **Drag across the exons** to mark a region; the ends snap to exon and CDS edges within a
  few pixels. **Double-click an exon** to mark exactly that exon. **Show in Sequence** opens
  the marked region (or the selected isoform) in the Sequence view.
- **Export SVG…** (here and on the Map) writes the drawing as a standalone SVG with the
  theme's colours resolved.

## CLI

```bash
dnagent isoforms GENE.locus.json                       # text table
dnagent isoforms GENE.locus.json --quantifier NanoCount_lr --output json
```

The JSON result (`isoforms` in `schemas/cli-envelope-0.9.0.schema.json`) lists the
quantifiers with their cell lines and order, and per isoform its exons, CDS, codon marks,
evidence, long-read mappings and per-quantifier expression with the panel mean and display
state. Other commands work as for any record (`features`, `translate --feature`, `sequence`, …).

## Bundle format (`dnagent-locus` v1)

```text
format: "dnagent-locus", version: 1
gene {symbol, gene_id, chrom, strand "+"|"-", gene_type}
annotation_version, genomic_start (1-based genomic coordinate of position 0), genomic_end, flank
sequence                       plus strand of chrom
transcripts[] {
  transcript_id, transcript_type, is_mane_select, tsl, appris, cds_start_nf, cds_end_nf,
  protein_id, aa_len, protein,
  exons [[start, end]], cds [[start, end]]     zero-based, half-open on `sequence`
  cds_includes_stop, codon_start (1-3)
  start_codon / stop_codon {position, split}   first base in transcription direction
  evidence {state quantified|discoverable|invisible, mappings[], junctions, novel_junctions}
  expression {quantifier: [{cell_line, n_libraries, mean, min, max}]}
}
panel {quantifier: {cell_lines [{cell_line, libraries}], reports_zeros}}
expression_units, novel_transcripts, provenance[]
```

Coordinates are converted from degron-db's 1-based inclusive intervals once, in the
exporter. Cell-line names are passed through as degron-db stores them (not normalised).

## Saving

Saving writes GenBank as for any document. The locus metadata (evidence, expression,
codon marks; not a second copy of the sequence) is carried in the DNAgent data block, so a
saved locus reopens with the same Isoforms view. Deleting an isoform's mRNA feature removes
its row; the view reports it as missing rather than drawing it from the metadata.

## Validation

- `cargo test` covers the reader, ordering and panel means, the GenBank round trip and the CLI.
- `uv run scripts/check_locus.py --binary target/debug/dnagent [--bundle FILE ...]`
  recomputes everything above from the bundle: feature parts, spliced CDS translation
  (Biopython), codon positions, panel means, display states, order, and the GenBank round trip.
- e2e scenarios `isoforms-order-structure` and `isoforms-select-zoom-sequence` check the
  drawing against `dnagent isoforms` on both strands and with compressed introns.

## Large loci

The desktop app opens ordinary records up to 100 kb, and gene loci up to 3 Mb (the longest
human genes, with flanks). Over 100 kb:

- the Isoforms tab starts with introns compressed;
- the Sequence view shows a window of at most 50 kb around the selection (a marked region,
  or the selected feature/isoform, plus 60 bases either side) instead of the whole record;
  with nothing selected it explains how to choose a region. Show in Sequence from the
  Isoforms tab is the intended route;
- ORFs, six-frame translation and range translation are not computed (New feature…
  still previews a translation; `dnagent orfs`/`translate --range` work on any size);
- restriction sites are scanned only for enzymes you **Choose…**; the unique-cutter sets
  need counts over the whole catalogue, which take seconds per megabase.

Larger loci open in the CLI. The e2e suite generates a 1.2 Mb locus
(`generate_synthetic_locus.py --large`) rather than committing it.

## Limits

- One gene per bundle; novel (unannotated) long-read transcripts are counted
  (`novel_transcripts`) but not drawn.

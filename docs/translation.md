# Translation and ORFs

`dnagent translate` and `dnagent orfs` translate DNA with pinned NCBI genetic codes.
The same engine (`dnagent-domain::translation`) feeds the desktop Sequence view and map.
Results are computational: they do not establish genes, expression, splicing or
experimental validity.

```bash
dnagent translate construct.dna --feature feature-0001 --output json
dnagent translate construct.dna --all-cds --output json
dnagent translate construct.dna --range 100..400 --strand reverse --frame 1 --table 11
dnagent orfs construct.dna --min-codons 75 --starts atg --table 1 --output json
```

## Genetic codes

All 27 tables of NCBI `gc.prt` version 4.6 (1–6, 9–16, 21–33), committed as
`references/ncbi/gc.prt` with its SHA-256 and generated into Rust by
`scripts/generate_genetic_codes.py`. Every result reports `genetic_code_source`. Table 1
is the default; a feature's `transl_table` qualifier is honoured, and `--table`
overrides both. In tables 27, 28 and 31 the context-dependent stop codons are
translated as amino acids (the `gc.prt` amino-acid row), never as stops.

## Translation rules

- **Coordinates:** each codon reports the zero-based forward-reference positions of its
  three coding bases, in coding order. They decrease on the reverse strand and can jump
  at a join.
- **Features:** parts are spliced in source order. A reverse-strand feature is the
  reverse complement of that splice, the GenBank `complement(join(a,b))` reading.
  Circular parts may cross the origin. `codon_start` (1–3) skips bases at the 5′ end. A
  feature with unknown strand is not translated; `--all-cds` lists it under `skipped`
  with a warning.
- **Ranges:** half-open `START..END` on the forward reference. `END < START` wraps
  through the origin on circular records only. `--strand reverse` reads the reverse
  complement from `END − 1`. `--frame` skips 0–2 bases at the 5′ end of the coding strand.
- **Ambiguity:** a codon translates to an amino acid only when every IUPAC expansion
  agrees (`CTN` → L, `AAN` → X). A stop requires every expansion to be a stop. Starts
  must be exact. Biopython prints B/Z/J for some ambiguous pairs; DNAagent prints X.
- **Initiator:** a complete feature CDS (codon_start 1, a table start codon, a terminal
  stop) whose first codon is an alternative start (e.g. GTG in table 11) is translated with
  M, and `initiator_as_methionine` is true. This is the NCBI and Biopython `cds=True`
  convention. It is never applied to ranges.
- **Reporting:** `protein` keeps every stop as `*`. The result also lists
  `internal_stops`, `terminal_stop`, `trailing_bases` (0–2), `ambiguous_codons` and
  `starts_with_atg`.
- **Imported translations:** a feature's `translation` qualifier is compared after
  removing whitespace, SnapGene's per-part commas and one terminal `*`. A mismatch is the
  structured warning `translation_imported_mismatch`, giving the first differing residue.
  Nothing is corrected silently.

Feature translation warnings are `translation_internal_stop`,
`translation_incomplete_codon`, `translation_ambiguous_codons`,
`translation_imported_mismatch` and `translation_skipped`. `--strict` rejects any of them,
like import warnings. Failures use the error code `translation_failed`.

## ORFs

An ORF is a complete start-to-stop reading frame on either strand.

- **Starts:** ATG only by default; `--starts table` accepts every start codon of the table.
- **One ORF per stop:** the longest, starting at the first start after the previous
  in-frame stop. Nested starts sharing that stop are not reported separately.
- **Minimum length:** at least `--min-codons` amino acids, excluding the stop (default 75).
- **Circular molecules:** an ORF may cross the origin (`wraps_origin`) but never exceeds
  one molecule length. A frame with no stop at all yields no ORF.
- **Linear molecules:** an ORF must end with a stop inside the sequence.
- **Order and ids:** ORFs are sorted by start, then forward before reverse, and numbered
  `orf-0001` onwards. Ids depend on the search parameters.

`start`/`length` give the lowest covered forward coordinate and the length including the
stop. `frame` is the 5′ codon base's offset from the 5′ end of its strand, modulo 3. It
matches the desktop's six-frame offsets.

## Desktop

The desktop document carries, all computed by the engine:
- translations of every CDS (with flattened codon positions and warnings);
- six whole-molecule frame translations (table 1; codons crossing the origin omitted);
- ORFs of at least 30 codons (table 1, ATG).

The GUI only places them:
- Amino acids sit under each codon's middle base.
- The ORF length control filters from 30 codons upwards.
- A dragged range's translation is a slice of the matching engine frame: forward codons
  start at the first selected base; reverse codons end at the last. This equals
  `dnagent translate --range` for ranges that don't cross the origin.

## Validation

- `crates/dnagent-domain/src/translation_tests.rs`: hand-derived cases for tables,
  ambiguity, joins with a split codon, origin wrap, codon_start, initiator M, ORFs and six
  frames.
- `crates/dnagent-cli/tests/translation.rs`: JSON contract, warnings, strict mode, errors.
- `uv run scripts/check_translation.py --binary target/debug/dnagent`: Biopython 1.85
  re-derives every CDS translation from the CLI's locations and sequence, 768 ranges
  (both strands, three frames, tables 1 and 11, wrapping), all 27 tables codon by codon,
  and ORFs from an independent implementation of the definition above.
- `fixtures/formats/snapgene/synthetic_translation.dna` covers the edge cases. The
  private construct used during development matched SnapGene's embedded translations for
  all seven CDSs, including a two-part AmpR; that file is not committed.
- Desktop e2e scenarios check the displayed amino acids, frames, ORFs and range
  translations against the CLI.

## Limits

- No protein properties (mass, pI), codon usage, optimisation or splicing prediction.
- No reading-frame repair across assembly junctions.
- ORFs are not genes.
- Six-frame display omits origin-crossing codons, and range selection by dragging
  doesn't cross the origin (the CLI `--range` does).

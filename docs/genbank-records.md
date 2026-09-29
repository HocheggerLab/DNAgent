# GenBank records (read and write)

DNAgent opens `.gb`, `.gbk` and `.genbank` files and saves records as GenBank. SnapGene
`.dna` files are read-only: saving never modifies them. A `.dna` writer is not
implemented. GenBank is the working format, and it keeps everything a later `.dna`
writer would need.

```bash
dnagent convert construct.dna --out construct.gb
dnagent annotate construct.dna --out edited.gb --add --range 1250..6188 --label "fusion ORF" --translate
dnagent annotate edited.gb --out edited.gb --remove feature-0021
dnagent features edited.gb --output json
```

The desktop app saves through the same code (**Save** / **Save as…**). Writes go to a
temporary sibling file and are then renamed over the target.

## What is written

The body is ordinary GenBank that Biopython, NCBI tools and other editors read normally.

- **LOCUS:** the record name (non-printable characters and spaces become `_`), length,
  topology and the save date. DEFINITION through COMMENT headers read from a GenBank
  input are written back verbatim. Otherwise a minimal synthetic-construct header is written.
- **Features:** in source order.
  - **Key:** the feature kind.
  - **Location:** standard notation. A reverse join is `complement(join(a,b))`, in
    DNAgent source order. An origin-spanning part is split into `join(x..end,1..y)`.
  - **Qualifiers:** every original qualifier, in order. Repeated and valueless ones are
    kept.
- **Sequence:** lowercase, 60 bases per line.

GenBank has no standard form for some things DNAgent models. These additions carry them:

| Addition | Holds |
| --- | --- |
| `/label` (always first) | the feature label; an imported `label` qualifier survives as the second one |
| `/dnagent_color` | the display colour |
| `/dnagent_location="v1;<operator>;<strand>;<parts>"` | the exact location when the standard one cannot express it: origin-spanning single parts (`A<start>+<length>`) or unknown strand |
| `/dnagent_id` | a feature id that differs from its position-based default |
| COMMENT block `BEGIN-DNAGENT-DATA` … `END-DNAGENT-DATA` (files with the pre-rename `…DNAAGENT…` markers still open) | JSON: the original record name, unplaced primers, uninterpreted SnapGene packets (base64, marked opaque or original interpreted source) and the source's import warnings |

The comment block is chunked JSON, with spaces escaped as ` `, so line trimming by
editors cannot change it. It deliberately avoids NCBI structured-comment syntax
(`##…##`), which other parsers would try to read. On reading, the block restores the name,
primers and packets. The original import warnings are re-reported, so a record that lost
something at its first import still says so.

Translated features added in DNAgent are CDSs with `/codon_start`, `/transl_table` and a
computed `/translation` (without the terminal stop, following GenBank convention).

## What is reported, not silently lost

- **When saving:** control characters (e.g. line breaks) in labels and qualifiers cannot
  be stored in GenBank. They are replaced by spaces and reported as
  `genbank_control_characters_flattened`. `--strict` refuses to write in that case, and
  nothing is written.
- **When reading third-party GenBank:**
  - `<`/`>` partial-end markers are dropped (`genbank_partial_location`).
  - Remote (`J00194.1:100..202`), between-base (`a^b`) and mixed-strand locations skip
    the feature (`genbank_feature_skipped`).
  - A `/dnagent_location` that disagrees with the standard location is ignored
    (`genbank_dnagent_location_ignored`).
  - A missing LOCUS topology imports as linear (`genbank_topology_missing`).
- **Individually complemented joins:** `join(complement(B),complement(A))` is read as
  `complement(join(A,B))` (the same sequence) and written back in the second form.
- **Labels:** features without `/label` take their label from `/gene`, `/product`,
  `/locus_tag`, `/standard_name` or `/note` (the qualifier stays), else the key.

## Validation

- Unit tests in `crates/dnagent-formats/src/genbank_record_tests.rs`:
  - a lossless round trip of a hand-built record, covering reverse joins, origin arcs,
    unknown strand, `order`, custom ids, quotes, repeated/valueless qualifiers, long
    translations, primers, both packet roles and warnings;
  - byte-identical rewrites;
  - hand-written third-party GenBank;
  - whitespace trimming at chunk boundaries;
  - malformed input.
- CLI tests (`crates/dnagent-cli/tests/editing.rs`): every public SnapGene fixture
  converts to GenBank and reopens with identical `inspect`, `features`, `primers` and
  translations. A second save is byte-identical.
- `uv run scripts/check_genbank.py --binary target/debug/dnagent`: Biopython 1.85 parses
  every written file. Its sequence, topology, keys, labels, strands, part-by-part
  locations and qualifiers must equal the CLI's view of the original. DNAgent must also
  read NCBI's own `pUC19_M77789.gb`, and a Biopython-written copy of it, exactly as
  Biopython does.
- The private construct used during development round-trips identically (warnings
  included; ten SnapGene packets carried along). It is not committed.

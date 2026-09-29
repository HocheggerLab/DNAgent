# Feature library

DNAgent keeps a **feature library**: a SQLite database of annotated parts collected from
your sequence files. `dnagent detect-features` finds those parts in any construct.

```bash
dnagent library import ~/constructs            # folders are searched recursively
dnagent library info
dnagent library list --kind CDS --limit 20
dnagent library search puro
dnagent library show 245                       # sequence, qualifiers, every file it was seen in
dnagent library edit 245 --name "PuroR (codon-optimised)"
dnagent library edit 312 --hide                # e.g. a partial copy
dnagent detect-features construct.gb --new-only --output json
```

All commands take `--output json` (envelope 0.9.0). The library lives at
`~/Library/Application Support/DNAgent/features.sqlite` on macOS
(`$XDG_DATA_HOME/dnagent/features.sqlite` elsewhere). Use `--db PATH` or
`DNAGENT_FEATURE_DB` to choose another file.

**The library is private data.** It is built from your constructs, so it is never
committed and never bundled. Tests use the public fixtures only.

## What is collected

Each annotated feature is collected unless it is:

| Skip reason | Rule |
| --- | --- |
| `source_feature` | a GenBank `source` feature (it describes the record) |
| `whole_molecule` | as long as the molecule |
| `too_short` | shorter than 12 bp (`--min-length`); short motifs match by chance |
| `generic_name` | unnamed, named after its type, or a placeholder ("Feature 3", "New feature", "untitled", "region", …) |
| `sequence_as_name` | named by a DNA sequence of 8+ letters (an unnamed primer) |
| `ambiguous_bases` | contains a base other than A, C, G or T |
| `incomplete_location` | some segments could not be imported (the file's import warnings say which), so its bases are not the feature it names |

Imported primers (SnapGene's primer list) are not features and are not collected.

A feature's sequence is its bases 5′→3′ in its own direction: parts joined in order,
through the origin if they wrap, reverse-complemented for reverse features.

## Identity, names and provenance

- **Identity is the exact sequence, on either strand.** The same part on the reverse
  strand, or under another name, is the same library feature. Point variants and
  codon-optimised versions are separate features (e.g. several 600 bp "PuroR").
- **Name, type and colour** are the most common among the feature's occurrences (ties:
  first seen). Other names are **aliases**, and `search` matches them too.
- **Occurrences** record every file (path and SHA-256), the label and type used there,
  and where it starts.
- **Re-importing is safe.** Unchanged files are skipped. A file whose content changed
  replaces its earlier occurrences. A copy of an imported file is reported as a
  `duplicate`. Features no file carries any more are removed.
- **Curation:** `library edit` renames, retypes, hides or unhides a feature. Edited
  features keep your changes on later imports, and hidden ones are not detected.
- **Strand:** if a feature is usually annotated without a strand, detections report
  strand `unknown`.

Every file is reported: `imported`, `unchanged`, `duplicate` or `failed` with a reason
(for example "is not a sequence file: it contains a PDF document").

## Detection

`detect-features` reports every exact occurrence of every visible library feature
(12 bp or longer by default) on both strands, through the origin of circular molecules.
Results are ordered by start, longer first. Each match carries its library id, name,
type, colour, qualifiers, a standard location (`linear` or `circular_arc`) and
`annotated_as`: existing features with exactly that span and a compatible strand.
`--new-only` drops those. Nested and overlapping matches are all reported (e.g. "lac
promoter" and "lac"). Sequences containing N can't match across the N.

## Opening files: format by content

Sequence files are read by what they contain, not only by their extension:

- GenBank text named `.dna` is read as GenBank, with a `content_format_mismatch` warning.
- Office documents, PDFs and images named like sequence files fail with a clear message.
- A `.dna` file without the SnapGene header is reported as an older or different format.

GenBank reading also accepts two things other editors write:

- ApE's unquoted qualifier values that continue onto the next line.
- `*` placeholders in ORIGIN. These are read as N, so feature coordinates after them stay
  right, with a `genbank_sequence_placeholder` warning. A LOCUS length that disagrees
  with ORIGIN gives `genbank_length_mismatch`.

## Validation

```bash
cargo test -p dnagent-library -p dnagent-app -p dnagent-cli
uv run scripts/check_feature_library.py --binary target/debug/dnagent
uv run scripts/check_feature_library.py --binary target/debug/dnagent --collection ~/constructs  # optional, private
```

`check_feature_library.py` builds a library and compares it with an independent
implementation. Biopython 1.85 parses every file and applies the rules above, and a
brute-force scan checks every detection. Three Biopython differences are handled
explicitly:

- It turns SnapGene primers into `primer_bind` features, so the primers packet is removed
  first.
- It lets a `label` qualifier replace a SnapGene feature's name, where SnapGene and
  DNAgent show the name. Names are read from the feature XML.
- It orders the pieces of a reverse feature that wraps the origin incorrectly. A feature
  whose parts tile one contiguous span is read from that span.

On the lab collection (2026-09-30), 281 files and 719 features from 4,472 occurrences
agreed, and 281 detection scans (13,099 matches) equalled the brute-force search.

## Not yet

- Detect features from the desktop app (next).
- Fuzzy matching for near-identical variants, and grouping of variants.
- Integration with the lab inventory database.

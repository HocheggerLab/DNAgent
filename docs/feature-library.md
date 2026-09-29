# Feature library

DNAgent keeps a **feature library**: a SQLite database of annotated parts collected from
your sequence files. `dnagent detect-features` finds those parts in any construct.

```bash
dnagent library import ~/constructs            # folders are searched recursively
dnagent library info
dnagent library list --kind CDS --limit 20    # one row per variant family; --all for every feature
dnagent library search puro
dnagent library show 245                       # sequence, qualifiers, every file it was seen in
dnagent library edit 245 --name "PuroR (codon-optimised)"
dnagent library edit 312 --hide                # e.g. a partial copy
dnagent library edit 88 --standalone           # keep out of a variant family
dnagent library import --rescan ~/constructs   # re-read unchanged files after rule changes
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
| `generic_name` | unnamed, named after its type, a placeholder ("Feature 3", "New feature", "untitled", "region", …), without any letter or digit (e.g. `"`), or starting with "(null)" |
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
- **Variant families:** the same part is often annotated with slightly different ends
  (ori 589 bp, pBR322_origin 620 bp, ColE1 origin 629 and 683 bp; AAV2 ITR 130 and
  141 bp). A feature joins a **family** when its sequence lies inside a longer one (either
  strand) and is at least 80 % of its length; the family head is the longest such
  feature. Precisely: features are taken longest first, and each joins the longest
  existing head that contains it within that length ratio, otherwise it heads its own
  family (no chains). Nested parts much shorter than their container (tet operator in a
  TRE promoter, T7 promoter in lacZα) stay separate. `library list` shows one row per
  family, ranked by the family's total occurrences, with a variant count; `library
  show` lists the family. Every variant stays in the library and is still detected on
  its own. `library edit --standalone` keeps a feature out of families (e.g. a homology
  arm that happens to lie inside an exon); `--grouped` undoes it. Families are
  recomputed after imports and edits. Point variants and codon-optimised versions are
  not substrings of each other and are not grouped (yet).
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
`--new-only` drops those. `family_id` names the match's family; `superseded_by` is the
index of the longest match of the same family whose span contains it (a shorter
variant at the same place), else null. All matches are listed; the desktop folds
superseded ones into their longer match. Nested and overlapping matches are all reported (e.g. "lac
promoter" and "lac"). Sequences containing N can't match across the N.

## In the desktop app

**Detect** (next to the feature list heading) runs the same detection on the active tab
and opens the **Detected** panel:

- one row per match, with type, length, position, strand, and "already annotated" or
  "inside <longer match>". Shorter variants of the same family at the same place are
  folded into the longer match ("+2 shorter variants"); a short variant on its own
  still gets its own row;
- rows that are new and not inside a longer match start ticked. Tick or untick freely;
- clicking a row selects its span. New matches are drawn on the map as dashed arrows
  (solid when ticked);
- **Add N features** adds the ticked ones as a single undoable edit. Each gets the
  library name, type and colour, a translation for CDSs, and a `/note` naming the
  library entry. The panel then refreshes, so added matches show as annotated.

Without a library the panel says how to build one. The app reads the library at the
default location (or `DNAGENT_FEATURE_DB`).

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

The script also recomputes variant families and `superseded_by` independently. On the
lab collection (2026-09-30): 281 files, 716 features from 4,466 occurrences, 614
families; 281 detection scans (13,096 matches, 5,041 of them folded variants) agreed.

## Not yet

- Similarity-based grouping of point variants and codon-optimised versions.
- Integration with the lab inventory database.

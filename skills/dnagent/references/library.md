# Feature library and detection

A private SQLite library of annotated parts collected from the user's constructs, at
`~/Library/Application Support/DNAgent/features.sqlite` on macOS (`--db PATH` or
`DNAGENT_FEATURE_DB` to choose another). It is **private lab data**: never copy it into
a repository or report its contents beyond what the user asked for.

```bash
"$DNA" library import ~/constructs --output json          # folders searched recursively
"$DNA" library import --rescan ~/constructs               # re-read unchanged files after rule changes
"$DNA" library info --output json
"$DNA" library list --kind CDS --limit 20 --output json   # one row per variant family; --all for every feature
"$DNA" library search puro --output json
"$DNA" library show 245 --output json                     # sequence, qualifiers, family, where seen
"$DNA" library edit 245 --name "PuroR" | --kind CDS | --hide | --unhide | --standalone | --grouped
"$DNA" detect-features construct.gb --new-only --output json
```

- Identity is the exact sequence on either strand; names are the most common label,
  other labels are aliases. Import reports every file (imported, unchanged, duplicate,
  failed with a reason) and skipped features by reason (short, placeholder names,
  ambiguous bases, incomplete locations, …).
- **Variant families** group a feature under a longer head when it is contained in it,
  or shares a name and is ≥ 97 % DNA- or ≥ 98 % protein-identical (≥ 80 % length).
  Differently named point mutants stay separate by design. `show` gives each variant's
  relation and identity.
- `detect-features` reports every exact match (≥ 12 bp, both strands, across the
  origin) with library id, name, kind, colour, qualifiers, location, `annotated_as`
  (existing features at that exact span), `family_id` and `superseded_by` (a shorter
  variant inside a longer match of the same family).

To annotate detected parts, add them with `annotate --add` (one call per feature, into
a new GenBank file) or, in the desktop app, the user can use **Detect**. Only change
the library (`import`, `edit`) when the user asks.

Canonical app reference: `docs/feature-library.md`.

# Editing, translation and writing GenBank

`$DNA` is the verified executable. Writes always go to a path you choose; pick a fresh
one unless the user asked to update a GenBank file in place. `.dna` sources are never
modified (saving a `.dna` always means writing a new `.gb`).

## Translation and ORFs

```bash
"$DNA" translate FILE --feature feature-0003 --output json   # honours codon_start, transl_table
"$DNA" translate FILE --all-cds --output json
"$DNA" translate FILE --range 1250..6188 --strand forward --frame 0 --output json
"$DNA" orfs FILE --min-codons 100 --starts atg --table 1 --output json
```

- Feature translations follow the annotation's parts, strand, `codon_start` and
  `transl_table`; `imported_matches` compares with an imported `/translation`.
  Warnings flag a non-start first codon, internal stops, a missing stop or a partial
  codon. Use them to check fusions and junctions: an in-frame fusion translates with no
  internal stop across the junction.
- `--range END < START` wraps on circular records. Only whole codons are translated.
- ORFs are computational (start to stop, all six frames), not annotated genes.

## Save and annotate

```bash
"$DNA" convert source.dna --out source.gb
"$DNA" annotate source.gb --out edited.gb --add --range 60..660 --label "my ORF" --translate
"$DNA" annotate edited.gb --out edited.gb --add --range 5..40 --label tag --kind misc_feature --strand forward --color "#4f8fe6"
"$DNA" annotate edited.gb --out edited.gb --remove feature-0005
```

- `convert` writes DNAgent GenBank losslessly for everything DNAgent models (labels,
  colours, exact multipart/origin locations, unplaced primers, SnapGene-only packets and
  the source's import warnings in a DNAgent comment block). Limitations it cannot keep
  are reported as warnings, never dropped silently.
- `annotate --add` makes one single-part feature over `--range` (`END < START` wraps).
  `--translate` makes a CDS with `codon_start`, `transl_table` and a computed
  `/translation`, and reports translation warnings. `--remove ID` deletes any feature.
  Ids come from `features`.
- Both return a JSON report (path, features, warnings). Check `ok` before reporting a
  file as written.

Canonical app references: `docs/genbank-records.md`, `docs/translation.md`.

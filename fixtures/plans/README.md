# Public synthetic plans

- `synthetic-religation.json`: original restriction fixture re-ligated linearly.
- `synthetic-closure.json`: reverse-oriented restriction fixture reclosed circularly.
- `synthetic-gibson.json`: two PCR-derived cores, including an origin-spanning
  interval and a reversed interval, assembled circularly with designed tails.
- `synthetic-gibson-optimisation.json`: bounded variable annealing-length search
  with explicit illustrative solution and screening constraints. The second core
  differs from the fixed-length example; these are separate declared designs.
- `synthetic-gibson-existing.json`: two declared linear views of the synthetic
  circle, merged across existing 25-base homology to recover its 300-base sequence.
- `synthetic-gibson-mixed.json`: a selected EcoRI digest strand, literal ordered
  bridge and ideal tailed PCR product assembled circularly; exercises mixed source
  loading, primer retention and product materialisation.
- `synthetic-cdna-into-puc19.json`: the 600 bp CDS of `../formats/genbank/synthetic_cdna.gb`
  cloned into public pUC19 (M77789.2) in place of its polylinker (backbone core through
  the origin); the Gibson product GenBank and desktop agent scenario use it.
- `synthetic-cdna-into-puc19-optimised.json`: the same design with Tm-constrained primer
  search (`gibson-optimise`); the `gibson-cloning` skill's worked example.

The ligation plans reference existing public synthetic format fixtures. The Gibson
source is a deterministic synthetic original, with no imported biological sequence;
regenerate it with `python3 fixtures/plans/generate_gibson.py`. Features are artificial
coordinate-test annotations, not functional claims. Plans resolve paths relative to
the plan directory. No private construct, lab plan or private report belongs here.

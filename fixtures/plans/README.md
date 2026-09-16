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

The ligation plans reference existing public synthetic format fixtures. The Gibson
source is a deterministic synthetic original, with no imported biological sequence;
regenerate it with `python3 fixtures/plans/generate_gibson.py`. Features are artificial
coordinate-test annotations, not functional claims. Plans resolve paths relative to
the plan directory. No private construct, lab plan or private report belongs here.

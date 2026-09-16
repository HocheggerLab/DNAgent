# Public synthetic plans

- `synthetic-religation.json`: original restriction fixture re-ligated linearly.
- `synthetic-closure.json`: reverse-oriented restriction fixture reclosed circularly.
- `synthetic-gibson.json`: two PCR-derived cores, including an origin-spanning
  interval and a reversed interval, assembled circularly with designed tails.

The ligation plans reference existing public synthetic format fixtures. The Gibson
source is a deterministic synthetic original, with no imported biological sequence;
regenerate it with `python3 fixtures/plans/generate_gibson.py`. Features are artificial
coordinate-test annotations, not functional claims. Plans resolve paths relative to
the plan directory. No private construct, lab plan or private report belongs here.

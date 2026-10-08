# DNAgent

**An agent-friendly DNA design and cloning workbench: a desktop app and a CLI on one Rust
engine.**

DNAgent is an open replacement for the plasmid viewing and cloning work usually done in
SnapGene, built so that a person at the desktop and an AI agent in a terminal work on the
same constructs with the same engine.

> Research software from the [Hochegger Lab](https://github.com/HocheggerLab). It predicts
> sequences and products; it does not validate experiments. Check designs before ordering.

## Start here

- **[Installing](install.md)** — macOS and Windows, and how to connect an agent.
- **[Agent handoff](agent-handoff.md)** — how the desktop app and an agent share work.
- **[Restriction sites](restriction.md)** — the first biological operation, and the model
  the rest of the engine follows.
- **[Gibson cloning](gibson.md)** — the full insert-into-vector workflow.

## How to read these pages

Each one is a **contract**, not a tutorial: what the command does, what it refuses to do,
how its output is validated and against what. Where a page says a result is checked
against Biopython, the NCBI genetic codes or a brute-force scan, that check runs in CI.

Two conventions hold everywhere:

- Coordinates are **zero-based and half-open**; `end < start` wraps through the origin on
  a circular molecule.
- Source files are never modified in place. Saving always writes a new path, losslessly
  for everything DNAgent models, or it reports what it could not keep.

The [repository README](https://github.com/HocheggerLab/DNAgent) covers installation and
the command list; the [API documentation](api/dnagent_domain/index.html) is generated from
the Rust crates.

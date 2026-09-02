# DNAagent

**An agent-friendly DNA design and cloning workbench.**

DNAagent is a Rust, library-first replacement for the plasmid viewing and cloning workflows commonly performed in SnapGene. Biological operations live in reusable crates and are exposed in parallel through a deterministic CLI and a desktop GUI.

## Milestone 1

The first vertical slice imports a SnapGene `.dna` file read-only and exposes the same normalized record through:

```bash
dnagent inspect construct.dna --output json
dnagent features construct.dna --output json
dnagent sequence construct.dna --range 100..300 --output json
dnagent map construct.dna --out map.svg
dnagent gui construct.dna
```

Current status: working Milestone 1 vertical slice with checked domain types, read-only sequence/feature/primer import, explicit fidelity reporting, deterministic JSON and SVG output, and synchronized Map/Features/Sequence GUI views.

## Architecture

```text
SnapGene bytes → format adapter → domain record → application projections
                                             ├── deterministic CLI / SVG
                                             └── desktop GUI
```

The GUI owns selection, hover, zoom and viewport state. It does not own file parsing or biological operations.

A redistribution-safe synthetic fixture and committed JSON contracts live in [`fixtures/formats/snapgene`](fixtures/formats/snapgene).

## Development

Rust 1.92 or newer is required.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Prior art and attribution

The narrow SnapGene migration adapter is informed by PlasCAD's MIT-licensed reader at commit `717459dcd780ec4266e9a2aa15297eae31956da5`. See [`NOTICE`](NOTICE) and [`licenses/PlasCAD-MIT.txt`](licenses/PlasCAD-MIT.txt).

## License

MIT

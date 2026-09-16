# DNAagent

**An agent-friendly DNA design and cloning workbench.**

DNAagent is a Rust, library-first replacement for the plasmid viewing and cloning workflows commonly performed in SnapGene. Biological operations live in reusable crates. Development is currently CLI-first for agent use; the desktop GUI remains a basic viewer, with further interaction work deferred.

## Milestone 1

The first vertical slice imports a SnapGene `.dna` file read-only and exposes the same normalized record through:

```bash
dnagent inspect construct.dna --output json
dnagent features construct.dna --output json
dnagent primers construct.dna --output json
dnagent sequence construct.dna --range 100..300 --output json
dnagent map construct.dna --out map.svg
dnagent enzymes --output json
dnagent sites construct.dna --enzymes EcoRI,BamHI,BsaI --output json
dnagent digest construct.dna --enzymes EcoRI,BamHI --output json
dnagent compatible-ends vector.dna --enzymes BsaI --other insert.dna --other-enzymes BsmBI --output json
# Only when built with --features gui:
dnagent gui construct.dna
```

Current status: working Milestone 1 vertical slice with checked domain types, read-only sequence/feature/primer import, explicit fidelity reporting, deterministic JSON and SVG output, and basic Map/Features/Sequence GUI views. Full cross-view selection remains unfinished.

### Restriction sites

The first biological operation finds recognition sites and nominal cut positions
for **EcoRI, BamHI, EcoRV, KpnI, BsaI and BsmBI**, including reverse orientations
and circular-origin sites. It reports blunt/5′/3′ overhang geometry, not digest
fragments. Ambiguous input is rejected; methylation and reaction conditions are
not modelled. See [restriction scope, provenance and validation](docs/restriction.md).

### Digest simulation

`digest` reports both strand sequences and lengths, paired cores and explicit
blunt/5′/3′ ends. Circular molecules and Type IIS orientations are supported.
It assumes complete cleavage; terminal, out-of-bounds and overlapping cuts are
rejected rather than approximated. This command remains sequence-only; use
`fragments` for source-linked annotation projections.
See [digest coordinates, limitations and validation](docs/digest.md).

### End compatibility and roadmap

`compatible-ends` compares all distinct ends from one or two digests, reporting
matching/mismatching polarity and oligos, plus proposed relative orientation.
It evaluates sequence compatibility, not experimental ligation or full assemblies.
See [the compatibility contract](docs/compatibility.md).

### Annotation-aware fragments and exports

`fragments` projects features onto each strand separately, retains original
qualifiers/operators as provenance, and flags clipped coverage and split source
parts. JSON preserves duplex geometry; FASTA emits two explicitly labelled
5′→3′ strand sequences per fragment. Coverage is not biological integrity.

```bash
dnagent fragments construct.dna --enzymes EcoRI,BamHI --output json > fragments.json
dnagent fragments construct.dna --enzymes EcoRI,BamHI --output fasta > strands.fasta
dnagent fragments construct.dna --enzymes EcoRI,BamHI --output genbank --strand top > top.gb
```

See [the annotation/export contract](docs/fragment-annotations.md) and
[conservative GenBank views](docs/genbank-export.md). GenBank requires an explicit
strand and uses source-linked `misc_feature` pieces, not inferred functional CDSs.
See [the roadmap](docs/roadmap.md): **Gibson cloning remains required before GUI integration**.

### Restriction/ligation products

`ligate plan.json` applies an explicit version-1 JSON plan: input digests, fragment
selection, order, orientation and linear/circular topology. It rejects incompatible
junctions and implicit fragment reuse, conserves both strands, and retains explicit
circular strand phase and component annotation placements. It does not infer gene
repair, reaction yield or experimental validation.

```bash
dnagent ligate fixtures/plans/synthetic-religation.json --output json
```

See [the plan/product contract](docs/ligation.md).

### Gibson PCR-tail candidates

`gibson plan.json` selects template intervals and orientations, designs explicit
fixed-length annealing oligos with overlap tails, and predicts a linear/circular
product with source annotation placements. Exact primer-site and overlap uniqueness
are checked. **These are primer candidates, not thermodynamically validated or
ordering-ready oligos.**

```bash
dnagent gibson fixtures/plans/synthetic-gibson.json --output json
```

Two separate, JSON-only operations extend this without changing fixed-length plans:

```bash
dnagent gibson-optimise fixtures/plans/synthetic-gibson-optimisation.json
dnagent gibson-assemble fixtures/plans/synthetic-gibson-existing.json
```

`gibson-optimise` searches annealing lengths under explicit nearest-neighbour Tm,
GC and full-oligo sequence-screen constraints. `gibson-assemble` merges declared
existing homology and retains both source associations in shared overlap regions.
Neither predicts experimental success. Hairpin/dimer checks are **sequence screens,
not folding-energy calculations**; there is no silent fallback on failed constraints.

See [fixed-length Gibson](docs/gibson.md), [primer optimisation](docs/primer-optimisation.md)
and [existing overlaps](docs/existing-overlaps.md). GUI work remains deferred.

### Import warnings and strict mode

All file-reading commands (`inspect`, `features`, `primers`, `sequence`, `map`, `sites`, `digest`, `compatible-ends`, `fragments`, `ligate`, `gibson`, `gibson-optimise`, `gibson-assemble`) return a
`warnings` array at the top level of JSON success and runtime-error envelopes.
The envelope schema version is now **0.9.0**, adding primer optimisation and existing-overlap assembly.
Version 0.8.0 added PCR-tail Gibson candidates.
Version 0.7.0 added explicit ligation products.
Version 0.6.0 added annotation-aware fragments.
Version 0.5.0 added end compatibility.
Version 0.4.0 added complete digest simulation.
Version 0.3.0 added feature qualifiers and the
`primers` command. `inspect.result.warnings` remains as a compatibility duplicate. In text mode,
warnings go to **stderr**, leaving sequence stdout suitable for piping.

```bash
dnagent features construct.dna --output json
dnagent sequence construct.dna --strict --output json
dnagent map construct.dna --strict --out map.svg
```

`--strict` rejects **any** import warning, including preserved uninterpreted
metadata, with a nonzero exit and JSON error code `import_warnings`. Rejection
happens before a result is printed or a map is written. Most real SnapGene files
currently contain uninterpreted metadata, so strict mode is intentionally
conservative; warning-free does not guarantee complete format fidelity.
`--strict` is not supported by the GUI.

CLI argument-parsing errors still use Clap's stderr diagnostics rather than a
JSON envelope. Site-analysis failures use `restriction_scan_failed`, digest
failures use `digest_failed`, compatibility-analysis failures use
`compatibility_failed`, and annotated-fragment failures use `annotation_failed`;
ligation plan/analysis failures use `ligation_failed`; Gibson plan/design failures
use `gibson_failed`; other runtime failures
retain `command_failed`.

### Annotation and schema contract

`features --output json` includes ordered `qualifiers` arrays of `{key, value}`
objects. Repeated keys/values and valueless qualifiers (`null`) are retained;
these are not flattened into a dictionary.

`primers --output json` returns retained `name`, canonical uppercase IUPAC
`sequence`, and nullable `description`, in source order. These are imported
oligos, **not** inferred binding sites, PCR products or newly designed primers.
Empty primer lists are valid. Both commands obey the same warning/strict policy.

The current [JSON Schema](schemas/cli-envelope-0.9.0.schema.json) covers all fourteen
commands' success and runtime-error envelopes. Older schemas are retained only
for archived responses, not emitted or accepted by the current CLI; see the
[schema version policy](schemas/README.md). It validates structure and basic
value constraints; relational coordinate bounds and biological correctness
remain domain checks. To validate live output and deliberately invalid examples:

```bash
cargo build -p dnagent-cli --locked
uv run scripts/check_cli_schema.py --binary target/debug/dnagent
```

### Coordinate contract

Regions can only be constructed through `Region::linear` and
`Region::circular_arc`; direct enum construction is no longer public. Accessors
provide start, length and circular-arc status. The serialized region shape is
unchanged. Locations require one part for `Contiguous`, or multiple parts for
`Join`/`Order`, retaining their source order. Records recheck bounds and topology
against the actual sequence.

## Architecture

```text
SnapGene bytes → format adapter → domain record → application projections
                                             ├── deterministic CLI / SVG
                                             └── desktop GUI
```

The GUI owns selection, hover, zoom and viewport state. It does not own file parsing or biological operations.

Redistribution-safe synthetic fixtures, importer regression tests and committed JSON contracts are documented in [`fixtures/formats/snapgene`](fixtures/formats/snapgene). An optional [private-corpus checker](docs/private-corpus.md) compares real-file CLI
results against raw sequence packets, Biopython and source XML, with schema,
strict-mode and range checks; lab data and reports stay outside the repository.

## Development

Rust 1.92 or newer is required. The default **CLI package** build excludes GUI
crates and window-system dependencies:

```bash
cargo build -p dnagent-cli --locked
cargo test -p dnagent-cli --locked
# Optional desktop viewer:
cargo build -p dnagent-cli --features gui --locked
```

Workspace-wide checks still build the separate GUI crate. Validate both CLI
feature configurations when changing shared interfaces:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p dnagent-cli --features gui --locked
```

## Prior art and attribution

The narrow SnapGene migration adapter is informed by PlasCAD's MIT-licensed reader at commit `717459dcd780ec4266e9a2aa15297eae31956da5`. See [`NOTICE`](NOTICE) and [`licenses/PlasCAD-MIT.txt`](licenses/PlasCAD-MIT.txt).

## License

MIT

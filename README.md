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
rejected rather than approximated. Source annotations are not yet propagated.
See [digest coordinates, limitations and validation](docs/digest.md).

### End compatibility and roadmap

`compatible-ends` compares all distinct ends from one or two digests, reporting
matching/mismatching polarity and oligos, plus proposed relative orientation.
It evaluates sequence compatibility, not experimental ligation or full assemblies.
See [the compatibility contract](docs/compatibility.md).

Next: annotated fragments/exports, restriction-ligation products, then **Gibson
cloning before GUI integration**. See [the roadmap](docs/roadmap.md).

### Import warnings and strict mode

All file-reading commands (`inspect`, `features`, `primers`, `sequence`, `map`, `sites`, `digest`, `compatible-ends`) return a
`warnings` array at the top level of JSON success and runtime-error envelopes.
The envelope schema version is now **0.5.0**, adding end compatibility.
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
`compatibility_failed`; other runtime failures retain `command_failed`.

### Annotation and schema contract

`features --output json` includes ordered `qualifiers` arrays of `{key, value}`
objects. Repeated keys/values and valueless qualifiers (`null`) are retained;
these are not flattened into a dictionary.

`primers --output json` returns retained `name`, canonical uppercase IUPAC
`sequence`, and nullable `description`, in source order. These are imported
oligos, **not** inferred binding sites, PCR products or newly designed primers.
Empty primer lists are valid. Both commands obey the same warning/strict policy.

The committed [JSON Schema](schemas/cli-envelope-0.5.0.schema.json) covers all nine
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

Redistribution-safe synthetic fixtures, importer regression tests and committed JSON contracts are documented in [`fixtures/formats/snapgene`](fixtures/formats/snapgene). An optional private-corpus checker compares CLI results against Biopython and source XML; lab data and reports stay outside the repository.

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

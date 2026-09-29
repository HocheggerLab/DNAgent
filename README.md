# DNAgent

## Desktop direction

The GUI is a central product interface. A read-only **Tauri 2 + TypeScript** prototype
is available in [`desktop/`](desktop/README.md), with native Rust calculations and
generated transport types. WASM is deferred; the CLI and existing egui viewer remain
available. See the [architecture decision](docs/desktop-architecture.md) and
[architecture review](docs/architecture-review.md).

**An agent-friendly DNA design and cloning workbench.**

DNAgent is a Rust, library-first replacement for the plasmid viewing and cloning workflows commonly performed in SnapGene. Biological operations live in reusable crates. Development is currently CLI-first for agent use; the desktop GUI remains a basic viewer, with further interaction work deferred.

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

Current status: working Milestone 1 vertical slice with checked domain types, read-only sequence/feature/primer import, explicit fidelity reporting, deterministic JSON and SVG output, and basic Map/Features/Sequence GUI views. SnapGene `.dna` and single-record FASTA are accepted; FASTA is explicitly treated as sequence-only linear DNA. Full cross-view selection remains unfinished.

### Restriction sites

`sites` finds recognition sites and nominal cut positions with a built-in set of
99 commercial enzymes, including degenerate and interrupted sites, enzymes cutting
outside their site, reverse orientations and circular-origin sites. A complete
REBASE catalogue can be installed locally (`python3 scripts/manage_enzymes.py
install`); it is never committed because of its licence. `sites` reports
blunt/5′/3′ overhang geometry, not digest fragments. Ambiguous input is rejected;
methylation and reaction conditions are not modelled. See [restriction scope, provenance and validation](docs/restriction.md).

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
See [the roadmap](docs/roadmap.md): Tauri desktop work and shared-engine hardening
proceed together; the CLI and GUI call the same application layer.

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

### Local reference data

A pinned human GRCh38/RefSeq RNA profile can be installed outside Git with
`python3 scripts/manage_references.py install` (requires BLAST+ and 20 GiB free).
Use `plan` to inspect it without downloading. See [reference storage](docs/reference-store.md).
This provisions databases only; BLAST specificity is not yet connected to primer design.

### Editing and GenBank

DNAgent opens and saves GenBank (`.gb`, `.gbk`, `.genbank`); SnapGene `.dna` files are
read-only. `dnagent convert` saves a record as GenBank without losing anything DNAgent
read. SnapGene-only data and the original import warnings travel along in a DNAgent
comment block. `dnagent annotate` adds a feature (optionally a translated CDS with a
computed `/translation`) or removes one. The desktop app does the same through
shift-click selection, a New-feature dialog, undo/redo and Save.

```bash
dnagent convert construct.dna --out construct.gb
dnagent annotate construct.gb --out construct.gb --add --range 1250..6188 --label "fusion ORF" --translate
```

See [GenBank records](docs/genbank-records.md).

### Tabs and agent handoff

The desktop app opens several constructs in tabs. **Hand off to agent** writes GenBank
snapshots of every tab (unsaved edits included) and a `context.json` with selections into
the workspace (`~/DNAgent/handoff/`), and copies a prompt for an agent running in a
terminal. GenBank files the agent writes into `~/DNAgent` are offered as new tabs, and
changed open files reload. See [agent handoff](docs/agent-handoff.md).

### Translation and ORFs

`dnagent translate` translates a feature (honouring joins, origin crossings, `codon_start`
and `transl_table`), a range on either strand and frame, or every CDS, with pinned NCBI
genetic codes. Each codon reports its reference coordinates, and embedded SnapGene/GenBank
translations are compared, with mismatches warned. `dnagent orfs` finds complete six-frame
ORFs, including origin-wrapping ones. Both are validated against Biopython 1.85.

```bash
dnagent translate construct.dna --all-cds --output json
dnagent orfs construct.dna --min-codons 75 --output json
```

See [translation and ORFs](docs/translation.md). ORFs are computational, not genes.

### Offline amplification primers

`dnagent primer-design fixtures/plans/synthetic-primer-design.json` designs bounded
candidate pairs within an explicit reference window, optionally crossing a junction,
and screens them against declared positive/negative templates. Results include source
hashes, explicit thermodynamics, binding sites and predicted template intervals.
See [amplification design](docs/amplification.md) for the model and hard limits:
this is not Primer3, genome-wide specificity or experimental validation. GUI integration
and automatic shared-region discovery are deferred.

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
All Gibson plans accept `.dna`, single-record FASTA and literal sequence sources;
`gibson-assemble` additionally supports selected digest strands and ideal PCR products
with explicit tails. The latter retain generated primer candidates in their derived
input records. Materialised exact-overlap products can be exported directly:

```bash
dnagent gibson-assemble mixed-plan.json --output fasta > product.fasta
dnagent gibson-assemble mixed-plan.json --output genbank > product.gb
```

FASTA is sequence-only. GenBank is conservative and represents component provenance
as `misc_feature`, without inferred CDSs or translations. Neither predicts
experimental success. Hairpin/dimer checks are **sequence screens, not folding-energy
calculations**; there is no silent fallback on failed constraints.

See [fixed-length Gibson](docs/gibson.md), [primer optimisation](docs/primer-optimisation.md)
and [existing overlaps](docs/existing-overlaps.md). GUI work remains deferred.

### Import warnings and strict mode

All file-reading commands (`inspect`, `features`, `primers`, `sequence`, `map`, `sites`, `digest`, `compatible-ends`, `fragments`, `ligate`, `gibson`, `gibson-optimise`, `gibson-assemble`, `primer-design`) return a
`warnings` array at the top level of JSON success and runtime-error envelopes.
The envelope schema version is now **0.9.0**, adding primer optimisation and existing-overlap assembly.
The additive `primer-design` command also uses 0.9.0; existing command result shapes
remain unchanged. Its plan/design failures use `amplification_failed`.
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
these are not flattened into a dictionary. They are imported metadata, not
DNAgent-computed translations. `features` also accepts `--label` (case-insensitive
substring) and `--kind` (case-insensitive exact match) filters.

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

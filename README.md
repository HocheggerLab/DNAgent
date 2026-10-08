# DNAgent

[![CI](https://github.com/HocheggerLab/DNAgent/actions/workflows/ci.yml/badge.svg)](https://github.com/HocheggerLab/DNAgent/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/HocheggerLab/DNAgent?include_prereleases&sort=semver)](https://github.com/HocheggerLab/DNAgent/releases/latest)
[![License: MIT](https://img.shields.io/badge/licence-MIT-blue.svg)](LICENSE)
[![Docs](https://img.shields.io/badge/docs-hocheggerlab.github.io%2FDNAgent-blue)](https://hocheggerlab.github.io/DNAgent/)

**An agent-friendly DNA design and cloning workbench: a desktop app and a CLI on one Rust engine.**

DNAgent is an open replacement for the plasmid viewing and cloning work usually done in
SnapGene, built so that a person at the desktop and an AI agent in a terminal work on the
same constructs with the same engine. Every biological operation lives in tested Rust crates;
the CLI returns versioned JSON for agents, and the Tauri desktop app calls the same
application layer. Results are checked against independent references (Biopython, NCBI
genetic codes, brute-force scans), and end-to-end GUI tests take their expected values from
the CLI, never from hand-typed literals.

> Research software from the [Hochegger Lab](https://github.com/HocheggerLab). It predicts
> sequences and products; it does not validate experiments. Check designs before ordering.

**Documentation: <https://hocheggerlab.github.io/DNAgent/>** — the handbook, plus the Rust
[API reference](https://hocheggerlab.github.io/DNAgent/api/) generated from the crates.

## What it does

- **Open and save constructs:** SnapGene `.dna`, GenBank and single-record FASTA,
  recognised by content. Saving writes GenBank or SnapGene `.dna`, losslessly for everything
  DNAgent models, or reports what it cannot keep. A `.dna` file DNAgent only read is written
  back byte for byte.
- **Desktop app** (Tauri 2): circular and linear maps, a duplex sequence view with
  translations, ORFs and six-frame translation, tabs, undo/redo, new and deleted features,
  restriction sites and digests, feature detection from your own library, SVG export, light and
  dark mode.
- **Cloning engine:** restriction sites, complete digests, end compatibility, annotated
  fragments, explicit ligation plans, Gibson assembly (fixed-length tails, Tm-optimised
  primers, existing overlaps) with the annotated product written as GenBank, and offline
  amplification primer design.
- **Translation:** features, ranges and all CDSs with pinned NCBI genetic codes; complete ORFs.
- **Feature library:** collect the annotated parts of your constructs into a private SQLite
  library and detect them in any sequence, with variant families.
- **Isoform viewer:** open a gene locus (from [degron-db](docs/isoform-viewer.md#getting-a-locus)
  or any tool writing the documented `dnagent-locus` JSON) to see every transcript's exons,
  codons, long-read evidence and expression across a cell-line panel; loci up to 3 Mb.
- **Agent handoff:** the desktop snapshots all open tabs and selections into a workspace and
  copies a prompt for an agent; files the agent writes there open as new tabs.

## Install

**macOS (Apple Silicon).** Paste into Terminal — **use these commands rather than a browser
download**: DNAgent is unsigned, and macOS refuses to open an unsigned app that a browser
marked as quarantined. `curl` does not set that mark.

```bash
# Desktop app
curl -fsSL https://github.com/HocheggerLab/DNAgent/releases/latest/download/DNAgent-macos-arm64.tar.gz | tar xz -C /Applications

# Command-line tool, which agents drive
curl -fsSL https://github.com/HocheggerLab/DNAgent/releases/latest/download/dnagent-cli-macos-arm64.tar.gz | tar xz -C /usr/local/bin
```

Then `open -a DNAgent` and `dnagent --version`. To let an agent see what you have open:

```bash
claude mcp add dnagent -- dnagent mcp
```

**Windows (x64).** Download `DNAgent-windows-x64-setup.exe` and `dnagent-cli-windows-x64.zip`
from the [latest release](https://github.com/HocheggerLab/DNAgent/releases/latest). It is
unsigned, so SmartScreen asks once: **More info → Run anyway**. The live agent connection
is macOS and Linux only for now; on Windows agents work through the file handoff.

Intel Macs and Linux: build from source. Full instructions, the reasoning about signing,
and troubleshooting are in **[docs/install.md](docs/install.md)**.

## Connect an agent

DNAgent is built to be driven by an AI agent as well as by hand. Two pieces do that, and
they are useful separately:

- **Skills** — the workflow instructions in [`skills/`](skills/): how to inspect a
  construct, choose a restriction site, express a Gibson assembly as cores, check the
  product. The agent runs the `dnagent` CLI; it never invents biology.
- **The MCP server** — `dnagent mcp`, a live link to the running desktop app. The agent
  can see which constructs you have open and what you have selected, and can put a
  finished result on your screen with the regions to check already highlighted.

You can use either alone. Skills without MCP is a terminal workflow that writes files the
app picks up; MCP without skills lets an agent see your screen but not design anything.

### Claude Code

```bash
mkdir -p ~/.claude/skills
cp -R skills/dnagent skills/gibson-cloning ~/.claude/skills/
claude mcp add dnagent -- dnagent mcp
```

### Claude Desktop (Cowork)

Cowork runs shell commands on your machine, so the skills work there too — which makes it
the option for colleagues who do not use a terminal.

**Skills**: zip each one and upload at Settings → Capabilities.

```bash
(cd skills/dnagent && zip -qr ../../dnagent.skill.zip .)
(cd skills/gibson-cloning && zip -qr ../../gibson-cloning.skill.zip .)
```

**MCP**: add DNAgent to `claude_desktop_config.json` — on macOS
`~/Library/Application Support/Claude/claude_desktop_config.json`, on Windows under
`%APPDATA%\Claude\`. Use the full path to the binary; the app does not read your shell
`PATH`.

```json
{
  "mcpServers": {
    "dnagent": { "command": "/usr/local/bin/dnagent", "args": ["mcp"] }
  }
}
```

Restart the app. Open a construct in DNAgent and ask *"what am I looking at?"* — it should
answer with the name and your selection.

### Other MCP clients

`dnagent mcp` is a standard MCP server over stdio, so anything that speaks local MCP can
use it: **Goose**, **LM Studio** (`~/.lmstudio/mcp.json`), **Cursor**, **Windsurf**,
**Cline**, **Continue**. The configuration is the same command and argument as above.

The skills are written in Claude's skill format. Other clients will not load them as
skills, but `SKILL.md` is plain markdown — paste the relevant part into a system prompt
or project instructions and the workflow still holds, because the real contract is the
CLI's JSON output.

### ChatGPT

**Not currently possible.** ChatGPT supports only *remote* MCP servers (SSE or streamable
HTTP); there is no stdio option, so a server on your own machine cannot be reached, and on
macOS the desktop app exposes no MCP configuration at all. Exposing DNAgent over the
network would mean serving your constructs to a public endpoint, which is not something
this tool should make easy. If OpenAI adds local MCP servers, the existing server works
unchanged.

### Without any of this

The [desktop handoff](docs/agent-handoff.md) needs no MCP and no skills: the app writes
every open construct and your selection into a workspace folder, and files an agent
writes back open as tabs. It works on every platform, including Windows, where the live
channel is not yet available.

## Build from source

Requires Rust 1.92+; the desktop app also needs Node 22+ and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
# CLI (headless, no GUI dependencies)
cargo build -p dnagent-cli --release --locked
target/release/dnagent inspect fixtures/formats/snapgene/pUC19_M77789.dna
target/release/dnagent sites fixtures/formats/snapgene/pUC19_M77789.dna --enzymes EcoRI,BamHI --output json

# Desktop app
cd desktop && npm ci && npm run tauri dev
```

Every command takes `--output json` and returns one envelope (`schema_version`, `ok`,
`result` or `error`, `warnings`) described by the [JSON Schema](schemas/cli-envelope-0.10.0.schema.json).
Coordinates are zero-based and half-open everywhere. See the [desktop guide](desktop/README.md)
for what the app does, and [agent handoff](docs/agent-handoff.md) for working with an agent.

```bash
dnagent inspect construct.dna --output json
dnagent features construct.dna --output json
dnagent sequence construct.dna --range 100..300 --output json
dnagent map construct.dna --out map.svg
dnagent digest construct.dna --enzymes EcoRI,BamHI --output json
dnagent translate construct.dna --all-cds --output json
dnagent gibson plan.json --out product.gb --output json
dnagent isoforms GENE.locus.json --output json
```

## Capabilities in detail

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

DNAgent opens and saves GenBank (`.gb`, `.gbk`, `.genbank`) and SnapGene `.dna`; a source
file is never modified in place. `dnagent convert` saves a record in either format, chosen
by the output extension, without losing anything DNAgent read. SnapGene-only data and the original import warnings travel along in a DNAgent
comment block. `dnagent annotate` adds a feature (optionally a translated CDS with a
computed `/translation`) or removes one. The desktop app does the same through
shift-click selection, a New-feature dialog, undo/redo and Save.

```bash
dnagent convert construct.dna --out construct.gb
dnagent annotate construct.gb --out construct.gb --add --range 1250..6188 --label "fusion ORF" --translate
```

See [GenBank records](docs/genbank-records.md). The same commands write SnapGene `.dna`
when the output path ends in `.dna`: a file DNAgent only read is reproduced byte for byte,
and an edited or constructed record has its annotations generated, with the losses reported.
See [SnapGene reading and writing](docs/snapgene.md).

### Tabs and agent handoff

The desktop app opens several constructs in tabs. **Hand off to agent** writes GenBank
snapshots of every tab (unsaved edits included) and a `context.json` with selections into
the workspace (`~/DNAgent/handoff/`), and copies a prompt for an agent running in a
terminal. GenBank files the agent writes into `~/DNAgent` are offered as new tabs, and
changed open files reload. See [agent handoff](docs/agent-handoff.md).

### Feature library

`dnagent library import <folder>` collects the annotated features of your constructs into
a private SQLite library (deduplicated by sequence on either strand, with aliases and
provenance). `dnagent detect-features <file>` finds library parts in any construct. See
[feature library](docs/feature-library.md). Files are read by content, so GenBank saved
as `.dna` opens too.

### Isoform viewer

A gene locus exported from degron-db (`degron-db locus <SYMBOL>` → `<SYMBOL>.locus.json`)
opens as a linear record with one mRNA and CDS feature per transcript. The desktop
**Isoforms** tab draws every transcript (exons, introns, start/stop codons) coloured by
long-read evidence and ordered by mean expression across the cell-line panel; click one
for its expression per cell line. `dnagent isoforms <file>` gives the same view as JSON.
See [isoform viewer](docs/isoform-viewer.md).

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

Each core declares how its fragment reaches the reaction: `pcr` (amplified here, so it
can take primer tails) or `provided` (a restriction fragment, synthesis or stock linear
DNA, used exactly as given). The usual insert-into-vector case marks the cut vector
`provided`, so both overlaps are written into the insert's two primers and the backbone
is never amplified. `preparation` is required, because assuming it would silently
amplify a vector you meant to digest.

See [fixed-length Gibson](docs/gibson.md), [primer optimisation](docs/primer-optimisation.md)
and [existing overlaps](docs/existing-overlaps.md). GUI work remains deferred.

### Import warnings and strict mode

All file-reading commands (`inspect`, `features`, `primers`, `sequence`, `map`, `sites`, `digest`, `compatible-ends`, `fragments`, `ligate`, `gibson`, `gibson-optimise`, `gibson-assemble`, `primer-design`) return a
`warnings` array at the top level of JSON success and runtime-error envelopes.
The envelope schema version is now **0.10.0**, adding primer optimisation and existing-overlap assembly.
The additive `primer-design` command also uses 0.10.0; existing command result shapes
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

The current [JSON Schema](schemas/cli-envelope-0.10.0.schema.json) covers every command's
success and runtime-error envelopes. Older schemas are retained only
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
crates and window-system dependencies. The full pre-commit validation list (Rust, Python
reference checks via `uv`, desktop unit and e2e tests) is in [`AGENTS.md`](AGENTS.md):

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

## Contributing

Issues and pull requests are welcome. Please run the validation list in
[`AGENTS.md`](AGENTS.md) before submitting; fixtures must be synthetic or redistributable,
with their source and licence recorded (no private lab constructs).

## License

MIT, see [`LICENSE`](LICENSE). Third-party notices are in [`NOTICE`](NOTICE).

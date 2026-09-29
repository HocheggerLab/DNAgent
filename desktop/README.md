# DNAgent desktop prototype

Read-only Tauri 2 + TypeScript/Vite viewer backed by the native Rust engine.
See `../docs/desktop-architecture.md` for the decision and limitations.

## Run

Requires Rust, Node/npm and the platform's Tauri native build prerequisites.
On macOS, the build requires Xcode command-line tooling. No WASM is used.

```bash
# From desktop/
npm ci
npm run tauri dev
```

**Editing:** click a feature and shift-click another to select everything between them
(forward from the first, through the origin on circular molecules), or drag across bases.
**New feature…** adds an annotation with a live engine preview; tick **Translate** to make
a CDS (with genetic code and codon start), which then shows its amino acids. Undo/Redo
(⌘Z/⇧⌘Z) and **Delete feature** (for features added in this session) work as expected.
**Save** / **Save as…** (⌘S) write GenBank, including everything DNAgent read from the
original; `.dna` files are never modified. GenBank files open like `.dna` files.

**Tabs and agent:** open several constructs; each tab keeps its own selection and undo
history (⌘1–⌘9 switch). **Hand off to agent** (⇧⌘C) snapshots all tabs into the
workspace (**Workspace…**, default `~/DNAgent`) and copies a prompt for your agent in a
terminal. Files the agent writes there are offered as new tabs; changed open files reload.
See [`../docs/agent-handoff.md`](../docs/agent-handoff.md).

Appearance follows macOS light/dark mode; override it with the **Appearance** menu.
The feature list collapses with **‹** so the map can use the whole window.

Use **Browse…** or enter an absolute path to a `.dna` or single-record `.fasta` file. Try the repository's
`fixtures/formats/snapgene/synthetic_linear.dna` and
`synthetic_multipart_origin.dna`. Select a feature in the list or map; click sequence
bases to select features (repeated clicks cycle overlapping annotations). **Map** and
**Sequence** tabs share the current selection. The Sequence view shows the forward
5′→3′ strand, aligned 3′→5′ complement and imported feature tracks; row breaks are
not treated as feature ends. Coordinates remain zero-based. Imported primers appear
in an **unplaced** list because binding coordinates are not retained by the importer.
Amino acids appear under every CDS (1- or 3-letter). **ORFs** (with a minimum-length
control) show as tracks and on the map, and **Six-frame translation** adds all frames.
Drag across bases to select a range; its translation on either strand appears above the
view. All of these come from the Rust engine. Restriction-site tracks are deferred. Map labels
and arrows show feature identity and each part's strand direction. Imported colours
are retained where available. Expand the import-warning panel to review fidelity limits.

A plain `npm run dev` serves only the frontend: native imports require the Tauri window.

## Validate / generate contracts

```bash
# From repository root
cargo run -p dnagent-desktop-api --example export_types > desktop/src/bindings.ts
cargo test -p dnagent-desktop-api
cd desktop
npm test # Node 22.6+ (native TypeScript stripping)
npm run build
cargo check --manifest-path src-tauri/Cargo.toml --locked
npm run tauri build -- --no-bundle
```

## End-to-end GUI scenarios

```bash
# From desktop/ (first time: npx playwright install chromium)
npm run e2e         # headless Chromium; checks GUI state against the dnagent CLI
npm run e2e:headed  # watch the same scenarios run
```

### Design review gallery

```bash
npm run review                          # public fixtures, including pUC19
npm run review -- ~/path/to/private.dna # plus local files, never committed
```

Renders each construct's map in light and dark mode at three window sizes plus
selected-feature views, and writes `e2e/artifacts/review/latest/index.html` with the
previous run shown underneath each tile for before/after comparison. Local files are
opened by a local Rust test server; only screenshots are written, into the gitignored
`e2e/artifacts/` folder.

Scenarios in `e2e/scenarios/*.json` drive the real frontend through a test-only
automation API (Vite `e2e` mode, absent from production builds). Native commands are
answered by the real Rust desktop session through a local test server
(`dnagent-desktop-api` example `e2e_server`). The harness uses Chromium, not the production WKWebView/WebKitGTK,
so it tests app logic and layout, not the webview engine or the native shell. See
[`e2e/README.md`](e2e/README.md).

The drift test deliberately fails if Rust DTOs change without regenerating the
committed bindings. The shell has a separate Cargo lockfile; the engine workspace
remains headless by default. Native build success is not a visual/interaction test.

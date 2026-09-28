# DNAagent desktop prototype

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
Translation and restriction-site tracks are deferred. Map labels
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
# From repository root, after DTO or fixture changes:
cargo run -p dnagent-desktop-api --example export_recordings > desktop/e2e/fixtures/recordings.json
```

### Design review gallery

```bash
npm run review                          # public fixtures, including pUC19
npm run review -- ~/path/to/private.dna # plus local files, never committed
```

Renders each construct's map in light and dark mode at three window sizes plus
selected-feature views, and writes `e2e/artifacts/review/latest/index.html` with the
previous run shown underneath each tile for before/after comparison. Local files are
recorded through the Rust desktop API into the gitignored `e2e/artifacts/` folder.

Scenarios in `e2e/scenarios/*.json` drive the real frontend through a test-only
automation API (Vite `e2e` mode, absent from production builds). Native calls are
answered from Rust-generated recordings, which `cargo test -p dnagent-desktop-api`
checks for drift. The harness uses Chromium, not the production WKWebView/WebKitGTK,
so it tests app logic and layout, not the webview engine or the native shell. See
[`e2e/README.md`](e2e/README.md).

The drift test deliberately fails if Rust DTOs change without regenerating the
committed bindings. The shell has a separate Cargo lockfile; the engine workspace
remains headless by default. Native build success is not a visual/interaction test.

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

The drift test deliberately fails if Rust DTOs change without regenerating the
committed bindings. The shell has a separate Cargo lockfile; the engine workspace
remains headless by default. Native build success is not a visual/interaction test.

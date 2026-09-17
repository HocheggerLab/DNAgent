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

Enter an absolute path to a `.dna` or single-record `.fasta` file. Try the repository's
`fixtures/formats/snapgene/synthetic_linear.dna` and
`synthetic_multipart_origin.dna`. Select a feature in the list or map; click sequence
bases to select features (repeated clicks cycle overlapping annotations).

A plain `npm run dev` serves only the frontend: native imports require the Tauri window.

## Validate / generate contracts

```bash
# From repository root
cargo run -p dnagent-desktop-api --example export_types > desktop/src/bindings.ts
cargo test -p dnagent-desktop-api
cd desktop
npm run build
cargo check --manifest-path src-tauri/Cargo.toml --locked
npm run tauri build -- --no-bundle
```

The drift test deliberately fails if Rust DTOs change without regenerating the
committed bindings. The shell has a separate Cargo lockfile; the engine workspace
remains headless by default. Native build success is not a visual/interaction test.

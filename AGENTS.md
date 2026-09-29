# DNAagent contributor guidance

## Architecture

- Keep domain types independent of filesystems, terminals and GUI toolkits.
- CLI and GUI must call the same typed application layer; neither reimplements biology.
- Keep the default CLI package build headless; gate desktop dependencies behind its `gui` feature.
- Keep the versioned JSON Schema and live-output validation in sync with CLI changes.
- Keep the CLI a first-class agent tool and the GUI a central desktop interface. Follow `docs/roadmap.md` and `docs/desktop-architecture.md`: Tauri/native Rust prototyping proceeds alongside engine hardening; WASM is deferred.
- Preserve import warnings in every CLI projection; strict rejection must happen before output-file writes.
- Keep GUI-only selection, hover, zoom and viewport state in the frontend (`desktop/` for Tauri; `dnagent-gui` for the retained egui viewer). Generate TypeScript DTOs from the Rust desktop API; never duplicate biological calculations in TypeScript.
- Use zero-based, half-open coordinates internally and in JSON.
- Preserve multipart and origin-spanning features; never silently flatten or discard them.
- SnapGene `.dna` is read-only; DNAagent saves GenBank (`docs/genbank-records.md`). Saving must be
  lossless for everything DNAagent models, or report the limitation; never drop metadata silently.
- Unsupported format content must be preserved or reported with structured warnings.

## Provenance and licensing

- Do not add private lab constructs to fixtures.
- Record the source and licence of adapted code and fixture data.
- Retain the PlasCAD MIT notice for closely adapted SnapGene parser code.

## Validation

Run before committing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p dnagent-cli --features gui --locked
cargo clippy -p dnagent-cli --all-targets --features gui --locked -- -D warnings
cargo build -p dnagent-cli --locked
uv run scripts/check_cli_schema.py --binary target/debug/dnagent
uv run scripts/check_restriction.py --binary target/debug/dnagent
uv run scripts/check_digest.py --binary target/debug/dnagent
uv run scripts/check_compatibility.py --binary target/debug/dnagent
uv run scripts/test_private_corpus.py
uv run scripts/check_fragment_annotations.py --binary target/debug/dnagent
uv run scripts/check_ligation.py --binary target/debug/dnagent
uv run scripts/check_gibson.py --binary target/debug/dnagent
uv run scripts/check_gibson_extensions.py --binary target/debug/dnagent
uv run scripts/check_amplification.py --binary target/debug/dnagent
uv run scripts/check_translation.py --binary target/debug/dnagent
uv run scripts/check_genbank.py --binary target/debug/dnagent
python3 scripts/test_manage_references.py
cargo test -p dnagent-desktop-api --test e2e_recordings  # recorded GUI responses match Rust
(cd desktop && npm test && npm run e2e)  # GUI scenarios checked against the CLI
```

GUI changes need a passing `npm run e2e`. Expected biological values in scenarios
come from the CLI, never from literals; see `desktop/e2e/README.md`. After changing
DTOs or public fixtures, regenerate `desktop/e2e/fixtures/recordings.json` with
`cargo run -p dnagent-desktop-api --example export_recordings`.
Map scenarios assert that every feature is drawn and every label is placed or
reported; keep those invariants when changing the layout. `npm run review -- <file>`
may render private constructs for design review, but only into the gitignored
`desktop/e2e/artifacts/`.

Schema checks use public synthetic fixtures only. Private-corpus validation is
optional and must keep manifests, constructs and reports outside Git.

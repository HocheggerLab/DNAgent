# DNAagent contributor guidance

## Architecture

- Keep domain types independent of filesystems, terminals and GUI toolkits.
- CLI and GUI must call the same typed application layer; neither reimplements biology.
- Keep the default CLI package build headless; gate desktop dependencies behind its `gui` feature.
- Keep the versioned JSON Schema and live-output validation in sync with CLI changes.
- Prioritise the CLI as an agent tool. Follow `docs/roadmap.md`: Gibson cloning is required before GUI integration.
- Preserve import warnings in every CLI projection; strict rejection must happen before output-file writes.
- Keep GUI-only selection, hover, zoom and viewport state in `dnagent-gui`.
- Use zero-based, half-open coordinates internally and in JSON.
- Preserve multipart and origin-spanning features; never silently flatten or discard them.
- SnapGene support is read-only until explicitly expanded.
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
```

Schema checks use public synthetic fixtures only. Private-corpus validation is
optional and must keep manifests, constructs and reports outside Git.

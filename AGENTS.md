# DNAagent contributor guidance

## Architecture

- Keep domain types independent of filesystems, terminals and GUI toolkits.
- CLI and GUI must call the same typed application layer; neither reimplements biology.
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
```

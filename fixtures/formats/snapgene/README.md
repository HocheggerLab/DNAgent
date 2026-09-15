# Synthetic SnapGene fixtures

`synthetic_circular.dna` was generated specifically for DNAagent and contains no private or third-party biological construct data. It exercises:

- circular topology;
- a forward linear feature;
- a reverse origin-spanning feature;
- feature colours and a repeated-value-capable qualifier representation;
- one retained primer;
- one unknown packet that must be preserved and reported.

The `.inspect.json` and `.features.json` files are deterministic CLI expectations.

## Additional synthetic cases

These are hand-authored format cases, not anonymised lab constructs. All are
covered by the repository MIT licence. Rebuild them deterministically with:

```bash
python3 fixtures/formats/snapgene/generate_cases.py
```

| File | Coverage |
| --- | --- |
| `synthetic_restriction_linear.dna` | All six enzyme motifs; both Type IIS orientations, ample flanks |
| `synthetic_restriction_circular.dna` | EcoRI recognition across circular origin |
| `synthetic_restriction_end.dna` | BsaI recognition without downstream cut positions |
| `synthetic_linear.dna` | Lowercase IUPAC DNA, reverse multipart annotation, repeated/valueless qualifiers, primer and description |
| `synthetic_unannotated.dna` | Minimal linear record without annotations |
| `synthetic_multipart_origin.dna` | Reverse feature with an origin-spanning part followed by a linear part |
| `synthetic_partial.dna` | Invalid feature segments and primer, explicit warnings, retained XML and opaque binary metadata |
| `invalid_duplicate_sequence.dna` | Duplicate DNA packets rejected |
| `invalid_missing_sequence.dna` | Missing DNA packet rejected |
| `invalid_truncated.dna` | Truncated payload rejected |
| `invalid_feature_xml.dna` | Malformed annotation XML rejected |

Adapter assertions in `crates/dnagent-format-snapgene/tests/fixture_cases.rs`
are hand-authored expectations rather than output snapshots. CLI tests cover
sequence boundaries, warning exposure through `inspect`, and nonzero JSON
failures for malformed files.

Coordinate invariants and cross-command warning reporting now have dedicated
regression coverage in domain tests and `dnagent-cli/tests/import_policy.rs`.
Strict-mode map tests verify that rejected imports neither create nor overwrite
output files. Malformed GUI colours and structured CLI argument errors remain
separate follow-up work.

## Optional private corpus

**Never put lab `.dna` files, their manifests or reports in this repository.**
Use an external directory. The database identifies records and relative file
paths; file bytes must be obtained separately from authorised local storage.
Keep byte-identical copies and record provenance and SHA-256 hashes.

A private manifest has this structure (replace the placeholders):

```json
{
  "schema_version": 1,
  "records": [
    {"file": "example.dna", "sha256": "<SHA-256 of original bytes>"}
  ]
}
```

Record database IDs, original paths, acquisition date and redistribution status
in the **external** manifest as additional fields. Fixture paths are relative to
that manifest's directory. The checker verifies hashes before import, compares
full sequences and topology against pinned Biopython, and compares feature
labels, types, strand, ordered coordinates, colours and qualifiers against source
XML. Primer sequences/descriptions, counts and `inspect` warning codes/packet
types are also checked, along with top-level warning consistency across
`inspect`, `features`, `primers` and `sequence`.

```bash
cargo build -p dnagent-cli --locked
# Set PRIVATE_CORPUS to your external corpus directory first.
uv run scripts/check_private_corpus.py "$PRIVATE_CORPUS/manifest.json" \
  --binary target/debug/dnagent > "$PRIVATE_CORPUS/report.json"
```

The command exits nonzero on a mismatch and records the binary hash. It does
not create expected values from DNAagent output. It is optional and is not part
of `cargo test`; public tests need neither private data nor Biopython.

Limitations: source XML is an annotation-level oracle, not an independent
biological validation. This does not verify primer binding, all SnapGene
metadata, GUI behaviour or
experimental correctness. Do not suppress a mismatch or regenerate hashes
without checking its cause.

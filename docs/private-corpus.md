# Private SnapGene import regression corpus

The opt-in `scripts/check_private_corpus.py` harness checks real `.dna` files,
not only synthetic recognition motifs. It is a read-only import regression test,
**not complete SnapGene fidelity or experimental validation**.

## Independent checks

For each manifest entry, the checker verifies the SHA-256 before invoking the CLI,
then compares:

- Complete sequence against **both** the raw DNA packet and Biopython 1.85.
  Uppercase normalisation follows the domain intake contract; source files stay
  unchanged. The raw topology flag independently checks imported topology.
- Feature counts, labels, kinds, directionality, multipart/origin-spanning locations,
  colours and ordered qualifiers against source XML with explicit coordinate conversion.
- Primer counts, names, sequences and descriptions against source XML.
- Import-warning codes and packet types against uninterpreted source packets;
  warning equality across inspect/features/primers/sequence projections.
- Half-open sequence ranges at termini, midpoint and boundaries from the first
  eight annotations, including the empty terminal interval.
- Strict acceptance for warning-free imports and strict rejection when warnings exist.
- Full current JSON Schema conformance for every response and no unexpected stderr.
- File hash again after checking to detect changes during the run.

Source XML is an independent coordinate/field reference, not an assertion that
all SnapGene metadata is understood. Biopython agrees on decoded sequence/topology;
it cannot certify every native SnapGene feature or interpretation.

## Reproducible use

```bash
cargo build -p dnagent-cli --locked
# Test the harness using public fixtures and intentionally injected faults:
uv run scripts/test_private_corpus.py
# Private paths are supplied by the operator; never put inputs/reports in Git:
umask 077
uv run scripts/check_private_corpus.py "$PRIVATE_CORPUS/manifest.json" \
  --binary target/debug/dnagent > "$PRIVATE_CORPUS/report.json"
```

The external manifest uses `schema_version: 1` and a nonempty `records` list;
each record needs a relative `file` and its `sha256`. Resolved paths must remain
inside the manifest directory. Do not silently replace manifest hashes when a
check fails. The checker exits nonzero for any failing record.

Reports contain per-record failure labels and coverage, packet counts, range-probe
counts, UTC generation time, plus manifest/checker/binary/CLI-schema hashes.
They contain private filenames but no full sequences. Schema failures are sanitised
so validation error messages do not echo private instance values. Reports belong
outside the repository and should retain restrictive permissions.

## Current verified coverage — 2026-09-15

Eight authorised real plasmids passed the strengthened harness, with **163 range
probes**, **10 multipart features**, **3 origin-spanning parts** and **30 reverse
features**. All eight inputs are circular. Private sequence files and reports
remain outside Git; no construct sequences or names are included here.

The public harness tests verify successful controls and intentional failures:
changed source hashes, altered decoded sequences/features/ranges, malformed CLI
schema output, path escapes, nonzero failed-report exit and truncated packets.
These tests ensure that the checker detects errors rather than only replaying
known passing inputs.

## Remaining evidence gaps

- More authorised real-file variants, especially linear DNA, are needed before
  broadening fidelity claims. Synthetic linear fixtures do not fill that gap.
- Native SnapGene application comparison remains separate; there is no native
  round-trip/export test and no comprehensive preserved-packet semantic validation.
- GUI interpretation, primer binding-site prediction and experimental sequence
  confirmation are outside this harness.
- Malformed or partially interpreted real annotations intentionally fail rather
  than being silently dropped from the reference comparison.

Restriction/digest correctness is checked separately by `check_restriction.py` and
`check_digest.py`, which can use the same private manifest. This separation makes
it clear whether a regression is in import fidelity or downstream biology.

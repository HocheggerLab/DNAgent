# Locus fixtures

| File | Source and licence |
| --- | --- |
| `synthetic_locus.locus.json` | Synthetic (random sequence, invented transcripts, cell lines and expression values), written by `generate_synthetic_locus.py`; no third-party data. Gene SYNLOC on the + strand, 6 kb, five transcripts covering every display state: MANE and expressed, an alternative last exon (ordered differently by the two quantifiers), a discoverable exon skip, a quantified-but-zero 5′-incomplete CDS (`codon_start` 2) and an invisible non-coding transcript. |
| `synthetic_minus.locus.json` | Same generator: the first three transcripts mirrored onto the − strand (gene SYNMIN). |

Bundles use the `dnagent-locus` v1 format described in [`docs/isoform-viewer.md`](../../../docs/isoform-viewer.md).
Real genes exported from degron-db (`degron-db locus <SYMBOL>`) carry private lab data and stay
outside Git; check them locally with `uv run scripts/check_locus.py --bundle <file>`.

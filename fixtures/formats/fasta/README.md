# FASTA fixtures

| File | Source and licence |
| --- | --- |
| `primer_*.fasta`, `synthetic_digest_source.fasta` | Synthetic, written for DNAgent; repository MIT licence. |
| `pUC19_M77789.fasta` | Cloning vector pUC19, complete sequence, NCBI GenBank **M77789.2** (Yanisch-Perron, Vieira and Messing, *Gene* 33:103–119, 1985), retrieved 2026-09-28 with NCBI E-utilities `efetch`. NCBI places no restrictions on the use or distribution of GenBank data; the record carries no patent or copyright notice. Sequence only, unmodified (uppercased, rewrapped at 70 columns). |
| `synthetic_variants_target.fasta` | Synthetic, written by `../genbank/generate_synthetic_variants.py`: the element of `synthetic_variants.gb` in full, its shorter variant alone, and the unrelated part reversed, unannotated (detection tests). |

`pUC19_M77789.fasta` is the source for `../snapgene/pUC19_M77789.dna`; see that README.

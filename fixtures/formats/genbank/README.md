# GenBank fixtures

| File | Source and licence |
| --- | --- |
| `pUC19_M77789.gb` | NCBI GenBank flat file for M77789.2 (cloning vector pUC19), retrieved 2026-09-28 with E-utilities `efetch` (`rettype=gb`), unmodified. NCBI places no restrictions on the use or distribution of GenBank data; the record carries no patent or copyright notice. Used as a third-party GenBank input (reader oracle against Biopython). |
| `synthetic_variants.gb` | Synthetic (random sequence), written by `generate_synthetic_variants.py`; no third-party data. A 300 bp element annotated in full and as a 270 bp variant inside it, a 20 bp nested motif and an unrelated part: variant families in the feature library. |

# GenBank fixtures

| File | Source and licence |
| --- | --- |
| `pUC19_M77789.gb` | NCBI GenBank flat file for M77789.2 (cloning vector pUC19), retrieved 2026-09-28 with E-utilities `efetch` (`rettype=gb`), unmodified. NCBI places no restrictions on the use or distribution of GenBank data; the record carries no patent or copyright notice. Used as a third-party GenBank input (reader oracle against Biopython). |
| `synthetic_variants.gb` | Synthetic (random sequence), written by `generate_synthetic_variants.py`; no third-party data. A 300 bp element annotated in full and as a 270 bp variant inside it, a 20 bp nested motif, an unrelated part, a same-named point variant, a differently named single-substitution "mutant" and a CDS in two synonymous codings: variant families (contained, similar DNA, same protein) in the feature library. |
| `synthetic_cdna.gb` | Synthetic (random sequence), written by `generate_synthetic_cdna.py`; no third-party data. A cDNA with 5′ UTR, a 600 bp CDS without internal stops, 3′ UTR and a polyA signal, for Gibson cloning tests. |

#!/usr/bin/env python3
"""Write synthetic_cdna.gb: a random, synthetic cDNA (deterministic; no dependencies).

5' UTR (60 bp), a 600 bp CDS (ATG, 198 sense codons without stops, TAA), 3' UTR (90 bp)
with an AATAAA polyA signal. Used with pUC19 in fixtures/plans/synthetic-cdna-into-puc19.json
(Gibson cloning of the CDS into the pUC19 polylinker) and the desktop agent scenario.
"""
from pathlib import Path
import random

HERE = Path(__file__).resolve().parent
rng = random.Random(20260930 + 1)
bases = lambda n: "".join(rng.choice("ACGT") for _ in range(n))
STOPS = {"TAA", "TAG", "TGA"}


def codon() -> str:
    while True:
        c = bases(3)
        if c not in STOPS:
            return c


utr5 = bases(60)
cds = "ATG" + "".join(codon() for _ in range(198)) + "TAA"
utr3 = bases(40) + "AATAAA" + bases(44)
sequence = utr5 + cds + utr3
c0, c1 = len(utr5), len(utr5) + len(cds)
features = [
    ("5'UTR", f"1..{c0}", ["/label=5' UTR"]),
    ("CDS", f"{c0 + 1}..{c1}", ["/label=synthetic ORF", "/codon_start=1", "/transl_table=1", '/product="synthetic protein"']),
    ("3'UTR", f"{c1 + 1}..{len(sequence)}", ["/label=3' UTR"]),
    ("polyA_signal", f"{c1 + 41}..{c1 + 46}", ["/label=polyA signal"]),
]
lines = [f"LOCUS       synthetic_cdna            {len(sequence)} bp    mRNA    linear   SYN 30-SEP-2026",
         "DEFINITION  Synthetic cDNA for DNAgent Gibson cloning tests.",
         "FEATURES             Location/Qualifiers"]
for kind, location, qualifiers in features:
    lines.append(f"     {kind:<16}{location}")
    lines += [f"                     {q}" for q in qualifiers]
lines.append("ORIGIN")
for i in range(0, len(sequence), 60):
    chunk = sequence[i:i + 60].lower()
    lines.append(f"{i + 1:>9} " + " ".join(chunk[j:j + 10] for j in range(0, len(chunk), 10)))
lines += ["//", ""]
(HERE / "synthetic_cdna.gb").write_text("\n".join(lines))

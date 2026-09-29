#!/usr/bin/env python3
"""Write the synthetic feature-variant fixtures (deterministic; no dependencies).

synthetic_variants.gb annotates one 300 bp element twice, as a full version and a
270 bp shorter variant inside it (90 %: a variant family), plus a 20 bp motif inside
it (a nested part, not a variant) and an unrelated 60 bp part. It also carries a
same-named copy of the element with two substitutions (a point variant, 99.3 %), a
differently named copy with one substitution (a "mutant": not grouped), and one CDS
written with two sets of synonymous codons (same protein, different DNA).
../fasta/synthetic_variants_target.fasta carries the full element, the shorter
variant on its own, and the unrelated part on the reverse strand, unannotated, for
detection tests.
"""
from pathlib import Path
import random

HERE = Path(__file__).resolve().parent
rng = random.Random(20260930)
bases = lambda n: "".join(rng.choice("ACGT") for _ in range(n))
revcomp = lambda s: s.translate(str.maketrans("ACGT", "TGCA"))[::-1]


def origin(sequence: str) -> list[str]:
    lines = ["ORIGIN"]
    for i in range(0, len(sequence), 60):
        chunk = sequence[i:i + 60].lower()
        lines.append(f"{i + 1:>9} " + " ".join(chunk[j:j + 10] for j in range(0, len(chunk), 10)))
    return lines


element, part = bases(300), bases(60)
flip = lambda s, positions: "".join(("C" if c == "A" else "A") if i in positions else c for i, c in enumerate(s))
point, mutant = flip(element, {60, 200}), flip(element, {150})
codons = [("GCT", "GCC"), ("GGT", "GGC"), ("CTG", "CTC"), ("ACT", "ACC")]
cds_a = "ATG" + "".join(codons[i % 4][0] for i in range(110)) + "TAA"
cds_b = "ATG" + "".join(codons[i % 4][1] for i in range(110)) + "TAA"
source = bases(40) + element + bases(50) + part + bases(40) + point + bases(30) + mutant + bases(30) + cds_a + bases(30) + cds_b + bases(40)
e, p = 40, 40 + 300 + 50
pt = p + 60 + 40
mu = pt + 300 + 30
ca = mu + 300 + 30
cb = ca + len(cds_a) + 30
features = [
    ("misc_feature", f"{e + 1}..{e + 300}", "variant element"),
    ("misc_feature", f"{e + 16}..{e + 285}", "variant element short"),
    ("protein_bind", f"{e + 101}..{e + 120}", "nested motif"),
    ("misc_feature", f"{p + 1}..{p + 60}", "unrelated part"),
    ("misc_feature", f"{pt + 1}..{pt + 300}", "Variant Element"),
    ("misc_feature", f"{mu + 1}..{mu + 300}", "variant element K50R"),
    ("CDS", f"{ca + 1}..{ca + len(cds_a)}", "demo CDS"),
    ("CDS", f"{cb + 1}..{cb + len(cds_b)}", "Demo-CDS"),
]
lines = [f"LOCUS       synthetic_variants       {len(source)} bp    DNA     linear   SYN 30-SEP-2026",
         "DEFINITION  Synthetic feature variants for DNAgent library tests.",
         "FEATURES             Location/Qualifiers"]
for kind, location, label in features:
    lines += [f"     {kind:<16}{location}", f"                     /label={label}"]
(HERE / "synthetic_variants.gb").write_text("\n".join(lines + origin(source) + ["//", ""]))

short = element[15:285]
target = bases(30) + element + bases(40) + short + bases(40) + revcomp(part) + bases(30)
(HERE.parent / "fasta/synthetic_variants_target.fasta").write_text(
    ">synthetic_variants_target\n" + "\n".join(target[i:i + 70] for i in range(0, len(target), 70)) + "\n")

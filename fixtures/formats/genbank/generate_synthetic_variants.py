#!/usr/bin/env python3
"""Write the synthetic feature-variant fixtures (deterministic; no dependencies).

synthetic_variants.gb annotates one 300 bp element twice, as a full version and a
270 bp shorter variant inside it (90 %: a variant family), plus a 20 bp motif inside
it (a nested part, not a variant) and an unrelated 60 bp part.
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
source = bases(40) + element + bases(50) + part + bases(40)
e, p = 40, 40 + 300 + 50
features = [
    ("misc_feature", f"{e + 1}..{e + 300}", "variant element"),
    ("misc_feature", f"{e + 16}..{e + 285}", "variant element short"),
    ("protein_bind", f"{e + 101}..{e + 120}", "nested motif"),
    ("misc_feature", f"{p + 1}..{p + 60}", "unrelated part"),
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

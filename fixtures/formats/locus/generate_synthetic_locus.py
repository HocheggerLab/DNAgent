#!/usr/bin/env python3
"""Write synthetic DNAgent locus bundles (format dnagent-locus v1). Deterministic; no deps.

synthetic_locus.locus.json (gene SYNLOC, + strand, 6 kb) has five transcripts:
  T1  MANE, 4 exons, quantified and expressed (bambu_lr, NanoCount_lr)
  T2  alternative last exon (its own stop), quantified; expressed less in bambu_lr,
      more in NanoCount_lr (so the two quantifiers order the rows differently)
  T3  skips exon 2 (in frame), discoverable (novel junction, not found)
  T4  5'-incomplete CDS starting mid-codon (codon_start 2), quantified but zero
  T5  non-coding (retained intron), invisible
synthetic_minus.locus.json (gene SYNMIN, - strand) mirrors T1-T3 on the minus strand.

`--large PATH` also writes SYNLOC enlarged to about 1.2 Mb (gene SYNBIG) by inserting random
~400 kb stretches at three positions no exon covers (introns grow; exons, ORFs and codons
are unchanged, only shifted). It is generated on demand (e2e global setup), not committed.

Real ORFs are written into the sequence, so every CDS reads ATG (or the in-frame
truncated start) ... stop. Expression values and cell lines are invented. All
positions are zero-based, half-open on the locus, like the real exporter's.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import random

HERE = Path(__file__).resolve().parent
BASES = "ACGT"
STOPS = {"TAA", "TAG", "TGA"}
CODE = {
    a + b + c: aa
    for (a, b, c), aa in zip(
        ((x, y, z) for x in "TCAG" for y in "TCAG" for z in "TCAG"),
        "FFLLSSSSYY**CC*WLLLLPPPPHHQQRRRRIIIMTTTTNNKKSSRRVVVVAAAADDEEGGGG",
        strict=True,
    )
}
COMP = str.maketrans("ACGT", "TGCA")


def translate(cds: str) -> str:
    return "".join(CODE[cds[i : i + 3]] for i in range(0, len(cds) - 2, 3))


def build(seed: int) -> tuple[list[str], dict[str, dict]]:
    """A 6 kb locus in transcription orientation with the five transcripts' ORFs written."""
    rng = random.Random(seed)
    seq = [rng.choice(BASES) for _ in range(6000)]
    exons = {
        "T1": [(1000, 1300), (2000, 2150), (3000, 3210), (4000, 4400)],
        "T2": [(1000, 1300), (2000, 2150), (3000, 3210), (4800, 5200)],
        "T3": [(1000, 1300), (3000, 3210), (4000, 4400)],
        "T4": [(2050, 2150), (3000, 3210), (4000, 4400)],
        "T5": [(1000, 1300), (2000, 3210)],
    }
    written: dict[int, str] = {}

    def spliced(tx: str) -> list[int]:
        return [p for s, e in exons[tx] for p in range(s, e)]

    def write_orf(tx: str, start: int, codons: int) -> tuple[int, int]:
        """Write ATG + codons + stop along tx from genomic `start`; return (first, last+1) CDS positions."""
        pos = spliced(tx)
        i0 = pos.index(start)
        for k in range(codons + 2):
            idx = pos[i0 + 3 * k : i0 + 3 * k + 3]
            fixed = [written.get(p) for p in idx]
            last = k == codons + 1
            for _ in range(1000):
                codon = "ATG" if k == 0 else "".join(f or rng.choice(BASES) for f in fixed)
                if k == 0 or (codon in STOPS) == last:
                    if not last and codon in STOPS:
                        continue
                    break
            else:
                raise RuntimeError(f"cannot write codon {k} of {tx}")
            if last and codon not in STOPS:
                codon = rng.choice(sorted(s for s in STOPS if all(f in (None, s[j]) for j, f in enumerate(fixed))))
            for p, b in zip(idx, codon, strict=True):
                written[p] = b
                seq[p] = b
        return pos[i0], pos[i0 + 3 * (codons + 2) - 1] + 1

    # T1: ATG in exon 1, 290 codons -> stop in exon 4.
    write_orf("T1", 1100, 290)
    # T2 follows T1 through exon 3 and continues into its own last exon.
    t1 = spliced("T1")
    i3 = t1.index(3209)
    through = (i3 - t1.index(1100) + 1) // 3  # codons wholly before the exon 3 end
    t2 = spliced("T2")
    write_orf_len = through + 60
    pos2 = t2[t2.index(1100) :]
    for k in range(write_orf_len + 2):
        idx = pos2[3 * k : 3 * k + 3]
        fixed = [written.get(p) for p in idx]
        last = k == write_orf_len + 1
        choices = sorted(STOPS) if last else [a + b + c for a in BASES for b in BASES for c in BASES if a + b + c not in STOPS]
        options = [c for c in choices if all(f in (None, c[j]) for j, f in enumerate(fixed))]
        codon = options[0] if fixed.count(None) == 0 else rng.choice(options)
        for p, b in zip(idx, codon, strict=True):
            written[p] = b
            seq[p] = b
    return seq, {"exons": exons, "written": written}


def blocks(positions: list[int]) -> list[list[int]]:
    out: list[list[int]] = []
    for p in sorted(positions):
        if out and out[-1][1] == p:
            out[-1][1] = p + 1
        else:
            out.append([p, p + 1])
    return out


def orf(seq: list[str], exons: list[tuple[int, int]], first: int, codon_start: int = 1) -> tuple[list[int], str]:
    """Coding positions from `first` to the first in-frame stop (inclusive) along the exons."""
    pos = [p for s, e in exons for p in range(s, e)]
    i = pos.index(first) + (codon_start - 1)
    coding = pos[pos.index(first) : i]
    while i + 3 <= len(pos):
        codon = "".join(seq[p] for p in pos[i : i + 3])
        coding += pos[i : i + 3]
        i += 3
        if codon in STOPS:
            return coding, "".join(seq[p] for p in coding)
    raise RuntimeError("no stop codon")


def expression(values: dict[str, float], n: int = 3) -> list[dict]:
    return [
        {"cell_line": c, "n_libraries": n, "mean": v, "min": v * 0.8, "max": v * 1.2}
        for c, v in values.items()
    ]


def bundle(symbol: str, strand: str, seq: list[str], transcripts: list[dict], genomic_start: int) -> dict:
    return {
        "format": "dnagent-locus",
        "version": 1,
        "coordinates": "zero-based, half-open positions on `sequence`, the plus strand of `gene.chrom`; genomic (1-based) = genomic_start + position",
        "gene": {"symbol": symbol, "gene_id": f"ENSGSYN{symbol}.1", "chrom": "chrSyn", "strand": strand, "gene_type": "protein_coding"},
        "annotation_version": "synthetic-1",
        "genomic_start": genomic_start,
        "genomic_end": genomic_start + len(seq) - 1,
        "flank": 1000,
        "sequence": "".join(seq),
        "transcripts": transcripts,
        "expression_units": "synthetic",
        "panel": {
            "bambu_lr": {"cell_lines": [{"cell_line": c, "libraries": 3} for c in ("A549", "HEK293T", "K562")], "reports_zeros": True},
            "NanoCount_lr": {"cell_lines": [{"cell_line": c, "libraries": 2} for c in ("A549", "HEK293T", "K562")], "reports_zeros": False},
        },
        "novel_transcripts": 0,
        "provenance": [{"source": "synthetic", "version": "generate_synthetic_locus.py"}],
    }


def transcript(tid: str, exons, coding, protein, **fields) -> dict:
    base = {
        "transcript_id": tid, "transcript_type": "protein_coding", "is_mane_select": False,
        "is_ensembl_canonical": False, "is_basic": True, "tsl": "1", "appris": None,
        "cds_start_nf": False, "cds_end_nf": False, "protein_id": f"{tid}P" if coding else None,
        "aa_len": len(protein) if coding else None,
        "exons": [list(e) for e in exons], "cds": blocks(coding) if coding else [],
        "cds_includes_stop": bool(coding), "codon_start": 1,
        "start_codon": None, "stop_codon": None, "protein": protein,
        "evidence": {"state": "invisible", "mappings": [], "junctions": len(exons) - 1, "novel_junctions": 0},
        "expression": {},
    }
    base.update(fields)
    return base


LARGE_INSERTS = ((1650, 400_000), (3600, 420_000), (4600, 380_000))  # (locus position, bases)


def enlarge(small: dict, seed: int = 20261001) -> dict:
    """SYNLOC with long random introns: every coordinate at or after an insert shifts right."""
    rng = random.Random(seed)
    covered = {p for t in small["transcripts"] for s, e in t["exons"] for p in range(s, e)}
    assert not any(at in covered or at - 1 in covered for at, _ in LARGE_INSERTS), "inserts must fall between exons"
    shift = lambda p: p + sum(n for at, n in LARGE_INSERTS if p >= at)
    seq = small["sequence"]
    pieces, last = [], 0
    for at, n in LARGE_INSERTS:
        pieces += [seq[last:at], "".join(rng.choice(BASES) for _ in range(n))]
        last = at
    big = json.loads(json.dumps(small))
    big["sequence"] = "".join(pieces) + seq[last:]
    big["genomic_end"] = big["genomic_start"] + len(big["sequence"]) - 1
    big["gene"].update(symbol="SYNBIG", gene_id="ENSGSYNSYNBIG.1")
    for t in big["transcripts"]:
        # Blocks: the start shifts like a position, the end like the last base inside it.
        t["exons"] = [[shift(s), shift(e - 1) + 1] for s, e in t["exons"]]
        t["cds"] = [[shift(s), shift(e - 1) + 1] for s, e in t["cds"]]
        for key in ("start_codon", "stop_codon"):
            if t[key]:
                t[key]["position"] = shift(t[key]["position"])
    return big


def main() -> None:
    parser = argparse.ArgumentParser(description="Write the synthetic locus fixtures.")
    parser.add_argument("--large", type=Path, help="also write the ~1.2 Mb SYNBIG locus here")
    args = parser.parse_args()
    seq, info = build(20260930)
    exons = info["exons"]
    quantified = lambda src: {"state": "quantified", "mappings": [{"source_transcript_id": src, "status": "exact_structure", "same_exon_chain": True, "same_cds_start": True, "same_cds_stop": True}], "junctions": 3, "novel_junctions": 0}
    txs = []
    c1, s1 = orf(seq, exons["T1"], 1100)
    txs.append(transcript("SYNT0001.1", exons["T1"], c1, translate(s1[:-3]), is_mane_select=True, is_ensembl_canonical=True,
                          start_codon={"position": c1[0], "split": False}, stop_codon={"position": c1[-3], "split": False},
                          evidence=quantified("SYNR0001"),
                          expression={"bambu_lr": expression({"A549": 40.0, "HEK293T": 25.0, "K562": 31.0}),
                                      "NanoCount_lr": expression({"A549": 0.02, "K562": 0.01}, 2)}))
    c2, s2 = orf(seq, exons["T2"], 1100)
    txs.append(transcript("SYNT0002.1", exons["T2"], c2, translate(s2[:-3]),
                          start_codon={"position": c2[0], "split": False}, stop_codon={"position": c2[-3], "split": c2[-3] + 2 != c2[-1]},
                          evidence=quantified("SYNR0002"),
                          expression={"bambu_lr": expression({"A549": 4.0, "HEK293T": 9.0, "K562": 2.0}),
                                      "NanoCount_lr": expression({"HEK293T": 0.05}, 2)}))
    c3, s3 = orf(seq, exons["T3"], 1100)
    txs.append(transcript("SYNT0003.1", exons["T3"], c3, translate(s3[:-3]),
                          start_codon={"position": c3[0], "split": False}, stop_codon={"position": c3[-3], "split": False},
                          evidence={"state": "discoverable", "mappings": [], "junctions": 2, "novel_junctions": 1}))
    c4, s4 = orf(seq, exons["T4"], 2050, codon_start=2)
    txs.append(transcript("SYNT0004.1", exons["T4"], c4, translate(s4[1:-3]), tsl="3", cds_start_nf=True, codon_start=2,
                          stop_codon={"position": c4[-3], "split": False},
                          evidence=quantified("SYNR0004"),
                          expression={"bambu_lr": expression({"A549": 0.0, "HEK293T": 0.0, "K562": 0.0})}))
    txs.append(transcript("SYNT0005.1", exons["T5"], [], "", transcript_type="retained_intron", tsl="2"))
    (HERE / "synthetic_locus.locus.json").write_text(json.dumps(bundle("SYNLOC", "+", seq, txs, 1_000_001), indent=1) + "\n")

    # Minus strand: mirror T1-T3 (transcription orientation -> reverse complement of the locus).
    n = len(seq)
    minus_seq = list("".join(seq).translate(COMP)[::-1])
    mirror = lambda blocks_: sorted([[n - e, n - s] for s, e in blocks_])
    minus = []
    for t in txs[:3]:
        m = dict(t)
        m["transcript_id"] = t["transcript_id"].replace("SYNT", "SYNM")
        m["exons"] = mirror(t["exons"])
        m["cds"] = mirror(t["cds"])
        for key in ("start_codon", "stop_codon"):
            if t[key]:
                m[key] = {"position": n - 1 - t[key]["position"], "split": t[key]["split"]}
        minus.append(m)
    (HERE / "synthetic_minus.locus.json").write_text(json.dumps(bundle("SYNMIN", "-", minus_seq, minus, 2_000_001), indent=1) + "\n")
    if args.large:
        small = json.loads((HERE / "synthetic_locus.locus.json").read_text())
        args.large.parent.mkdir(parents=True, exist_ok=True)
        args.large.write_text(json.dumps(enlarge(small)) + "\n")


if __name__ == "__main__":
    main()

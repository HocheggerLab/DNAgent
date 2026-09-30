# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85"]
# ///
"""Check Gibson product GenBank (`gibson … --out`, `gibson-optimise … --out`) independently.

For each public plan, Biopython 1.85 reads the sources and the written product, and:
- the product sequence equals the oriented cores concatenated (cores cut from the sources
  here, wrapping on circular sources) and the JSON `product_sequence_5to3`;
- the carried features are exactly the source features lying wholly inside a core, each
  at the product positions computed here from the core interval and orientation, with the
  same type and the same extracted bases; every feature only partly inside a core is
  reported by one `gibson_feature_clipped` warning per core and is absent;
- each primer_bind "Gibson F<n>/R<n>" reads (reverse-complemented on the reverse strand)
  as the JSON primer, whole oligo including its tail; each "Gibson overlap a-b" reads as
  the JSON junction overlap;
- the file's primer list holds every JSON primer.
"""

import argparse
import io
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import warnings

from Bio import BiopythonWarning, SeqIO
from Bio.Seq import Seq

ROOT = Path(__file__).resolve().parents[1]
PLANS = [("gibson", "synthetic-gibson.json"), ("gibson", "synthetic-cdna-into-puc19.json"),
         ("gibson-optimise", "synthetic-gibson-optimisation.json"), ("gibson-optimise", "synthetic-cdna-into-puc19-optimised.json")]


def read(path: Path):
    """Biopython record; SnapGene primers (packet 5) removed: DNAgent keeps them as unplaced primers."""
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", BiopythonWarning)
        if path.suffix == ".dna":
            data, kept, offset = path.read_bytes(), [], 0
            while offset + 5 <= len(data):
                kind, length = struct.unpack(">BI", data[offset:offset + 5])
                if kind != 5:
                    kept.append(data[offset:offset + 5 + length])
                offset += 5 + length
            return SeqIO.read(io.BytesIO(b"".join(kept)), "snapgene")
        return SeqIO.read(path, "genbank")


def positions(feature, n: int) -> list[int]:
    """Covered positions in feature (transcript) order."""
    out = []
    for part in feature.location.parts:
        run = [p % n for p in range(int(part.start), int(part.end))]
        out += run[::-1] if part.strand == -1 else run
    return out


def bases(feature, sequence) -> str:
    """A feature's bases; a feature tiling one contiguous (circular) span is read from the
    span, because Biopython 1.85 mis-orders the pieces of reverse SnapGene features that
    wrap the origin (see check_feature_library.py)."""
    text, n = str(sequence).upper(), len(sequence)
    cover = [p % n for part in feature.location.parts for p in range(int(part.start), int(part.end))]
    if len(set(cover)) == len(cover):
        starts = [p for p in set(cover) if (p - 1) % n not in set(cover)]
        if len(starts) == 1:
            span = "".join(text[(starts[0] + i) % n] for i in range(len(cover)))
            return str(Seq(span).reverse_complement()) if feature.location.strand == -1 else span
    return str(feature.extract(sequence)).upper()


def check_plan(binary: Path, command: str, plan_name: str, scratch: Path) -> str:
    plan_path = ROOT / "fixtures/plans" / plan_name
    plan = json.loads(plan_path.read_text())
    product_path = scratch / f"{plan_path.stem}.gb"
    args = [str(binary), command, str(plan_path), "--out", str(product_path)] + (["--output", "json"] if command == "gibson" else [])
    body = json.loads(subprocess.run(args, capture_output=True, text=True, timeout=300).stdout)
    assert body["ok"], body.get("error")
    result = body["result"]
    design = result["design"] if command == "gibson-optimise" else result
    primers = ([(p["forward"]["primer"], p["reverse"]["primer"]) for p in result["pairs"]] if command == "gibson-optimise"
               else [(c["forward_primer"], c["reverse_primer"]) for c in design["components"]])
    sources = [read((plan_path.parent / i["path"]).resolve()) for i in plan["inputs"]]
    product = read(product_path)
    seq = str(product.seq).upper()
    n = len(seq)
    problems = []

    # Product = oriented cores concatenated.
    cores, offset = [], 0
    for core in plan["cores"]:
        src = str(sources[core["input"] - 1].seq).upper()
        piece = "".join(src[(core["start"] + i) % len(src)] for i in range(core["length"]))
        if core["orientation"] == "reverse":
            piece = str(Seq(piece).reverse_complement())
        cores.append((core, offset))
        offset += core["length"]
    expected_product = "".join(
        (lambda c: (lambda s: s if c["orientation"] == "forward" else str(Seq(s).reverse_complement()))(
            "".join(str(sources[c["input"] - 1].seq).upper()[(c["start"] + i) % len(sources[c["input"] - 1].seq)] for i in range(c["length"]))))(c)
        for c in plan["cores"])
    if seq != expected_product or seq != design["product_sequence_5to3"]:
        problems.append("product sequence differs from the concatenated oriented cores")

    # Carried and clipped features, computed from the core intervals.
    expected, clipped = [], 0
    for core, start in cores:
        src_record = sources[core["input"] - 1]
        m = len(src_record.seq)
        inside = {(core["start"] + i) % m: i for i in range(core["length"])}
        for feature in src_record.features:
            if feature.type == "source":
                continue
            covered = positions(feature, m)
            hits = [p for p in covered if p in inside]
            if not hits:
                continue
            if len(hits) < len(covered):
                clipped += 1
                continue
            local = [inside[p] for p in covered]
            if core["orientation"] == "reverse":
                local = [core["length"] - 1 - i for i in local]
            strand = feature.location.strand or 0
            strand = -strand if core["orientation"] == "reverse" else strand
            expected.append((feature.type, frozenset(start + i for i in local), strand,
                             bases(feature, src_record.seq)))
    carried = [f for f in product.features if not str(f.qualifiers.get("label", [""])[0]).startswith("Gibson ")]
    got = sorted((f.type, frozenset(positions(f, n)), f.location.strand or 0, bases(f, product.seq)) for f in carried)
    want = sorted(expected, key=lambda e: (e[0], sorted(e[1]), e[2], e[3]))
    got_keys = sorted((t, tuple(sorted(p)), s, b) for t, p, s, b in got)
    want_keys = sorted((t, tuple(sorted(p)), s, b) for t, p, s, b in want)
    if got_keys != want_keys:
        problems.append(f"carried features differ: {len(got_keys)} written, {len(want_keys)} expected; "
                        f"written only {[(t, p[:3], s) for t, p, s, _ in set(got_keys) - set(want_keys)]}, "
                        f"expected only {[(t, p[:3], s) for t, p, s, _ in set(want_keys) - set(got_keys)]}")
    reported = sum(w["code"] == "gibson_feature_clipped" for w in body["warnings"])
    if reported != clipped:
        problems.append(f"{reported} gibson_feature_clipped warnings, {clipped} partly covered features")

    # Primer sites and overlaps read as the JSON oligos.
    by_label = {f.qualifiers.get("label", [""])[0]: f for f in product.features}
    for i, (forward, reverse) in enumerate(primers, start=1):
        for letter, primer in (("F", forward), ("R", reverse)):
            feature = by_label.get(f"Gibson {letter}{i}")
            if feature is None or bases(feature, product.seq) != primer["sequence_5to3"]:
                problems.append(f"primer {letter}{i} is not annotated where it reads as the oligo")
    for junction in design["junctions"]:
        feature = by_label.get(f"Gibson overlap {junction['after_component']}-{junction['before_component']}")
        if feature is None or bases(feature, product.seq) != junction["overlap_sequence_5to3"]:
            problems.append(f"overlap {junction['after_component']}-{junction['before_component']} is wrong or missing")
    listed = subprocess.run([str(binary), "primers", str(product_path), "--output", "json"], capture_output=True, text=True).stdout
    listed = {p["sequence"] for p in json.loads(listed)["result"]}
    if listed != {p["sequence_5to3"] for pair in primers for p in pair}:
        problems.append("the file's primer list differs from the designed primers")
    if problems:
        raise AssertionError(f"{plan_name}: " + "; ".join(problems))
    return f"{plan_name}: {n} bp product, {len(got_keys)} carried features, {clipped} left out, {2 * len(primers)} primers, {len(design['junctions'])} overlaps"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve()
    with tempfile.TemporaryDirectory(prefix="dnagent-gibson-product-") as scratch:
        lines = [check_plan(binary, command, plan, Path(scratch)) for command, plan in PLANS]
    print("Gibson product GenBank agrees with an independent Biopython reading:\n  " + "\n  ".join(lines))


if __name__ == "__main__":
    main()

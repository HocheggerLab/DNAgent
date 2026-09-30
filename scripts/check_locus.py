# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85"]
# ///
"""Check DNAgent's reading of gene locus bundles (`dnagent-locus` v1) against the bundle itself.

Public synthetic fixtures by default; pass `--bundle PATH` (repeatable) to also check
locally exported real genes (e.g. from `degron-db locus`). Nothing is written outside
a temporary folder, so private bundles never enter Git.

Independent references, per transcript:
- `dnagent features`: the mRNA (misc_RNA) and CDS feature parts equal the bundle's exon and
  CDS blocks, on the gene's strand;
- the CDS is spliced from the bundle sequence in Python (reverse complement on the minus
  strand) and translated with Biopython; it must end in a stop codon, start with ATG unless
  5'-incomplete, and equal the bundle protein and `dnagent translate` (a non-ATG annotated
  start is read as its literal amino acid by DNAgent and as M by the exporter: reported);
- `start_codon`/`stop_codon` sit at the first and last codon of that splice;
- `dnagent isoforms`: panel means, display states and the row order are recomputed here
  from the bundle's expression and evidence (dense quantifiers average the measured cell
  lines; sparse ones count missing lines as 0; no rows at all is no data);
- the same view survives `dnagent convert` to GenBank and back.
"""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from Bio.Seq import Seq

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = [ROOT / "fixtures/formats/locus/synthetic_locus.locus.json", ROOT / "fixtures/formats/locus/synthetic_minus.locus.json"]
EVIDENCE_RANK = {"quantified": 0, "discoverable": 1}


def run(binary: Path, *args: str) -> dict:
    # `convert` always reports JSON and takes no --output flag.
    extra = [] if args[0] == "convert" else ["--output", "json"]
    proc = subprocess.run([str(binary), *args, *extra], capture_output=True, text=True, timeout=120)
    body = json.loads(proc.stdout)
    if not body["ok"]:
        raise ValueError(f"dnagent {' '.join(args)} failed: {body['error']}")
    return body["result"]


def blocks(parts: list[dict]) -> list[list[int]]:
    return sorted([p["start"], p["end"]] for p in parts)


def splice(sequence: str, cds: list[list[int]], minus: bool) -> str:
    joined = "".join(sequence[s:e] for s, e in sorted(cds))
    return str(Seq(joined).reverse_complement()) if minus else joined


def panel_mean(cells: list[dict], panel: dict) -> float | None:
    if not cells:
        return None
    by_line = {c["cell_line"]: c["mean"] for c in cells}
    lines = [p["cell_line"] for p in panel["cell_lines"]]
    values = [by_line[line] for line in lines if line in by_line] if panel.get("reports_zeros") else [by_line.get(line, 0.0) for line in lines]
    return sum(values) / len(values) if values else None


def display(state: str, mean: float | None) -> str:
    if state == "quantified":
        return "no_data" if mean is None else "expressed" if mean > 0 else "not_detected"
    return "discoverable" if state == "discoverable" else "invisible"


def check_bundle(binary: Path, path: Path, scratch: Path) -> list[str]:
    bundle = json.loads(path.read_text())
    sequence = bundle["sequence"].upper()
    minus = bundle["gene"]["strand"] == "-"
    features = {f["id"]: f for f in run(binary, "features", str(path))}
    view = run(binary, "isoforms", str(path))
    by_id = {i["transcript_id"]: i for i in view["isoforms"]}
    notes: list[str] = []
    for t in bundle["transcripts"]:
        tid = t["transcript_id"]
        isoform = by_id[tid]
        mrna = features[isoform["mrna_feature_id"]]
        assert blocks(mrna["location"]["parts"]) == sorted(t["exons"]), f"{tid}: mRNA parts differ from the exons"
        assert mrna["strand"] == ("reverse" if minus else "forward"), f"{tid}: strand"
        assert mrna["kind"] == ("mRNA" if t["cds"] else "misc_RNA"), f"{tid}: kind {mrna['kind']}"
        if not t["cds"]:
            assert isoform["cds_feature_id"] is None and t["protein"] in ("", None), f"{tid}: non-coding with a CDS"
            continue
        cds = features[isoform["cds_feature_id"]]
        assert blocks(cds["location"]["parts"]) == sorted(t["cds"]), f"{tid}: CDS parts differ"
        coding = splice(sequence, t["cds"], minus)
        frame = t["codon_start"] - 1
        protein = str(Seq(coding[frame : frame + (len(coding) - frame) // 3 * 3]).translate())
        if t["cds_includes_stop"]:
            assert protein.endswith("*"), f"{tid}: CDS does not end in a stop codon"
            protein = protein[:-1]
        assert "*" not in protein, f"{tid}: internal stop codon"
        if not t["cds_start_nf"]:
            if coding[:3] != "ATG":
                notes.append(f"{path.name} {tid}: annotated start codon {coding[:3]} (exporter reads M)")
                protein = "M" + protein[1:]
        assert protein == t["protein"], f"{tid}: bundle protein differs from the spliced CDS translation"
        translated = run(binary, "translate", str(path), "--feature", isoform["cds_feature_id"])
        assert translated["terminal_stop"] == t["cds_includes_stop"], f"{tid}: terminal stop"
        dnagent = translated["protein"].removesuffix("*") if translated["terminal_stop"] else translated["protein"]
        expected_dnagent = protein if t["cds_start_nf"] or coding[:3] == "ATG" else str(Seq(coding[frame : frame + 3]).translate()) + protein[1:]
        assert dnagent == expected_dnagent, f"{tid}: dnagent translate differs"
        exonic = [p for s, e in sorted(t["cds"]) for p in range(s, e)]
        if minus:
            exonic = exonic[::-1]
        if t["start_codon"]:
            assert t["start_codon"]["position"] == exonic[0] and not t["cds_start_nf"], f"{tid}: start codon position"
        if t["stop_codon"]:
            assert t["stop_codon"]["position"] == exonic[-3], f"{tid}: stop codon position"
        assert isoform["start_codon"] == t["start_codon"] and isoform["stop_codon"] == t["stop_codon"], f"{tid}: codon marks"
    # Expression, display and order, recomputed from the bundle.
    for quantifier, panel in bundle["panel"].items():
        means = {}
        for t in bundle["transcripts"]:
            cells = t["expression"].get(quantifier, [])
            mean = panel_mean(cells, panel) if t["evidence"]["state"] == "quantified" else None
            got = next(e for e in by_id[t["transcript_id"]]["expression"] if e["quantifier"] == quantifier)
            assert (got["panel_mean"] is None) == (mean is None) and (mean is None or abs(got["panel_mean"] - mean) <= 1e-9 * max(1.0, mean)), f"{t['transcript_id']} {quantifier}: panel mean {got['panel_mean']} != {mean}"
            assert got["display"] == display(t["evidence"]["state"], mean), f"{t['transcript_id']} {quantifier}: display"
            means[t["transcript_id"]] = mean
        order = sorted(bundle["transcripts"], key=lambda t: (
            -(means[t["transcript_id"]] if means[t["transcript_id"]] is not None else float("-inf")),
            EVIDENCE_RANK.get(t["evidence"]["state"], 2), not t["is_mane_select"], t["transcript_id"]))
        got_order = run(binary, "isoforms", str(path), "--quantifier", quantifier)
        assert [i["transcript_id"] for i in got_order["isoforms"]] == [t["transcript_id"] for t in order], f"{quantifier}: order"
    # Lossless GenBank round trip of everything the view shows.
    saved = scratch / f"{path.name}.gb"
    run(binary, "convert", str(path), "--out", str(saved))
    assert run(binary, "isoforms", str(saved)) == view, f"{path.name}: isoform view changed after a GenBank round trip"
    return notes


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/dnagent")
    parser.add_argument("--bundle", type=Path, action="append", default=[], help="extra (local, private) locus bundle to check")
    args = parser.parse_args()
    paths = [*FIXTURES, *(p.expanduser().resolve() for p in args.bundle)]
    with tempfile.TemporaryDirectory() as scratch:
        for path in paths:
            notes = check_bundle(args.binary, path, Path(scratch))
            count = len(json.loads(path.read_text())["transcripts"])
            print(f"{path.name}: {count} transcripts agree" + "".join(f"\n  note: {n}" for n in notes))


if __name__ == "__main__":
    main()

# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85"]
# ///
"""Check DNAgent GenBank reading and writing against Biopython 1.85; public fixtures only.

1. Every public SnapGene fixture is converted with `dnagent convert`; Biopython must parse
   the result, and its sequence, topology, feature keys, labels, strands, locations (part
   by part, in DNAgent source order) and qualifiers must equal the CLI's view of the
   original. The file must also reopen in DNAgent with identical JSON projections.
2. Third-party GenBank (NCBI M77789.2, and the same record re-written by Biopython) is
   read by DNAgent and compared with Biopython's parse of the same file.
"""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from Bio import SeqIO

ROOT = Path(__file__).resolve().parents[1]
SNAPGENE = ROOT / "fixtures/formats/snapgene"
DERIVED_KEYS = {"label", "dnagent_color", "dnagent_location", "dnagent_id"}


def cli(binary: Path, *args: str) -> dict:
    json_flag = [] if args[0] in ("convert", "annotate") else ["--output", "json"]  # those are always JSON
    proc = subprocess.run([str(binary), *args, *json_flag], capture_output=True, text=True, timeout=60)
    body = json.loads(proc.stdout)
    if not body["ok"]:
        raise AssertionError(f"dnagent {' '.join(args)} failed: {body['error']}")
    return body


def flat_ranges(parts: list[dict], length: int) -> list[tuple[int, int]]:
    ranges = []
    for part in parts:
        if part["kind"] == "linear":
            ranges.append((part["start"], part["end"]))
        elif part["start"] + part["length"] <= length:
            ranges.append((part["start"], part["start"] + part["length"]))
        else:
            ranges += [(part["start"], length), (0, part["start"] + part["length"] - length)]
    return ranges


def biopython_ranges(feature) -> tuple[list[tuple[int, int]], int | None]:
    parts = list(feature.location.parts)
    strand = feature.location.strand
    if strand == -1:
        parts = parts[::-1]  # Biopython lists complement(join(a,b)) as [b, a]
    return [(int(p.start), int(p.end)) for p in parts], strand


def biopython_qualifiers(feature, skip=()) -> dict[str, list[str]]:
    return {k: list(v) for k, v in feature.qualifiers.items() if k not in skip}


def cli_qualifiers(feature: dict) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for q in feature["qualifiers"]:
        out.setdefault(q["key"], []).append(q["value"] if q["value"] is not None else "")
    return out


def check_written(binary: Path, fixture: Path, directory: Path) -> int:
    out = directory / fixture.with_suffix(".gb").name
    cli(binary, "convert", str(fixture), "--out", str(out))
    record = SeqIO.read(out, "genbank")
    sequence = cli(binary, "sequence", str(fixture))["result"]["sequence"]
    inspect = cli(binary, "inspect", str(fixture))["result"]
    features = cli(binary, "features", str(fixture))["result"]
    assert str(record.seq).upper() == sequence, fixture.name
    assert record.annotations.get("topology") == inspect["topology"], fixture.name
    assert len(record.features) == len(features), fixture.name
    for ours, theirs in zip(features, record.features):
        where = f"{fixture.name} {ours['id']}"
        assert theirs.type == ours["kind"], where
        assert theirs.qualifiers["label"][0] == ours["label"], where
        ranges, strand = biopython_ranges(theirs)
        assert ranges == flat_ranges(ours["location"]["parts"], len(sequence)), (where, ranges)
        expected_strand = {"forward": 1, "reverse": -1, "unknown": 1}[ours["strand"]]
        assert strand == expected_strand, where
        if ours["strand"] == "unknown" or any(p["kind"] == "circular_arc" for p in ours["location"]["parts"]):
            assert "dnagent_location" in theirs.qualifiers, where
        assert biopython_qualifiers(theirs, DERIVED_KEYS) == cli_qualifiers(ours) | {}, where
        if ours["color"]:
            assert theirs.qualifiers["dnagent_color"] == [ours["color"]], where
    for command in ("inspect", "features", "primers"):
        a, b = cli(binary, command, str(fixture)), cli(binary, command, str(out))
        assert (a["result"], a["warnings"]) == (b["result"], b["warnings"]), (fixture.name, command)
    return len(features)


def check_third_party(binary: Path, path: Path) -> int:
    record = SeqIO.read(path, "genbank")
    features = cli(binary, "features", str(path))["result"]
    sequence = cli(binary, "sequence", str(path))["result"]["sequence"]
    assert sequence == str(record.seq).upper(), path.name
    assert cli(binary, "inspect", str(path))["result"]["topology"] == record.annotations["topology"], path.name
    assert len(features) == len(record.features), path.name
    for ours, theirs in zip(features, record.features):
        where = f"{path.name} {ours['id']}"
        assert ours["kind"] == theirs.type, where
        ranges, strand = biopython_ranges(theirs)
        assert ranges == flat_ranges(ours["location"]["parts"], len(sequence)), where
        assert {1: "forward", -1: "reverse", None: "forward"}[strand] == ours["strand"], where
        assert cli_qualifiers(ours) == biopython_qualifiers(theirs), where
    return len(features)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve()
    written = third = 0
    with tempfile.TemporaryDirectory(prefix="dnagent-genbank-") as name:
        directory = Path(name)
        for fixture in sorted(SNAPGENE.glob("synthetic_*.dna")) + [SNAPGENE / "pUC19_M77789.dna"]:
            written += check_written(binary, fixture, directory)
        ncbi = ROOT / "fixtures/formats/genbank/pUC19_M77789.gb"
        third += check_third_party(binary, ncbi)
        rewritten = directory / "biopython_rewrite.gb"
        SeqIO.write(SeqIO.read(ncbi, "genbank"), rewritten, "genbank")
        third += check_third_party(binary, rewritten)
    print(f"GenBank matches Biopython 1.85: {written} written features across all SnapGene fixtures, "
          f"{third} third-party features (NCBI and Biopython-written).")


if __name__ == "__main__":
    main()

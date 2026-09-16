# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85", "jsonschema==4.23.0"]
# ///
"""Independent whole-product and component-placement checks for explicit ligation.

Uses source recovery and designed Type IIS fusions, not an experimental oracle.
Temporary constructs are deterministic synthetic originals; private inputs opt in.
"""

import argparse
import hashlib
from itertools import product
import json
from pathlib import Path
import struct
import subprocess
import tempfile

from Bio import SeqIO
from Bio.Seq import Seq
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
VALIDATOR = Draft202012Validator(
    json.loads((ROOT / "schemas/cli-envelope-0.8.0.schema.json").read_text())
)
PLAN_VALIDATOR = Draft202012Validator(
    json.loads((ROOT / "schemas/ligation-plan-1.schema.json").read_text())
)


def rc(s):
    return str(Seq(s).reverse_complement())


def fixture(path, sequence, circular=False):
    def packet(kind, body):
        return struct.pack(">BI", kind, len(body)) + body

    xml = f'<Features><Feature name="synthetic" type="CDS" directionality="1"><Segment range="1-{len(sequence)}" type="standard"/></Feature></Features>'
    path.write_bytes(
        packet(9, b"SnapGene\0\1\0\1\0\1")
        + packet(0, bytes([int(circular)]) + sequence.encode())
        + packet(10, xml.encode())
    )


def select(i, f, reverse=False):
    return {
        "input": i,
        "fragment_id": f"fragment-{f:04}",
        "orientation": "reverse" if reverse else "forward",
    }


def make_plan(sources, selected, topology):
    return {
        "schema_version": 1,
        "inputs": [
            {"path": str(path), "enzymes": enzymes} for path, enzymes in sources
        ],
        "fragments": selected,
        "topology": topology,
    }


def verify(body, plan):
    result = body["result"]
    p = result["product"]
    assert p["topology"] == plan["topology"]
    assert len(p["components"]) == len(plan["fragments"])
    expected_top, expected_bottom = [], []
    selected_keys = set()
    for placement, selection in zip(p["components"], plan["fragments"], strict=True):
        assert placement["selection"] == selection
        index = selection["input"] - 1
        selected_keys.add((index + 1, selection["fragment_id"]))
        source = result["inputs"][index]
        fragment = next(
            f
            for f in source["digest"]["fragments"]
            if f["id"] == selection["fragment_id"]
        )
        annotations = next(
            a for a in source["annotations"] if a["fragment_id"] == fragment["id"]
        )
        for side in ["top", "bottom"]:
            original_side = (
                side
                if selection["orientation"] == "forward"
                else {"top": "bottom", "bottom": "top"}[side]
            )
            strand = fragment[original_side]
            span = placement[side]
            assert span["source_strand"] == (
                "forward" if original_side == "top" else "reverse"
            )
            assert span["length"] == strand["length"]
            assert span["start"] == (
                sum(map(len, expected_top))
                if side == "top"
                else len(p["bottom_sequence_5to3"])
                - sum(map(len, expected_bottom))
                - strand["length"]
            )
            sequence = p[f"{side}_sequence_5to3"]
            assert (
                sequence[span["start"] : span["start"] + span["length"]]
                == strand["sequence_5to3"]
            )
            # Component annotations remain valid after the explicit local-to-product offset.
            for mapping in annotations[original_side]:
                for part in mapping["parts"]:
                    region = part["fragment_region"]
                    a, b = region["start"], region["end"]
                    assert (
                        sequence[span["start"] + a : span["start"] + b]
                        == strand["sequence_5to3"][a:b]
                    )
            (expected_top if side == "top" else expected_bottom).append(
                strand["sequence_5to3"]
            )
    assert p["top_sequence_5to3"] == "".join(expected_top)
    assert p["bottom_sequence_5to3"] == "".join(reversed(expected_bottom))
    all_keys = [
        (i, f["id"])
        for i, source in enumerate(result["inputs"], 1)
        for f in source["digest"]["fragments"]
    ]
    assert [(u["input"], u["fragment_id"]) for u in result["unused_fragments"]] == [
        k for k in all_keys if k not in selected_keys
    ]
    count = len(plan["fragments"])
    assert len(p["junctions"]) == count - (p["topology"] == "linear")
    for i, junction in enumerate(p["junctions"]):
        assert junction["after_component"] == i + 1
        assert junction["before_component"] == (i + 1) % count + 1
        assert junction["closure"] == (i + 1 == count)
        assert junction["assessment"]["compatible"]
        assert junction["top_boundary"] == (
            0 if junction["closure"] else sum(map(len, expected_top[: i + 1]))
        )
        assert junction["bottom_boundary"] == sum(map(len, expected_bottom[i + 1 :]))
    top, bottom = p["top_sequence_5to3"], rc(p["bottom_sequence_5to3"])
    offset = p["bottom_forward_start"]
    if p["topology"] == "circular":
        assert len(top) == len(bottom) == p["paired_length"]
        assert bottom == top[offset:] + top[:offset]
        assert p["left_end"] is None and p["right_end"] is None
    else:
        lo, hi = max(0, offset), min(len(top), offset + len(bottom))
        assert hi - lo == p["paired_length"]
        assert top[lo:hi] == bottom[lo - offset : hi - offset]
        assert p["left_end"] is not None and p["right_end"] is not None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    cases = private = 0
    with tempfile.TemporaryDirectory(prefix="dnagent-ligation-") as directory:
        root = Path(directory)
        first, second, path = (
            root / "first.dna",
            root / "second.dna",
            root / "plan.json",
        )

        def call(plan, success=True, strict=False):
            nonlocal cases
            if success:
                assert PLAN_VALIDATOR.is_valid(plan)
            path.write_text(json.dumps(plan))
            proc = subprocess.run(
                [str(binary), "ligate", str(path), *(["--strict"] if strict else [])],
                capture_output=True,
                text=True,
                timeout=30,
            )
            body = json.loads(proc.stdout)
            assert VALIDATOR.is_valid(body), "ligation envelope schema mismatch"
            assert not proc.stderr
            assert (proc.returncode == 0) == success == body["ok"]
            if success:
                verify(body, plan)
            else:
                assert "result" not in body
            cases += 1
            return body

        for bases in product("ACGT", repeat=4):
            overhang = "".join(bases)
            fixture(first, f"AAAGGTCTCA{overhang}TTT")
            for reverse in [False, True]:
                fixture(second, f"AAACGTCTCA{rc(overhang) if reverse else overhang}CCC")
                plan = make_plan(
                    [(first, ["BsaI"]), (second, ["BsmBI"])],
                    [select(1, 1), select(2, 1 if reverse else 2, reverse)],
                    "linear",
                )
                body = call(plan)
                expected = (
                    "AAAGGTCTCA" + overhang + (rc("AAACGTCTCA") if reverse else "CCC")
                )
                assert body["result"]["product"]["top_sequence_5to3"] == expected
                assert body["result"]["product"]["bottom_sequence_5to3"] == rc(expected)
        for sequence, enzyme in [
            ("AAAGAATTCTGC", "EcoRI"),
            ("AAAGGTACCTGC", "KpnI"),
            ("AAAGATATCTGC", "EcoRV"),
        ]:
            for rotation in range(len(sequence)):
                rotated = sequence[rotation:] + sequence[:rotation]
                fixture(first, rotated, True)
                for reverse in [False, True]:
                    body = call(
                        make_plan(
                            [(first, [enzyme])], [select(1, 1, reverse)], "circular"
                        )
                    )
                    actual = body["result"]["product"]["top_sequence_5to3"]
                    expected = rc(sequence) if reverse else sequence
                    assert len(actual) == len(expected) and actual in expected * 2
            fixture(first, sequence)
            for reverse in [False, True]:
                selected = [
                    select(1, n, reverse) for n in ([2, 1] if reverse else [1, 2])
                ]
                body = call(make_plan([(first, [enzyme])], selected, "linear"))
                assert body["result"]["product"]["top_sequence_5to3"] == (
                    rc(sequence) if reverse else sequence
                )
        fixture(first, "AAAGAATTCTGC")
        fixture(second, "AAAGGTACCTGC")
        bad = make_plan(
            [(first, ["EcoRI"]), (second, ["KpnI"])],
            [select(1, 1), select(2, 2)],
            "linear",
        )
        assert call(bad, False)["error"]["code"] == "ligation_failed"
        bad["fragments"] = [select(1, 1)] * 2
        call(bad, False)
        bad["fragments"] = [select(0, 1)]
        call(bad, False)
        bad["fragments"] = [select(1, 99)]
        call(bad, False)
        bad["fragments"] = [select(1, 1)]
        bad["topology"] = "circular"
        call(bad, False)  # Blunt-to-cohesive closure, not a circular product.
        bad["fragments"] *= 129
        call(bad, False)
        warning_source = ROOT / "fixtures/formats/snapgene/synthetic_circular.dna"
        plan = make_plan([(warning_source, ["EcoRI"])], [select(1, 1)], "linear")
        assert call(plan, False, True)["error"]["code"] == "import_warnings"
        # Check warning retention if a later source cannot be loaded.
        plan["inputs"].append({"path": str(root / "missing.dna"), "enzymes": ["EcoRI"]})
        assert call(plan, False)["warnings"]
        for filename in ["synthetic-religation.json", "synthetic-closure.json"]:
            plan = json.loads((ROOT / "fixtures/plans" / filename).read_text())
            for source in plan["inputs"]:
                source["path"] = str(
                    (ROOT / "fixtures/plans" / source["path"]).resolve()
                )
            call(plan)
        if args.manifest:
            manifest = args.manifest.resolve()
            data = json.loads(manifest.read_text())
            assert data["schema_version"] == 1 and data["records"]
            for entry in data["records"]:
                source = (manifest.parent / entry["file"]).resolve()
                assert source.is_relative_to(manifest.parent)
                before = hashlib.sha256(source.read_bytes()).hexdigest()
                assert before == entry["sha256"]
                proc = subprocess.run(
                    [
                        str(binary),
                        "digest",
                        str(source),
                        "--enzymes",
                        "EcoRI,BamHI",
                        "--output",
                        "json",
                    ],
                    capture_output=True,
                    text=True,
                    check=True,
                    timeout=30,
                )
                digest = json.loads(proc.stdout)["result"]
                count = len(digest["fragments"])
                for reverse in [False, True]:
                    numbers = range(count, 0, -1) if reverse else range(1, count + 1)
                    body = call(
                        make_plan(
                            [(source, ["EcoRI", "BamHI"])],
                            [select(1, n, reverse) for n in numbers],
                            "circular",
                        )
                    )
                    # Biopython source sequence, not DNAagent product/digest sequence.
                    expected = str(SeqIO.read(source, "snapgene").seq).upper()
                    if reverse:
                        expected = rc(expected)
                    actual = body["result"]["product"]["top_sequence_5to3"]
                    assert len(actual) == len(expected) and actual in expected * 2
                assert hashlib.sha256(source.read_bytes()).hexdigest() == before
                private += 1
    print(
        f"Ligation/schema/placement/source-recovery checks passed: {cases} cases, including {private} private inputs and 512 designed Type IIS fusions."
    )


if __name__ == "__main__":
    main()

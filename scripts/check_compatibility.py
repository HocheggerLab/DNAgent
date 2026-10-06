# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85", "jsonschema==4.23.0"]
# ///
"""Check end matrices using all 256 synthetic Type IIS four-base overhangs.

Reference oligo reverse complements use Biopython 1.85. This is not an
experimental ligation reference; polarity/orientation rules are explicit below.
No private data is required or written.
"""

import argparse
from itertools import product, combinations
import json
from pathlib import Path
import struct
import subprocess
import tempfile

from Bio.Seq import Seq
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def reference_reason(a, b):
    if a["polarity"] != b["polarity"]:
        return "polarity_mismatch"
    if a["polarity"] == "blunt":
        return "blunt_ends"
    if len(a["overhang_sequence"]) != len(b["overhang_sequence"]):
        return "overhang_length_mismatch"
    if a["overhang_sequence"] == str(Seq(b["overhang_sequence"]).reverse_complement()):
        return "complementary_overhangs"
    return "overhang_sequence_mismatch"


def check_matrix(body):
    result = body["result"]
    endpoints = result["analysis"]["endpoints"]
    expected = []
    for index, source in enumerate(result["inputs"], 1):
        for fragment in source["digest"]["fragments"]:
            for side in ["left", "right"]:
                end = fragment[f"{side}_end"]
                if end is not None:
                    expected.append(
                        {
                            "id": f"input-{index}:{fragment['id']}:{side}",
                            "input": index,
                            "fragment_id": fragment["id"],
                            "side": side,
                            "end": end,
                        }
                    )
    require(endpoints == expected, "endpoint provenance/order mismatch")
    pairs = result["analysis"]["pairs"]
    require(
        len(pairs) == len(endpoints) * (len(endpoints) - 1) // 2,
        "pair matrix is incomplete",
    )
    for actual, (a, b) in zip(pairs, combinations(endpoints, 2)):
        reason = reference_reason(a["end"], b["end"])
        require(
            actual["first"] == a["id"] and actual["second"] == b["id"],
            "pair ordering or identity mismatch",
        )
        require(
            actual["same_fragment"]
            == (a["input"] == b["input"] and a["fragment_id"] == b["fragment_id"]),
            "same-fragment provenance mismatch",
        )
        expected = {
            "compatible": reason in ["blunt_ends", "complementary_overhangs"],
            "reason": reason,
            "second_fragment_orientation": "reverse"
            if a["side"] == b["side"]
            else "forward",
            "second_placement": "before_first"
            if a["side"] == "left"
            else "after_first",
        }
        require(
            actual["assessment"] == expected, "end assessment differs from reference"
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    binary = parser.parse_args().binary.resolve()
    validator = Draft202012Validator(
        json.loads((ROOT / "schemas/cli-envelope-0.10.0.schema.json").read_text())
    )
    checked = 0

    def call(first, enzymes, second=None, second_enzymes=None, success=True):
        nonlocal checked
        cmd = [
            str(binary),
            "compatible-ends",
            str(first),
            "--enzymes",
            enzymes,
            "--output",
            "json",
        ]
        if second is not None:
            cmd += ["--other", str(second), "--other-enzymes", second_enzymes]
        output = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        body = json.loads(output.stdout)
        validator.validate(body)
        require(
            (output.returncode == 0) == success
            and body["ok"] == success
            and not output.stderr,
            "unexpected response status",
        )
        if success:
            check_matrix(body)
        else:
            require(
                body["error"]["code"] == "compatibility_failed"
                and "result" not in body,
                "large matrices must fail explicitly",
            )
        checked += 1
        return body

    def fixture(path, sequence):
        def packet(kind, data):
            return struct.pack(">BI", kind, len(data)) + data

        path.write_bytes(
            packet(9, b"SnapGene\x00\x01\x00\x01\x00\x01")
            + packet(0, b"\x00" + sequence.encode())
        )

    with tempfile.TemporaryDirectory(prefix="dnagent-compatibility-") as directory:
        first, second = Path(directory) / "first.dna", Path(directory) / "second.dna"
        fixture(second, "AAACGTCTCAACGATTT")
        for letters in product("ACGT", repeat=4):
            oligo = "".join(letters)
            fixture(first, f"AAAGGTCTCA{oligo}TTT")
            body = call(first, "BsaI", second, "BsmBI")
            # Do not merely trust the digester's reported oligo in the matrix test.
            ends = body["result"]["analysis"]["endpoints"]
            exposed = sorted(
                e["end"]["overhang_sequence"]
                for e in ends
                if e["input"] == 1 and e["end"]["polarity"] == "five_prime"
            )
            require(
                exposed == sorted([oligo, str(Seq(oligo).reverse_complement())]),
                "Type IIS oligo extraction differs from designed input",
            )
        fixture(first, "AAAGAATTCTTT" * 65)
        call(first, "EcoRI", success=False)
    root = ROOT / "fixtures/formats/snapgene"
    call(root / "synthetic_restriction_linear.dna", "EcoRI,BamHI,EcoRV,KpnI,BsaI,BsmBI")
    call(root / "synthetic_restriction_circular.dna", "EcoRI")
    call(root / "synthetic_restriction_circular.dna", "BamHI")
    print(
        f"Compatibility reference/schema checks passed: {checked} cases, including all 256 Type IIS four-base oligos and bounded-output rejection."
    )


if __name__ == "__main__":
    main()

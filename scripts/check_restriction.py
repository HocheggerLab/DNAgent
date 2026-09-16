# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85", "jsonschema==4.23.0"]
# ///
"""Compare catalogue constants and nominal top-strand cuts with Biopython 1.85.

Synthetic inputs include both orientations and rotations across circular origins.
Optional private manifests stay outside Git; hashes are checked before reading.
This is an independent software comparison, not experimental validation.
"""

import argparse
import hashlib
import json
from pathlib import Path
import random
import struct
import subprocess
import tempfile

from Bio import Restriction, SeqIO
from Bio.Seq import Seq
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
NAMES = ["EcoRI", "BamHI", "EcoRV", "KpnI", "BsaI", "BsmBI"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    validator = Draft202012Validator(
        json.loads((ROOT / "schemas/cli-envelope-0.9.0.schema.json").read_text())
    )

    def call(*arguments):
        output = subprocess.run(
            [str(binary), *arguments, "--output", "json"],
            capture_output=True,
            text=True,
            timeout=30,
            check=True,
        )
        body = json.loads(output.stdout)
        validator.validate(body)
        if body["ok"] is not True:
            raise ValueError("unexpected failure")
        return body["result"]

    for item in call("enzymes"):
        enzyme = getattr(Restriction, item["name"])
        expected = {
            "name": str(enzyme),
            "recognition_sequence": enzyme.site,
            "top_cut_offset": enzyme.fst5,
            "bottom_cut_offset": len(enzyme.site) + enzyme.fst3,
        }
        if item != expected:
            raise ValueError(f"catalogue differs from Biopython: {item['name']}")

    def check(path, names=NAMES):
        reference = SeqIO.read(path, "snapgene")
        circular = reference.annotations["topology"] == "circular"
        result = call("sites", str(path), "--enzymes", ",".join(names))
        for name in names:
            enzyme = getattr(Restriction, name)
            expected = sorted(
                position - 1
                for position in enzyme.search(reference.seq, linear=not circular)
            )
            actual = sorted(
                site["top_cut"]
                for site in result["sites"]
                if site["enzyme"] == name and site["cleavage_available"]
            )
            if actual != expected:
                raise ValueError(
                    f"top-cut mismatch for {name} in {path.name}: DNAagent {actual}, Biopython {expected}"
                )

    def packet(kind, data):
        return struct.pack(">BI", kind, len(data)) + data

    count = 0
    rng = random.Random(1729)
    with tempfile.TemporaryDirectory(
        prefix="dnagent-restriction-reference-"
    ) as directory:
        path = Path(directory) / "synthetic.dna"
        for name in NAMES:
            motif = getattr(Restriction, name).site
            for oriented in [motif, str(Seq(motif).reverse_complement())]:
                # Ample flanks avoid differing end-accessibility policies in the two tools.
                sequence = (
                    "".join(rng.choices("ACGT", k=20))
                    + oriented
                    + "".join(rng.choices("ACGT", k=20))
                )
                for circular in [False, True]:
                    rotations = range(len(sequence)) if circular else [0]
                    for shift in rotations:
                        rotated = sequence[shift:] + sequence[:shift]
                        path.write_bytes(
                            packet(9, b"SnapGene\x00\x01\x00\x01\x00\x01")
                            + packet(0, bytes([int(circular)]) + rotated.encode())
                        )
                        check(path, [name])
                        count += 1
        for file in [
            "synthetic_restriction_linear.dna",
            "synthetic_restriction_circular.dna",
        ]:
            check(ROOT / "fixtures/formats/snapgene" / file)
            count += 1
    private_count = 0
    if args.manifest:
        manifest_path = args.manifest.resolve()
        manifest = json.loads(manifest_path.read_text())
        if manifest["schema_version"] != 1 or not manifest["records"]:
            raise ValueError("expected a non-empty schema_version 1 private manifest")
        for entry in manifest["records"]:
            path = (manifest_path.parent / entry["file"]).resolve()
            if not path.is_relative_to(manifest_path.parent):
                raise ValueError("fixture outside manifest directory")
            if hashlib.sha256(path.read_bytes()).hexdigest() != entry["sha256"]:
                raise ValueError("private fixture hash changed")
            check(path)
            private_count += 1
    print(
        f"Biopython comparison and schema validation passed: {count} synthetic scans, {private_count} private scans, six catalogue entries."
    )


if __name__ == "__main__":
    main()

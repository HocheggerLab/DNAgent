# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85", "jsonschema==4.23.0"]
# ///
"""Check digest strand products against Biopython, plus conservation/end invariants.

Synthetic substrate generation is deterministic; optional private inputs are
hash-checked and neither sequences nor reports are written into the repository.
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


def require(condition, message):
    if not condition:
        raise ValueError(message)


def reference_products(sequence, names, linear):
    if len(names) == 1:
        # Public Biopython digest operation, independently run on BOTH strands.
        return sorted(
            str(s)
            for s in getattr(Restriction, names[0]).catalyse(sequence, linear=linear)
        )
    cuts = sorted(
        {
            p - 1
            for positions in Restriction.RestrictionBatch(names)
            .search(sequence, linear=linear)
            .values()
            for p in positions
        }
    )
    text = str(sequence)
    if not cuts:
        return [text]
    if linear:
        bounds = [0, *cuts, len(text)]
        return sorted(text[a:b] for a, b in zip(bounds, bounds[1:]))
    return sorted(
        [text[a:b] for a, b in zip(cuts, cuts[1:])]
        + [text[cuts[-1] :] + text[: cuts[0]]]
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    validator = Draft202012Validator(
        json.loads((ROOT / "schemas/cli-envelope-0.10.0.schema.json").read_text())
    )

    def check(path, names, success=True):
        output = subprocess.run(
            [
                str(binary),
                "digest",
                str(path),
                "--enzymes",
                ",".join(names),
                "--output",
                "json",
            ],
            capture_output=True,
            text=True,
            timeout=30,
        )
        body = json.loads(output.stdout)
        validator.validate(body)
        require(
            (output.returncode == 0) == success
            and body["ok"] == success
            and not output.stderr,
            "unexpected digest status/diagnostics",
        )
        if not success:
            require(
                body["error"]["code"] == "digest_failed" and "result" not in body,
                "unsupported digest must not emit fragments",
            )
            return
        record = SeqIO.read(path, "snapgene")
        # Match DnaSeq's documented case canonicalisation; source file is unchanged.
        reference_sequence = record.seq.upper()
        sequence = str(reference_sequence)
        n = len(sequence)
        linear = record.annotations["topology"] == "linear"
        result = body["result"]
        fragments = result["fragments"]
        require(result["input_length"] == n, "input length changed")
        for strand, reference in [
            ("top", reference_sequence),
            ("bottom", reference_sequence.reverse_complement()),
        ]:
            actual = sorted(f[strand]["sequence_5to3"] for f in fragments)
            require(
                actual == reference_products(reference, names, linear),
                f"{strand} products differ from Biopython",
            )
            require(
                sum(f[strand]["length"] for f in fragments) == n,
                f"{strand} base conservation failed",
            )
            for fragment in fragments:
                part = fragment[strand]
                start, length = part["source_start"], part["length"]
                require(0 <= start < n and 0 < length <= n, "invalid strand interval")
                source = (sequence * 2)[start : start + length]
                if strand == "bottom":
                    source = str(Seq(source).reverse_complement())
                require(
                    source == part["sequence_5to3"] and len(source) == length,
                    "source-coordinate reconstruction failed",
                )
        for fragment in fragments:
            end_bases = sum(
                len(e["overhang_sequence"])
                for e in [fragment["left_end"], fragment["right_end"]]
                if e
            )
            require(
                2 * fragment["paired_length"] + end_bases
                == fragment["top"]["length"] + fragment["bottom"]["length"],
                "duplex core plus overhangs does not account for all bases",
            )
            for side in ["left_end", "right_end"]:
                end = fragment[side]
                if end is None or end["polarity"] == "blunt":
                    continue
                top = end["protruding_strand"] == "forward"
                oligo = fragment["top" if top else "bottom"]["sequence_5to3"]
                prefix = (side == "left_end") == top
                require(
                    oligo.startswith(end["overhang_sequence"])
                    if prefix
                    else oligo.endswith(end["overhang_sequence"]),
                    "overhang is not at the reported strand end",
                )
        pairs = list(zip(fragments, fragments[1:]))
        if not linear and result["cuts"]:
            pairs.append((fragments[-1], fragments[0]))
        for left, right in pairs:
            a, b = left["right_end"], right["left_end"]
            require(
                a["polarity"] == b["polarity"] and a["enzymes"] == b["enzymes"],
                "adjacent end provenance disagrees",
            )
            require(
                a["overhang_sequence"]
                == str(Seq(b["overhang_sequence"]).reverse_complement()),
                "adjacent overhangs are not complementary",
            )

    def write_fixture(path, sequence, circular):
        def packet(kind, data):
            return struct.pack(">BI", kind, len(data)) + data

        path.write_bytes(
            packet(9, b"SnapGene\x00\x01\x00\x01\x00\x01")
            + packet(0, bytes([int(circular)]) + sequence.encode())
        )

    count = 0
    rng = random.Random(314159)
    with tempfile.TemporaryDirectory(prefix="dnagent-digest-reference-") as directory:
        path = Path(directory) / "synthetic.dna"
        for name in NAMES:
            motif = getattr(Restriction, name).site
            for oriented in [motif, str(Seq(motif).reverse_complement())]:
                sequence = (
                    "".join(rng.choices("ACGT", k=20))
                    + oriented
                    + "".join(rng.choices("ACGT", k=20))
                )
                for circular in [False, True]:
                    for shift in range(len(sequence)) if circular else [0]:
                        write_fixture(
                            path, sequence[shift:] + sequence[:shift], circular
                        )
                        check(path, [name])
                        count += 1
        # Mixed polarities and Type IIS cuts on circles at every origin position.
        sequence = "AAAGAATTCAAAAAAGGTACCAAAAAAGGTCTCAACGATTT"
        for circular in [False, True]:
            for shift in range(len(sequence)) if circular else [0]:
                write_fixture(path, sequence[shift:] + sequence[:shift], circular)
                check(path, ["EcoRI", "KpnI", "BsaI"])
                count += 1
        for sequence, names, success in [
            ("GGTCTCAAAAAAGAGACG", ["BsaI", "BsmBI"], True),
            ("GGTCTCAAAAAGAGACC", ["BsaI"], False),
            ("GGTCTCAAAAAAAAAAGAGACC", ["BsaI"], False),
            ("GGTCTC", ["BsaI"], False),
            ("AAAAAGAGACCAAA", ["BsaI"], False),
            ("ACGTNACGT", ["EcoRI"], False),
            ("ACGTACGTACGT", ["EcoRI"], True),
        ]:
            write_fixture(path, sequence, False)
            check(path, names, success)
            count += 1
        for file in [
            "synthetic_restriction_linear.dna",
            "synthetic_restriction_circular.dna",
        ]:
            check(ROOT / "fixtures/formats/snapgene" / file, NAMES)
            count += 1
    private_count = 0
    if args.manifest:
        manifest_path = args.manifest.resolve()
        manifest = json.loads(manifest_path.read_text())
        require(
            manifest["schema_version"] == 1 and manifest["records"],
            "expected nonempty private manifest version 1",
        )
        for entry in manifest["records"]:
            path = (manifest_path.parent / entry["file"]).resolve()
            require(
                path.is_relative_to(manifest_path.parent),
                "fixture outside manifest directory",
            )
            require(
                hashlib.sha256(path.read_bytes()).hexdigest() == entry["sha256"],
                "private fixture hash changed",
            )
            check(path, ["EcoRI", "BamHI"])
            private_count += 1
    print(
        f"Digest reference/schema/invariant checks passed: {count} synthetic cases, {private_count} private EcoRI/BamHI digests."
    )


if __name__ == "__main__":
    main()

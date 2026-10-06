# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85", "jsonschema==4.23.0"]
# ///
"""Independent per-base annotation oracle; optional hash-checked private corpus.

Generated substrates are synthetic originals and remain in a temporary directory.
Checks every base association rather than repeating the Rust interval algorithm.
"""

import argparse
import hashlib
import io
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import xml.etree.ElementTree as ET
import warnings

from Bio import BiopythonParserWarning, SeqIO
from Bio.Seq import Seq
from jsonschema import Draft202012Validator

from check_private_corpus import packets

ROOT = Path(__file__).resolve().parents[1]
VALIDATOR = Draft202012Validator(
    json.loads((ROOT / "schemas/cli-envelope-0.10.0.schema.json").read_text())
)


def packet(kind, body):
    return bytes([kind]) + struct.pack(">I", len(body)) + body


def synthetic(path, seq, circular):
    n = len(seq)
    root = ET.Element("Features")
    for start in range(n):
        for length in range(1, n + 1 if circular else n - start + 1):
            for strand in [0, 1, 2]:
                feature = ET.SubElement(
                    root,
                    "Feature",
                    name=f"f{len(root)}",
                    type="misc_feature",
                    directionality=str(strand),
                )
                ET.SubElement(
                    feature,
                    "Segment",
                    range=f"{start + 1}-{(start + length - 1) % n + 1}",
                    type="standard",
                )
    path.write_bytes(
        packet(9, b"SnapGene" + struct.pack(">HHH", 1, 1, 1))
        + packet(0, bytes([int(circular)]) + seq.encode())
        + packet(10, ET.tostring(root))
    )


def region_positions(region, n):
    length = region.get("length", region.get("end", 0) - region["start"])
    return [(region["start"] + i) % n for i in range(length)]


def check(binary, path, enzymes):
    proc = subprocess.run(
        [str(binary), "fragments", str(path), "--enzymes", enzymes],
        capture_output=True,
        text=True,
        check=True,
        timeout=30,
    )
    envelope = json.loads(proc.stdout)
    # Keep potential private values out of schema exception messages.
    assert VALIDATOR.is_valid(envelope), "annotation schema failure"
    assert not proc.stderr
    result = envelope["result"]
    n = result["digest"]["input_length"]
    # Biopython is the sequence oracle only. Exclude annotation packets in this
    # in-memory view: 1.85 interprets a full-circle arc such as 2-1 as empty.
    # The unmodified source XML below remains our per-base annotation oracle.
    sequence_packets = b"".join(
        packet(kind, body)
        for kind, body in packets(path.read_bytes())
        if kind in (9, 0)
    )
    source = str(SeqIO.read(io.BytesIO(sequence_packets), "snapgene").seq).upper()
    assert len(source) == n
    features = [
        f
        for kind, body in packets(path.read_bytes())
        if kind == 10
        for f in ET.fromstring(body).findall("Feature")
    ]
    assert len(features) == len(result["source_features"])
    assert len(result["annotations"]) == len(result["digest"]["fragments"])
    checks = 0
    for fragment, annotations in zip(
        result["digest"]["fragments"], result["annotations"], strict=True
    ):
        assert annotations["fragment_id"] == fragment["id"]
        for side in ["top", "bottom"]:
            strand = fragment[side]
            positions = [
                (strand["source_start"] + i) % n for i in range(strand["length"])
            ]
            sequence = "".join(source[i] for i in positions)
            if side == "bottom":
                positions.reverse()
                sequence = str(Seq(sequence).reverse_complement())
            assert sequence == strand["sequence_5to3"]
            local = {position: i for i, position in enumerate(positions)}
            observed = {m["source_feature_id"]: m for m in annotations[side]}
            assert len(observed) == len(annotations[side])
            expected_ids = []
            for original, exported in zip(
                features, result["source_features"], strict=True
            ):
                expected = []
                total = 0
                split_parts = []
                for index, part in enumerate(original.findall("Segment")):
                    first, last = map(int, part.attrib["range"].split("-"))
                    bases = (
                        list(range(first - 1, last))
                        if first <= last
                        else list(range(first - 1, n)) + list(range(last))
                    )
                    total += len(bases)
                    retained = [
                        (offset, local[base])
                        for offset, base in enumerate(bases)
                        if base in local
                    ]
                    # A discontinuity in original offset OR local traversal splits
                    # one source part. Circular display seams are not physical cuts.
                    step = -1 if side == "bottom" else 1
                    if fragment["topology"] == "linear" and any(
                        b[0] != a[0] + 1 or b[1] != a[1] + step
                        for a, b in zip(retained, retained[1:])
                    ):
                        split_parts.append(index)
                    expected.extend(
                        (index, offset, local_index) for offset, local_index in retained
                    )
                if not expected:
                    assert exported["id"] not in observed
                    continue
                expected_ids.append(exported["id"])
                mapping = observed[exported["id"]]
                actual = []
                for part in mapping["parts"]:
                    coordinates = region_positions(
                        part["fragment_region"], strand["length"]
                    )
                    if side == "bottom":
                        coordinates.reverse()
                    source_positions = region_positions(part["source_region"], n)
                    assert source_positions == [positions[i] for i in coordinates]
                    actual.extend(
                        (
                            part["source_part"],
                            part["source_offset"] + offset,
                            coordinate,
                        )
                        for offset, coordinate in enumerate(coordinates)
                    )
                assert actual == expected, "annotation base-association mismatch"
                assert mapping["source_bases"] == total
                assert mapping["retained_bases"] == len(expected)
                assert mapping["complete"] == (len(expected) == total)
                assert mapping["split_source_parts"] == split_parts
                direction = original.get("directionality", "0")
                expected_strand = {"1": "forward", "2": "reverse"}.get(
                    direction, "unknown"
                )
                if side == "bottom":
                    expected_strand = {
                        "forward": "reverse",
                        "reverse": "forward",
                        "unknown": "unknown",
                    }[expected_strand]
                assert mapping["fragment_strand"] == expected_strand
                checks += 1
            assert list(observed) == expected_ids
    fasta = subprocess.run(
        [
            str(binary),
            "fragments",
            str(path),
            "--enzymes",
            enzymes,
            "--output",
            "fasta",
        ],
        capture_output=True,
        text=True,
        check=True,
        timeout=30,
    )
    records = list(SeqIO.parse(io.StringIO(fasta.stdout), "fasta"))
    expected_strands = [
        (f, side) for f in result["digest"]["fragments"] for side in ["top", "bottom"]
    ]
    assert len(records) == len(expected_strands)
    for record, (fragment, side) in zip(records, expected_strands, strict=True):
        assert record.id == f"{fragment['id']}|{side}"
        assert str(record.seq) == fragment[side]["sequence_5to3"]
    for side in ["top", "bottom"]:
        validate_genbank(binary, path, enzymes, result, side)
    return checks


def validate_genbank(binary, path, enzymes, result, side):
    proc = subprocess.run(
        [
            str(binary),
            "fragments",
            str(path),
            "--enzymes",
            enzymes,
            "--output",
            "genbank",
            "--strand",
            side,
        ],
        capture_output=True,
        text=True,
        check=True,
        timeout=30,
    )
    assert "not a duplex product" in proc.stderr
    with warnings.catch_warnings():
        warnings.simplefilter("error", BiopythonParserWarning)
        records = list(SeqIO.parse(io.StringIO(proc.stdout), "genbank"))
    assert len(records) == len(result["digest"]["fragments"])
    sources = {f["id"]: i for i, f in enumerate(result["source_features"])}
    for record, fragment, annotations in zip(
        records, result["digest"]["fragments"], result["annotations"], strict=True
    ):
        assert str(record.seq) == fragment[side]["sequence_5to3"]
        assert record.annotations["topology"] == fragment["topology"]
        assert record.annotations["molecule_type"] == "DNA"
        assert "NOT a duplex product" in record.annotations["comment"]
        expected = [
            (mapping, part)
            for mapping in annotations[side]
            for part in mapping["parts"]
        ]
        assert len(record.features) == len(expected)
        for feature, (mapping, part) in zip(record.features, expected, strict=True):
            assert feature.type == "misc_feature"
            region = part["fragment_region"]
            assert int(feature.location.start) == region["start"]
            assert int(feature.location.end) == region["end"]
            assert feature.location.strand == (
                -1 if mapping["fragment_strand"] == "reverse" else 1
            )
            notes = feature.qualifiers["note"]
            assert (
                f"source_feature_index={sources[mapping['source_feature_id']]}" in notes
            )
            assert (
                f"source_part={part['source_part']} source_offset={part['source_offset']}"
                in notes
            )
            assert ("source part split across linear fragment ends" in notes) == (
                part["source_part"] in mapping["split_source_parts"]
            )
            assert ("source strand unknown; no orientation inferred" in notes) == (
                mapping["fragment_strand"] == "unknown"
            )
            assert not any(
                key in feature.qualifiers
                for key in ["translation", "codon_start", "protein_id"]
            )
            extracted = record.seq[region["start"] : region["end"]]
            if mapping["fragment_strand"] == "reverse":
                extracted = extracted.reverse_complement()
            assert feature.extract(record.seq) == extracted


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    cases = mappings = private = 0
    with tempfile.TemporaryDirectory(prefix="dnagent-annotations-") as directory:
        path = Path(directory) / "synthetic.dna"
        for seq, enzyme in [
            ("AAAGAATTCTTT", "EcoRI"),
            ("AAAGGTACCTTT", "KpnI"),
            ("AAAGATATCTTT", "EcoRV"),
        ]:
            for rotation in range(len(seq)):
                synthetic(path, seq[rotation:] + seq[:rotation], True)
                mappings += check(binary, path, enzyme)
                cases += 1
            synthetic(path, seq, False)
            mappings += check(binary, path, enzyme)
            cases += 1
        synthetic(path, "AAAGAATTCTTT", True)
        mappings += check(binary, path, "BamHI")
        cases += 1
        mappings += check(
            binary,
            ROOT / "fixtures/formats/snapgene/synthetic_multipart_origin.dna",
            "EcoRI",
        )
        cases += 1
    if args.manifest:
        manifest = args.manifest.resolve()
        entries = json.loads(manifest.read_text())
        assert entries["schema_version"] == 1 and entries["records"]
        for entry in entries["records"]:
            path = (manifest.parent / entry["file"]).resolve()
            assert path.is_relative_to(manifest.parent)
            before = hashlib.sha256(path.read_bytes()).hexdigest()
            assert before == entry["sha256"]
            mappings += check(binary, path, "EcoRI,BamHI")
            assert hashlib.sha256(path.read_bytes()).hexdigest() == before
            private += 1
    print(
        f"Annotation base-association/schema/FASTA/GenBank checks passed: {cases} synthetic and {private} private cases; {mappings} strand-feature projections."
    )


if __name__ == "__main__":
    main()

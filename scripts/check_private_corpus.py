# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85"]
# ///
"""Opt-in read-only CLI checks; lab files and reports must stay outside Git.

Sequence/topology oracle: Biopython 1.85. Annotation oracle: source XML,
using explicit one-based-inclusive to zero-based-half-open conversion.
This is NOT a complete SnapGene fidelity or biological-validity check.
"""

import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import xml.etree.ElementTree as ET

from Bio import SeqIO


def packets(data):
    offset = 0
    while offset < len(data):
        if len(data) - offset < 5:
            raise ValueError("truncated packet header")
        kind, size = struct.unpack_from(">BI", data, offset)
        end = offset + 5 + size
        if end > len(data):
            raise ValueError("truncated packet payload")
        yield kind, data[offset + 5 : end]
        offset = end


def cli_json(binary, command, path, *extra, expected_warnings=None):
    proc = subprocess.run(
        [str(binary), command, str(path), *extra, "--output", "json"],
        capture_output=True,
        text=True,
        timeout=30,
        check=True,
    )
    envelope = json.loads(proc.stdout)
    if envelope.get("ok") is not True or envelope.get("command") != command:
        raise ValueError("unexpected CLI envelope")
    if envelope.get("schema_version") != "0.3.0" or not isinstance(
        envelope.get("warnings"), list
    ):
        raise ValueError("expected schema 0.3.0 with top-level warnings")
    if command == "inspect":
        expected_warnings = envelope["result"]["warnings"]
    if expected_warnings is not None and envelope["warnings"] != expected_warnings:
        raise ValueError(f"{command}: inconsistent import warnings")
    return envelope["result"]


def check_record(binary, path, entry):
    data = path.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != entry["sha256"]:
        raise ValueError(
            "source hash differs from manifest; do not silently rebaseline"
        )
    reference = SeqIO.read(path, "snapgene")
    source_packets = list(packets(data))
    features = [
        f
        for k, p in source_packets
        if k == 10
        for f in ET.fromstring(p).findall("Feature")
    ]
    primers = [
        p
        for k, body in source_packets
        if k == 5
        for p in ET.fromstring(body).findall("Primer")
    ]
    inspect = cli_json(binary, "inspect", path)
    actual = cli_json(binary, "features", path, expected_warnings=inspect["warnings"])
    sequence = cli_json(
        binary,
        "sequence",
        path,
        "--range",
        f"0..{len(reference)}",
        expected_warnings=inspect["warnings"],
    )
    failures = []

    def compare(field, observed, expected):
        if observed != expected:
            failures.append(field)

    compare("length", inspect["length"], len(reference))
    compare("topology", inspect["topology"], reference.annotations["topology"])
    compare(
        "sequence",
        sequence,
        {"start": 0, "end": len(reference), "sequence": str(reference.seq).upper()},
    )
    compare("feature_count", inspect["feature_count"], len(features))
    compare("feature_rows", len(actual), len(features))
    compare("primer_count", inspect["primer_count"], len(primers))
    compare(
        "primers",
        cli_json(binary, "primers", path, expected_warnings=inspect["warnings"]),
        [
            {
                "name": p.get("name", ""),
                "sequence": p.attrib["sequence"].upper(),
                "description": p.get("description") or None,
            }
            for p in primers
        ],
    )
    coverage = {"multipart": 0, "origin_spanning": 0, "reverse": 0}
    for i, (source, observed) in enumerate(zip(features, actual, strict=False)):
        strand = {"1": "forward", "2": "reverse"}.get(
            source.get("directionality"), "unknown"
        )
        parts = []
        segments = source.findall("Segment")
        for segment in segments:
            start, end = map(int, segment.attrib["range"].split("-"))
            if not (1 <= start <= len(reference) and 1 <= end <= len(reference)):
                raise ValueError(
                    "reference contains out-of-bounds feature; requires manual review"
                )
            if start <= end:
                parts.append({"kind": "linear", "start": start - 1, "end": end})
            else:
                if reference.annotations["topology"] != "circular":
                    raise ValueError("wrapping feature on linear reference")
                parts.append(
                    {
                        "kind": "circular_arc",
                        "start": start - 1,
                        "length": len(reference) - start + 1 + end,
                    }
                )
                coverage["origin_spanning"] += 1
        coverage["multipart"] += len(parts) > 1
        coverage["reverse"] += strand == "reverse"
        qualifiers = []
        for qualifier in source.findall("Q"):
            values = qualifier.findall("V")
            if not values:
                qualifiers.append({"key": qualifier.attrib["name"], "value": None})
            for value in values:
                retained = next(
                    (
                        value.get(key)
                        for key in ("text", "predef", "int")
                        if value.get(key) is not None
                    ),
                    None,
                )
                qualifiers.append({"key": qualifier.attrib["name"], "value": retained})
        expected = {
            "qualifiers": qualifiers,
            "id": f"feature-{i + 1:04d}",
            "label": source.get("name", ""),
            "kind": source.get("type", "misc_feature"),
            "strand": strand,
            "location": {
                "parts": parts,
                "strand": strand,
                "operator": "join" if len(parts) > 1 else "contiguous",
            },
            "color": next(
                (s.get("color") for s in segments if s.get("color") is not None), None
            ),
        }
        compare(f"feature[{i}]", observed, expected)
    expected_warnings = sorted(
        (
            "snapgene_notes_not_interpreted"
            if k == 6
            else "snapgene_packet_not_interpreted",
            k,
        )
        for k, _ in source_packets
        if k not in {9, 0, 5, 10}
    )
    compare(
        "warning_codes_and_packet_types",
        sorted((w["code"], w["packet_type"]) for w in inspect["warnings"]),
        expected_warnings,
    )
    return {
        "file": entry["file"],
        "sha256": digest,
        "length": len(reference),
        "topology": reference.annotations["topology"],
        "features": len(features),
        "primers": len(primers),
        "coverage": coverage,
        "warning_count": len(inspect["warnings"]),
        "failures": failures,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path, help="external private manifest.json")
    parser.add_argument("--binary", required=True, type=Path)
    args = parser.parse_args()
    manifest_path = args.manifest.resolve()
    manifest = json.loads(manifest_path.read_text())
    if manifest["schema_version"] != 1 or not manifest["records"]:
        parser.error("expected non-empty schema_version 1 manifest")
    results = []
    binary = args.binary.resolve()
    for entry in manifest["records"]:
        path = (manifest_path.parent / entry["file"]).resolve()
        if not path.is_relative_to(manifest_path.parent):
            parser.error("fixture paths must remain inside the manifest directory")
        try:
            results.append(check_record(binary, path, entry))
        except (
            ValueError,
            KeyError,
            OSError,
            ET.ParseError,
            subprocess.SubprocessError,
        ) as error:
            results.append({"file": entry["file"], "failures": [str(error)]})
    report = {
        "reference": "Biopython 1.85 sequence/topology; source XML annotations",
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "limitations": [
            "Primer sequences and qualifiers checked against source XML, not experimental evidence",
            "No GUI, biological validation or complete format-fidelity check",
            "Warning consistency checked across inspect/features/primers/sequence; map checked by public Rust tests",
        ],
        "records": results,
    }
    print(json.dumps(report, indent=2))
    return int(any(row["failures"] for row in results))


if __name__ == "__main__":
    raise SystemExit(main())

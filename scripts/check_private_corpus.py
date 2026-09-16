# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85", "jsonschema==4.23.0"]
# ///
"""Opt-in read-only CLI checks; lab files and reports must stay outside Git.

Sequence/topology oracle: Biopython 1.85. Annotation oracle: source XML,
using explicit one-based-inclusive to zero-based-half-open conversion.
This is NOT a complete SnapGene fidelity or biological-validity check.
"""

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import xml.etree.ElementTree as ET

from Bio import SeqIO
from jsonschema import Draft202012Validator

SCHEMA_PATH = (
    Path(__file__).resolve().parents[1] / "schemas/cli-envelope-0.6.0.schema.json"
)
VALIDATOR = Draft202012Validator(json.loads(SCHEMA_PATH.read_text()))


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


def cli_envelope(binary, command, path, *extra, success=True):
    proc = subprocess.run(
        [str(binary), command, str(path), *extra, "--output", "json"],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    envelope = json.loads(proc.stdout)
    # Do not include instance values in failures: they may contain private sequences.
    error = next(VALIDATOR.iter_errors(envelope), None)
    if error is not None:
        location = "/".join(map(str, error.absolute_path))
        raise ValueError(f"CLI schema violation at {location}")
    if (
        (proc.returncode == 0) != success
        or envelope["ok"] != success
        or envelope["command"] != command
        or proc.stderr
    ):
        raise ValueError("unexpected CLI status, envelope or stderr diagnostics")
    return envelope


def cli_json(binary, command, path, *extra, expected_warnings=None):
    envelope = cli_envelope(binary, command, path, *extra)
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

    # Direct sequence-packet oracle in addition to Biopython. Uppercase is the
    # documented domain intake convention; neither source nor copy is modified.
    dna_packets = [body for kind, body in source_packets if kind == 0]
    if len(dna_packets) != 1 or len(dna_packets[0]) < 2:
        raise ValueError("expected exactly one nonempty DNA sequence packet")
    raw_sequence = dna_packets[0][1:].decode("ascii").upper()
    raw_topology = "circular" if dna_packets[0][0] & 1 else "linear"
    compare(
        "raw_packet_vs_biopython_sequence", str(reference.seq).upper(), raw_sequence
    )
    compare("raw_packet_vs_cli_sequence", sequence["sequence"], raw_sequence)
    compare("raw_packet_topology", inspect["topology"], raw_topology)
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
    # Check half-open extraction at termini, midpoint and annotation boundaries.
    # Full sequence equality above is stronger for content, but cannot catch a
    # broken range API (off-by-one, ignored bounds, or mishandled empty intervals).
    positions = {0, len(reference) // 2, len(reference) - 1, len(reference)}
    for feature in features[:8]:
        for segment in feature.findall("Segment"):
            start, end = map(int, segment.attrib["range"].split("-"))
            positions.update([start - 1, end])
    for start in sorted(positions):
        end = min(start + 17, len(reference))
        observed = cli_json(
            binary,
            "sequence",
            path,
            "--range",
            f"{start}..{end}",
            expected_warnings=inspect["warnings"],
        )
        compare(
            f"range[{start}:{end}]",
            observed,
            {"start": start, "end": end, "sequence": raw_sequence[start:end]},
        )
    strict = cli_envelope(
        binary, "inspect", path, "--strict", success=not inspect["warnings"]
    )
    compare("strict_warnings", strict["warnings"], inspect["warnings"])
    if inspect["warnings"]:
        compare("strict_error", strict["error"]["code"], "import_warnings")
    else:
        compare("strict_result", strict["result"], inspect)
    compare("file_unchanged", hashlib.sha256(path.read_bytes()).hexdigest(), digest)
    return {
        "file": entry["file"],
        "sha256": digest,
        "length": len(reference),
        "topology": reference.annotations["topology"],
        "features": len(features),
        "primers": len(primers),
        "coverage": coverage,
        "range_probes": len(positions),
        "packet_counts": dict(sorted(Counter(k for k, _ in source_packets).items())),
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
        "report_schema_version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "checker_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "cli_schema_sha256": hashlib.sha256(SCHEMA_PATH.read_bytes()).hexdigest(),
        "reference": "Raw DNA packet and Biopython 1.85 sequence/topology; source XML annotations",
        "records_checked": len(results),
        "records_passed": sum(not row["failures"] for row in results),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "limitations": [
            "Primer sequences and qualifiers checked against source XML, not experimental evidence",
            "No native SnapGene comparison, GUI, biological validation or complete format-fidelity check",
            "Corpus coverage is not universal: linear private constructs and more format variants require additional authorised inputs",
            "Warning consistency checked across inspect/features/primers/sequence; map checked by public Rust tests",
        ],
        "records": results,
    }
    print(json.dumps(report, indent=2))
    return int(any(row["failures"] for row in results))


if __name__ == "__main__":
    raise SystemExit(main())

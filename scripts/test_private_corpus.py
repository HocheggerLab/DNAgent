# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85", "jsonschema==4.23.0"]
# ///
"""Exercise the private-corpus checker with public fixtures and injected failures.

Run after cargo build -p dnagent-cli. No private data is needed. Override the
binary with DNAGENT_BINARY if not using target/debug/dnagent.
"""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import check_private_corpus as checker

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("DNAGENT_BINARY", ROOT / "target/debug/dnagent")).resolve()
FIXTURES = ROOT / "fixtures/formats/snapgene"


def entry(path):
    return {"file": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


class CorpusCheckerTests(unittest.TestCase):
    def test_public_controls_match_independent_sources(self):
        for name in [
            "synthetic_linear.dna",
            "synthetic_unannotated.dna",
            "synthetic_multipart_origin.dna",
        ]:
            with self.subTest(name=name):
                path = FIXTURES / name
                result = checker.check_record(BINARY, path, entry(path))
                self.assertEqual(result["failures"], [])
                self.assertGreaterEqual(result["range_probes"], 3)

    def test_changed_file_is_rejected_before_cli(self):
        path = FIXTURES / "synthetic_unannotated.dna"
        with patch.object(checker, "cli_json") as cli:
            with self.assertRaisesRegex(ValueError, "source hash differs"):
                checker.check_record(
                    BINARY, path, {"file": path.name, "sha256": "0" * 64}
                )
            cli.assert_not_called()

    def test_sequence_and_annotation_faults_are_detected(self):
        path = FIXTURES / "synthetic_multipart_origin.dna"
        original = checker.cli_json
        for fault, expected in [
            ("sequence", "raw_packet_vs_cli_sequence"),
            ("feature", "feature[0]"),
            ("range", "range["),
        ]:

            def altered(binary, command, path, *args, **kwargs):
                result = original(binary, command, path, *args, **kwargs)
                if fault == "feature" and command == "features":
                    result[0]["strand"] = "unknown"
                if command == "sequence" and result["sequence"]:
                    full = result["start"] == 0 and result["end"] == 12
                    if (fault == "sequence" and full) or (
                        fault == "range" and result["start"] > 0
                    ):
                        base = "C" if result["sequence"][0] == "A" else "A"
                        result["sequence"] = base + result["sequence"][1:]
                return result

            with self.subTest(fault=fault), patch.object(
                checker, "cli_json", side_effect=altered
            ):
                result = checker.check_record(BINARY, path, entry(path))
                self.assertTrue(
                    any(f.startswith(expected) for f in result["failures"]),
                    result["failures"],
                )

    def test_schema_failure_does_not_echo_private_payload(self):
        envelope = {
            "schema_version": "0.6.0",
            "command": "sequence",
            "ok": True,
            "warnings": [],
            "result": {"start": 0, "end": 3, "sequence": "PRIVATE_PAYLOAD"},
        }
        proc = subprocess.CompletedProcess([], 0, json.dumps(envelope), "")
        with patch.object(checker.subprocess, "run", return_value=proc):
            with self.assertRaisesRegex(ValueError, "CLI schema violation") as error:
                checker.cli_json(BINARY, "sequence", Path("synthetic.dna"))
            self.assertNotIn("PRIVATE_PAYLOAD", str(error.exception))

    def test_path_escape_rejected_and_failed_report_is_nonzero(self):
        with tempfile.TemporaryDirectory(prefix="dnagent-corpus-test-") as directory:
            root = Path(directory)
            path = root / "synthetic.dna"
            path.write_bytes((FIXTURES / "synthetic_unannotated.dna").read_bytes())
            manifest = root / "manifest.json"
            for file, sha, status in [
                ("../escape.dna", "0" * 64, 2),
                (path.name, "0" * 64, 1),
            ]:
                manifest.write_text(
                    json.dumps(
                        {
                            "schema_version": 1,
                            "records": [{"file": file, "sha256": sha}],
                        }
                    )
                )
                proc = subprocess.run(
                    [
                        sys.executable,
                        str(ROOT / "scripts/check_private_corpus.py"),
                        str(manifest),
                        "--binary",
                        str(BINARY),
                    ],
                    capture_output=True,
                    text=True,
                    timeout=30,
                )
                self.assertEqual(proc.returncode, status)
                if status == 1:
                    body = json.loads(proc.stdout)
                    self.assertEqual(body["records_passed"], 0)
                    self.assertEqual(body["records_checked"], 1)
                    self.assertTrue(body["records"][0]["failures"])

    def test_truncated_source_packets_rejected(self):
        for data in [b"\x00", b"\x00\x00\x00\x00\x05AA"]:
            with self.assertRaisesRegex(ValueError, "truncated"):
                list(checker.packets(data))


if __name__ == "__main__":
    unittest.main()

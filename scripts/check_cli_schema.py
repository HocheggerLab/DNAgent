# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Validate actual CLI responses against the committed schema; public fixtures only."""

import argparse
import copy
import json
from pathlib import Path
import subprocess
import tempfile

from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    schema = json.loads((ROOT / "schemas/cli-envelope-0.3.0.schema.json").read_text())
    Draft202012Validator.check_schema(schema)
    validator = Draft202012Validator(schema)
    binary = args.binary.resolve()
    checked = []
    with tempfile.TemporaryDirectory(prefix="dnagent-schema-") as directory:
        for command in ["inspect", "features", "primers", "sequence", "map"]:
            for fixture, strict, success in [
                ("synthetic_linear.dna", False, True),
                ("synthetic_unannotated.dna", True, True),
                ("synthetic_multipart_origin.dna", False, True),
                ("synthetic_partial.dna", False, True),
                ("synthetic_partial.dna", True, False),
                ("invalid_truncated.dna", False, False),
            ]:
                cmd = [
                    str(binary),
                    command,
                    str(ROOT / "fixtures/formats/snapgene" / fixture),
                ]
                cmd += (
                    ["--out", str(Path(directory) / "map.svg")]
                    if command == "map"
                    else ["--output", "json"]
                )
                if strict:
                    cmd.append("--strict")
                proc = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
                body = json.loads(proc.stdout)
                validator.validate(body)
                if (
                    (proc.returncode == 0) != success
                    or body["ok"] != success
                    or proc.stderr
                ):
                    raise ValueError(
                        f"unexpected status or diagnostics: {command}, {fixture}"
                    )
                checked.append(body)
    for fixture, enzyme, strict, success in [
        ("synthetic_restriction_linear.dna", "EcoRI,BsaI", False, True),
        ("synthetic_restriction_circular.dna", "EcoRI", True, True),
        ("synthetic_restriction_end.dna", "BsaI", False, True),
        ("synthetic_restriction_end.dna", "BsaI", True, False),
        ("synthetic_linear.dna", "EcoRI", False, False),
        ("synthetic_restriction_linear.dna", "unknown", False, False),
    ]:
        cmd = [
            str(binary),
            "sites",
            str(ROOT / "fixtures/formats/snapgene" / fixture),
            "--enzymes",
            enzyme,
            "--output",
            "json",
        ]
        if strict:
            cmd.append("--strict")
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        body = json.loads(proc.stdout)
        validator.validate(body)
        if (proc.returncode == 0) != success or body["ok"] != success or proc.stderr:
            raise ValueError("unexpected restriction response")
        checked.append(body)
    proc = subprocess.run(
        [str(binary), "enzymes", "--output", "json"],
        capture_output=True,
        text=True,
        timeout=30,
        check=True,
    )
    body = json.loads(proc.stdout)
    validator.validate(body)
    checked.append(body)
    for path in (ROOT / "fixtures/formats/snapgene").glob("*.json"):
        validator.validate(json.loads(path.read_text()))
    # Guard against an accidentally permissive schema, not just bad CLI output.
    bad = copy.deepcopy(checked[0])
    del bad["warnings"]
    mutations = [bad]
    bad = copy.deepcopy(checked[0])
    bad["result"]["length"] = -1
    mutations.append(bad)
    bad = copy.deepcopy(checked[0])
    bad["error"] = {"code": "command_failed", "message": "mixed"}
    mutations.append(bad)
    feature = next(
        b for b in checked if b["command"] == "features" and b["ok"] and b["result"]
    )
    bad = copy.deepcopy(feature)
    del bad["result"][0]["qualifiers"]
    mutations.append(bad)
    bad = copy.deepcopy(feature)
    bad["result"][0]["location"]["parts"][0]["start"] = -1
    mutations.append(bad)
    primer = next(
        b for b in checked if b["command"] == "primers" and b["ok"] and b["result"]
    )
    bad = copy.deepcopy(primer)
    bad["result"][0]["sequence"] = "AC!T"
    mutations.append(bad)
    restriction = next(
        b
        for b in checked
        if b["command"] == "sites" and b["ok"] and b["result"]["sites"]
    )
    bad = copy.deepcopy(restriction)
    bad["result"]["sites"][0]["top_cut"] = -1
    mutations.append(bad)
    if any(validator.is_valid(body) for body in mutations):
        raise ValueError("schema accepted an intentionally invalid response")
    print(
        f"Validated {len(checked)} live responses, committed JSON contracts and {len(mutations)} rejection checks."
    )


if __name__ == "__main__":
    main()

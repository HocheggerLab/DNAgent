#!/usr/bin/env python3
"""Smoke-check the gibson-cloning skill on the app's public cDNA-into-pUC19 example.

No external dependencies; public fixtures only; not an agent or biological evaluation.
Usage: python3 scripts/check_skill.py --repo CHECKOUT --binary EXECUTABLE
"""

import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile

SKILL = Path(__file__).resolve().parents[1]


def run(binary: Path, *args: str) -> dict:
    process = subprocess.run([str(binary), *args], capture_output=True, text=True, timeout=300)
    body = json.loads(process.stdout)
    assert process.returncode == 0 and body["ok"], (args[0], body.get("error"))
    assert body["schema_version"] == "0.10.0", "read the new contract before updating the skill"
    return body


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    repo, binary = args.repo.resolve(), args.binary.resolve()
    text = (SKILL / "SKILL.md").read_text()
    header = text.split("---", 2)[1]
    assert re.search(r"^name: gibson-cloning$", header, re.M)
    assert 0 < len(re.search(r"^description: (.+)$", header, re.M).group(1)) <= 1024
    plans = repo / "fixtures/plans"
    with tempfile.TemporaryDirectory(prefix="gibson-cloning-smoke-") as directory:
        for command, plan, extra in [
            ("gibson-optimise", "synthetic-cdna-into-puc19-optimised.json", []),
            ("gibson", "synthetic-cdna-into-puc19.json", ["--output", "json"]),
        ]:
            product = Path(directory) / f"{command}.gb"
            body = run(binary, command, str(plans / plan), "--out", str(product), "--name", "pUC19-ORF", *extra)
            saved = body["result"]["product_genbank"]
            assert saved["length"] == 3229 and saved["primers"] == 4 and saved["features_left_out"] == 1, saved
            assert [w["code"] for w in body["warnings"] if w["code"].startswith("gibson_")] == ["gibson_feature_clipped"]
            features = run(binary, "features", str(product), "--output", "json")["result"]
            labels = [f["label"] for f in features]
            for expected in ["synthetic ORF", "Gibson F1", "Gibson R1", "Gibson F2", "Gibson R2",
                             "Gibson overlap 1-2", "Gibson overlap 2-1"]:
                assert expected in labels, (expected, labels)
            orf = next(f for f in features if f["label"] == "synthetic ORF")
            translated = run(binary, "translate", str(product), "--feature", orf["id"], "--output", "json")["result"]
            assert translated["protein"].startswith("M") and translated["protein"].count("*") <= 1
            assert run(binary, "inspect", str(product), "--output", "json")["result"]["name"] == "pUC19-ORF"
    print("gibson-cloning smoke check passed: both primer methods write the annotated 3,229 bp product")


if __name__ == "__main__":
    main()

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
    schema = json.loads((ROOT / "schemas/cli-envelope-0.9.0.schema.json").read_text())
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
    for fixture, enzyme, strict, success in [
        (
            "synthetic_restriction_linear.dna",
            "EcoRI,BamHI,EcoRV,KpnI,BsaI,BsmBI",
            True,
            True,
        ),
        ("synthetic_restriction_circular.dna", "EcoRI", True, True),
        ("synthetic_restriction_circular.dna", "BamHI", True, True),
        ("synthetic_restriction_end.dna", "BsaI", False, False),
        ("synthetic_linear.dna", "EcoRI", False, False),
        ("synthetic_partial.dna", "EcoRI", False, True),
        ("synthetic_partial.dna", "EcoRI", True, False),
    ]:
        cmd = [
            str(binary),
            "digest",
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
            raise ValueError("unexpected digest response")
        checked.append(body)
    for fixture, enzymes, success in [
        ("synthetic_restriction_linear.dna", "EcoRI,BsaI", True),
        ("synthetic_restriction_circular.dna", "EcoRI", True),
        ("synthetic_restriction_circular.dna", "BamHI", True),
        ("synthetic_restriction_end.dna", "BsaI", False),
        ("synthetic_partial.dna", "EcoRI", True),
    ]:
        proc = subprocess.run(
            [
                str(binary),
                "compatible-ends",
                str(ROOT / "fixtures/formats/snapgene" / fixture),
                "--enzymes",
                enzymes,
                "--output",
                "json",
            ],
            capture_output=True,
            text=True,
            timeout=30,
        )
        body = json.loads(proc.stdout)
        validator.validate(body)
        if (proc.returncode == 0) != success or body["ok"] != success or proc.stderr:
            raise ValueError("unexpected compatibility response")
        checked.append(body)
    for fixture, enzymes, success, strict in [
        ("synthetic_multipart_origin.dna", "EcoRI", True, False),
        ("synthetic_restriction_linear.dna", "EcoRI,BamHI", True, False),
        ("synthetic_restriction_end.dna", "BsaI", False, False),
        ("synthetic_circular.dna", "EcoRI", False, True),
        ("synthetic_partial.dna", "EcoRI", True, False),
    ]:
        proc = subprocess.run(
            [
                str(binary),
                "fragments",
                str(ROOT / "fixtures/formats/snapgene" / fixture),
                "--enzymes",
                enzymes,
                *(["--strict"] if strict else []),
            ],
            capture_output=True,
            text=True,
            timeout=30,
        )
        body = json.loads(proc.stdout)
        validator.validate(body)
        if (proc.returncode == 0) != success or body["ok"] != success or proc.stderr:
            raise ValueError("unexpected fragments response")
        checked.append(body)
    translation_fixture = str(ROOT / "fixtures/formats/snapgene/synthetic_translation.dna")
    for args, success in [
        (["translate", translation_fixture, "--feature", "feature-0001"], True),
        (["translate", translation_fixture, "--feature", "feature-0002"], True),
        (["translate", translation_fixture, "--range", "552..12"], True),
        (["translate", translation_fixture, "--range", "0..30", "--strand", "reverse", "--frame", "2"], True),
        (["translate", translation_fixture, "--all-cds"], True),
        (["translate", translation_fixture, "--all-cds", "--strict"], False),
        (["translate", translation_fixture, "--feature", "missing"], False),
        (["translate", str(ROOT / "fixtures/formats/snapgene/synthetic_linear.dna"), "--all-cds"], True),
        (["orfs", translation_fixture, "--min-codons", "5"], True),
        (["orfs", translation_fixture, "--starts", "table", "--table", "11", "--min-codons", "3"], True),
        (["orfs", str(ROOT / "fixtures/formats/snapgene/pUC19_M77789.dna")], True),
        (["orfs", translation_fixture, "--min-codons", "0"], False),
    ]:
        proc = subprocess.run(
            [str(binary), *args, "--output", "json"], capture_output=True, text=True, timeout=30
        )
        body = json.loads(proc.stdout)
        validator.validate(body)
        if (proc.returncode == 0) != success or body["ok"] != success or proc.stderr:
            raise ValueError(f"unexpected response for {args}")
        checked.append(body)
    with tempfile.TemporaryDirectory(prefix="dnagent-schema-edit-") as scratch:
        saved = str(Path(scratch) / "saved.gb")
        for args, success in [
            (["convert", translation_fixture, "--out", saved], True),
            (["annotate", saved, "--out", saved, "--add", "--range", "32..278", "--label", "orf", "--translate"], True),
            (["annotate", saved, "--out", saved, "--add", "--range", "550..10", "--label", "wrap", "--strand", "unknown"], True),
            (["annotate", saved, "--out", saved, "--remove", "feature-0008"], True),
            (["annotate", saved, "--out", saved, "--remove", "missing"], False),
            (["convert", translation_fixture, "--out", str(Path(scratch) / "bad.dna")], False),
            (["inspect", saved, "--output", "json"], True),
            (["features", str(ROOT / "fixtures/formats/genbank/pUC19_M77789.gb"), "--output", "json"], True),
            (["enzyme-catalogue"], True),
        ]:
            proc = subprocess.run([str(binary), *args], capture_output=True, text=True, timeout=30)
            body = json.loads(proc.stdout)
            validator.validate(body)
            if (proc.returncode == 0) != success or body["ok"] != success or proc.stderr:
                raise ValueError(f"unexpected response for {args}")
            checked.append(body)
        # Gibson product GenBank (--out): written only on success; strict refusal writes nothing.
        cdna_plan = str(ROOT / "fixtures/plans/synthetic-cdna-into-puc19.json")
        product = Path(scratch) / "product.gb"
        for args, success, written in [
            (["gibson", cdna_plan, "--out", str(product), "--output", "json"], True, True),
            (["gibson-optimise", str(ROOT / "fixtures/plans/synthetic-gibson-optimisation.json"), "--out", str(Path(scratch) / "optimised.gb")], True, True),
            (["gibson", cdna_plan, "--out", str(Path(scratch) / "strict.gb"), "--strict", "--output", "json"], False, False),
            (["gibson", cdna_plan, "--out", str(Path(scratch) / "bad.dna"), "--output", "json"], False, False),
        ]:
            target = Path(args[args.index("--out") + 1])
            proc = subprocess.run([str(binary), *args], capture_output=True, text=True, timeout=60)
            body = json.loads(proc.stdout)
            validator.validate(body)
            if (proc.returncode == 0) != success or body["ok"] != success or target.exists() != written:
                raise ValueError(f"unexpected Gibson product response for {args}")
            checked.append(body)
        checked.append(json.loads(subprocess.run([str(binary), "features", str(product), "--output", "json"], capture_output=True, text=True).stdout))
        validator.validate(checked[-1])
        # Feature library: built from the public fixtures into a scratch database.
        db = ["--db", str(Path(scratch) / "features.sqlite")]
        missing = ["--db", str(Path(scratch) / "missing.sqlite")]
        puc19 = str(ROOT / "fixtures/formats/snapgene/pUC19_M77789.dna")
        for args, success in [
            (["library", *missing, "info", "--output", "json"], False),
            (["detect-features", puc19, *missing, "--output", "json"], False),
            (["library", *db, "import", str(ROOT / "fixtures/formats"), "--output", "json"], True),
            (["library", *db, "import", str(ROOT / "fixtures/formats"), "--output", "json"], True),
            (["library", *db, "info", "--output", "json"], True),
            (["library", *db, "list", "--output", "json"], True),
            (["library", *db, "list", "--kind", "CDS", "--limit", "3", "--output", "json"], True),
            (["library", *db, "search", "amp", "--output", "json"], True),
            (["library", *db, "show", "1", "--output", "json"], True),
            (["library", *db, "show", "999999", "--output", "json"], False),
            (["library", *db, "edit", "1", "--name", "renamed", "--output", "json"], True),
            (["library", *db, "edit", "1", "--hide", "--output", "json"], True),
            (["detect-features", puc19, *db, "--output", "json"], True),
            (["detect-features", puc19, *db, "--new-only", "--min-length", "100", "--output", "json"], True),
            (["detect-features", str(ROOT / "fixtures/formats/snapgene/synthetic_multipart_origin.dna"), *db, "--output", "json"], True),
        ]:
            proc = subprocess.run([str(binary), *args], capture_output=True, text=True, timeout=30)
            body = json.loads(proc.stdout)
            validator.validate(body)
            if (proc.returncode == 0) != success or body["ok"] != success or proc.stderr:
                raise ValueError(f"unexpected response for {args}")
            checked.append(body)
        # Gene locus bundles: both strands, a second quantifier, and the two refusals.
        locus = str(ROOT / "fixtures/formats/locus/synthetic_locus.locus.json")
        for args, success in [
            (["isoforms", locus, "--output", "json"], True),
            (["isoforms", locus, "--quantifier", "NanoCount_lr", "--output", "json"], True),
            (["isoforms", str(ROOT / "fixtures/formats/locus/synthetic_minus.locus.json"), "--output", "json"], True),
            (["isoforms", locus, "--quantifier", "no_such", "--output", "json"], False),
            (["isoforms", puc19, "--output", "json"], False),
            (["features", locus, "--output", "json"], True),
            (["inspect", locus, "--output", "json"], True),
        ]:
            proc = subprocess.run([str(binary), *args], capture_output=True, text=True, timeout=30)
            body = json.loads(proc.stdout)
            validator.validate(body)
            if (proc.returncode == 0) != success or body["ok"] != success or proc.stderr:
                raise ValueError(f"unexpected response for {args}")
            checked.append(body)
    for command, filename, schema_file in [
        ("primer-design", "synthetic-primer-design.json", "primer-design-plan-1.schema.json"),
        (
            "gibson-optimise",
            "synthetic-gibson-optimisation.json",
            "gibson-optimisation-plan-1.schema.json",
        ),
        (
            "gibson-assemble",
            "synthetic-gibson-existing.json",
            "gibson-existing-plan-1.schema.json",
        ),
        (
            "gibson-assemble",
            "synthetic-gibson-mixed.json",
            "gibson-existing-plan-1.schema.json",
        ),
    ]:
        plan = ROOT / "fixtures/plans" / filename
        Draft202012Validator(
            json.loads((ROOT / "schemas" / schema_file).read_text())
        ).validate(json.loads(plan.read_text()))
        for path, success in [
            (plan, True),
            (ROOT / "fixtures/plans/synthetic_gibson.dna", False),
        ]:
            proc = subprocess.run(
                [str(binary), command, str(path)],
                capture_output=True,
                text=True,
                timeout=30,
            )
            body = json.loads(proc.stdout)
            validator.validate(body)
            if (
                (proc.returncode == 0) != success
                or body["ok"] != success
                or proc.stderr
            ):
                raise ValueError("unexpected extended Gibson response")
            checked.append(body)
    gibson_plan = ROOT / "fixtures/plans/synthetic-gibson.json"
    Draft202012Validator(
        json.loads((ROOT / "schemas/gibson-plan-1.schema.json").read_text())
    ).validate(json.loads(gibson_plan.read_text()))
    for path, success in [
        (gibson_plan, True),
        (ROOT / "fixtures/plans/synthetic_gibson.dna", False),
    ]:
        proc = subprocess.run(
            [str(binary), "gibson", str(path)],
            capture_output=True,
            text=True,
            timeout=30,
        )
        body = json.loads(proc.stdout)
        validator.validate(body)
        if (proc.returncode == 0) != success or body["ok"] != success or proc.stderr:
            raise ValueError("unexpected Gibson response")
        checked.append(body)
    plan_validator = Draft202012Validator(
        json.loads((ROOT / "schemas/ligation-plan-1.schema.json").read_text())
    )
    for filename in ["synthetic-religation.json", "synthetic-closure.json"]:
        path = ROOT / "fixtures/plans" / filename
        plan_validator.validate(json.loads(path.read_text()))
        proc = subprocess.run(
            [str(binary), "ligate", str(path)],
            capture_output=True,
            text=True,
            timeout=30,
        )
        body = json.loads(proc.stdout)
        validator.validate(body)
        if proc.returncode or not body["ok"] or proc.stderr:
            raise ValueError("unexpected ligation response")
        checked.append(body)
    proc = subprocess.run(
        [
            str(binary),
            "ligate",
            str(ROOT / "fixtures/formats/snapgene/synthetic_restriction_linear.dna"),
        ],
        capture_output=True,
        text=True,
        timeout=30,
    )
    body = json.loads(proc.stdout)
    validator.validate(body)
    if (
        proc.returncode == 0
        or body["ok"]
        or body["error"]["code"] != "ligation_failed"
        or proc.stderr
    ):
        raise ValueError("malformed plan must fail without a partial product")
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
    digest = next(b for b in checked if b["command"] == "digest" and b["ok"])
    bad = copy.deepcopy(digest)
    del bad["result"]["fragments"][0]["bottom"]
    mutations.append(bad)
    bad = copy.deepcopy(digest)
    bad["result"]["fragments"][0]["right_end"]["overhang_sequence"] = "N"
    mutations.append(bad)
    compatibility = next(
        b for b in checked if b["command"] == "compatible-ends" and b["ok"]
    )
    bad = copy.deepcopy(compatibility)
    del bad["result"]["analysis"]["pairs"][0]["assessment"]["reason"]
    mutations.append(bad)
    bad = copy.deepcopy(compatibility)
    bad["result"]["analysis"]["pairs"][0]["assessment"][
        "second_fragment_orientation"
    ] = "unknown"
    mutations.append(bad)
    annotated = next(
        b
        for b in checked
        if b["command"] == "fragments" and b["ok"] and b["result"]["source_features"]
    )
    bad = copy.deepcopy(annotated)
    bad["result"]["annotations"][0]["top"][0]["complete"] = "yes"
    mutations.append(bad)
    bad = copy.deepcopy(annotated)
    bad["result"]["annotations"][0]["bottom"][0]["parts"][0]["source_part"] = -1
    mutations.append(bad)
    bad = copy.deepcopy(annotated)
    del bad["result"]["source_features"][0]["qualifiers"]
    mutations.append(bad)
    ligation = next(b for b in checked if b["command"] == "ligate" and b["ok"])
    bad = copy.deepcopy(ligation)
    bad["result"]["product"]["components"][0]["top"]["start"] = -1
    mutations.append(bad)
    bad = copy.deepcopy(ligation)
    bad["result"]["product"]["junctions"][0]["assessment"]["compatible"] = False
    mutations.append(bad)
    bad = copy.deepcopy(ligation)
    bad["result"]["product"]["topology"] = (
        "circular"  # Free ends must be null on circles.
    )
    mutations.append(bad)
    gibson = next(b for b in checked if b["command"] == "gibson" and b["ok"])
    bad = copy.deepcopy(gibson)
    bad["result"]["components"][0]["selection"]["orientation"] = "unknown"
    mutations.append(bad)
    bad = copy.deepcopy(gibson)
    bad["result"]["components"][0]["reverse_primer"]["sequence_5to3"] = "N"
    mutations.append(bad)
    bad = copy.deepcopy(gibson)
    bad["result"]["overlap_length"] = 19
    mutations.append(bad)
    optimised = next(
        b for b in checked if b["command"] == "gibson-optimise" and b["ok"]
    )
    bad = copy.deepcopy(optimised)
    bad["result"]["pairs"][0]["forward"]["annealing_tm_c"] = "62 C"
    mutations.append(bad)
    bad = copy.deepcopy(optimised)
    bad["result"]["constraints"]["solution"]["magnesium_mm"] = -1
    mutations.append(bad)
    existing = next(b for b in checked if b["command"] == "gibson-assemble" and b["ok"])
    bad = copy.deepcopy(existing)
    bad["result"]["components"][0]["product_start"] = -1
    mutations.append(bad)
    if any(validator.is_valid(body) for body in mutations):
        raise ValueError("schema accepted an intentionally invalid response")
    print(
        f"Validated {len(checked)} live responses, committed JSON contracts and {len(mutations)} rejection checks."
    )


if __name__ == "__main__":
    main()

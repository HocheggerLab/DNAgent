# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85", "jsonschema==4.23.0"]
# ///
"""Independent template/site/product, NN-Tm and provenance acceptance checks."""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

from jsonschema import Draft202012Validator
from check_gibson import write_fixture
from check_gibson_extensions import rc, tm, dimer, hairpin

ROOT = Path(__file__).resolve().parents[1]
ENVELOPE = Draft202012Validator(
    json.loads((ROOT / "schemas/cli-envelope-0.10.0.schema.json").read_text())
)
PLAN = Draft202012Validator(
    json.loads((ROOT / "schemas/primer-design-plan-1.schema.json").read_text())
)


def binding_sites(sequence, oligos, circular, request):
    result = []
    for primer, oligo in enumerate(oligos, 1):
        for reverse, motif in [(False, oligo), (True, rc(oligo))]:
            limit = len(sequence) if circular else len(sequence) - len(motif) + 1
            for start in range(max(0, limit)):
                ref = (sequence + sequence)[start : start + len(motif)]
                mismatches = [i for i, (a, b) in enumerate(zip(ref, motif)) if a != b]
                anchor = (
                    range(request["exact_three_prime_bases"])
                    if reverse
                    else range(
                        len(motif) - request["exact_three_prime_bases"], len(motif)
                    )
                )
                if len(mismatches) <= request["max_mismatches"] and not set(
                    mismatches
                ).intersection(anchor):
                    result.append(
                        dict(
                            primer=primer,
                            start=start,
                            length=len(motif),
                            reverse=reverse,
                            mismatches=len(mismatches),
                        )
                    )
    return result


def products(sequence, sites, circular, maximum):
    result = []
    for a in sites:
        for b in sites:
            if a["reverse"] or not b["reverse"]:
                continue
            distance = b["start"] - a["start"]
            if circular:
                distance %= len(sequence)
            length = distance + b["length"]
            if distance < a["length"] or length > min(maximum, len(sequence)):
                continue
            result.append(
                dict(
                    start=a["start"],
                    length=length,
                    plus_primer=a["primer"],
                    minus_primer=b["primer"],
                    mismatches=a["mismatches"] + b["mismatches"],
                    template_sequence_forward=(sequence + sequence)[
                        a["start"] : a["start"] + length
                    ],
                )
            )
    return result


def verify(report, path, sequences, circular):
    assert report["plan_sha256"] == hashlib.sha256(path.read_bytes()).hexdigest()
    request = report["design"]["request"]
    constraints = request["constraints"]
    for source, sequence in zip(report["sources"], sequences):
        assert (
            source["file_sha256"]
            == hashlib.sha256(Path(source["path"]).read_bytes()).hexdigest()
        )
        assert (
            source["sequence_sha256"] == hashlib.sha256(sequence.encode()).hexdigest()
        )
    for pair in report["design"]["pairs"]:
        oligos = [pair[k]["sequence_5to3"] for k in ("forward", "reverse")]
        assert pair["heterodimer"] == dimer(*oligos)
        for key in ("forward", "reverse"):
            p = pair[key]
            seq = p["sequence_5to3"]
            assert abs(tm(seq, constraints) - p["tm_c"]) < 1e-8
            assert p["hairpin_stem"] == hairpin(seq)
            assert p["self_dimer"] == dimer(seq, seq)
            ref = sequences[request["reference_input"] - 1]
            interval = (ref + ref)[
                p["reference_start"] : p["reference_start"] + p["length"]
            ]
            assert seq == (rc(interval) if p["reverse"] else interval)
        if request["junction_offset"] is not None:
            j = request["junction_offset"]
            margin = request["junction_min_bases"]
            n = len(sequences[request["reference_input"] - 1])
            assert any(
                (p["reference_start"] - request["window_start"]) % n + margin
                <= j
                <= (p["reference_start"] - request["window_start"]) % n
                + p["length"]
                - margin
                for p in (pair["forward"], pair["reverse"])
            )
        for screen, seq in zip(pair["screens"], sequences):
            expected = binding_sites(seq, oligos, circular, request)
            assert screen["sites"] == expected
            expected_products = products(
                seq, expected, circular, request["screen_max_product_length"]
            )
            assert screen["products"] == expected_products
            if screen["input"] in request["positive_inputs"]:
                assert (
                    len(expected_products) == 1
                    and expected_products[0]["mismatches"] == 0
                )
            else:
                assert not expected_products


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve()
    original = json.loads(
        (ROOT / "fixtures/plans/synthetic-primer-design.json").read_text()
    )
    sequences = [
        (ROOT / "fixtures/plans" / p).read_text().splitlines()[1]
        for p in original["inputs"]
    ]
    with tempfile.TemporaryDirectory() as directory:
        base = Path(directory)
        path = base / "plan.json"

        def run(plan, success=True, strict=False):
            path.write_text(json.dumps(plan))
            if success:
                PLAN.validate(plan)
            proc = subprocess.run(
                [
                    str(binary),
                    *(["--strict"] if strict else []),
                    "primer-design",
                    str(path),
                ],
                capture_output=True,
                text=True,
            )
            result = json.loads(proc.stdout)
            ENVELOPE.validate(result)
            assert (proc.returncode == 0) == success, proc.stdout + proc.stderr
            assert result["ok"] == success
            return result

        for circular in (False, True):
            plan = copy.deepcopy(original)
            plan["inputs"] = []
            for i, seq in enumerate(sequences):
                file = base / f"source{i}.dna"
                write_fixture(file, seq, circular)
                plan["inputs"].append(str(file))
            if circular:
                plan["request"]["window_start"] = 140
            first = run(plan)
            verify(first["result"], path, sequences, circular)
            assert first == run(plan), "nondeterministic result"
            if not circular:
                by_feature = copy.deepcopy(plan)
                by_feature["feature_id"] = "feature-0001"
                verify(run(by_feature)["result"], path, sequences, False)
            for mutate in (
                "negative",
                "junction",
                "roles",
                "tm",
                "ambiguous",
                "feature",
            ):
                bad = copy.deepcopy(plan)
                if mutate == "negative":
                    bad["inputs"][2] = bad["inputs"][0]
                if mutate == "junction":
                    bad["request"]["junction_offset"] = 2**64 - 1
                if mutate == "roles":
                    bad["request"]["positive_inputs"] = [1, 1]
                if mutate == "tm":
                    bad["request"]["constraints"]["solution"]["primer_nm"] = 0
                if mutate == "ambiguous":
                    file = base / "ambiguous.dna"
                    write_fixture(file, "N" * 180, circular)
                    bad["inputs"][0] = str(file)
                if mutate == "feature":
                    bad["feature_id"] = "does-not-exist"
                run(bad, success=False)
        # Preserve earlier import warnings when a later input fails; strict refuses FASTA.
        plan = copy.deepcopy(original)
        plan["inputs"] = [
            str((ROOT / "fixtures/plans" / p).resolve()) for p in plan["inputs"]
        ]
        result = run(plan)
        verify(result["result"], path, sequences, False)
        assert result["result"]["sources"][0]["warnings"]
        failure = run(plan, success=False, strict=True)
        assert failure["warnings"]
        plan["inputs"][1] = str(base / "missing.dna")
        failure = run(plan, success=False)
        assert failure["warnings"]
    print(
        "Amplification: independent linear/circular duplex-site/product/Tm/provenance checks and 14 rejection/diagnostic checks passed."
    )


if __name__ == "__main__":
    main()

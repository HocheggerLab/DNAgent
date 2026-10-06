# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85", "jsonschema==4.23.0"]
# ///
"""Independent NN-Tm, exhaustive primer search and existing-overlap assembly checks."""

import argparse
import copy
import json
from pathlib import Path
import random
import subprocess
import tempfile

from Bio import SeqIO
from Bio.Seq import Seq
from Bio.SeqUtils import MeltingTemp as mt
from jsonschema import Draft202012Validator
from check_gibson import write_fixture, positions

ROOT = Path(__file__).resolve().parents[1]
VALIDATOR = Draft202012Validator(
    json.loads((ROOT / "schemas/cli-envelope-0.10.0.schema.json").read_text())
)


def rc(s):
    return str(Seq(s).reverse_complement())


def dimer(a, b):
    b = rc(b)
    longest = three = 0
    for i in range(len(a)):
        for j in range(len(b)):
            k = 0
            while i + k < len(a) and j + k < len(b) and a[i + k] == b[j + k]:
                k += 1
            longest = max(longest, k)
            if j == 0 or i + k == len(a):
                three = max(three, k)
    return {"longest_run": longest, "longest_three_prime_run": three}


def hairpin(s):
    best = 0
    reverse = rc(s)
    # Enumerate separated stem substrings, not the inward-walking Rust routine.
    for size in range(1, (len(s) - 3) // 2 + 1):
        for i in range(len(s) - 2 * size - 2):
            for j in range(i + size + 3, len(s) - size + 1):
                if s[i : i + size] == reverse[len(s) - j - size : len(s) - j]:
                    best = size
    return best


def unique(s, motif, circular):
    if motif == rc(motif):
        return False
    s = s + s[: len(motif) - 1] if circular else s
    patterns = (motif, rc(motif))
    return (
        sum(s[i : i + len(motif)] in patterns for i in range(len(s) - len(motif) + 1))
        == 1
    )


def tm(s, c):
    sol = c["solution"]
    return mt.Tm_NN(
        s,
        nn_table=mt.DNA_NN3,
        Na=sol["sodium_mm"],
        K=sol["potassium_mm"],
        Tris=sol["tris_mm"],
        Mg=sol["magnesium_mm"],
        dNTPs=sol["dntp_mm"],
        dnac1=sol["primer_nm"],
        dnac2=0,
        selfcomp=False,
        saltcorr=5,
    )


def screen_pass(d, c):
    return (
        d["longest_run"] <= c["max_dimer_run"]
        and d["longest_three_prime_run"] <= c["max_three_prime_run"]
    )


def candidates(core, tail, reverse, source, circular, c):
    result = []
    for length in range(c["min_length"], min(c["max_length"], len(core)) + 1):
        anneal = rc(core[-length:]) if reverse else core[:length]
        if not unique(source, anneal, circular):
            continue
        full = tail + anneal
        temp = tm(anneal, c)
        gc = sum(anneal.count(b) for b in "GC") / length
        hp = hairpin(full)
        sd = dimer(full, full)
        if (
            c["min_tm_c"] <= temp <= c["max_tm_c"]
            and c["min_gc_fraction"] <= gc <= c["max_gc_fraction"]
            and hp <= c["max_hairpin_stem"]
            and screen_pass(sd, c)
        ):
            result.append((length, temp, full, hp, sd))
    return result


def oracle_pair(core, tail, source, circular, c, forward_tail=""):
    fs = candidates(core, forward_tail, False, source, circular, c)
    rs = candidates(core, tail, True, source, circular, c)
    feasible = []
    for f in fs:
        for r in rs:
            ds = dimer(f[2], r[2])
            if (
                f[0] + r[0] <= len(core)
                and abs(f[1] - r[1]) <= c["max_pair_tm_difference_c"]
                and screen_pass(ds, c)
            ):
                score = abs(f[1] - c["target_tm_c"]) + abs(r[1] - c["target_tm_c"])
                feasible.append(((score, f[0] + r[0], f[0], r[0]), f, r, ds))
    return (min(feasible) if feasible else None), len(fs) * len(rs), len(feasible)


def get_cores(plan, records, key="cores"):
    cores = []
    for s in plan[key]:
        source = str(records[s["input"] - 1].seq)
        value = (source + source)[s["start"] : s["start"] + s["length"]]
        cores.append(rc(value) if s["orientation"] == "reverse" else value)
    return cores


def verify_optimised(body, plan, records, expected):
    out = body["result"]
    assert out["constraints"] == plan["constraints"]
    cores = get_cores(plan, records)
    design = out["design"]
    assert design["product_sequence_5to3"] == "".join(cores)
    prep = [core["preparation"] for core in plan["cores"]]
    assert len(out["pairs"]) == prep.count("pcr")
    for component, fragment in zip(design["components"], prep, strict=True):
        assert component["preparation"] == fragment
        if fragment == "provided":
            # Used as given: no primers, and nothing added to either end.
            assert component["forward_primer"] is None
            assert component["reverse_primer"] is None
            assert (
                component["fragment_sequence_5to3"] == component["core_sequence_5to3"]
            )
    for pair, (best, examined, feasible) in zip(out["pairs"], expected, strict=True):
        i = pair["component"] - 1
        assert prep[i] == "pcr"
        key, f, r, ds = best
        assert abs(pair["score"] - key[0]) < 1e-9
        assert pair["candidate_pairs_examined"] == examined
        assert pair["feasible_pairs"] == feasible
        assert pair["heterodimer"] == ds
        for name, reference in [("forward", f), ("reverse", r)]:
            item = pair[name]
            assert item["primer"]["sequence_5to3"] == reference[2]
            assert abs(item["annealing_tm_c"] - reference[1]) < 1e-9
            assert item["hairpin_stem"] == reference[3]
            assert item["self_dimer"] == reference[4]
            assert design["components"][i][f"{name}_primer"] == item["primer"]
        # Asymmetric chosen primer lengths still recover the exact PCR product.
        pcr = f[2] + cores[i][f[0] : len(cores[i]) - r[0]] + rc(r[2])
        assert design["components"][i]["fragment_sequence_5to3"] == pcr


def verify_existing(body, plan, records):
    out = body["result"]
    cores = get_cores(plan, records, "fragments")
    merged = cores[0]
    starts = [0]
    for i, fragment in enumerate(cores[1:]):
        length = plan["overlaps"][i]
        assert merged[-length:] == fragment[:length]
        starts.append(len(merged) - length)
        merged += fragment[length:]
    if plan["topology"] == "circular":
        length = plan["overlaps"][-1]
        assert merged[-length:] == merged[:length]
        merged = merged[:-length]
    assert out["product_sequence_5to3"] == merged
    assert len(merged) == sum(map(len, cores)) - sum(plan["overlaps"])
    for component, sequence, start in zip(
        out["components"], cores, starts, strict=True
    ):
        assert component["sequence_5to3"] == sequence
        assert component["product_start"] == start
        assert component["wraps_origin"] == (start + len(sequence) > len(merged))
        assert all(
            merged[(start + i) % len(merged)] == base for i, base in enumerate(sequence)
        )
        # Full-span synthetic feature associates every original fragment base,
        # including BOTH source copies of merged homology, with its product base.
        full = next(
            m
            for m in component["annotations"]
            if m["source_feature_id"]
            == out["inputs"][component["selection"]["input"] - 1]["features"][0]["id"]
        )
        local = sorted(
            p
            for part in full["parts"]
            for p in positions(part["fragment_region"], len(sequence))
        )
        assert local == list(range(len(sequence)))
        selection = component["selection"]
        n = len(records[selection["input"] - 1])
        associations = []
        for part in full["parts"]:
            original = positions(part["source_region"], n)
            if selection["orientation"] == "reverse":
                original.reverse()
            fragment = positions(part["fragment_region"], len(sequence))
            associations.extend(
                (src, (start + loc) % len(merged))
                for src, loc in zip(original, fragment, strict=True)
            )
        expected = [
            (
                (
                    selection["start"]
                    + (
                        len(sequence) - 1 - j
                        if selection["orientation"] == "reverse"
                        else j
                    )
                )
                % n,
                (start + j) % len(merged),
            )
            for j in range(len(sequence))
        ]
        assert sorted(associations) == sorted(expected)
    assert len(out["junctions"]) == len(plan["overlaps"])
    for i, junction in enumerate(out["junctions"]):
        next_ = (i + 1) % len(cores)
        assert junction["product_start"] == starts[next_]
        assert junction["overlap_sequence_5to3"] == cores[next_][: plan["overlaps"][i]]
        assert junction["closure"] == (next_ == 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    rng = random.Random(99177)
    optimised = existing = refusals = 0
    base = json.loads(
        (ROOT / "fixtures/plans/synthetic-gibson-optimisation.json").read_text()
    )
    with tempfile.TemporaryDirectory(prefix="dnagent-gibson-extensions-") as tmp:
        root = Path(tmp)
        path = root / "plan.json"
        sources = [root / "a.dna", root / "b.dna"]

        def call(command, p, success, strict=False):
            nonlocal refusals
            path.write_text(json.dumps(p))
            proc = subprocess.run(
                [str(binary), command, str(path), *(["--strict"] if strict else [])],
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=30,
            )
            body = json.loads(proc.stdout)
            assert VALIDATOR.is_valid(body), "envelope schema mismatch"
            assert (proc.returncode == 0) == body["ok"] == success, body.get(
                "error", {}
            )
            assert not proc.stderr
            if not success:
                assert body["error"]["code"] in [
                    "gibson_failed",
                    "import_warnings",
                    "command_failed",
                ]
                assert "result" not in body
                refusals += 1
            return body

        for index in range(8):
            sequences = ["".join(rng.choices("ACGT", k=180)) for _ in sources]
            for s, seq in zip(sources, sequences, strict=True):
                write_fixture(s, seq, False)
            p = copy.deepcopy(base)
            p["inputs"] = [{"path": s.name} for s in sources]
            p["cores"] = [
                {
                    "input": i + 1,
                    "start": 0,
                    "length": 180,
                    "orientation": "reverse" if (index + i) % 2 else "forward",
                    # Every other round, the first fragment is digested and provided, so
                    # both overlaps must move onto the amplified one.
                    "preparation": "provided" if (i == 0 and index % 4 == 3) else "pcr",
                }
                for i in range(2)
            ]
            p["topology"] = "circular" if index % 2 else "linear"
            # Wide but explicit model limits exercise ranking rather than prescribe
            # a lab protocol. Also check tighter, potentially infeasible constraints.
            c = p["constraints"]
            c.update(
                min_length=18,
                max_length=40 if index == 7 else 25,
                min_tm_c=40,
                max_tm_c=85,
                target_tm_c=62,
                max_pair_tm_difference_c=20,
                min_gc_fraction=0,
                max_gc_fraction=1,
                max_hairpin_stem=20,
                max_dimer_run=40,
                max_three_prime_run=20,
            )
            c["solution"].update(
                sodium_mm=[50, 0, 80, 20][index % 4],
                potassium_mm=10,
                tris_mm=20,
                magnesium_mm=[0, 1.5, 0.2, 0.5][index % 4],
                dntp_mm=0.2,
                primer_nm=100 + index * 50,
            )
            records = [SeqIO.read(s, "snapgene") for s in sources]
            cores = get_cores(p, records)
            for tight in [False, True]:
                if tight:
                    c.update(
                        max_hairpin_stem=5,
                        max_dimer_run=7,
                        max_three_prime_run=3,
                        max_pair_tm_difference_c=3,
                    )
                overlap = p["overlap_length"]
                prep = [core["preparation"] for core in p["cores"]]
                forward_tails = ["" for _ in cores]
                reverse_tails = ["" for _ in cores]
                for i in range(len(cores)):
                    if not (i == 0 or p["topology"] == "circular"):
                        continue
                    nxt = (i + 1) % len(cores)
                    if prep[i] == "provided":
                        forward_tails[nxt] = cores[i][len(cores[i]) - overlap :]
                    else:
                        reverse_tails[i] = rc(cores[nxt][:overlap])
                expected = [
                    oracle_pair(
                        core, reverse_tails[i], sequences[i], False, c, forward_tails[i]
                    )
                    for i, core in enumerate(cores)
                    if prep[i] == "pcr"
                ]
                success = all(item[0] is not None for item in expected)
                body = call("gibson-optimise", p, success)
                if success:
                    verify_optimised(body, p, records, expected)
                    optimised += 1
        # Existing fragments: variable overlaps, both source orientations, circular
        # source arcs and linear/circular products, plus one-fragment reclosure.
        for circular in [False, True]:
            for reverse in [False, True]:
                for length in [20, 37, 60]:
                    sequence = "".join(rng.choices("ACGT", k=400))
                    write_fixture(sources[0], sequence, True)
                    frags = [
                        {
                            "input": 1,
                            "start": 350,
                            "length": 210,
                            "orientation": "forward",
                        },
                        {
                            "input": 1,
                            "start": 160 - length,
                            "length": 190 + length + (25 if circular else 0),
                            "orientation": "forward",
                        },
                    ]
                    overlaps = [length] + ([25] if circular else [])
                    if reverse:
                        frags.reverse()
                        for f in frags:
                            f["orientation"] = "reverse"
                    p = {
                        "schema_version": 1,
                        "inputs": [{"path": sources[0].name}],
                        "fragments": frags,
                        "topology": "circular" if circular else "linear",
                        "overlaps": overlaps,
                    }
                    body = call("gibson-assemble", p, True)
                    verify_existing(body, p, [SeqIO.read(sources[0], "snapgene")])
                    existing += 1
        # Single fragment stores repeated terminal homology in a linear source.
        core = "".join(rng.choices("ACGT", k=160))
        write_fixture(sources[0], core + core[:25], False)
        p = {
            "schema_version": 1,
            "inputs": [{"path": sources[0].name}],
            "fragments": [
                {"input": 1, "start": 0, "length": 185, "orientation": "forward"}
            ],
            "topology": "circular",
            "overlaps": [25],
        }
        verify_existing(
            call("gibson-assemble", p, True), p, [SeqIO.read(sources[0], "snapgene")]
        )
        existing += 1
        for key, value in [
            ("overlaps", [24]),
            ("overlaps", []),
            ("schema_version", 2),
            ("unexpected", 1),
        ]:
            bad = copy.deepcopy(p)
            bad[key] = value
            call("gibson-assemble", bad, False)
        bad = copy.deepcopy(p)
        bad["fragments"][0]["length"] = 50
        call("gibson-assemble", bad, False)
        write_fixture(sources[0], "A" * 185, False)
        call("gibson-assemble", p, False)
        # Deterministic impossibility: no relaxed fallback and no partial product.
        bad = copy.deepcopy(base)
        bad["inputs"][0]["path"] = str(ROOT / "fixtures/plans/synthetic_gibson.dna")
        bad["constraints"].update(min_tm_c=99, max_tm_c=100, target_tm_c=100)
        call("gibson-optimise", bad, False)
        for key, value in [
            ("min_length", 17),
            ("min_gc_fraction", 1.1),
            ("target_tm_c", 0),
        ]:
            bad = copy.deepcopy(base)
            bad["inputs"][0]["path"] = str(ROOT / "fixtures/plans/synthetic_gibson.dna")
            bad["constraints"][key] = value
            call("gibson-optimise", bad, False)
        for command, shape in [("gibson-optimise", base), ("gibson-assemble", p)]:
            bad = copy.deepcopy(shape)
            bad["inputs"][0]["path"] = str(
                ROOT / "fixtures/formats/snapgene/synthetic_circular.dna"
            )
            assert call(command, bad, False, True)["error"]["code"] == "import_warnings"
            bad["inputs"].append({"path": "missing.dna"})
            assert call(command, bad, False)["warnings"]
    print(
        f"Gibson extensions: {optimised} exhaustively checked optimised designs, {existing} existing-overlap assemblies, {refusals} explicit refusals."
    )


if __name__ == "__main__":
    main()

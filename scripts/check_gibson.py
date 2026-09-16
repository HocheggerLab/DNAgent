# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85", "jsonschema==4.23.0"]
# ///
"""Independent source/primer/PCR-tail assembly oracle; synthetic originals only."""

import argparse
import copy
import json
from pathlib import Path
import random
import struct
import subprocess
import tempfile

from Bio import SeqIO
from Bio.Seq import Seq
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
VALIDATOR = Draft202012Validator(
    json.loads((ROOT / "schemas/cli-envelope-0.9.0.schema.json").read_text())
)
PLAN_VALIDATOR = Draft202012Validator(
    json.loads((ROOT / "schemas/gibson-plan-1.schema.json").read_text())
)


def rc(s):
    return str(Seq(s).reverse_complement())


def write_fixture(path, sequence, circular):
    def packet(kind, data):
        return struct.pack(">BI", kind, len(data)) + data

    n = len(sequence)
    xml = f'<Features><Feature name="full" type="CDS" directionality="1"><Segment range="1-{n}" type="standard"/></Feature>'
    if circular:
        xml += f'<Feature name="seam" type="misc_feature" directionality="2"><Segment range="{n - 19}-15" type="standard"/></Feature>'
    xml += "</Features>"
    path.write_bytes(
        packet(9, b"SnapGene\0\1\0\1\0\1")
        + packet(0, bytes([int(circular)]) + sequence.encode())
        + packet(10, xml.encode())
    )


def positions(region, n):
    if region["kind"] == "linear":
        return list(range(region["start"], region["end"]))
    return [(region["start"] + i) % n for i in range(region["length"])]


def verify(result, plan, sources):
    cores = []
    indices = []
    for selection in plan["cores"]:
        template = sources[selection["input"] - 1]
        coords = [
            (selection["start"] + i) % len(template) for i in range(selection["length"])
        ]
        core = "".join(template[i] for i in coords)
        if selection["orientation"] == "reverse":
            core = rc(core)
            coords.reverse()
        cores.append(core)
        indices.append(coords)
    expected = "".join(cores)
    assert result["product_sequence_5to3"] == expected
    assert result["topology"] == plan["topology"]
    assert result["overlap_length"] == plan["overlap_length"]
    assert result["annealing_length"] == plan["annealing_length"]
    assert len(result["components"]) == len(cores)
    amplicons = []
    start = 0
    a, overlap = plan["annealing_length"], plan["overlap_length"]
    for i, (component, core, coords) in enumerate(
        zip(result["components"], cores, indices, strict=True)
    ):
        selection = plan["cores"][i]
        assert component["selection"] == selection
        assert component["product_start"] == start
        assert component["core_sequence_5to3"] == core
        f, r = component["forward_primer"], component["reverse_primer"]
        assert f["annealing_sequence_5to3"] == core[:a]
        assert r["annealing_sequence_5to3"] == rc(core[-a:])
        assert f["tail_sequence_5to3"] == ""
        tail = (
            cores[(i + 1) % len(cores)][:overlap]
            if i + 1 < len(cores) or plan["topology"] == "circular"
            else ""
        )
        assert r["tail_sequence_5to3"] == rc(tail)
        for primer in [f, r]:
            assert (
                primer["sequence_5to3"]
                == primer["tail_sequence_5to3"] + primer["annealing_sequence_5to3"]
            )
            assert primer["annealing_gc_bases"] == sum(
                primer["annealing_sequence_5to3"].count(b) for b in "GC"
            )
        # Build the PCR product from whole oligos and template interior, independently
        # of the Rust core+tail construction. Double-stranded PCR top is explicit.
        pcr = f["sequence_5to3"] + core[a:-a] + rc(r["sequence_5to3"])
        assert component["pcr_product_sequence_5to3"] == pcr
        amplicons.append(pcr)
        source = result["inputs"][selection["input"] - 1]
        assert source["sequence"] == sources[selection["input"] - 1]
        features = {f["id"]: f for f in source["features"]}
        associations = {}
        for mapping in component["annotations"]:
            feature = features[mapping["source_feature_id"]]
            label = feature["label"]
            expected_strand = "forward" if label == "full" else "reverse"
            if selection["orientation"] == "reverse":
                expected_strand = {"forward": "reverse", "reverse": "forward"}[
                    expected_strand
                ]
            assert mapping["fragment_strand"] == expected_strand
            actual = []
            for part in mapping["parts"]:
                source_coords = positions(
                    part["source_region"], len(source["sequence"])
                )
                if selection["orientation"] == "reverse":
                    source_coords.reverse()
                local = positions(part["fragment_region"], len(core))
                actual.extend(zip(source_coords, local, strict=True))
            associations[label] = set(actual)
            assert len(actual) == len(set(actual)) == mapping["retained_bases"]
            assert mapping["complete"] == (
                mapping["retained_bases"] == mapping["source_bases"]
            )
        # Independent fixture/XML semantics: full forward feature and, on circles,
        # reverse feature covering the last 20 + first 15 source bases.
        wanted = {"full": {(base, local) for local, base in enumerate(coords)}}
        if source["topology"] == "circular":
            seam = {
                (base, local)
                for local, base in enumerate(coords)
                if base >= len(source["sequence"]) - 20 or base < 15
            }
            if seam:
                wanted["seam"] = seam
        assert associations == wanted
        start += len(core)
    assembled = amplicons[0]
    for amplicon in amplicons[1:]:
        assert assembled[-overlap:] == amplicon[:overlap]
        assembled += amplicon[overlap:]
    if plan["topology"] == "circular":
        assert assembled[-overlap:] == assembled[:overlap]
        assembled = assembled[:-overlap]
    assert assembled == expected
    junctions = result["junctions"]
    assert len(junctions) == len(cores) - (plan["topology"] == "linear")
    for i, junction in enumerate(junctions):
        assert junction["after_component"] == i + 1
        assert junction["before_component"] == (i + 1) % len(cores) + 1
        assert junction["closure"] == (i + 1 == len(cores))
        assert (
            junction["overlap_sequence_5to3"] == cores[(i + 1) % len(cores)][:overlap]
        )
        assert junction["product_start"] == (
            0 if junction["closure"] else sum(map(len, cores[: i + 1]))
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    rng = random.Random(20260708)
    count = rejected = 0
    with tempfile.TemporaryDirectory(prefix="dnagent-gibson-") as tmp:
        root = Path(tmp)
        paths = [root / "first.dna", root / "second.dna"]
        path = root / "plan.json"
        plan = {
            "schema_version": 1,
            "inputs": [{"path": p.name} for p in paths],
            "cores": [
                {"input": 1, "start": 470, "length": 130, "orientation": "forward"},
                {"input": 2, "start": 30, "length": 170, "orientation": "forward"},
            ],
            "topology": "circular",
            "overlap_length": 30,
            "annealing_length": 24,
        }

        def call(p, success=True, strict=False):
            nonlocal count, rejected
            path.write_text(json.dumps(p))
            proc = subprocess.run(
                [str(binary), "gibson", str(path), *(["--strict"] if strict else [])],
                capture_output=True,
                text=True,
                timeout=30,
                cwd=ROOT,
            )
            body = json.loads(proc.stdout)
            VALIDATOR.validate(body)
            assert (proc.returncode == 0) == body["ok"] == success
            assert not proc.stderr
            if success:
                PLAN_VALIDATOR.validate(p)
                sources = [
                    str(SeqIO.read(root / source["path"], "snapgene").seq)
                    for source in p["inputs"]
                ]
                verify(body["result"], p, sources)
                count += 1
            else:
                assert "result" not in body
                rejected += 1
            return body

        for seed in range(4):
            sequences = ["".join(rng.choices("ACGT", k=500)) for _ in paths]
            for p, seq, circular in zip(paths, sequences, [True, False], strict=True):
                write_fixture(p, seq, circular)
            for topology in ["linear", "circular"]:
                for first_orientation in ["forward", "reverse"]:
                    for second_orientation in ["forward", "reverse"]:
                        for overlap, annealing in [(20, 18), (60, 40)]:
                            p = copy.deepcopy(plan)
                            p["topology"] = topology
                            p["cores"][0]["orientation"] = first_orientation
                            p["cores"][1]["orientation"] = second_orientation
                            p["overlap_length"], p["annealing_length"] = (
                                overlap,
                                annealing,
                            )
                            call(p, strict=True)
            for topology in ["linear", "circular"]:
                p = copy.deepcopy(plan)
                p["topology"] = topology
                p["cores"] = p["cores"][:1]
                call(p)
        for key, value in [
            ("schema_version", 2),
            ("overlap_length", 19),
            ("annealing_length", 41),
            ("unknown", True),
            ("cores", []),
            ("inputs", []),
        ]:
            p = copy.deepcopy(plan)
            p[key] = value
            assert call(p, False)["error"]["code"] == "gibson_failed"
        for key, value in [
            ("input", 0),
            ("start", 2**64 - 1),
            ("length", 10),
            ("orientation", "unknown"),
        ]:
            p = copy.deepcopy(plan)
            p["cores"][0][key] = value
            call(p, False)
        p = copy.deepcopy(plan)
        p["cores"][1]["start"] = 490  # Linear source cannot wrap.
        call(p, False)
        p = copy.deepcopy(plan)
        p["cores"] *= 65
        call(p, False)
        write_fixture(paths[0], "A" * 500, True)
        assert "primer" in call(plan, False)["error"]["message"]
        write_fixture(paths[0], sequences[0][:-1] + "N", True)
        call(plan, False)
        # Primer sites are unique, but shorter overlap duplicated in another core.
        first = sequences[0]
        motif = (first + first)[470:490]
        for duplicated in [motif, rc(motif)]:
            second = sequences[1][:30] + duplicated + sequences[1][50:]
            write_fixture(paths[0], first, True)
            write_fixture(paths[1], second, False)
            p = copy.deepcopy(plan)
            p["overlap_length"] = 20
            assert "overlap" in call(p, False)["error"]["message"]
        warning_source = ROOT / "fixtures/formats/snapgene/synthetic_circular.dna"
        p = copy.deepcopy(plan)
        p["inputs"][0]["path"] = str(warning_source)
        assert call(p, False, True)["error"]["code"] == "import_warnings"
        p["inputs"][1]["path"] = "missing.dna"
        assert call(p, False)["warnings"]
    print(
        f"Gibson source/primer/PCR/assembly/annotation/schema checks passed: {count} designs and {rejected} rejected plans."
    )


if __name__ == "__main__":
    main()

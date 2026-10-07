#!/usr/bin/env python3
"""Smoke-check this skill and all headless command families on public app fixtures.

No external dependencies; no private inputs; no biological/agent-evaluation claim.
Usage: python3 scripts/check_skill.py --repo CHECKOUT --binary EXECUTABLE
"""

import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile

SKILL = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    repo, binary = args.repo.resolve(), args.binary.resolve()
    assert binary.is_file(), "build or locate the public CLI first"
    text = (SKILL / "SKILL.md").read_text()
    header = text.split("---", 2)[1]
    assert re.search(r"^name: dnagent$", header, re.M)
    description = re.search(r"^description: (.+)$", header, re.M).group(1)
    assert 0 < len(description) <= 1024
    for path in SKILL.rglob("*.md"):
        for link in re.findall(r"\]\(([^)]+)\)", path.read_text()):
            if "://" not in link and not link.startswith("#"):
                assert (path.parent / link.split("#")[0]).is_file(), (path, link)

    fixture = repo / "fixtures/formats/snapgene"
    plans = repo / "fixtures/plans"
    source = fixture / "synthetic_restriction_linear.dna"
    assert source.is_file(), "expected public DNAagent fixture checkout"
    with tempfile.TemporaryDirectory(prefix="dnagent-skill-smoke-") as directory:

        def run(arguments):
            return subprocess.run(
                [str(binary), *map(str, arguments)],
                cwd=directory,
                capture_output=True,
                text=True,
                timeout=30,
            )

        help_result = run(["--help"])
        assert help_result.returncode == 0
        commands = set(
            re.findall(r"^  ([a-z][a-z-]+) {2,}", help_result.stdout, re.M)
        ) - {"help", "gui"}
        assert all(f"`{command}`" in text for command in commands)
        for command in sorted(
            commands | ({"gui"} if "gui" in help_result.stdout else set())
        ):
            assert run([command, "--help"]).returncode == 0

        cases = [
            ["inspect", source, "--output", "json"],
            ["features", source, "--output", "json"],
            ["primers", source, "--output", "json"],
            ["sequence", source, "--range", "10..30", "--output", "json"],
            ["enzymes", "--output", "json"],
            ["sites", source, "--enzymes", "EcoRI,BamHI", "--output", "json"],
            ["digest", source, "--enzymes", "EcoRI,BamHI", "--output", "json"],
            [
                "compatible-ends",
                source,
                "--enzymes",
                "EcoRI",
                "--other",
                source,
                "--other-enzymes",
                "BamHI",
                "--output",
                "json",
            ],
            ["fragments", source, "--enzymes", "EcoRI,BamHI", "--output", "json"],
            ["map", source, "--out", Path(directory) / "map.svg"],
            ["ligate", plans / "synthetic-religation.json", "--output", "json"],
            ["gibson", plans / "synthetic-gibson.json", "--output", "json"],
            ["gibson-optimise", plans / "synthetic-gibson-optimisation.json"],
            ["gibson-assemble", plans / "synthetic-gibson-existing.json"],
            ["primer-design", plans / "synthetic-primer-design.json"],
            ["enzyme-catalogue"],
            ["translate", repo / "fixtures/formats/genbank/synthetic_cdna.gb", "--all-cds", "--output", "json"],
            ["orfs", repo / "fixtures/formats/genbank/synthetic_cdna.gb", "--min-codons", "100", "--output", "json"],
            ["convert", source, "--out", Path(directory) / "converted.gb"],
            ["annotate", Path(directory) / "converted.gb", "--out", Path(directory) / "annotated.gb",
             "--add", "--range", "10..40", "--label", "smoke"],
            ["library", "--db", Path(directory) / "features.sqlite", "import", repo / "fixtures/formats", "--output", "json"],
            ["detect-features", repo / "fixtures/formats/fasta/pUC19_M77789.fasta", "--db", Path(directory) / "features.sqlite", "--output", "json"],
            ["isoforms", repo / "fixtures/formats/locus/synthetic_locus.locus.json", "--output", "json"],
        ]
        product = Path(directory) / "product.gb"
        cases.append(["gibson", plans / "synthetic-cdna-into-puc19.json", "--out", product, "--output", "json"])
        # `mcp` is a stdio relay to the running desktop app, not a one-shot JSON command.
        covered = commands - {"mcp"}
        assert {case[0] for case in cases} == covered, f"CLI/skill coverage drift: {covered ^ {case[0] for case in cases}}"
        envelope_name = {"library": "library-import"}
        for case in cases:
            process = run(case)
            assert process.returncode == 0, (case[0], process.stderr, process.stdout)
            body = json.loads(process.stdout)
            assert (
                body["schema_version"] == "0.10.0"
            ), "read new contract before updating skill"
            assert body["command"] == envelope_name.get(case[0], case[0]) and body["ok"] is True, (case[0], body.get("error"))
            assert isinstance(body["warnings"], list) and "result" in body
            if case[0] == "sequence":
                assert len(body["result"]["sequence"]) == 20
        # Without the app it must fail loudly and keep stdout clean for the protocol.
        relay = run(["mcp", "--socket", str(Path(directory) / "absent.sock")])
        assert relay.returncode != 0 and not relay.stdout and "not running" in relay.stderr

        assert "<svg" in (Path(directory) / "map.svg").read_text()
        # Written GenBank reopens; the annotated product carries its primers.
        assert "smoke" in [f["label"] for f in json.loads(run(["features", Path(directory) / "annotated.gb", "--output", "json"]).stdout)["result"]]
        assert len(json.loads(run(["primers", product, "--output", "json"]).stdout)["result"]) == 4

        for output, strand in [
            ("fasta", None),
            ("genbank", "top"),
            ("genbank", "bottom"),
        ]:
            case = ["fragments", source, "--enzymes", "EcoRI,BamHI", "--output", output]
            if strand:
                case += ["--strand", strand]
            process = run(case)
            assert process.returncode == 0
            assert process.stdout.startswith(">" if output == "fasta" else "LOCUS")
        for command, plan in [
            ("ligate", "synthetic-religation.json"),
            ("gibson", "synthetic-gibson.json"),
        ]:
            process = run([command, plans / plan, "--output", "text"])
            assert process.returncode == 0 and process.stdout.strip()
        alias = run(["gibson-optimize", plans / "synthetic-gibson-optimisation.json"])
        assert (
            alias.returncode == 0
            and json.loads(alias.stdout)["command"] == "gibson-optimise"
        )
        strict = run(
            [
                "inspect",
                fixture / "synthetic_circular.dna",
                "--strict",
                "--output",
                "json",
            ]
        )
        body = json.loads(strict.stdout)
        assert strict.returncode != 0 and body["ok"] is False
        assert body["error"]["code"] == "import_warnings" and body["warnings"]
        assert "result" not in body
        wrong_kind = run(["gibson-assemble", plans / "synthetic-gibson.json"])
        assert (
            wrong_kind.returncode != 0
            and json.loads(wrong_kind.stdout)["error"]["code"] == "gibson_failed"
        )
        syntax = run(["sites", source])
        assert syntax.returncode != 0 and not syntax.stdout.strip() and syntax.stderr
    print(
        f"Skill smoke passed: {len(commands)} JSON command families (incl. GenBank writes, library and Gibson product), 3 strand exports, 2 text summaries, alias and 3 failure paths. Not an agent/biology evaluation."
    )


if __name__ == "__main__":
    main()

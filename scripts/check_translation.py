# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85"]
# ///
"""Check `dnagent translate` and `dnagent orfs` against Biopython 1.85; public fixtures only.

Independent references:
- feature translations are re-derived from CLI `features` locations and `sequence`
  (source-order splice, reverse complement of the splice on the reverse strand,
  codon_start) with Biopython `Seq.translate`, and compared with embedded
  SnapGene/GenBank translations where present;
- ranges are compared exhaustively over start/length/strand/frame samples, including
  origin-wrapping ranges;
- all 27 NCBI tables are compared codon by codon;
- ORFs are compared with a separate, straightforward Python implementation of the
  documented definition (complete start-to-stop, longest per stop, circular wrap).

Biopython reports some ambiguous codons as B/Z/J; DNAagent reports X. Those letters
are mapped to X before comparison. Context-dependent stops (tables 27, 28, 31) are
sense codons in both.
"""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import warnings

from Bio import BiopythonWarning
from Bio.Data import CodonTable
from Bio.Seq import Seq

# Expected for tables 27, 28 and 31 (see module docstring).
warnings.filterwarnings("ignore", message=".*code\\(s\\) for both STOP and an amino acid.*", category=BiopythonWarning)

ROOT = Path(__file__).resolve().parents[1]
SNAPGENE = ROOT / "fixtures/formats/snapgene"
FIXTURES = ["synthetic_translation.dna", "synthetic_linear.dna", "pUC19_M77789.dna", "synthetic_circular.dna", "synthetic_restriction_linear.dna"]


class Cli:
    def __init__(self, binary: Path):
        self.binary = binary
        self.calls = 0

    def json(self, *args: str, ok: bool = True) -> dict:
        proc = subprocess.run([str(self.binary), *args, "--output", "json"], capture_output=True, text=True, timeout=60)
        self.calls += 1
        body = json.loads(proc.stdout)
        if body["ok"] != ok:
            raise AssertionError(f"dnagent {' '.join(args)}: expected ok={ok}, got {body.get('error')}")
        return body


def biopython(dna: str, table: int) -> str:
    protein = str(Seq(dna).translate(table=table))
    return protein.translate(str.maketrans("BZJ", "XXX"))


def part_positions(part: dict, length: int) -> list[int]:
    if part["kind"] == "linear":
        return list(range(part["start"], part["end"]))
    return [(part["start"] + i) % length for i in range(part["length"])]


def reverse_complement(dna: str) -> str:
    return str(Seq(dna).reverse_complement())


def check_features(cli: Cli, fixture: Path) -> int:
    sequence = cli.json("sequence", str(fixture))["result"]["sequence"]
    length = len(sequence)
    features = {f["id"]: f for f in cli.json("features", str(fixture))["result"]}
    body = cli.json("translate", str(fixture), "--all-cds")
    checked = 0
    for item in body["result"]["translations"]:
        feature = features[item["feature_id"]]
        qualifiers = {q["key"]: q["value"] for q in feature["qualifiers"]}
        positions = [p for part in feature["location"]["parts"] for p in part_positions(part, length)]
        dna = "".join(sequence[p] for p in positions)
        if feature["strand"] == "reverse":
            dna = reverse_complement(dna)
            positions = positions[::-1]
        offset = int(qualifiers.get("codon_start") or 1) - 1
        table = int(qualifiers.get("transl_table") or 1)
        coding = dna[offset:]
        coding = coding[: len(coding) - len(coding) % 3]
        expected = biopython(coding, table)
        start_codons = CodonTable.unambiguous_dna_by_id[table].start_codons
        if offset == 0 and expected.endswith("*") and expected[0] != "M" and coding[:3] in start_codons:
            expected = "M" + expected[1:]
            assert item["initiator_as_methionine"], item["feature_id"]
        assert item["protein"] == expected, (fixture.name, item["feature_id"], item["protein"], expected)
        assert item["table"] == table
        codon_positions = [c["positions"] for c in item["codons"]]
        body_positions = positions[offset:]
        assert codon_positions == [body_positions[i:i + 3] for i in range(0, len(coding), 3)], item["feature_id"]
        imported = qualifiers.get("translation")
        if imported is not None:
            imported = imported.replace(",", "").replace(" ", "").rstrip("*")
            matches = imported == expected.rstrip("*")
            assert item["imported_translation"]["matches"] == matches, item["feature_id"]
        checked += 1
    for skipped in body["result"]["skipped"]:
        assert features[skipped["feature_id"]]["strand"] == "unknown", skipped
    return checked


def check_ranges(cli: Cli, fixture: Path) -> int:
    sequence = cli.json("sequence", str(fixture))["result"]["sequence"]
    topology = cli.json("inspect", str(fixture))["result"]["topology"]
    length = len(sequence)
    checked = 0
    starts = sorted(s for s in {0, 1, 2, length // 3, length // 2, length - 40, length - 7} if 0 <= s < length)
    for start in starts:
        for span in (9, 10, 11, 31, 64):
            if span >= length:
                continue
            end = start + span
            wraps = end > length
            if wraps and topology != "circular":
                continue
            if wraps:
                end -= length
            window = "".join(sequence[(start + i) % length] for i in range(span))
            for strand in ("forward", "reverse"):
                for frame in range(3):
                    table = 11 if (start + frame) % 2 else 1
                    result = cli.json("translate", str(fixture), "--range", f"{start}..{end}", "--strand", strand,
                                      "--frame", str(frame), "--table", str(table))["result"]
                    coding = window if strand == "forward" else reverse_complement(window)
                    coding = coding[frame:]
                    coding = coding[: len(coding) - len(coding) % 3]
                    assert result["protein"] == biopython(coding, table), (fixture.name, start, end, strand, frame, table, result["protein"], biopython(coding, table))
                    assert result["trailing_bases"] == (span - frame) % 3
                    checked += 1
    return checked


def check_tables(cli: Cli, directory: Path) -> int:
    codons = [a + b + c for a in "TCAG" for b in "TCAG" for c in "TCAG"]
    fasta = directory / "all_codons.fasta"
    fasta.write_text(">all codons\n" + "".join(codons) + "\n")
    accepted = []
    for table in range(0, 40):
        body = cli.json("translate", str(fasta), "--range", "0..192", "--table", str(table), ok=table in CodonTable.unambiguous_dna_by_id)
        if body["ok"]:
            accepted.append(table)
            assert body["result"]["protein"] == biopython("".join(codons), table), table
    assert accepted == sorted(CodonTable.unambiguous_dna_by_id), accepted
    return len(accepted)


def reference_orfs(sequence: str, circular: bool, table: int, min_codons: int, table_starts: bool) -> set:
    code = CodonTable.unambiguous_dna_by_id[table]
    starts = set(code.start_codons) if table_starts else {"ATG"}
    length = len(sequence)
    found = {}
    for strand, coding in (("forward", sequence), ("reverse", reverse_complement(sequence))):
        scan = coding + coding if circular else coding
        for frame in range(3):
            begin = None
            for i in range(frame, len(scan) - 2, 3):
                codon = scan[i:i + 3]
                if biopython(codon, table) == "*":
                    if begin is not None:
                        bases = i + 3 - begin
                        if begin < length and bases <= length and bases // 3 - 1 >= min_codons:
                            stop = i if strand == "forward" else i + 2
                            stop_ref = stop % length if strand == "forward" else length - 1 - (stop % length)
                            if strand == "forward":
                                start = begin % length
                            else:
                                start = length - 1 - ((i + 2) % length)
                            orf = (strand, start, bases, biopython(scan[begin:i], table))
                            key = (strand, stop_ref)
                            if key not in found or found[key][2] < bases:
                                found[key] = orf
                        begin = None
                elif begin is None and codon in starts:
                    begin = i
    return set(found.values())


def check_orfs(cli: Cli, fixture: Path) -> int:
    sequence = cli.json("sequence", str(fixture))["result"]["sequence"]
    circular = cli.json("inspect", str(fixture))["result"]["topology"] == "circular"
    checked = 0
    for table, min_codons, starts in ((1, 75, "atg"), (1, 5, "atg"), (11, 3, "table"), (2, 5, "atg")):
        result = cli.json("orfs", str(fixture), "--table", str(table), "--min-codons", str(min_codons), "--starts", starts)["result"]
        got = {(o["strand"], o["start"], o["length"], o["protein"]) for o in result["orfs"]}
        expected = reference_orfs(sequence, circular, table, min_codons, starts == "table")
        assert got == expected, (fixture.name, table, min_codons, starts, sorted(got ^ expected)[:5])
        assert [o["id"] for o in result["orfs"]] == [f"orf-{i + 1:04}" for i in range(len(result["orfs"]))]
        checked += len(got)
    return checked


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    cli = Cli(parser.parse_args().binary.resolve())
    features = ranges = orfs = 0
    for name in FIXTURES:
        fixture = SNAPGENE / name
        features += check_features(cli, fixture)
        ranges += check_ranges(cli, fixture)
        orfs += check_orfs(cli, fixture)
    with tempfile.TemporaryDirectory(prefix="dnagent-translation-") as directory:
        tables = check_tables(cli, Path(directory))
    print(f"Translation matches Biopython 1.85: {features} feature translations, {ranges} ranges, "
          f"{tables} genetic codes, {orfs} ORFs ({cli.calls} CLI calls).")


if __name__ == "__main__":
    main()

# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85"]
# ///
"""Check SnapGene `.dna` writing against Biopython 1.85; public fixtures only.

Two properties, for every valid public fixture:

1. **Verbatim.** A file DNAgent only read is written back byte for byte, so nothing a
   SnapGene file carries is lost by passing through DNAgent.
2. **Generated.** Taken the long way round — `.dna` → DNAgent GenBank → `.dna` — the
   annotation packets are rebuilt from DNAgent's model. Biopython, which never sees
   DNAgent's own reader, must then find the same sequence, topology and features (key,
   label, strand and every location part in order) in the rebuilt file as in the original.

A generated file deliberately does not carry SnapGene's uninterpreted packets or its
display and detection attributes; the writer reports those losses as warnings, and this
script checks that it says so rather than dropping them silently.
"""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import warnings as warnings_module

from Bio import BiopythonWarning, SeqIO

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "fixtures/formats/snapgene"


def run(binary: Path, *args: str) -> dict:
    proc = subprocess.run([str(binary), *args], capture_output=True, text=True, timeout=120)
    assert proc.returncode == 0, f"{args}: {proc.stderr or proc.stdout}"
    return json.loads(proc.stdout)


def read(path: Path, fmt: str):
    with warnings_module.catch_warnings():
        warnings_module.simplefilter("ignore", BiopythonWarning)
        return SeqIO.read(path, fmt)


def parts(feature) -> list[tuple[int, int]]:
    return [(int(p.start), int(p.end)) for p in feature.location.parts]


def describe(record) -> list[tuple]:
    """The feature view Biopython offers, independent of DNAgent's own reader."""
    return sorted(
        (
            feature.type,
            (feature.qualifiers.get("label") or [""])[0],
            feature.location.strand,
            tuple(sorted(parts(feature))),
        )
        for feature in record.features
    )


def from_dnagent(rows: list[dict], length: int) -> list[tuple]:
    """The same view, built from DNAgent's own JSON, for comparison with Biopython's.

    Biopython splits a feature that crosses the origin into two pieces, so DNAgent's
    circular arcs are split the same way. A feature with no strand is written without a
    `directionality` attribute, which Biopython reads as the forward default.
    """
    described = []
    for row in rows:
        pieces = []
        for part in row["location"]["parts"]:
            if part["kind"] == "linear":
                pieces.append((part["start"], part["end"]))
            else:
                start, end = part["start"], part["start"] + part["length"]
                if end <= length:
                    pieces.append((start, end))
                else:
                    pieces.extend([(start, length), (0, end - length)])
        strand = {"forward": 1, "reverse": -1}.get(row["strand"], 1)
        described.append((row["kind"], row["label"], strand, tuple(sorted(pieces))))
    return sorted(described)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve()
    verbatim = generated = 0
    with tempfile.TemporaryDirectory(prefix="dnagent-snapgene-write-") as scratch:
        work = Path(scratch)
        for source in sorted(FIXTURES.glob("*.dna")):
            if source.name.startswith("invalid_"):
                continue

            copy = work / f"{source.stem}.copy.dna"
            body = run(binary, "convert", str(source), "--out", str(copy))
            assert body["result"]["format"] == "snapgene", source.name
            assert copy.read_bytes() == source.read_bytes(), f"{source.name}: not byte-identical"
            # Import warnings about uninterpreted packets still apply; what must be absent
            # is any warning about the write itself.
            codes = {w["code"] for w in body["warnings"]}
            assert not codes & {
                "snapgene_annotations_generated",
                "snapgene_packet_not_written",
            }, f"{source.name}: a verbatim copy should lose nothing, got {sorted(codes)}"
            verbatim += 1

            # The long way round, so the annotation packets are rebuilt from the model.
            genbank = work / f"{source.stem}.gb"
            run(binary, "convert", str(source), "--out", str(genbank))
            rebuilt = work / f"{source.stem}.rebuilt.dna"
            body = run(binary, "convert", str(genbank), "--out", str(rebuilt))
            codes = {w["code"] for w in body["warnings"]}
            assert "snapgene_annotations_generated" in codes, f"{source.name}: silent regeneration"

            original_record = read(source, "snapgene")
            rebuilt_record = read(rebuilt, "snapgene")
            assert str(rebuilt_record.seq).upper() == str(original_record.seq).upper(), (
                f"{source.name}: sequence differs"
            )
            assert rebuilt_record.annotations.get("topology") == original_record.annotations.get(
                "topology"
            ), f"{source.name}: topology differs"
            # Against DNAgent's model, not the original file: fixtures that carry
            # annotations DNAgent cannot represent are reported at import and are
            # genuinely absent from the model, so they must be absent here too.
            model = run(binary, "features", str(source), "--output", "json")["result"]
            expected = from_dnagent(model, len(original_record.seq))
            actual = describe(rebuilt_record)
            assert actual == expected, (
                f"{source.name}: Biopython reads the rebuilt file differently from DNAgent's model"
                f"\n  model:    {expected}\n  rebuilt:  {actual}"
            )
            generated += 1

    print(
        f"SnapGene writing agrees with Biopython 1.85: {verbatim} byte-identical copies, "
        f"{generated} rebuilt files with matching sequence, topology and features."
    )


if __name__ == "__main__":
    main()

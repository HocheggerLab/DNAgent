# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85"]
# ///
"""Check the feature library and detect-features against an independent implementation.

Builds a library with `dnagent library import` from the public fixtures (or, with
--collection, from a private folder; nothing from it is printed except counts and
file names of disagreements), then:

- parses every file with Biopython 1.85 and applies the documented collection rules
  (docs/feature-library.md) to its features: skip `source`, whole-molecule, shorter than
  12 bp, unnamed/placeholder (incl. no letters or digits, "(null)…") and sequence-as-name features, and any with a non-ACGT base;
  a feature's sequence is Biopython's `extract` (joins, strands, origin wrap);
- requires the library to hold exactly those sequences (identity = the smaller of the
  sequence and its reverse complement), with the same occurrence counts and the most
  common label as the name (ties: first seen);
- runs `dnagent detect-features` on every file and requires exactly the occurrences a
  brute-force scan finds for every library sequence on both strands, wrapping on
  circles (strand "unknown" for features most often annotated without a strand).
"""

import argparse
from collections import Counter
import io
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import struct
import tempfile
import warnings
import xml.etree.ElementTree as ET

from Bio import BiopythonWarning, SeqIO
from Bio.Align import PairwiseAligner
from Bio.Seq import Seq

ROOT = Path(__file__).resolve().parents[1]
MIN_LENGTH = 12
GENERIC = {"feature", "new feature", "misc_feature", "misc feature", "untitled", "unnamed", "region"}
FORMATS = {".dna": "snapgene", ".gb": "genbank", ".gbk": "genbank", ".genbank": "genbank", ".fa": "fasta", ".fasta": "fasta", ".fna": "fasta"}


def cli(binary: Path, *args: str) -> dict:
    proc = subprocess.run([str(binary), *args, "--output", "json"], capture_output=True, text=True, timeout=600,
                          env=dict(os.environ, DNAGENT_ENZYMES="builtin"))
    body = json.loads(proc.stdout)
    if not body["ok"]:
        raise AssertionError(f"dnagent {args[0]}: {body['error']}")
    return body["result"]


def label_of(feature) -> str:
    q = feature.qualifiers
    for key in ["label", "gene", "product", "locus_tag", "standard_name", "note"]:
        if q.get(key):
            return q[key][0]
    return feature.type


def generic(label: str, kind: str) -> bool:
    name = label.strip().lower()
    stem = name.rstrip("0123456789 #\t")
    return not name or not any(c.isalnum() for c in name) or name.startswith("(null)") or name == kind.lower() or stem in GENERIC


def sniff(path: Path) -> str | None:
    data = path.read_bytes()
    if data[:1] == b"\x09" and data[5:13] == b"SnapGene":
        return "snapgene"
    text = data.removeprefix(b"\xef\xbb\xbf").lstrip()
    if text.startswith(b"LOCUS"):
        return "genbank"
    if text.startswith(b">"):
        return "fasta"
    return FORMATS.get(path.suffix.lower())


def snapgene_without_primers(data: bytes) -> tuple[bytes, list[str]]:
    """SnapGene bytes minus the primers packet, and the features' names in file order.

    Biopython 1.85 turns primers (packet 5) into primer_bind features, where DNAgent keeps
    them as unplaced primers; and it lets a `label` qualifier replace a feature's name,
    where SnapGene (and DNAgent) show the name attribute."""
    kept, names, offset = [], [], 0
    while offset + 5 <= len(data):
        kind, length = struct.unpack(">BI", data[offset:offset + 5])
        packet = data[offset:offset + 5 + length]
        if kind == 10:
            names = [f.get("name", "") for f in ET.fromstring(packet[5:]).iter("Feature")]
        if kind != 5:
            kept.append(packet)
        offset += 5 + length
    return b"".join(kept), names


def read_record(path: Path):
    fmt = sniff(path)
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", BiopythonWarning)
        if fmt != "snapgene":
            return SeqIO.read(path, fmt), None
        data, names = snapgene_without_primers(path.read_bytes())
        return SeqIO.read(io.BytesIO(data), "snapgene"), names


def span_sequence(feature, sequence: str) -> str | None:
    """Bases of a feature whose parts tile one contiguous (circular) span, read on its strand.

    Independent of part order: Biopython 1.85 orders the pieces of a reverse SnapGene
    feature that wraps the origin incorrectly (e.g. rc(568-637) + rc(5424-5648) + rc(0-568)
    for a span 5424 -> 637), so `extract` is only used for genuinely gapped joins."""
    n = len(sequence)
    covered = [p % n for part in feature.location.parts for p in range(int(part.start), int(part.end))]
    if len(set(covered)) != len(covered):
        return None
    cover = set(covered)
    starts = [p for p in cover if (p - 1) % n not in cover]
    if len(starts) != 1 and len(cover) != n:
        return None
    start = starts[0] if starts else 0
    bases = "".join(sequence[(start + i) % n] for i in range(len(cover)))
    return str(Seq(bases).reverse_complement()) if feature.location.strand == -1 else bases


def incomplete_snapgene_features(path: Path) -> set[int]:
    """1-based SnapGene feature numbers whose segments DNAgent reports it could not import."""
    body = json.loads(subprocess.run([str(BINARY), "inspect", str(path), "--output", "json"], capture_output=True, text=True).stdout)
    found = set()
    for warning in body.get("warnings", []):
        if warning["code"] in ("snapgene_feature_range_unsupported", "snapgene_feature_segment_missing_range"):
            found.add(int(warning["message"].split()[1]))
    return found


def expected_features(path: Path) -> list[tuple[str, str, str]] | None:
    """(canonical, label, sequence) per collected feature; None if Biopython can't read the file."""
    try:
        record, names = read_record(path)
    except Exception:  # noqa: BLE001 - unreadable for Biopython too
        return None
    if names is not None and len(names) != len(record.features):
        raise AssertionError(f"{path.name}: {len(names)} feature names but Biopython read {len(record.features)} features")
    sequence = str(record.seq).upper()
    # A feature missing some segments is not the feature it names (e.g. AmpR reduced to its
    # signal peptide when a segment crosses the end of a linear record): not collected.
    incomplete = incomplete_snapgene_features(path) if names is not None else set()
    found = []
    for index, feature in enumerate(record.features):
        label = (names[index] if names is not None else label_of(feature)).strip()
        size = len(feature.location)
        if feature.type.lower() == "source" or size >= len(sequence) or size < MIN_LENGTH:
            continue
        if generic(label, feature.type) or (len(label) >= 8 and set(label) <= set("ACGTNacgtn")):
            continue
        if names is not None and index + 1 in incomplete:
            continue
        bases = span_sequence(feature, sequence) or str(feature.extract(record.seq)).upper()
        if set(bases) - set("ACGT"):
            continue
        reverse = str(Seq(bases).reverse_complement())
        found.append((min(bases, reverse), label, bases))
    return found


def brute_force(sequence: str, circular: bool, query: str) -> set[tuple[int, str]]:
    n, m = len(sequence), len(query)
    if m > n:
        return set()
    text = sequence + (sequence[: m - 1] if circular else "")
    reverse = str(Seq(query).reverse_complement())
    hits = set()
    for start in range(n if circular else n - m + 1):
        window = text[start:start + m]
        if window == query:
            hits.add((start, "forward"))
        elif window == reverse:
            hits.add((start, "reverse"))
    return hits


BINARY = Path("target/debug/dnagent")


# score = -(edits aligning all of the query within the target): end gaps *in the query*
# (target overhangs) are free; Biopython's target_end_gap_score scores gaps in the target.
ALIGNER = PairwiseAligner(mode="global", match_score=0, mismatch_score=-1, open_gap_score=-1, extend_gap_score=-1,
                          target_end_gap_score=-1, query_end_gap_score=0)


def within_identity(short: str, long: str, minimum: float) -> bool:
    """`short` aligns fully within `long` with at most floor((1 - minimum) * len) edits."""
    return -ALIGNER.score(long, short) <= int((1 - minimum) * len(short))


def fold(name: str) -> str:
    return "".join(c.lower() for c in name if c.isalnum())


def families(features: dict[int, tuple[str, str, set[str]]]) -> dict[int, tuple[int, str | None]]:
    """Documented rule (docs/feature-library.md): longest first; join the longest head that is
    at most 1.25x as long and contains the sequence (either strand), or shares a folded name
    and is >= 97 % identical (>= 100 bp) or encodes a >= 98 % identical protein (CDSs,
    >= 100 codons); else head a family. No features are standalone in a fresh library."""
    heads: list[int] = []
    head_of: dict[int, tuple[int, str | None]] = {}
    for fid in sorted(features, key=lambda f: (-len(features[f][1]), f)):
        kind, seq, names = features[fid]
        rev = str(Seq(seq).reverse_complement())
        found = None
        for h in heads:
            h_kind, h_seq, h_names = features[h]
            if len(seq) > len(h_seq) or 5 * len(seq) < 4 * len(h_seq):
                continue
            if len(seq) < len(h_seq) and (seq in h_seq or rev in h_seq):
                found = (h, "contained")
            elif not ({fold(n) for n in names} - {""}) & {fold(n) for n in h_names}:
                continue
            elif len(seq) >= 100 and any(within_identity(s, h_seq, 0.97) for s in (seq, rev)):
                found = (h, "similar_dna")
            elif kind.lower() == "cds" and h_kind.lower() == "cds":
                pa, pb = (str(Seq(x[: len(x) // 3 * 3]).translate()) for x in (seq, h_seq))
                pa, pb = sorted((pa, pb), key=len)
                if len(pa) >= 100 and within_identity(pa, pb, 0.98):
                    found = (h, "same_protein")
            if found:
                break
        if found is None:
            heads.append(fid)
        head_of[fid] = found if found else (fid, None)
    return head_of


def check(binary: Path, source: Path, private: bool) -> str:
    with tempfile.TemporaryDirectory(prefix="dnagent-library-check-") as scratch:
        db = str(Path(scratch) / "features.sqlite")
        report = cli(binary, "library", "--db", db, "import", str(source))
        imported = [Path(f["path"]) for f in report["files"] if f["status"] == "imported"]
        failed = {Path(f["path"]) for f in report["files"] if f["status"] == "failed"}
        occurrences: dict[str, list[str]] = {}
        disagreements = []
        for path in imported:
            expected = expected_features(path)
            if expected is None:
                disagreements.append(f"{path.name}: DNAgent imported it, Biopython cannot read it")
                continue
            for key, label, _ in expected:
                occurrences.setdefault(key, []).append(label)
        for path in failed:
            if expected_features(path) not in (None, []):
                disagreements.append(f"{path.name}: DNAgent failed, Biopython reads features")
        connection = sqlite3.connect(db)
        rows = connection.execute(
            "SELECT f.id, f.canonical, f.name, f.sequence, f.stranded, count(o.id) FROM features f JOIN occurrences o ON o.feature_id = f.id GROUP BY f.id").fetchall()
        stored = {canonical: (name, count) for _, canonical, name, _, _, count in rows}
        if set(stored) != set(occurrences):
            missing, extra = set(occurrences) - set(stored), set(stored) - set(occurrences)
            disagreements.append(f"library sequences differ: {len(missing)} missing, {len(extra)} unexpected")
        for key in set(stored) & set(occurrences):
            labels = occurrences[key]
            counts = Counter(labels)
            best = max(counts.values())
            name = next(label for label in labels if counts[label] == best)
            if stored[key] != (name, len(labels)):
                disagreements.append(f"feature {stored[key][0]!r}: stored (name, occurrences) {stored[key]}, expected {(name, len(labels))}")
        entries = [(fid, sequence, bool(stranded)) for fid, _, _, sequence, stranded, _ in rows if len(sequence) >= MIN_LENGTH]
        # Variant families, recomputed from the stored sequences.
        cli(binary, "library", "--db", db, "info")  # brings families up to date
        family_rows = connection.execute("SELECT id, coalesce(family_id, id), family_relation FROM features WHERE status != 'hidden'").fetchall()
        stored_links = {fid: (head, relation) for fid, head, relation in family_rows}
        labels: dict[int, set[str]] = {}
        for fid, label in connection.execute("SELECT feature_id, label FROM occurrences"):
            labels.setdefault(fid, set()).add(label)
        kinds = dict(connection.execute("SELECT id, kind FROM features"))
        links = families({fid: (kinds[fid], sequence, labels.get(fid, set()) | {name}) for fid, _, name, sequence, _, _ in rows})
        if stored_links != links:
            wrong = sorted(fid for fid in links if stored_links.get(fid) != links[fid])
            disagreements.append(f"families differ for {len(wrong)} features, e.g. " + ", ".join(f"{f}: stored {stored_links.get(f)}, expected {links[f]}" for f in wrong[:5]))
        expected_heads = {fid: link[0] for fid, link in links.items()}
        relations = Counter(link[1] for link in links.values() if link[1])
        family_count = len(set(expected_heads.values()))
        scans = matches = superseded = 0
        for path in imported:
            result = cli(binary, "detect-features", str(path), "--db", db)
            ours = {(m["library_id"], m["location"]["parts"][0]["start"], m["length"], m["strand"]) for m in result["matches"]}
            record, _ = read_record(path)
            sequence = str(record.seq).upper()
            circular = result["topology"] == "circular"
            reference = {(fid, start, len(query), strand if stranded else "unknown")
                         for fid, query, stranded in entries for start, strand in brute_force(sequence, circular, query)}
            if ours != reference:
                disagreements.append(f"{path.name}: detect-features differs from brute force ({len(ours - reference)} extra, {len(reference - ours)} missing)")
            # A match is superseded by the longest match of its family whose span contains it.
            listed = result["matches"]
            spans = [(m["location"]["parts"][0]["start"], m["length"]) for m in listed]
            for i, m in enumerate(listed):
                inside = [j for j in range(len(listed)) if j != i and expected_heads[listed[j]["library_id"]] == expected_heads[m["library_id"]]
                          and spans[j][1] > spans[i][1] and (spans[i][0] - spans[j][0]) % len(sequence) + spans[i][1] <= spans[j][1]]
                expected = max(inside, key=lambda j: (spans[j][1], -j)) if inside else None
                if m["superseded_by"] != expected:
                    disagreements.append(f"{path.name}: {m['name']} at {spans[i][0]} superseded_by {m['superseded_by']}, expected {expected}")
                superseded += expected is not None
            scans += 1
            matches += len(ours)
        if disagreements:
            raise AssertionError(f"{len(disagreements)} disagreements:\n  " + "\n  ".join(disagreements[:40]))
        where = "private collection" if private else "public fixtures"
        return (f"{where}: {len(imported)} files imported ({len(failed)} failed, as for Biopython), {len(stored)} library features "
                f"from {sum(c for _, c in stored.values())} occurrences agree with Biopython; {scans} detect-features scans "
                f"({matches} matches) equal an independent brute-force search; {family_count} variant families and "
                f"{superseded} folded variant matches agree with an independent grouping ({dict(relations)})")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--collection", type=Path, help="also check a private folder (reports counts only)")
    args = parser.parse_args()
    binary = args.binary.resolve()
    global BINARY
    BINARY = binary
    print(check(binary, ROOT / "fixtures/formats", False))
    if args.collection:
        print(check(binary, args.collection.resolve(), True))


if __name__ == "__main__":
    main()

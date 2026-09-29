# /// script
# requires-python = ">=3.11"
# dependencies = ["biopython==1.85"]
# ///
"""Check DNAgent's enzyme catalogue and site finding against Biopython 1.85 Bio.Restriction.

Runs for the built-in set (always) and for the installed REBASE catalogue (if present).
For every enzyme Biopython also knows with the same recognition sequence:
- overhang length and polarity must agree (except enzymes cutting upstream of their site,
  where Biopython 1.85 is off by one; see the note in check_catalogue);
- on synthetic sequences carrying every site in both orientations (degenerate positions
  filled randomly), linear and circular, the set of top-strand cut positions reported by
  `dnagent sites` must equal an independent brute-force scan written here, and must equal
  Biopython's `search` except for two documented Biopython behaviours: it omits a nominal
  cut exactly at a linear molecule's end, and at a start where a degenerate site matches
  in both orientations it keeps only the forward one (regex alternation), e.g. MspJI CNNR
  at CAAG. Every such difference is checked against that explanation.
Built-in enzymes must all be known to Biopython with the same site. Biopython 1.85 bundles
REBASE release 404; REBASE enzymes whose definition changed since, or that are new, are
counted and reported, not compared.
"""

import argparse
import json
import os
from pathlib import Path
import random
import subprocess
import tempfile

from Bio.Restriction import Restriction as R
from Bio.Restriction.Restriction_Dictionary import rest_dict
from Bio.Seq import Seq

IUPAC = {"A": "A", "C": "C", "G": "G", "T": "T", "R": "AG", "Y": "CT", "S": "CG", "W": "AT", "K": "GT", "M": "AC",
         "B": "CGT", "D": "AGT", "H": "ACT", "V": "ACG", "N": "ACGT"}


def cli(binary: Path, env: dict, *args: str) -> dict:
    flag = [] if args[0] == "enzyme-catalogue" else ["--output", "json"]  # always JSON
    proc = subprocess.run([str(binary), *args, *flag], capture_output=True, text=True, env=env, timeout=300)
    body = json.loads(proc.stdout)
    if not body["ok"]:
        raise AssertionError(f"dnagent {' '.join(args[:2])}: {body['error']}")
    return body


def genbank(path: Path, sequence: str, circular: bool) -> None:
    lines = [f"LOCUS       test {len(sequence):>20} bp    DNA     {'circular' if circular else 'linear':<8} SYN 01-JAN-2026", "FEATURES             Location/Qualifiers", "ORIGIN"]
    for i in range(0, len(sequence), 60):
        lines.append(f"{i + 1:>9} " + " ".join(sequence[i + j:i + j + 10].lower() for j in range(0, 60, 10)))
    path.write_text("\n".join(lines) + "\n//\n")


def matches(pattern: str, sequence: str, start: int) -> bool:
    n = len(sequence)
    return all(sequence[(start + i) % n] in IUPAC[b] for i, b in enumerate(pattern))


def brute_force(enzyme: dict, sequence: str, circular: bool) -> tuple[set[int], set[int]]:
    """Top-strand cut positions (1-based, first base after the cut) from both orientations,
    and the subset contributed only by reverse matches at starts that also match forward."""
    site = enzyme["recognition_sequence"]
    reverse = str(Seq(site).reverse_complement())
    n, m = len(sequence), len(site)
    top, bottom = enzyme["top_cut_offset"], enzyme["bottom_cut_offset"]
    cuts, shadowed = set(), set()
    for start in range(n if circular else n - m + 1):
        forward_hit = matches(site, sequence, start)
        hits = [(True, forward_hit), (False, reverse != site and matches(reverse, sequence, start))]
        for is_forward, hit in hits:
            if not hit:
                continue
            boundary = start + (top if is_forward else m - bottom)
            other = start + (bottom if is_forward else m - top)
            if circular:
                position = boundary % n + 1
            elif 0 <= boundary <= n and 0 <= other <= n:
                position = boundary + 1
            else:
                continue
            cuts.add(position)
            if not is_forward and forward_hit:
                shadowed.add(position)
    return cuts, shadowed


def build_sequence(enzymes: list[dict], rng: random.Random) -> str:
    parts = []
    for enzyme in enzymes:
        site = "".join(rng.choice(IUPAC[b]) for b in enzyme["recognition_sequence"])
        for orientation in (site, str(Seq(site).reverse_complement())):
            parts.append("".join(rng.choice("ACGT") for _ in range(40)))
            parts.append(orientation)
    parts.append("".join(rng.choice("ACGT") for _ in range(60)))
    return "".join(parts)


def check_catalogue(binary: Path, env: dict, label: str, require_all: bool, directory: Path) -> str:
    catalogue = cli(binary, env, "enzyme-catalogue")["result"]
    enzymes = cli(binary, env, "enzymes")["result"]
    comparable, unknown, changed = [], [], []
    for enzyme in enzymes:
        name = enzyme["name"]
        theirs = rest_dict.get(name)
        if theirs is None:
            unknown.append(name)
        elif theirs["site"] != enzyme["recognition_sequence"] or theirs["scd5"] is not None:
            changed.append(name)
        else:
            comparable.append(enzyme)
    if require_all and (unknown or changed):
        raise AssertionError(f"{label}: built-in enzymes unknown to Biopython {unknown} or with a different site {changed}")
    upstream = 0
    for enzyme in comparable:
        delta = enzyme["bottom_cut_offset"] - enzyme["top_cut_offset"]
        ovhg = getattr(R, enzyme["name"]).ovhg
        if min(enzyme["top_cut_offset"], enzyme["bottom_cut_offset"]) < 0:
            # Cuts upstream of the site: Biopython's ovhg is one too large (REBASE numbering has
            # no residue 0; e.g. TspRI NNCASTGNN^ leaves a 9-nt 3' overhang per NEB/REBASE, Biopython
            # says 10). Cut positions are still compared below.
            upstream += 1
            continue
        # Biopython: negative ovhg = 5' overhang; DNAgent: bottom after top = 5' overhang.
        assert abs(ovhg) == abs(delta) and (ovhg < 0) == (delta > 0) and (ovhg == 0) == (delta == 0), (enzyme["name"], ovhg, delta)
    rng = random.Random(20260929)
    names = [e["name"] for e in comparable]
    sequence = build_sequence(comparable, rng)
    compared = 0
    explained = 0
    mismatches = []
    for circular in (False, True):
        path = directory / f"{label}-{'circular' if circular else 'linear'}.gb"
        genbank(path, sequence, circular)
        ours: dict[str, set[int]] = {name: set() for name in names}
        for start in range(0, len(names), 150):
            batch = names[start:start + 150]
            for site in cli(binary, env, "sites", str(path), "--enzymes", ",".join(batch))["result"]["sites"]:
                if site["cleavage_available"]:  # both strand cuts inside the molecule
                    ours[site["enzyme"]].add((site["top_cut"] % len(sequence) if circular else site["top_cut"]) + 1)
        for name in names:
            enzyme = next(e for e in comparable if e["name"] == name)
            reference, shadowed = brute_force(enzyme, sequence, circular)
            if ours[name] != reference:
                mismatches.append((name, "brute force", sorted(ours[name] ^ reference)[:4]))
            found = getattr(R, name).search(Seq(sequence), linear=not circular)
            theirs = {((p - 1) % len(sequence)) + 1 for p in found}
            ends = set() if circular else {1, len(sequence) + 1}
            unexplained = (ours[name] - theirs) - ends - shadowed
            if unexplained or theirs - ours[name]:
                mismatches.append((name, "circular" if circular else "linear", sorted(unexplained)[:4], sorted(theirs - ours[name])[:4]))
            explained += len((ours[name] - theirs) & (ends | shadowed))
            compared += 1
    if mismatches:
        raise AssertionError(f"{label}: {len(mismatches)} cut-position mismatches:\n" + "\n".join(map(str, mismatches)))
    return (f"{label} ({catalogue['source']['name']} {catalogue['source']['version']}): {len(comparable)} enzymes agree with Biopython "
            f"({compared} site scans, {len(sequence)} bp test sequence); {len(changed)} changed since Biopython's REBASE, "
            f"{len(unknown)} unknown to it, {len(catalogue['unsupported'])} unsupported by DNAgent; "
            f"overhang not compared for {upstream} upstream-cutting enzymes (Biopython off-by-one); "
            f"{explained} extra DNAgent cuts explained by Biopython's end/alternation behaviour, all confirmed by brute force")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve()
    with tempfile.TemporaryDirectory(prefix="dnagent-enzymes-") as name:
        directory = Path(name)
        builtin = dict(os.environ, DNAGENT_ENZYMES="builtin")
        print(check_catalogue(binary, builtin, "built-in", True, directory))
        installed = dict(os.environ)
        installed.pop("DNAGENT_ENZYMES", None)
        if cli(binary, installed, "enzyme-catalogue")["result"]["source"]["name"] == "REBASE":
            print(check_catalogue(binary, installed, "installed", False, directory))
        else:
            print("installed: no REBASE catalogue installed (python3 scripts/manage_enzymes.py install); skipped")


if __name__ == "__main__":
    main()

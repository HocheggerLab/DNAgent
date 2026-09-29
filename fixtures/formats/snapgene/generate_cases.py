"""Generate additional synthetic fixtures. No lab sequences or metadata are used.

Run with Python 3.11+. Files are deterministic and covered by the repository MIT
licence. Existing synthetic_circular.dna and its JSON contracts are untouched.
"""

from pathlib import Path
import random
import struct

ROOT = Path(__file__).resolve().parent


def packet(kind, payload):
    return struct.pack(">BI", kind, len(payload)) + payload


COOKIE = packet(9, b"SnapGene\x00\x01\x00\x01\x00\x01")


def molecule(sequence, circular=False):
    return COOKIE + packet(0, bytes([int(circular)]) + sequence.encode("ascii"))


def cases():
    motifs = [
        "GAATTC",
        "GGATCC",
        "GATATC",
        "GGTACC",
        "GGTCTC",
        "GAGACC",
        "CGTCTC",
        "GAGACG",
    ]
    yield (
        "synthetic_restriction_linear.dna",
        molecule("A" * 12 + ("A" * 12).join(motifs) + "A" * 12),
    )
    yield (
        "synthetic_restriction_circular.dna",
        molecule("TTC" + "A" * 20 + "GAA", circular=True),
    )
    yield "synthetic_restriction_end.dna", molecule("GGTCTC")
    linear = molecule("acgtryswkmbdhvn")
    linear += packet(
        10,
        b"""<Features><Feature name="reverse multipart" type="CDS" directionality="2"><Segment range="2-4" color="#123456"/><Segment range="10-14"/><Q name="note"><V text="first"/><V text="second"/></Q><Q name="pseudo"/></Feature></Features>""",
    )
    linear += packet(
        5,
        b"""<Primers><Primer name="synthetic primer" sequence="acgtn" description="retained description"/></Primers>""",
    )
    yield "synthetic_linear.dna", linear
    yield "synthetic_unannotated.dna", molecule("ACGT")
    origin = molecule("ACGTACGTACGT", circular=True)
    origin += packet(
        10,
        b"""<Features><Feature name="multipart wrap" directionality="2"><Segment range="11-2"/><Segment range="5-6"/></Feature></Features>""",
    )
    yield "synthetic_multipart_origin.dna", origin
    partial = molecule("ACGTACGTACGT")
    partial += packet(
        10,
        b"""<Features><Feature name="partial"><Segment range="1-3"/><Segment range="10-99"/></Feature><Feature name="unsupported"><Segment range="0-2"/></Feature></Features>""",
    )
    partial += packet(
        5,
        b"""<Primers><Primer name="valid" sequence="ACGT"/><Primer name="invalid" sequence="AC!T"/></Primers>""",
    )
    partial += packet(6, b"<Notes><Custom>synthetic opaque notes</Custom></Notes>")
    partial += packet(0x1C, b"\x00\xffopaque")
    yield "synthetic_partial.dna", partial
    # Three hand-authored annotations sharing zero-based bases 11 and 12, for
    # GUI click-cycling tests. Arbitrary sequence without restriction motifs.
    overlaps = molecule("ATGACCGTTAGCCTAGCATTCATTGCAGTC")
    overlaps += packet(
        10,
        b"""<Features><Feature name="overlap forward" type="misc_feature" directionality="1"><Segment range="6-15"/></Feature><Feature name="overlap reverse" type="misc_feature" directionality="2"><Segment range="11-20"/></Feature><Feature name="overlap multipart" type="misc_feature" directionality="1"><Segment range="2-4"/><Segment range="12-13"/></Feature></Features>""",
    )
    yield "synthetic_overlaps.dna", overlaps
    yield "pUC19_M77789.dna", puc19()
    yield "synthetic_translation.dna", translation_case()
    yield "invalid_duplicate_sequence.dna", molecule("ACGT") + packet(0, b"\x00TGCA")
    yield "invalid_missing_sequence.dna", COOKIE
    yield "invalid_truncated.dna", COOKIE + struct.pack(">BI", 0, 8) + b"\x00AC"
    yield (
        "invalid_feature_xml.dna",
        molecule("ACGT") + packet(10, b"<Features><Feature"),
    )


# Recognition sequences of the polylinker enzymes named in the M77789.2 note.
PUC19_SITES = [
    ("HindIII", "AAGCTT"), ("SphI", "GCATGC"), ("PstI", "CTGCAG"), ("SalI", "GTCGAC"),
    ("XbaI", "TCTAGA"), ("BamHI", "GGATCC"), ("SmaI", "CCCGGG"), ("KpnI", "GGTACC"),
    ("SacI", "GAGCTC"), ("EcoRI", "GAATTC"),
]


def puc19():
    """Public pUC19 (NCBI GenBank M77789.2) for realistic-size map layout tests.

    The seven misc_features are transcribed from the record's feature table
    (one-based, inclusive). Site features are located by exact, unique
    recognition-sequence match. Colours are display-only choices.
    """
    fasta = (ROOT.parent / "fasta" / "pUC19_M77789.fasta").read_text().splitlines()
    sequence = "".join(line for line in fasta if not line.startswith(">"))
    assert len(sequence) == 2686
    record = [
        ("M13mp19", "1-447", None, None),
        ("Lac-operon", "1-230", None, "#b1ff67"),
        ("polylinker of M13mp19", "233-289", None, "#ffef86"),
        ("Lac-Operon", "290-447", None, "#b1ff67"),
        ("pBR322", "448-547", "2", "#ffffff"),
        ("pBR322", "548-684", "2", "#c6c9d1"),
        ("pBR322", "685-2686", "2", "#ffd4a1"),
    ]
    features = []
    for name, segment, direction, color in record:
        strand = f' directionality="{direction}"' if direction else ""
        colour = f' color="{color}"' if color else ""
        features.append(
            f'<Feature name="{name}" type="misc_feature"{strand}>'
            f'<Segment range="{segment}"{colour}/>'
            '<Q name="note"><V text="transcribed from GenBank M77789.2"/></Q></Feature>'
        )
    for enzyme, motif in PUC19_SITES:
        hits = [i for i in range(len(sequence)) if sequence.startswith(motif, i)]
        assert len(hits) == 1 and 232 <= hits[0] and hits[0] + 6 <= 289, (enzyme, hits)
        start = hits[0] + 1
        features.append(
            f'<Feature name="{enzyme} site" type="misc_feature">'
            f'<Segment range="{start}-{start + 5}"/></Feature>'
        )
    xml = "<Features>" + "".join(features) + "</Features>"
    return molecule(sequence, circular=True) + packet(10, xml.encode("ascii"))



def reverse_complement(sequence):
    return sequence[::-1].translate(str.maketrans("ACGTN", "TGCAN"))


def translation_case():
    """Hand-designed CDS layouts for translation tests (synthetic, deterministic).

    Covers a >=75-codon forward ORF, a reverse two-part CDS whose junction splits a
    codon, an origin-spanning CDS, codon_start 2, a table-11 GTG start, ambiguous bases
    with a deliberately wrong imported translation, and an unknown-strand CDS.
    """
    rng = random.Random(20260929)
    codons = [a + b + c for a in "TCAG" for b in "TCAG" for c in "TCAG"]
    sense = [c for c in codons if c not in ("TAA", "TAG", "TGA", "ATG")]

    def orf(n):
        return "".join(rng.choice(sense) for _ in range(n))

    def filler(n):
        return "".join(rng.choice("ACGT") for _ in range(n))

    origin_cds = "ATG" + orf(6) + "TAA"  # 24 bp, split 12 | 12 across the origin
    reverse_mrna = "ATG" + orf(10) + "TGA"  # 36 bp
    reverse_rc = reverse_complement(reverse_mrna)
    pieces = [
        ("origin_tail", origin_cds[12:]),
        ("f", filler(20)),
        ("forward", "ATG" + orf(80) + "TAA"),
        ("f", filler(20)),
        ("reverse_1", reverse_rc[:14]),
        ("intron", filler(9)),
        ("reverse_2", reverse_rc[14:]),
        ("f", filler(20)),
        ("shifted", "C" + "ATG" + orf(8) + "TAG"),
        ("f", filler(20)),
        ("gtg", "GTG" + orf(8) + "TAA"),
        ("f", filler(20)),
        ("ambiguous", "ATG" + orf(2) + "CTNAANNNN" + orf(2) + "TAA"),
        ("f", filler(20)),
        ("unknown", "ATG" + orf(5) + "TAA"),
        ("f", filler(20)),
        ("origin_head", origin_cds[:12]),
    ]
    spans = {}
    sequence = ""
    for name, piece in pieces:
        spans[name] = (len(sequence), len(sequence) + len(piece))
        sequence += piece
    length = len(sequence)

    def one_based(name):
        start, end = spans[name]
        return f"{start + 1}-{end}"

    def q(key, value):
        return f'<Q name="{key}"><V text="{value}"/></Q>'

    def translate(dna):
        table = {c: aa for c, aa in zip(codons, "FFLLSSSSYY**CC*WLLLLPPPPHHQQRRRRIIIMTTTTNNKKSSRRVVVVAAAADDEEGGGG")}
        return "".join(table.get(dna[i:i + 3], "X") for i in range(0, len(dna) - 2, 3))

    forward_protein = translate(spans and sequence[slice(*spans["forward"])]).rstrip("*")
    reverse_protein = translate(reverse_mrna).rstrip("*")
    gtg_protein = "M" + translate(sequence[slice(*spans["gtg"])])[1:].rstrip("*")
    features = [
        f'<Feature name="forward CDS" type="CDS" directionality="1"><Segment range="{one_based("forward")}" color="#b1ff67"/>'
        + q("codon_start", 1) + q("transl_table", 1) + q("translation", forward_protein) + "</Feature>",
        f'<Feature name="reverse split CDS" type="CDS" directionality="2"><Segment range="{one_based("reverse_1")}" color="#9eafd2"/>'
        f'<Segment range="{one_based("reverse_2")}" color="#9eafd2"/>'
        + q("translation", reverse_protein[:4] + "," + reverse_protein[4:]) + "</Feature>",
        f'<Feature name="origin CDS" type="CDS" directionality="1"><Segment range="{length - 11}-{spans["origin_tail"][1]}" color="#ffef86"/></Feature>',
        f'<Feature name="codon_start 2 CDS" type="CDS" directionality="1"><Segment range="{one_based("shifted")}"/>' + q("codon_start", 2) + "</Feature>",
        f'<Feature name="bacterial GTG CDS" type="CDS" directionality="1"><Segment range="{one_based("gtg")}"/>'
        + q("transl_table", 11) + q("translation", gtg_protein) + "</Feature>",
        f'<Feature name="ambiguous CDS" type="CDS" directionality="1"><Segment range="{one_based("ambiguous")}"/>'
        + q("translation", "MWRONG") + "</Feature>",
        f'<Feature name="unknown-strand CDS" type="CDS"><Segment range="{one_based("unknown")}"/></Feature>',
    ]
    xml = "<Features>" + "".join(features) + "</Features>"
    return molecule(sequence, circular=True) + packet(10, xml.encode("ascii"))


if __name__ == "__main__":
    for name, data in cases():
        (ROOT / name).write_bytes(data)
        print(name, len(data))

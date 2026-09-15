"""Generate additional synthetic fixtures. No lab sequences or metadata are used.

Run with Python 3.11+. Files are deterministic and covered by the repository MIT
licence. Existing synthetic_circular.dna and its JSON contracts are untouched.
"""

from pathlib import Path
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
    yield "invalid_duplicate_sequence.dna", molecule("ACGT") + packet(0, b"\x00TGCA")
    yield "invalid_missing_sequence.dna", COOKIE
    yield "invalid_truncated.dna", COOKIE + struct.pack(">BI", 0, 8) + b"\x00AC"
    yield (
        "invalid_feature_xml.dna",
        molecule("ACGT") + packet(10, b"<Features><Feature"),
    )


if __name__ == "__main__":
    for name, data in cases():
        (ROOT / name).write_bytes(data)
        print(name, len(data))

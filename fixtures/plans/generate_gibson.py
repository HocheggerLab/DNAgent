"""Regenerate the deterministic synthetic-original Gibson example template."""

from pathlib import Path
import struct


def main():
    state = 1_234_567
    sequence = []
    for _ in range(300):
        state = (state ^ (state << 13)) & ((1 << 64) - 1)
        state ^= state >> 7
        state = (state ^ (state << 17)) & ((1 << 64) - 1)
        sequence.append("ACGT"[state % 4])

    def packet(kind, value):
        return struct.pack(">BI", kind, len(value)) + value

    features = b'<Features><Feature name="synthetic full span" type="misc_feature" directionality="1"><Segment range="1-300" type="standard"/></Feature><Feature name="synthetic seam" type="misc_feature" directionality="2"><Segment range="281-15" type="standard"/></Feature></Features>'
    path = Path(__file__).parent / "synthetic_gibson.dna"
    path.write_bytes(
        packet(9, b"SnapGene\0\1\0\1\0\1")
        + packet(0, b"\1" + "".join(sequence).encode())
        + packet(10, features)
    )


if __name__ == "__main__":
    main()

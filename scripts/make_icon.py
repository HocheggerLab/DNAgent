# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow==11.0.0"]
# ///
"""Draw the DNAgent app icon source: a double helix on a rounded square.

Tauri generates every platform size from one 1024x1024 PNG. This script is that
source, kept in the repo so the icon can be regenerated or adjusted rather than
being an opaque binary. Run it, then `npx tauri icon` in `desktop/`.

Usage:
    uv run scripts/make_icon.py [--out desktop/src-tauri/icons/source.png]
"""

from __future__ import annotations

import argparse
import math
from pathlib import Path

from PIL import Image, ImageDraw

SIZE = 1024
SUPERSAMPLE = 4
BACKGROUND = (45, 111, 210)  # --accent, the app's own blue
STRAND = (255, 255, 255)
RUNG = (176, 206, 247)
TURNS = 2.0
STRANDS = 2
RUNGS = 11


def helix_points(width: int, height: int, phase: float, samples: int = 1200) -> list[tuple[float, float]]:
    """One strand: a sine wave down the icon, in supersampled pixel coordinates.

    Args:
        width: drawing width.
        height: drawing height.
        phase: radians offset; the two strands are half a turn apart.
        samples: points along the strand.

    Returns:
        Points from top to bottom.
    """
    amplitude = width * 0.24
    centre = width / 2
    top, bottom = height * 0.17, height * 0.83
    return [
        (
            centre + amplitude * math.sin(2 * math.pi * TURNS * (i / (samples - 1)) + phase),
            top + (bottom - top) * (i / (samples - 1)),
        )
        for i in range(samples)
    ]


def draw(size: int) -> Image.Image:
    """Render the icon at `size`, supersampled then reduced for smooth edges."""
    s = size * SUPERSAMPLE
    image = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    canvas = ImageDraw.Draw(image)
    # macOS masks its own corners, but a rounded source looks right everywhere else too.
    canvas.rounded_rectangle([0, 0, s - 1, s - 1], radius=int(s * 0.22), fill=BACKGROUND)

    strands = [helix_points(s, s, phase=i * math.pi) for i in range(STRANDS)]
    # Rungs first, so the strands are drawn over their ends. Those near a crossing are
    # skipped: there the strands meet and a rung would be a dot.
    rung_width = int(s * 0.014)
    for i in range(1, RUNGS + 1):
        at = int(len(strands[0]) * i / (RUNGS + 1))
        a, b = strands[0][at], strands[1][at]
        if abs(a[0] - b[0]) < s * 0.16:
            continue
        canvas.line([a, b], fill=RUNG, width=rung_width)
    # A round brush along the path: `line` on a dense polyline leaves ragged edges,
    # because PIL draws each segment as a rectangle with no join.
    radius = s * 0.021
    for strand in strands:
        for x, y in strand:
            canvas.ellipse([x - radius, y - radius, x + radius, y + radius], fill=STRAND)

    return image.resize((size, size), Image.LANCZOS)


def main() -> None:
    """Write the icon source PNG."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path("desktop/src-tauri/icons/source.png"))
    parser.add_argument("--size", type=int, default=SIZE)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    draw(args.size).save(args.out)
    print(f"{args.out}  {args.size}x{args.size}")


if __name__ == "__main__":
    main()

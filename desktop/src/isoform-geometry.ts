// Axis for the isoform view: locus positions to screen x, true to scale or with introns
// compressed. Pure display arithmetic; positions are zero-based on the locus.

export interface Span { start: number; length: number }

/** A piecewise-linear scale over [viewStart, viewEnd) of the locus. */
export interface AxisScale {
  toX(position: number): number;
  fromX(x: number): number;
  /** Compressed stretches (for drawing break marks), in locus positions. */
  breaks: { start: number; end: number }[];
}

/** Stretches of the locus covered by no exon of any isoform (the introns and flanks). */
export function uncovered(length: number, exons: Span[]): { start: number; end: number }[] {
  const sorted = [...exons].sort((a, b) => a.start - b.start);
  const gaps: { start: number; end: number }[] = [];
  let at = 0;
  for (const e of sorted) {
    if (e.start > at) gaps.push({ start: at, end: e.start });
    at = Math.max(at, e.start + e.length);
  }
  if (at < length) gaps.push({ start: at, end: length });
  return gaps;
}

/**
 * Scale from locus positions to x in [x0, x1]. With `compress`, every uncovered stretch
 * longer than `gapBases` is drawn as `gapBases` wide (in base units), so exons dominate.
 */
export function axisScale(
  length: number, exons: Span[], compress: boolean, viewStart: number, viewEnd: number,
  x0: number, x1: number, gapBases = 60,
): AxisScale {
  // Segments of (locus start, locus end, weight per base).
  const knots: { start: number; end: number; weight: number }[] = [];
  const breaks: { start: number; end: number }[] = [];
  if (compress) {
    let at = 0;
    for (const gap of uncovered(length, exons)) {
      if (gap.start > at) knots.push({ start: at, end: gap.start, weight: 1 });
      const size = gap.end - gap.start;
      if (size > gapBases) {
        knots.push({ start: gap.start, end: gap.end, weight: gapBases / size });
        breaks.push(gap);
      } else {
        knots.push({ start: gap.start, end: gap.end, weight: 1 });
      }
      at = gap.end;
    }
    if (at < length) knots.push({ start: at, end: length, weight: 1 });
  } else {
    knots.push({ start: 0, end: length, weight: 1 });
  }
  // Cumulative display units at each knot start.
  const cumulative: number[] = [];
  let total = 0;
  for (const k of knots) { cumulative.push(total); total += (k.end - k.start) * k.weight; }
  const unitsAt = (position: number): number => {
    const p = Math.min(Math.max(position, 0), length);
    for (let i = knots.length - 1; i >= 0; i--) {
      if (p >= knots[i].start) return cumulative[i] + (p - knots[i].start) * knots[i].weight;
    }
    return 0;
  };
  const positionAt = (units: number): number => {
    for (let i = knots.length - 1; i >= 0; i--) {
      if (units >= cumulative[i]) return Math.min(knots[i].end, knots[i].start + (units - cumulative[i]) / knots[i].weight);
    }
    return 0;
  };
  const u0 = unitsAt(viewStart);
  const u1 = Math.max(u0 + 1e-9, unitsAt(viewEnd));
  const perUnit = (x1 - x0) / (u1 - u0);
  return {
    toX: position => x0 + (unitsAt(position) - u0) * perUnit,
    fromX: x => positionAt(u0 + (x - x0) / perUnit),
    breaks: breaks.filter(b => b.end > viewStart && b.start < viewEnd),
  };
}

/** Round tick spacing (1, 2, 5 × 10^k) giving about `target` ticks over `span` bases. */
export function tickStep(span: number, target = 8): number {
  const raw = Math.max(1, span / target);
  const power = 10 ** Math.floor(Math.log10(raw));
  return [1, 2, 5, 10].map(m => m * power).find(step => step >= raw) ?? 10 * power;
}

/** Locus positions of ticks at round genomic coordinates within the view. */
export function genomicTicks(genomicStart: number, viewStart: number, viewEnd: number, target = 8): number[] {
  const step = tickStep(viewEnd - viewStart, target);
  const first = Math.ceil((genomicStart + viewStart) / step) * step;
  const out: number[] = [];
  for (let g = first; g - genomicStart <= viewEnd; g += step) out.push(g - genomicStart);
  return out;
}

/**
 * Locus blocks covered by a three-base codon whose first base (in transcription direction)
 * is `position`, following the exons so a split codon yields two blocks.
 */
export function codonBlocks(exons: Span[], position: number, reverse: boolean): { start: number; end: number }[] {
  const exonic = [...exons].sort((a, b) => a.start - b.start).flatMap(e => Array.from({ length: e.length }, (_, i) => e.start + i));
  const at = exonic.indexOf(position);
  if (at < 0) return [{ start: position, end: position + 1 }];
  const bases = reverse ? exonic.slice(Math.max(0, at - 2), at + 1) : exonic.slice(at, at + 3);
  const blocks: { start: number; end: number }[] = [];
  for (const p of bases) {
    const last = blocks[blocks.length - 1];
    if (last && last.end === p) last.end = p + 1; else blocks.push({ start: p, end: p + 1 });
  }
  return blocks;
}

/** Parts of each exon inside and outside the coding blocks (for thick CDS / thin UTR drawing). */
export function exonPieces(exons: Span[], cds: Span[]): { start: number; end: number; coding: boolean }[] {
  const pieces: { start: number; end: number; coding: boolean }[] = [];
  for (const e of exons) {
    const end = e.start + e.length;
    const cuts = new Set([e.start, end]);
    for (const c of cds) {
      for (const p of [c.start, c.start + c.length]) if (p > e.start && p < end) cuts.add(p);
    }
    const sorted = [...cuts].sort((a, b) => a - b);
    for (let i = 0; i + 1 < sorted.length; i++) {
      const [s, t] = [sorted[i], sorted[i + 1]];
      pieces.push({ start: s, end: t, coding: cds.some(c => c.start <= s && t <= c.start + c.length) });
    }
  }
  return pieces;
}

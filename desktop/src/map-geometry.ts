// Pure display geometry for the construct map: angles, block-arrow paths, overlap
// lanes, tick steps and label placement. Display projection only — no biology here;
// coordinates arrive as the engine's ordered source parts and are never merged.

export type Arrow = 'forward' | 'reverse' | 'none';
export interface Span { start: number; length: number }

const TAU = Math.PI * 2;

/** Angle of a reference boundary: 0 at the top, clockwise (SVG y points down). */
export function angleOf(base: number, moleculeLength: number): number {
  return (base / moleculeLength) * TAU - Math.PI / 2;
}

export function polar(cx: number, cy: number, radius: number, angle: number): [number, number] {
  return [cx + Math.cos(angle) * radius, cy + Math.sin(angle) * radius];
}

const f = (n: number) => Number(n.toFixed(2));
const pt = ([x, y]: [number, number]) => `${f(x)} ${f(y)}`;

/** Clockwise circular arc from a0 to a1 (a1 > a0), as an SVG path fragment. */
function arcTo(cx: number, cy: number, r: number, a0: number, a1: number, clockwise: boolean): string {
  const large = Math.abs(a1 - a0) > Math.PI ? 1 : 0;
  return `A ${f(r)} ${f(r)} 0 ${large} ${clockwise ? 1 : 0} ${pt(polar(cx, cy, r, clockwise ? a1 : a0))}`;
}

/**
 * Annular block arrow between angles a0 < a1 around radius `mid` with the given
 * thickness. The head sits at a1 (forward), a0 (reverse) or nowhere ('none').
 * A span of a full turn is drawn as a closed ring without a head.
 */
export function blockArrowPath(cx: number, cy: number, mid: number, thickness: number,
  a0: number, a1: number, arrow: Arrow, headLength: number): string {
  const outer = mid + thickness / 2;
  const inner = mid - thickness / 2;
  const span = a1 - a0;
  if (span >= TAU - 1e-9) {
    const half = a0 + Math.PI;
    return [`M ${pt(polar(cx, cy, outer, a0))}`, arcTo(cx, cy, outer, a0, half, true), arcTo(cx, cy, outer, half, a0 + TAU, true),
      `M ${pt(polar(cx, cy, inner, a0))}`, arcTo(cx, cy, inner, a0, half, true), arcTo(cx, cy, inner, half, a0 + TAU, true), 'Z'].join(' ');
  }
  const head = arrow === 'none' ? 0 : Math.min(headLength / mid, span * 0.6);
  const flare = thickness * 0.18;
  if (arrow === 'forward') {
    const neck = a1 - head;
    return [`M ${pt(polar(cx, cy, inner, a0))}`, `L ${pt(polar(cx, cy, outer, a0))}`, arcTo(cx, cy, outer, a0, neck, true),
      `L ${pt(polar(cx, cy, outer + flare, neck))}`, `L ${pt(polar(cx, cy, mid, a1))}`, `L ${pt(polar(cx, cy, inner - flare, neck))}`,
      `L ${pt(polar(cx, cy, inner, neck))}`, arcTo(cx, cy, inner, a0, neck, false), 'Z'].join(' ');
  }
  if (arrow === 'reverse') {
    const neck = a0 + head;
    return [`M ${pt(polar(cx, cy, mid, a0))}`, `L ${pt(polar(cx, cy, outer + flare, neck))}`, `L ${pt(polar(cx, cy, outer, neck))}`,
      arcTo(cx, cy, outer, neck, a1, true), `L ${pt(polar(cx, cy, inner, a1))}`, arcTo(cx, cy, inner, neck, a1, false),
      `L ${pt(polar(cx, cy, inner - flare, neck))}`, 'Z'].join(' ');
  }
  return [`M ${pt(polar(cx, cy, inner, a0))}`, `L ${pt(polar(cx, cy, outer, a0))}`, arcTo(cx, cy, outer, a0, a1, true),
    `L ${pt(polar(cx, cy, inner, a1))}`, arcTo(cx, cy, inner, a0, a1, false), 'Z'].join(' ');
}

/** Centre-line arc for text on a feature; reversed on the lower half so text stays upright. */
export function textArcPath(cx: number, cy: number, radius: number, a0: number, a1: number): { d: string; flipped: boolean } {
  const middle = (a0 + a1) / 2;
  const flipped = Math.sin(middle) > 0.05;
  const from = flipped ? a1 : a0;
  const to = flipped ? a0 : a1;
  const large = Math.abs(a1 - a0) > Math.PI ? 1 : 0;
  return { d: `M ${pt(polar(cx, cy, radius, from))} A ${f(radius)} ${f(radius)} 0 ${large} ${flipped ? 0 : 1} ${pt(polar(cx, cy, radius, to))}`, flipped };
}

/** Horizontal block arrow for linear molecules, from x0 to x1 (x1 > x0). */
export function linearArrowPath(x0: number, x1: number, y: number, thickness: number, arrow: Arrow, headLength: number): string {
  const top = y - thickness / 2;
  const bottom = y + thickness / 2;
  const flare = thickness * 0.18;
  const head = arrow === 'none' ? 0 : Math.min(headLength, (x1 - x0) * 0.6);
  if (arrow === 'forward') {
    const neck = x1 - head;
    return `M ${f(x0)} ${f(top)} L ${f(neck)} ${f(top)} L ${f(neck)} ${f(top - flare)} L ${f(x1)} ${f(y)} L ${f(neck)} ${f(bottom + flare)} L ${f(neck)} ${f(bottom)} L ${f(x0)} ${f(bottom)} Z`;
  }
  if (arrow === 'reverse') {
    const neck = x0 + head;
    return `M ${f(x1)} ${f(top)} L ${f(neck)} ${f(top)} L ${f(neck)} ${f(top - flare)} L ${f(x0)} ${f(y)} L ${f(neck)} ${f(bottom + flare)} L ${f(neck)} ${f(bottom)} L ${f(x1)} ${f(bottom)} Z`;
  }
  return `M ${f(x0)} ${f(top)} L ${f(x1)} ${f(top)} L ${f(x1)} ${f(bottom)} L ${f(x0)} ${f(bottom)} Z`;
}

/** A "nice" tick step (1, 2 or 5 × 10ⁿ) giving at most about `target` ticks. */
export function niceStep(moleculeLength: number, target = 10): number {
  const raw = Math.max(1, moleculeLength / target);
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  for (const multiple of [1, 2, 5, 10]) if (multiple * magnitude >= raw) return multiple * magnitude;
  return 10 * magnitude;
}

/** Tick boundaries after the origin, below the molecule length. */
export function ticks(moleculeLength: number, target = 10): number[] {
  const step = niceStep(moleculeLength, target);
  const result: number[] = [];
  for (let base = step; base < moleculeLength; base += step) result.push(base);
  return result;
}

export interface LaneInput { id: string; parts: Span[] }

/**
 * Greedy overlap lanes: longest features first (ties in source order), each into the
 * lowest lane where none of its parts overlap an occupied interval. All parts of a
 * feature share one lane, so multipart identity is preserved. `minLength` widens tiny
 * parts to their drawn size and `padding` keeps neighbours visually apart (both in bases).
 */
export function assignLanes(features: LaneInput[], moleculeLength: number, circular: boolean,
  minLength = 0, padding = 0): Map<string, number> {
  const pieces = (part: Span): [number, number][] => {
    const extra = Math.max(0, minLength - part.length) / 2;
    const start = part.start - extra - padding;
    const end = part.start + part.length + extra + padding;
    if (!circular) return [[start, end]];
    if (end - start >= moleculeLength) return [[0, moleculeLength]];
    const s = ((start % moleculeLength) + moleculeLength) % moleculeLength;
    const e = s + (end - start);
    return e <= moleculeLength ? [[s, e]] : [[s, moleculeLength], [0, e - moleculeLength]];
  };
  const total = (feature: LaneInput) => feature.parts.reduce((sum, part) => sum + part.length, 0);
  const order = features.map((feature, index) => ({ feature, index }))
    .sort((a, b) => total(b.feature) - total(a.feature) || a.index - b.index);
  const lanes: [number, number][][] = [];
  const result = new Map<string, number>();
  for (const { feature } of order) {
    const wanted = feature.parts.flatMap(pieces);
    let lane = 0;
    while (lanes[lane]?.some(([s, e]) => wanted.some(([ws, we]) => ws < e && s < we))) lane++;
    (lanes[lane] ??= []).push(...wanted);
    result.set(feature.id, lane);
  }
  return result;
}

export interface LabelRequest { id: string; anchorX: number; anchorY: number; width: number; priority: number }
export interface PlacedLabel { id: string; x: number; y: number; width: number; right: boolean }

/**
 * Place outside labels in two columns around a circle of `radius` (right side for
 * anchors right of centre). Labels keep their anchor order, are pushed apart to a
 * minimum `pitch`, hug the circle horizontally, and must fit between `top` and
 * `bottom`. When a column cannot hold every label, the lowest-priority labels are
 * dropped and returned in `hidden` — never silently lost.
 */
export function placeCircularLabels(requests: LabelRequest[], cx: number, cy: number, radius: number,
  top: number, bottom: number, pitch: number, left = -Infinity, right = Infinity): { placed: PlacedLabel[]; hidden: string[] } {
  const placed: PlacedLabel[] = [];
  const hidden: string[] = [];
  for (const onRight of [false, true]) {
    let side = requests.filter(request => (request.anchorX >= cx) === onRight);
    const capacity = Math.max(0, Math.floor((bottom - top) / pitch) + 1);
    if (side.length > capacity) {
      const keep = new Set([...side].sort((a, b) => b.priority - a.priority).slice(0, capacity).map(r => r.id));
      hidden.push(...side.filter(r => !keep.has(r.id)).map(r => r.id));
      side = side.filter(r => keep.has(r.id));
    }
    const sorted = [...side].sort((a, b) => a.anchorY - b.anchorY);
    const ys = sorted.map(request => {
      const dy = request.anchorY - cy;
      const dx = Math.abs(request.anchorX - cx);
      const scale = radius / Math.max(1e-9, Math.hypot(dx, dy));
      return Math.min(bottom, Math.max(top, cy + dy * scale));
    });
    spread(ys, top, bottom, pitch);
    sorted.forEach((request, i) => {
      const dy = ys[i] - cy;
      const reach = Math.sqrt(Math.max(0, radius * radius - dy * dy));
      const edge = cx + (onRight ? 1 : -1) * Math.max(reach, 12);
      // Keep the whole label on the canvas, even if that tucks it towards the ring.
      const x = Math.min(Math.max(onRight ? edge : edge - request.width, left), right - request.width);
      placed.push({ id: request.id, x, y: ys[i], width: request.width, right: onRight });
    });
  }
  return { placed, hidden };
}

/**
 * Spread sorted desired positions to a minimum `pitch` inside [top, bottom], keeping
 * each run of colliding labels centred on the mean of its desired positions (so a
 * crowded cluster grows both ways instead of drifting in one direction).
 */
export function spread(ys: number[], top: number, bottom: number, pitch: number): void {
  type Group = { first: number; count: number; desired: number; start: number };
  const groups: Group[] = [];
  const place = (group: Group) => {
    const span = (group.count - 1) * pitch;
    group.start = Math.min(Math.max(group.desired / group.count - span / 2, top), Math.max(top, bottom - span));
  };
  ys.forEach((y, index) => {
    const group: Group = { first: index, count: 1, desired: y, start: y };
    place(group);
    groups.push(group);
    while (groups.length > 1) {
      const last = groups[groups.length - 1];
      const previous = groups[groups.length - 2];
      if (previous.start + previous.count * pitch <= last.start + 1e-9) break;
      previous.count += last.count;
      previous.desired += last.desired;
      groups.pop();
      place(previous);
    }
  });
  for (const group of groups) for (let i = 0; i < group.count; i++) ys[group.first + i] = group.start + i * pitch;
}

export interface RowRequest { id: string; centre: number; width: number; priority: number }

/**
 * Row packing for labels under a linear map: each label is centred on its feature
 * (clamped to [left, right]) in the first row where it does not collide. Labels that
 * need more than `maxRows` rows are hidden, lowest priority first.
 */
export function placeRowLabels(requests: RowRequest[], left: number, right: number, maxRows: number, gap: number):
  { placed: { id: string; x: number; row: number; width: number }[]; hidden: string[] } {
  const rows: [number, number][][] = [];
  const placed: { id: string; x: number; row: number; width: number }[] = [];
  const hidden: string[] = [];
  const order = requests.map((request, index) => ({ request, index })).sort((a, b) => b.request.priority - a.request.priority || a.index - b.index);
  for (const { request } of order) {
    const x = Math.min(Math.max(left, request.centre - request.width / 2), Math.max(left, right - request.width));
    let row = 0;
    while (row < maxRows && rows[row]?.some(([s, e]) => x < e + gap && s < x + request.width + gap)) row++;
    if (row >= maxRows) { hidden.push(request.id); continue; }
    (rows[row] ??= []).push([x, x + request.width]);
    placed.push({ id: request.id, x, row, width: request.width });
  }
  return { placed, hidden };
}

/** WCAG relative luminance of a #rrggbb colour. */
export function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

/** Black or white text, whichever contrasts more with the fill. */
export function textOn(hex: string): '#111111' | '#ffffff' {
  const l = luminance(hex);
  return (l + 0.05) / 0.05 >= 1.05 / (l + 0.05) ? '#111111' : '#ffffff';
}

// SVG construct map. Layout (angles, lanes, labels) comes from map-geometry.ts; this
// module only builds DOM. Every feature part is drawn; labels that cannot be placed
// are returned in `unlabelled` so the caller can report them — never dropped silently.
import type { Document, Feature, Orf } from './bindings';
import { featureColor } from './map-layout';
import {
  angleOf, assignLanes, blockArrowPath, linearArrowPath, placeCircularLabels, placeRowLabels,
  polar, textArcPath, textOn, ticks, type Arrow,
} from './map-geometry';

const NS = 'http://www.w3.org/2000/svg';
const TAU = Math.PI * 2;
const LABEL_FONT = '12px -apple-system, system-ui, sans-serif';
const PILL_HEIGHT = 20;
const PILL_PITCH = 24;
/** Bottom strip kept free of labels for the overlaid "labels not shown" notice. */
const NOTICE_RESERVE = 40;

/** ORFs to draw (already filtered) and the ORF selection. */
export interface MapOrfs {
  orfs: Orf[];
  selectedOrf: string | null;
  selectOrf: (id: string) => void;
  /** A selected base range (half-open; `end < start` wraps), drawn as a selection band. */
  range?: { start: number; end: number } | null;
}

export interface MapReport {
  /** Features drawn without a map label, in source order. */
  unlabelled: string[];
  /** ORFs requested but not drawn for lack of room (still listed in the sequence view). */
  undrawnOrfs: string[];
}

let measure: CanvasRenderingContext2D | null = null;
function textWidth(text: string, font = LABEL_FONT): number {
  measure ??= document.createElement('canvas').getContext('2d');
  if (!measure) return text.length * 7;
  measure.font = font;
  return measure.measureText(text).width;
}

const nameOf = (feature: Feature) => feature.label || feature.kind;
const arrowOf = (feature: Feature): Arrow => feature.strand === 'forward' ? 'forward' : feature.strand === 'reverse' ? 'reverse' : 'none';
const totalLength = (feature: Feature) => feature.parts.reduce((sum, part) => sum + part.length, 0);
const longestPart = (feature: Feature) => feature.parts.reduce((a, b) => b.length > a.length ? b : a);

type Shape = (tag: string, attributes: Record<string, string | number>, parent?: Element) => SVGElement;

function shapes(root: SVGSVGElement): Shape {
  return (tag, attributes, parent = root) => {
    const node = document.createElementNS(NS, tag) as SVGElement;
    for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value));
    parent.append(node);
    return node;
  };
}

function interactive(node: Element, feature: Feature, select: (id: string, extend?: boolean) => void, shape: Shape) {
  node.setAttribute('tabindex', '0');
  node.setAttribute('role', 'button');
  node.setAttribute('aria-label', `${nameOf(feature)}, ${feature.strand} strand`);
  node.addEventListener('click', event => select(feature.id, (event as MouseEvent).shiftKey));
  node.addEventListener('keydown', event => {
    if (['Enter', ' '].includes((event as KeyboardEvent).key)) { event.preventDefault(); select(feature.id, (event as KeyboardEvent).shiftKey); }
  });
  shape('title', {}, node).textContent = `${nameOf(feature)} (${feature.strand}) · ${feature.parts.map(p => `[${p.start}, ${p.start + p.length})`).join(', ')}`;
}

function pill(shape: Shape, parent: Element, feature: Feature, x: number, y: number, width: number, active: boolean, select: (id: string, extend?: boolean) => void): SVGElement {
  const color = featureColor(feature);
  const group = shape('g', { class: `map-pill map-label${active ? ' active' : ''}`, 'data-testid': 'map-label', 'data-feature-id': feature.id, 'data-label-mode': 'outside' }, parent);
  shape('rect', { x, y: y - PILL_HEIGHT / 2, width, height: PILL_HEIGHT, rx: PILL_HEIGHT / 2, fill: color }, group);
  const text = shape('text', { x: x + width / 2, y, 'text-anchor': 'middle', 'dominant-baseline': 'central', fill: textOn(color) }, group);
  text.textContent = nameOf(feature);
  interactive(group, feature, select, shape);
  return group;
}

export function renderMap(svg: SVGSVGElement, doc: Document, selected: string | null, select: (id: string, extend?: boolean) => void,
  orfs: MapOrfs = { orfs: [], selectedOrf: null, selectOrf: () => undefined }): MapReport {
  svg.replaceChildren();
  const width = Math.max(240, svg.clientWidth || 900);
  const height = Math.max(160, svg.clientHeight || 600);
  svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
  svg.dataset.width = String(width);
  svg.dataset.height = String(height);
  return doc.circular ? circular(svg, doc, selected, select, width, height, orfs) : linear(svg, doc, selected, select, width, height, orfs);
}

function orfInteractive(node: Element, orf: Orf, select: (id: string, extend?: boolean) => void, shape: Shape) {
  node.setAttribute('tabindex', '0');
  node.setAttribute('role', 'button');
  node.setAttribute('aria-label', `ORF ${orf.codons} amino acids, ${orf.strand} strand`);
  node.addEventListener('click', () => select(orf.id));
  node.addEventListener('keydown', event => {
    if (['Enter', ' '].includes((event as KeyboardEvent).key)) { event.preventDefault(); select(orf.id); }
  });
  shape('title', {}, node).textContent = `ORF · ${orf.strand} · [${orf.start}, ${orf.start + orf.length}) · ${orf.codons} aa`;
}

function circular(svg: SVGSVGElement, doc: Document, selected: string | null, select: (id: string, extend?: boolean) => void, width: number, height: number, orfs: MapOrfs): MapReport {
  const shape = shapes(svg);
  const length = doc.sequence.length;
  const cx = width / 2;
  const cy = height / 2;
  const labelRoom = Math.min(240, Math.max(90, width * 0.2));
  const radius = Math.max(70, Math.min(height / 2 - 44, width / 2 - labelRoom - 30));
  svg.dataset.radius = radius.toFixed(1);
  const angle = (base: number) => angleOf(base, length);

  // Backbone: double ring, origin mark, ticks with tangential labels inside the ring.
  shape('circle', { cx, cy, r: radius, class: 'map-backbone' });
  shape('circle', { cx, cy, r: radius - 4, class: 'map-backbone' });
  const [ox0, oy0] = polar(cx, cy, radius - 10, angle(0));
  const [ox1, oy1] = polar(cx, cy, radius + 8, angle(0));
  shape('line', { x1: ox0, y1: oy0, x2: ox1, y2: oy1, class: 'map-origin' });
  for (const base of ticks(length, Math.max(6, Math.round(radius / 24)))) {
    const a = angle(base);
    const [x0, y0] = polar(cx, cy, radius - 8, a);
    const [x1, y1] = polar(cx, cy, radius + 2, a);
    shape('line', { x1: x0, y1: y0, x2: x1, y2: y1, class: 'map-tick' });
    const [tx, ty] = polar(cx, cy, radius - 17, a);
    const degrees = a * 180 / Math.PI + 90 + (Math.sin(a) > 0.05 ? 180 : 0);
    shape('text', { x: tx, y: ty, 'text-anchor': 'middle', 'dominant-baseline': 'central', class: 'map-tick-label', transform: `rotate(${degrees.toFixed(1)} ${tx.toFixed(1)} ${ty.toFixed(1)})` }).textContent = String(base);
  }
  shape('text', { x: cx, y: cy - 10, 'text-anchor': 'middle', class: 'map-centre-name' }).textContent = doc.name;
  shape('text', { x: cx, y: cy + 12, 'text-anchor': 'middle', class: 'map-centre-length' }).textContent = `${length.toLocaleString()} bp`;

  // Features: block arrows in overlap lanes inside the ring.
  const outer = radius - 27;
  const minPx = 4;
  let thickness = Math.min(20, Math.max(8, radius * 0.07));
  const basesPerPx = length / (TAU * outer);
  const lanes = assignLanes(doc.features, length, true, minPx * basesPerPx, 1.5 * basesPerPx);
  const laneCount = Math.max(1, ...[...lanes.values()].map(l => l + 1));
  const gap = 3;
  thickness = Math.max(5, Math.min(thickness, (radius * 0.5) / laneCount - gap));
  const midOf = (feature: Feature) => outer - thickness / 2 - (lanes.get(feature.id) ?? 0) * (thickness + gap);
  const spanAngles = (part: { start: number; length: number }, mid: number): [number, number] => {
    let a0 = angle(part.start);
    let a1 = a0 + (part.length / length) * TAU;
    const min = minPx / mid;
    if (a1 - a0 < min) { const extra = (min - (a1 - a0)) / 2; a0 -= extra; a1 += extra; }
    return [a0, a1];
  };

  const bands = shape('g', { class: 'map-bands' });
  const featureLayer = shape('g', { class: 'map-features' });
  const leaderLayer = shape('g', { class: 'map-leaders' });
  const labelLayer = shape('g', { class: 'map-labels' });
  const requests: { id: string; anchorX: number; anchorY: number; width: number; priority: number }[] = [];
  const anchors = new Map<string, { angle: number; mid: number }>();

  doc.features.forEach(feature => {
    const color = featureColor(feature);
    const mid = midOf(feature);
    const isSelected = feature.id === selected;
    feature.parts.forEach((part, partIndex) => {
      const [a0, a1] = spanAngles(part, mid);
      const node = shape('path', {
        d: blockArrowPath(cx, cy, mid, thickness, a0, a1, arrowOf(feature), Math.min(thickness * 0.9, 14)),
        fill: color, class: `map-feature${isSelected ? ' selected' : ''}`,
        'data-testid': 'map-feature', 'data-feature-id': feature.id, 'data-part-index': partIndex,
      }, featureLayer);
      interactive(node, feature, select, shape);
      if (isSelected) {
        // Selection band outside the ring, one per source part, in source order.
        shape('path', {
          d: blockArrowPath(cx, cy, radius + 13, 14, a0, a1, 'none', 0), class: 'map-band',
          'data-testid': 'map-selection-part', 'data-part-index': partIndex, 'data-part-start': part.start, 'data-part-length': part.length,
        }, bands);
        for (const edge of [a0, a1]) {
          const [x0, y0] = polar(cx, cy, radius + 5, edge);
          const [x1, y1] = polar(cx, cy, radius + 21, edge);
          shape('line', { x1: x0, y1: y0, x2: x1, y2: y1, class: 'map-band-edge' }, bands);
        }
      }
    });

    // Labels: on the arc when the name fits, otherwise an outside pill.
    const part = longestPart(feature);
    const [a0, a1] = spanAngles(part, mid);
    const head = feature.strand === 'unknown' ? 0 : Math.min(thickness * 0.9, 14);
    const font = Math.min(12, thickness * 0.72);
    const name = nameOf(feature);
    const inlineWidth = textWidth(name, `${font}px -apple-system, system-ui, sans-serif`);
    if (font >= 8 && inlineWidth + 10 <= (a1 - a0) * mid - head) {
      const { d } = textArcPath(cx, cy, mid, a0 + (feature.strand === 'reverse' ? head / mid : 0), a1 - (feature.strand === 'forward' ? head / mid : 0));
      const id = `label-path-${feature.id}`;
      shape('path', { id, d, fill: 'none', stroke: 'none' }, labelLayer);
      const text = shape('text', {
        class: `map-label map-inline${isSelected ? ' active' : ''}`, 'data-testid': 'map-label', 'data-feature-id': feature.id, 'data-label-mode': 'inline',
        'font-size': font.toFixed(1), fill: textOn(color), 'dominant-baseline': 'central',
      }, labelLayer);
      const path = shape('textPath', { href: `#${id}`, startOffset: '50%', 'text-anchor': 'middle' }, text);
      path.textContent = name;
      return;
    }
    const middle = (a0 + a1) / 2;
    const [ax, ay] = polar(cx, cy, radius + 24, middle);
    anchors.set(feature.id, { angle: middle, mid });
    requests.push({ id: feature.id, anchorX: ax, anchorY: ay, width: textWidth(name) + 18,
      priority: isSelected ? Number.POSITIVE_INFINITY : totalLength(feature) });
  });

  // ORFs: thin arrows in their own lanes inside the features.
  const undrawnOrfs: string[] = [];
  if (orfs.orfs.length) {
    const orfLanes = assignLanes(orfs.orfs.map(orf => ({ id: orf.id, parts: [{ start: orf.start, length: orf.length }] })), length, true, minPx * basesPerPx, 1.5 * basesPerPx);
    const orfThickness = Math.max(3, Math.min(7, thickness * 0.4));
    const orfOuter = outer - laneCount * (thickness + gap) - 8;
    const orfLayer = shape('g', { class: 'map-orfs' });
    featureLayer.before(orfLayer);
    for (const orf of orfs.orfs) {
      const mid = orfOuter - orfThickness / 2 - (orfLanes.get(orf.id) ?? 0) * (orfThickness + 2);
      if (mid < radius * 0.25) { undrawnOrfs.push(orf.id); continue; }
      const [a0, a1] = spanAngles({ start: orf.start, length: orf.length }, mid);
      const isSelected = orf.id === orfs.selectedOrf;
      const node = shape('path', {
        d: blockArrowPath(cx, cy, mid, orfThickness, a0, a1, orf.strand === 'reverse' ? 'reverse' : 'forward', orfThickness * 1.6),
        class: `map-orf${isSelected ? ' selected' : ''}`, 'data-testid': 'map-orf', 'data-orf-id': orf.id,
        'data-orf-start': orf.start, 'data-orf-length': orf.length, 'data-orf-strand': orf.strand,
      }, orfLayer);
      orfInteractive(node, orf, orfs.selectOrf, shape);
      if (isSelected) {
        shape('path', { d: blockArrowPath(cx, cy, radius + 13, 14, a0, a1, 'none', 0), class: 'map-band',
          'data-testid': 'map-selection-part', 'data-part-index': 0, 'data-part-start': orf.start, 'data-part-length': orf.length }, bands);
        for (const edge of [a0, a1]) {
          const [x0, y0] = polar(cx, cy, radius + 5, edge);
          const [x1, y1] = polar(cx, cy, radius + 21, edge);
          shape('line', { x1: x0, y1: y0, x2: x1, y2: y1, class: 'map-band-edge' }, bands);
        }
      }
    }
  }

  if (orfs.range) {
    const { start, end } = orfs.range;
    const span = ((end - start + length) % length) || length;
    const a0 = angle(start);
    const a1 = a0 + (span / length) * TAU;
    shape('path', { d: blockArrowPath(cx, cy, radius + 13, 14, a0, a1, 'none', 0), class: 'map-band',
      'data-testid': 'map-selection-part', 'data-part-index': 0, 'data-part-start': start, 'data-part-length': span }, bands);
    for (const edge of [a0, a1]) {
      const [x0, y0] = polar(cx, cy, radius + 5, edge);
      const [x1, y1] = polar(cx, cy, radius + 21, edge);
      shape('line', { x1: x0, y1: y0, x2: x1, y2: y1, class: 'map-band-edge' }, bands);
    }
  }

  const { placed, hidden } = placeCircularLabels(requests, cx, cy, radius + 44, 14 + PILL_HEIGHT / 2, height - NOTICE_RESERVE - PILL_HEIGHT / 2, PILL_PITCH, 8, width - 8);
  const byId = new Map(doc.features.map(feature => [feature.id, feature]));
  for (const label of placed) {
    const feature = byId.get(label.id)!;
    const anchor = anchors.get(label.id)!;
    const [fx, fy] = polar(cx, cy, anchor.mid + thickness / 2, anchor.angle);
    const [rx, ry] = polar(cx, cy, radius + 24, anchor.angle);
    const edge = label.right ? label.x : label.x + label.width;
    shape('polyline', { points: `${fx.toFixed(1)},${fy.toFixed(1)} ${rx.toFixed(1)},${ry.toFixed(1)} ${edge.toFixed(1)},${label.y.toFixed(1)}`, class: 'map-leader' }, leaderLayer);
    pill(shape, labelLayer, feature, label.x, label.y, label.width, feature.id === selected, select);
  }
  const hiddenSet = new Set(hidden);
  return { unlabelled: doc.features.filter(feature => hiddenSet.has(feature.id)).map(feature => feature.id), undrawnOrfs };
}

function linear(svg: SVGSVGElement, doc: Document, selected: string | null, select: (id: string, extend?: boolean) => void, width: number, height: number, orfs: MapOrfs): MapReport {
  const shape = shapes(svg);
  const length = doc.sequence.length;
  const margin = 40;
  const x = (base: number) => margin + (base / length) * (width - 2 * margin);
  const backbone = 70;
  svg.dataset.radius = '0';

  shape('text', { x: width / 2, y: 24, 'text-anchor': 'middle', class: 'map-centre-name' }).textContent = doc.name;
  shape('text', { x: width / 2, y: 42, 'text-anchor': 'middle', class: 'map-centre-length' }).textContent = `${length.toLocaleString()} bp · linear`;
  shape('line', { x1: x(0), y1: backbone, x2: x(length), y2: backbone, class: 'map-backbone' });
  shape('line', { x1: x(0), y1: backbone + 4, x2: x(length), y2: backbone + 4, class: 'map-backbone' });
  for (const base of [0, ...ticks(length, Math.max(4, Math.round(width / 110))), length]) {
    shape('line', { x1: x(base), y1: backbone - 6, x2: x(base), y2: backbone + 10, class: base === 0 ? 'map-origin' : 'map-tick' });
    shape('text', { x: x(base), y: backbone - 10, 'text-anchor': 'middle', class: 'map-tick-label' }).textContent = String(base);
  }

  const minPx = 4;
  const basesPerPx = length / (width - 2 * margin);
  const lanes = assignLanes(doc.features, length, false, minPx * basesPerPx, 1.5 * basesPerPx);
  const thickness = 18;
  const gap = 4;
  const top = backbone + 30;
  const laneCount = Math.max(1, ...[...lanes.values()].map(l => l + 1));
  const centreOf = (feature: Feature) => top + thickness / 2 + (lanes.get(feature.id) ?? 0) * (thickness + gap);
  const spanX = (part: { start: number; length: number }): [number, number] => {
    let x0 = x(part.start);
    let x1 = x(part.start + part.length);
    if (x1 - x0 < minPx) { const extra = (minPx - (x1 - x0)) / 2; x0 -= extra; x1 += extra; }
    return [x0, x1];
  };
  const bands = shape('g', { class: 'map-bands' });
  const featureLayer = shape('g', { class: 'map-features' });
  const leaderLayer = shape('g', { class: 'map-leaders' });
  const labelLayer = shape('g', { class: 'map-labels' });
  const requests: { id: string; centre: number; width: number; priority: number }[] = [];

  doc.features.forEach(feature => {
    const color = featureColor(feature);
    const y = centreOf(feature);
    const isSelected = feature.id === selected;
    feature.parts.forEach((part, partIndex) => {
      const [x0, x1] = spanX(part);
      const node = shape('path', {
        d: linearArrowPath(x0, x1, y, thickness, arrowOf(feature), 12), fill: color, class: `map-feature${isSelected ? ' selected' : ''}`,
        'data-testid': 'map-feature', 'data-feature-id': feature.id, 'data-part-index': partIndex,
      }, featureLayer);
      interactive(node, feature, select, shape);
      if (isSelected) {
        shape('rect', { x: x0, y: backbone + 10, width: x1 - x0, height: 12, class: 'map-band',
          'data-testid': 'map-selection-part', 'data-part-index': partIndex, 'data-part-start': part.start, 'data-part-length': part.length }, bands);
        for (const edge of [x0, x1]) shape('line', { x1: edge, y1: backbone + 6, x2: edge, y2: backbone + 26, class: 'map-band-edge' }, bands);
      }
    });
    const [x0, x1] = spanX(longestPart(feature));
    const name = nameOf(feature);
    if (textWidth(name) + 22 <= x1 - x0) {
      const text = shape('text', { x: (x0 + x1) / 2, y, 'text-anchor': 'middle', 'dominant-baseline': 'central', fill: textOn(color),
        class: `map-label map-inline${isSelected ? ' active' : ''}`, 'data-testid': 'map-label', 'data-feature-id': feature.id, 'data-label-mode': 'inline' }, labelLayer);
      text.textContent = name;
      return;
    }
    requests.push({ id: feature.id, centre: (x0 + x1) / 2, width: textWidth(name) + 18, priority: isSelected ? Number.POSITIVE_INFINITY : totalLength(feature) });
  });

  if (orfs.range) {
    const [x0, x1] = [x(orfs.range.start), x(orfs.range.end)];
    shape('rect', { x: x0, y: backbone + 10, width: Math.max(1, x1 - x0), height: 12, class: 'map-band',
      'data-testid': 'map-selection-part', 'data-part-index': 0, 'data-part-start': orfs.range.start, 'data-part-length': orfs.range.end - orfs.range.start }, bands);
    for (const edge of [x0, x1]) shape('line', { x1: edge, y1: backbone + 6, x2: edge, y2: backbone + 26, class: 'map-band-edge' }, bands);
  }
  let orfBottom = top + laneCount * (thickness + gap);
  if (orfs.orfs.length) {
    const orfLanes = assignLanes(orfs.orfs.map(orf => ({ id: orf.id, parts: [{ start: orf.start, length: orf.length }] })), length, false, minPx * basesPerPx, 1.5 * basesPerPx);
    const orfLayer = shape('g', { class: 'map-orfs' });
    const orfTop = orfBottom + 8;
    for (const orf of orfs.orfs) {
      const y = orfTop + 3 + (orfLanes.get(orf.id) ?? 0) * 9;
      const [x0, x1] = spanX({ start: orf.start, length: orf.length });
      const isSelected = orf.id === orfs.selectedOrf;
      const node = shape('path', { d: linearArrowPath(x0, x1, y, 6, orf.strand === 'reverse' ? 'reverse' : 'forward', 8),
        class: `map-orf${isSelected ? ' selected' : ''}`, 'data-testid': 'map-orf', 'data-orf-id': orf.id,
        'data-orf-start': orf.start, 'data-orf-length': orf.length, 'data-orf-strand': orf.strand }, orfLayer);
      orfInteractive(node, orf, orfs.selectOrf, shape);
      if (isSelected) {
        shape('rect', { x: x0, y: backbone + 10, width: x1 - x0, height: 12, class: 'map-band',
          'data-testid': 'map-selection-part', 'data-part-index': 0, 'data-part-start': orf.start, 'data-part-length': orf.length }, bands);
      }
      orfBottom = Math.max(orfBottom, y + 6);
    }
  }
  const rowsTop = orfBottom + 24;
  const maxRows = Math.max(0, Math.floor((height - rowsTop - NOTICE_RESERVE) / PILL_PITCH));
  const { placed, hidden } = placeRowLabels(requests, 8, width - 8, maxRows, 8);
  const byId = new Map(doc.features.map(feature => [feature.id, feature]));
  for (const label of placed) {
    const feature = byId.get(label.id)!;
    const [x0, x1] = spanX(longestPart(feature));
    const y = rowsTop + label.row * PILL_PITCH;
    shape('polyline', { points: `${((x0 + x1) / 2).toFixed(1)},${(centreOf(feature) + thickness / 2).toFixed(1)} ${(label.x + label.width / 2).toFixed(1)},${(y - PILL_HEIGHT / 2).toFixed(1)}`, class: 'map-leader' }, leaderLayer);
    pill(shape, labelLayer, feature, label.x, y, label.width, feature.id === selected, select);
  }
  // Centre the drawing vertically: linear maps are short and wide.
  const rows = placed.length ? Math.max(...placed.map(label => label.row)) + 1 : 0;
  const contentBottom = rows ? rowsTop + (rows - 1) * PILL_PITCH + PILL_HEIGHT / 2 : top + laneCount * (thickness + gap);
  const offset = Math.max(0, (height - contentBottom - 12) / 2);
  const group = document.createElementNS(NS, 'g');
  group.setAttribute('transform', `translate(0 ${offset.toFixed(1)})`);
  group.append(...svg.childNodes);
  svg.append(group);
  const hiddenSet = new Set(hidden);
  return { unlabelled: doc.features.filter(feature => hiddenSet.has(feature.id)).map(feature => feature.id), undrawnOrfs: [] };
}

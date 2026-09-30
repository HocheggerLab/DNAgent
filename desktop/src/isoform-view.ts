// Isoform (locus) view: one row per transcript, exons as boxes (CDS thick, UTR thin),
// introns as lines, start/stop codons in red. Order, evidence state and expression
// come from the Rust app layer (`doc.locus`); this module only draws and keeps the
// zoom window. Every isoform is drawn; the caller can check `drawn` against the model.
import type { Isoform, LocusView, QuantifierPanel } from './bindings';
import { axisScale, codonBlocks, exonPieces, genomicTicks, type AxisScale } from './isoform-geometry';

const NS = 'http://www.w3.org/2000/svg';
const LABEL_WIDTH = 200;
const RIGHT = 24;
const AXIS_Y = 40;
const ROWS_TOP = 64;
const PITCH = 30;
const MIN_SPAN = 30;

/** Display states from the app layer, as drawn. */
export const DISPLAY_STYLE: Record<string, { fill: string; hatched: boolean; words: string }> = {
  expressed: { fill: '#e8870e', hatched: false, words: 'expressed in long-read data' },
  not_detected: { fill: '#3b7dd8', hatched: false, words: 'quantified, not detected' },
  discoverable: { fill: '#3b7dd8', hatched: false, words: 'discoverable, not found' },
  invisible: { fill: '#a0a4a8', hatched: true, words: 'not distinguishable in long reads' },
  no_data: { fill: '#a0a4a8', hatched: true, words: 'no data for this quantifier' },
};
const styleOf = (display: string) => DISPLAY_STYLE[display] ?? DISPLAY_STYLE.no_data;

/** Per-document view window and options (presentation state only). */
export interface IsoformUi { start: number; end: number; compress: boolean; quantifier: string | null }
const uiByDoc = new Map<number, IsoformUi>();

/** `large`: records over the regular size limit start with introns compressed (exons would be sub-pixel). */
export function isoformUi(documentId: number, locus: LocusView, length: number, large = false): IsoformUi {
  let ui = uiByDoc.get(documentId);
  if (!ui) {
    ui = { start: 0, end: length, compress: large, quantifier: locus.default_quantifier ?? locus.quantifiers[0]?.quantifier ?? null };
    uiByDoc.set(documentId, ui);
  }
  return ui;
}

/** Zoom by `factor` (< 1 zooms in) about locus position `centre`, clamped to the locus. */
export function zoom(ui: IsoformUi, length: number, factor: number, centre = (ui.start + ui.end) / 2) {
  const span = Math.min(length, Math.max(MIN_SPAN, (ui.end - ui.start) * factor));
  let start = centre - (centre - ui.start) * (span / (ui.end - ui.start));
  start = Math.min(Math.max(0, start), length - span);
  ui.start = start; ui.end = start + span;
}

export function pan(ui: IsoformUi, length: number, bases: number) {
  const span = ui.end - ui.start;
  ui.start = Math.min(Math.max(0, ui.start + bases), length - span);
  ui.end = ui.start + span;
}

export function zoomTo(ui: IsoformUi, length: number, start: number, end: number) {
  const pad = Math.max(3, (end - start) * 0.05);
  ui.start = Math.max(0, start - pad); ui.end = Math.min(length, end + pad);
  if (ui.end - ui.start < MIN_SPAN) zoom(ui, length, 1, (start + end) / 2);
}

export const panelFor = (locus: LocusView, quantifier: string | null): QuantifierPanel | undefined =>
  locus.quantifiers.find(q => q.quantifier === quantifier);

/** Isoforms in the chosen quantifier's order (all of them, even without that quantifier's data). */
export function ordered(locus: LocusView, quantifier: string | null): Isoform[] {
  const order = panelFor(locus, quantifier)?.order ?? [];
  const rank = (i: Isoform) => { const r = order.indexOf(i.transcript_id); return r < 0 ? order.length : r; };
  return locus.isoforms.map((isoform, index) => ({ isoform, index }))
    .sort((a, b) => rank(a.isoform) - rank(b.isoform) || a.index - b.index).map(e => e.isoform);
}

export const displayOf = (isoform: Isoform, quantifier: string | null) =>
  isoform.expression.find(e => e.quantifier === quantifier)?.display ?? (isoform.evidence_state === 'quantified' ? 'no_data' : isoform.evidence_state);

export interface IsoformCallbacks {
  /** Select an isoform (its mRNA feature). */
  select(featureId: string): void;
  /** Select a half-open locus range. */
  selectRange(start: number, end: number): void;
  /** The zoom window changed (wheel); re-render. */
  changed(): void;
}

export interface IsoformRender { drawn: string[]; scale: AxisScale; x0: number; x1: number }

type Attrs = Record<string, string | number>;
function shape(tag: string, attrs: Attrs, parent: Element): SVGElement {
  const node = document.createElementNS(NS, tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, String(value));
  parent.append(node);
  return node;
}

const fmt = (n: number) => Math.round(n).toLocaleString('en-GB');

export function renderIsoforms(
  svg: SVGSVGElement, locus: LocusView, length: number, ui: IsoformUi,
  view: { selectedFeature: string | null; range: { start: number; end: number } | null }, callbacks: IsoformCallbacks,
): IsoformRender {
  const rows = ordered(locus, ui.quantifier);
  const width = Math.max(480, svg.clientWidth || 900);
  const legendY = ROWS_TOP + rows.length * PITCH + 18;
  const height = legendY + 34;
  svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
  svg.setAttribute('preserveAspectRatio', 'xMinYMin meet');
  svg.style.height = `${height}px`;
  svg.dataset.width = String(width);
  svg.replaceChildren();
  const x0 = LABEL_WIDTH;
  const x1 = width - RIGHT;
  const allExons = locus.isoforms.flatMap(i => i.exons);
  const scale = axisScale(length, allExons, ui.compress, ui.start, ui.end, x0, x1);
  const X = (p: number) => scale.toX(p);
  const reverse = locus.strand === 'reverse';

  const defs = shape('defs', {}, svg);
  const hatch = shape('pattern', { id: 'iso-hatch', width: 6, height: 6, patternUnits: 'userSpaceOnUse', patternTransform: 'rotate(45)' }, defs);
  shape('rect', { width: 6, height: 6, class: 'iso-hatch-bg' }, hatch);
  shape('line', { x1: 0, y1: 0, x2: 0, y2: 6, class: 'iso-hatch-line' }, hatch);
  const clip = shape('clipPath', { id: 'iso-clip' }, defs);
  shape('rect', { x: x0, y: 0, width: x1 - x0, height }, clip);

  // Header: locus, strand and the visible window in genomic coordinates.
  const g0 = locus.genomic_start;
  const header = shape('text', { x: 8, y: 16, class: 'iso-header' }, svg);
  header.textContent = `${locus.symbol} · ${locus.chrom}:${fmt(g0 + ui.start)}–${fmt(g0 + ui.end - 1)} · ${reverse ? '− strand, transcribed ←' : '+ strand, transcribed →'}${ui.compress ? ' · introns compressed' : ''}`;

  // Axis with ticks at round genomic coordinates (thinned where compression crowds them).
  shape('line', { x1: x0, y1: AXIS_Y, x2: x1, y2: AXIS_Y, class: 'iso-axis' }, svg);
  let lastX = -Infinity;
  for (const p of genomicTicks(g0, ui.start, ui.end, Math.max(3, Math.floor((x1 - x0) / 110)))) {
    const x = X(p);
    if (x < x0 - 0.5 || x > x1 + 0.5 || x - lastX < 80) continue;
    lastX = x;
    shape('line', { x1: x, y1: AXIS_Y - 4, x2: x, y2: AXIS_Y + 4, class: 'iso-tick', 'data-position': p }, svg);
    const label = shape('text', { x, y: AXIS_Y - 8, class: 'iso-tick-label', 'text-anchor': 'middle' }, svg);
    label.textContent = fmt(g0 + p);
  }
  for (const b of scale.breaks) {
    const x = (X(b.start) + X(b.end)) / 2;
    if (x < x0 || x > x1) continue;
    const mark = shape('text', { x, y: AXIS_Y + 4, class: 'iso-break', 'text-anchor': 'middle' }, svg);
    mark.textContent = '⫽';
    shape('title', {}, mark).textContent = `${fmt(b.end - b.start)} bp not drawn to scale`;
  }

  const track = shape('g', { 'clip-path': 'url(#iso-clip)' }, svg);
  const selectedIsoform = view.selectedFeature === null ? undefined
    : rows.find(i => i.mrna_feature_id === view.selectedFeature || i.cds_feature_id === view.selectedFeature);
  // The selected transcript's exons, as bands across every row, to compare structures.
  if (selectedIsoform) {
    for (const e of selectedIsoform.exons) {
      shape('rect', { x: X(e.start), y: ROWS_TOP - 6, width: Math.max(1, X(e.start + e.length) - X(e.start)), height: rows.length * PITCH + 4,
        class: 'iso-exon-band', 'data-testid': 'isoform-exon-highlight', 'data-start': e.start, 'data-end': e.start + e.length }, track);
    }
  }
  if (view.range && view.range.end > view.range.start) {
    const { start, end } = view.range;
    shape('rect', { x: X(start), y: ROWS_TOP - 8, width: Math.max(1, X(end) - X(start)), height: rows.length * PITCH + 8,
      class: 'iso-range', 'data-testid': 'isoform-range', 'data-start': start, 'data-end': end }, track);
  }

  rows.forEach((isoform, index) => {
    const y = ROWS_TOP + index * PITCH;
    const mid = y + PITCH / 2 - 4;
    const display = displayOf(isoform, ui.quantifier);
    const style = styleOf(display);
    const selected = isoform === selectedIsoform;
    const fill = style.hatched ? 'url(#iso-hatch)' : style.fill;
    const row = shape('g', { class: `iso-row${selected ? ' selected' : ''}`, 'data-testid': 'isoform-row', 'data-transcript': isoform.transcript_id,
      'data-display': display, 'data-feature-id': isoform.mrna_feature_id, tabindex: 0, role: 'button', 'aria-pressed': String(selected) }, svg);
    shape('rect', { x: 0, y: y - 4, width, height: PITCH - 2, class: 'iso-row-hit' }, row);
    shape('title', {}, row).textContent = `${isoform.transcript_id} · ${style.words}`;
    shape('rect', { x: 8, y: mid - 5, width: 10, height: 10, fill, class: 'iso-swatch', stroke: style.fill }, row);
    const label = shape('text', { x: 24, y: mid + 4, class: 'iso-label' }, row);
    label.textContent = isoform.transcript_id;
    if (isoform.is_mane_select) {
      const tag = shape('tspan', { class: 'iso-tag', dx: 6 }, label);
      tag.textContent = 'MANE';
    }
    const body = shape('g', { 'clip-path': 'url(#iso-clip)' }, row);
    const first = Math.min(...isoform.exons.map(e => e.start));
    const last = Math.max(...isoform.exons.map(e => e.start + e.length));
    shape('line', { x1: X(first), y1: mid, x2: X(last), y2: mid, class: 'iso-intron', stroke: style.hatched ? 'var(--muted)' : style.fill }, body);
    for (const piece of exonPieces(isoform.exons, isoform.cds)) {
      const h = piece.coding ? 14 : 7;
      const node = shape('rect', { x: X(piece.start), y: mid - h / 2, width: Math.max(1, X(piece.end) - X(piece.start)), height: h,
        fill, stroke: style.fill, class: `iso-exon${piece.coding ? ' coding' : ''}`, 'data-testid': 'isoform-exon',
        'data-start': piece.start, 'data-end': piece.end, 'data-coding': String(piece.coding) }, body);
      const exon = isoform.exons.find(e => e.start <= piece.start && piece.end <= e.start + e.length)!;
      // Double-click marks the exon. The first click selects the isoform and re-renders (and
      // may shift the layout), so the exon is remembered here and the second click is handled
      // by the drawing wherever it lands; the browser's own `dblclick` would never fire.
      node.addEventListener('click', event => {
        if ((event as MouseEvent).detail === 1) pendingExon = { start: exon.start, end: exon.start + exon.length, at: performance.now() };
      });
    }
    for (const [kind, codon] of [['start', isoform.start_codon], ['stop', isoform.stop_codon]] as const) {
      if (!codon) continue;
      for (const block of codonBlocks(isoform.exons, codon.position, reverse)) {
        const xa = X(block.start); const xb = X(block.end);
        const w = Math.max(2, xb - xa);
        const mark = shape('rect', { x: (xa + xb) / 2 - w / 2, y: mid - 10, width: w, height: 20, class: 'iso-codon',
          'data-testid': 'isoform-codon', 'data-kind': kind, 'data-position': codon.position, 'data-start': block.start, 'data-end': block.end }, body);
        shape('title', {}, mark).textContent = `${kind === 'start' ? 'Start' : 'Stop'} codon at ${locus.chrom}:${fmt(g0 + block.start)}${codon.split ? ' (split by an intron)' : ''}`;
      }
    }
    const choose = () => callbacks.select(isoform.mrna_feature_id);
    row.addEventListener('click', event => { if (!dragMoved && event.detail < 2) choose(); });
    row.addEventListener('keydown', event => { if (['Enter', ' '].includes((event as KeyboardEvent).key)) { event.preventDefault(); choose(); } });
  });

  // Legend (part of the exported drawing).
  const legend = shape('g', { class: 'iso-legend', transform: `translate(8, ${legendY})` }, svg);
  let lx = 0;
  const entries: [string, string, boolean][] = [
    ['Expressed (long reads)', DISPLAY_STYLE.expressed.fill, false],
    ['Not found / not detected', DISPLAY_STYLE.discoverable.fill, false],
    ['Not visible / no data', DISPLAY_STYLE.invisible.fill, true],
  ];
  for (const [text, colour, hatched] of entries) {
    shape('rect', { x: lx, y: 0, width: 14, height: 10, fill: hatched ? 'url(#iso-hatch)' : colour, stroke: colour }, legend);
    const t = shape('text', { x: lx + 20, y: 9, class: 'iso-legend-text' }, legend);
    t.textContent = text;
    lx += 26 + text.length * 6.6;
  }
  shape('rect', { x: lx, y: -3, width: 3, height: 16, class: 'iso-codon' }, legend);
  shape('text', { x: lx + 9, y: 9, class: 'iso-legend-text' }, legend).textContent = 'Start / stop codon · thick = CDS, thin = UTR';
  const quant = locus.quantifiers.length ? ` · ordered by mean ${ui.quantifier ?? '—'} expression across the panel${locus.expression_units ? ` (${locus.expression_units})` : ''}` : '';
  shape('text', { x: 0, y: 26, class: 'iso-legend-text muted' }, legend).textContent =
    `${locus.annotation_version ?? ''}${quant}`.replace(/^ · /, '');

  const edges = [...new Set(locus.isoforms.flatMap(i => [...i.exons, ...i.cds].flatMap(e => [e.start, e.start + e.length])))];
  bindPointer(svg, scale, x0, x1, length, ui, callbacks, edges);
  return { drawn: rows.map(i => i.transcript_id), scale, x0, x1 };
}

// Drag across the track to select a range; ⌘/Ctrl + wheel zooms, horizontal wheel pans.
let dragMoved = false;
/** The exon under the first click of a possible double-click. */
let pendingExon: { start: number; end: number; at: number } | null = null;
const DOUBLE_CLICK_MS = 800;
const SNAP_PX = 5;
function bindPointer(svg: SVGSVGElement, scale: AxisScale, x0: number, x1: number, length: number, ui: IsoformUi, callbacks: IsoformCallbacks, edges: number[]) {
  // Client → drawing coordinates through the SVG's own transform (exact under any scaling).
  const toSvgX = (event: MouseEvent) => {
    const matrix = svg.getScreenCTM();
    if (!matrix) return 0;
    return new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix.inverse()).x;
  };
  // A pixel can span many bases, so drag ends snap to the nearest exon or CDS edge within a few pixels.
  const position = (x: number) => {
    const clamped = Math.min(x1, Math.max(x0, x));
    let best: number | null = null;
    for (const edge of edges) {
      const d = Math.abs(scale.toX(edge) - clamped);
      if (d <= SNAP_PX && (best === null || d < Math.abs(scale.toX(best) - clamped))) best = edge;
    }
    return best ?? Math.round(Math.min(length, Math.max(0, scale.fromX(clamped))));
  };
  svg.onmousedown = event => {
    if (event.button !== 0) return;
    const startX = toSvgX(event);
    if (startX < x0) return;
    event.preventDefault();
    dragMoved = false;
    let band: SVGElement | null = null;
    const move = (e: MouseEvent) => {
      const x = toSvgX(e);
      if (!dragMoved && Math.abs(x - startX) < 4) return;
      dragMoved = true;
      band ??= shape('rect', { y: ROWS_TOP - 8, height: Number(svg.viewBox.baseVal.height) - ROWS_TOP - 30, class: 'iso-range dragging' }, svg);
      const [a, b] = [Math.max(x0, Math.min(startX, x)), Math.min(x1, Math.max(startX, x))];
      band.setAttribute('x', String(a)); band.setAttribute('width', String(Math.max(1, b - a)));
    };
    const up = (e: MouseEvent) => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
      if (!dragMoved) return;
      const [a, b] = [position(Math.min(startX, toSvgX(e))), position(Math.max(startX, toSvgX(e)))];
      // The click that follows this mouseup must not also select a row.
      setTimeout(() => { dragMoved = false; });
      if (b > a) callbacks.selectRange(a, b);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  };
  svg.onclick = event => {
    if (event.detail < 2 || !pendingExon) return;
    const exon = pendingExon;
    pendingExon = null;
    if (performance.now() - exon.at <= DOUBLE_CLICK_MS) callbacks.selectRange(exon.start, exon.end);
  };
  svg.onwheel = event => {
    if (event.ctrlKey || event.metaKey) {
      event.preventDefault();
      zoom(ui, length, Math.exp(event.deltaY * 0.004), scale.fromX(Math.min(x1, Math.max(x0, toSvgX(event)))));
      callbacks.changed();
    } else if (Math.abs(event.deltaX) > Math.abs(event.deltaY) || event.shiftKey) {
      event.preventDefault();
      const dx = event.shiftKey && !event.deltaX ? event.deltaY : event.deltaX;
      pan(ui, length, (dx / (x1 - x0)) * (ui.end - ui.start));
      callbacks.changed();
    }
  };
}

// ------------------------------------------------------------------ expression popup

const EVIDENCE_WORDS: Record<string, string> = {
  quantified: 'Quantified: matched to a long-read transcript model.',
  discoverable: 'Discoverable: has a junction not seen in the reference annotation, but no long-read model matched it.',
  invisible: 'Not distinguishable: no long-read model matched it and no junction sets it apart.',
};

function el<K extends keyof HTMLElementTagNameMap>(tag: K, text?: string, className?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}

const value = (v: number) => (v === 0 ? '0' : v >= 100 ? v.toFixed(0) : v >= 1 ? v.toFixed(1) : v.toPrecision(2));

/** Expression across the panel for one isoform and quantifier, with its evidence in words. */
export function renderIsoformDetail(container: HTMLElement, locus: LocusView, isoform: Isoform, quantifier: string | null, close: () => void) {
  container.replaceChildren();
  container.hidden = false;
  container.dataset.transcript = isoform.transcript_id;
  const head = el('div', undefined, 'iso-detail-head');
  head.append(el('strong', isoform.transcript_id));
  const closeButton = el('button', '×'); closeButton.type = 'button'; closeButton.title = 'Close'; closeButton.dataset.testid = 'isoform-detail-close';
  closeButton.onclick = close;
  head.append(closeButton);
  container.append(head);
  const facts = [
    isoform.transcript_type, isoform.is_mane_select ? 'MANE Select' : null, isoform.tsl ? `TSL ${isoform.tsl}` : null, isoform.appris,
    isoform.aa_len !== null ? `${isoform.aa_len} aa` : 'non-coding', isoform.cds_start_nf ? '5′ incomplete CDS' : null, isoform.cds_end_nf ? '3′ incomplete CDS' : null,
    `${isoform.exons.length} exon${isoform.exons.length === 1 ? '' : 's'}`,
  ].filter(Boolean).join(' · ');
  container.append(el('p', facts, 'iso-facts'));
  const display = displayOf(isoform, quantifier);
  const state = el('p', undefined, 'iso-state');
  const swatch = el('span', undefined, `iso-swatch-inline${styleOf(display).hatched ? ' hatched' : ''}`);
  swatch.style.setProperty('--swatch', styleOf(display).fill);
  state.append(swatch, `${styleOf(display).words[0].toUpperCase()}${styleOf(display).words.slice(1)}`);
  state.dataset.testid = 'isoform-detail-state'; state.dataset.display = display;
  container.append(state);
  const evidence = EVIDENCE_WORDS[isoform.evidence_state] ?? isoform.evidence_state;
  const extra = isoform.evidence_state === 'discoverable' && isoform.novel_junctions ? ` (${isoform.novel_junctions} novel junction${isoform.novel_junctions === 1 ? '' : 's'})` : '';
  container.append(el('p', evidence + extra, 'iso-evidence'));
  for (const m of isoform.mappings) {
    container.append(el('p', `↳ ${m.source_transcript_id}: ${m.status.replaceAll('_', ' ')}${m.same_exon_chain === false ? ' (different exon chain)' : ''}`, 'iso-mapping'));
  }
  const panel = panelFor(locus, quantifier);
  const expression = isoform.expression.find(e => e.quantifier === quantifier);
  if (!panel) { container.append(el('p', 'No expression data in this locus bundle.', 'iso-note')); return; }
  const units = locus.expression_units ? ` (${locus.expression_units})` : '';
  container.append(el('h4', `${quantifier}${units} · panel mean ${expression?.panel_mean != null ? value(expression.panel_mean) : '—'}`));
  const cells = new Map((expression?.cells ?? []).map(c => [c.cell_line, c]));
  const lines = [...new Set([...panel.cell_lines, ...cells.keys()])];
  const top = Math.max(1e-12, ...[...cells.values()].map(c => c.max));
  const table = el('div', undefined, 'iso-bars');
  table.dataset.testid = 'isoform-expression';
  for (const line of lines) {
    const cell = cells.get(line);
    const row = el('div', undefined, 'iso-bar-row');
    row.dataset.testid = 'isoform-cell'; row.dataset.cellLine = line;
    row.dataset.mean = cell ? String(cell.mean) : '';
    row.append(el('span', line, 'iso-cell'));
    const bar = el('span', undefined, 'iso-bar');
    if (cell) {
      const fill = el('span', undefined, 'iso-bar-fill');
      fill.style.width = `${(cell.mean / top) * 100}%`;
      fill.style.background = styleOf(display).fill;
      const whisker = el('span', undefined, 'iso-bar-range');
      whisker.style.left = `${(cell.min / top) * 100}%`; whisker.style.width = `${((cell.max - cell.min) / top) * 100}%`;
      bar.append(fill, whisker);
      bar.title = `mean ${value(cell.mean)} · min ${value(cell.min)} · max ${value(cell.max)} · ${cell.n_libraries} librar${cell.n_libraries === 1 ? 'y' : 'ies'}`;
    }
    row.append(bar);
    row.append(el('span', cell ? `${value(cell.mean)} (n=${cell.n_libraries})` : panel.reports_zeros ? 'not measured' : '0 (not reported)', 'iso-value'));
    table.append(row);
  }
  container.append(table);
  if (!panel.reports_zeros) container.append(el('p', `${quantifier} omits zero values; cell lines without a row count as 0 in the panel mean.`, 'iso-note'));
  const others = isoform.expression.filter(e => e.quantifier !== quantifier);
  if (others.length) {
    container.append(el('p', `Other quantifiers: ${others.map(e => `${e.quantifier} ${e.panel_mean != null ? value(e.panel_mean) : '—'}`).join(' · ')}`, 'iso-note'));
  }
}

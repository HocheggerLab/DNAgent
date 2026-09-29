// Duplex sequence view. Amino acids, frames and ORFs are engine output (bindings.ts);
// this module only places them: a codon's letter sits under its middle base.
import type { Document, Feature, FeatureTranslation, FrameTranslation, Orf } from './bindings';
import { contains, featureColor } from './map-layout';
import { textOn } from './map-geometry';
import { rowSpans } from './sequence-layout';

export interface SequenceOptions {
  aminoAcids: 'one' | 'three';
  showFrames: boolean;
  showOrfs: boolean;
  orfMinCodons: number;
}

export interface SequenceContext {
  activeFeature: Feature | undefined;
  activeOrf: Orf | undefined;
  range: { start: number; end: number } | null;
  options: SequenceOptions;
  columns: number;
  select: (id: string, extend?: boolean) => void;
  selectOrf: (id: string) => void;
}

// Display names only; the genetic code itself lives in Rust.
const THREE_LETTER: Record<string, string> = {
  A: 'Ala', R: 'Arg', N: 'Asn', D: 'Asp', C: 'Cys', Q: 'Gln', E: 'Glu', G: 'Gly', H: 'His', I: 'Ile', L: 'Leu', K: 'Lys',
  M: 'Met', F: 'Phe', P: 'Pro', S: 'Ser', T: 'Thr', W: 'Trp', Y: 'Tyr', V: 'Val', X: 'Xaa', U: 'Sec', O: 'Pyl', '*': 'Ter',
};

/** Codon placements of an engine frame: middle reference base of codon i (display arithmetic). */
export function frameMiddles(frame: FrameTranslation): number[] {
  return Array.from(frame.protein, (_, i) => frame.strand === 'reverse' ? frame.first - 3 * i - 1 : frame.first + 3 * i + 1);
}

/** Bases per row that fit the available width, in multiples of ten. */
export function columnsFor(width: number, basePx: number, captionPx: number): number {
  const fit = Math.floor((width - captionPx - 24) / basePx / 10) * 10;
  return Math.max(30, Math.min(200, fit));
}

interface Letter { middle: number; codon: number; amino: string; span?: [number, number] }

function translationLetters(t: FeatureTranslation): Letter[] {
  return Array.from(t.protein, (amino, codon) => {
    const [p0, p1, p2] = t.codon_positions.slice(codon * 3, codon * 3 + 3);
    const low = Math.min(p0, p2);
    // A codon whose bases are adjacent can be drawn across its three columns.
    return { middle: p1, codon, amino, span: Math.abs(p2 - p0) === 2 ? [low, low + 3] as [number, number] : undefined };
  });
}

function frameLetters(frame: FrameTranslation): Letter[] {
  return frameMiddles(frame).map((middle, codon) => ({ middle, codon, amino: frame.protein[codon], span: [middle - 1, middle + 2] }));
}

/** Renders the view; returns base elements by reference position (forward, complement) for drag feedback. */
export function renderSequence(target: HTMLElement, doc: Document, context: SequenceContext): HTMLElement[][] {
  const { activeFeature, activeOrf, range, options, columns } = context;
  target.replaceChildren();
  target.style.setProperty('--columns', String(columns));
  const length = doc.sequence.length;
  const orfs = options.showOrfs ? doc.orfs.filter(orf => orf.codons >= options.orfMinCodons) : [];
  const orfContains = (orf: Orf, base: number) => (base - orf.start + length) % length < orf.length;
  const highlighted = (base: number) => (activeFeature !== undefined && contains(activeFeature, base, length))
    || (activeOrf !== undefined && orfContains(activeOrf, base))
    || (range !== null && (range.start < range.end ? base >= range.start && base < range.end : base >= range.start || base < range.end));
  const byFeature = new Map(doc.translations.map(t => [t.feature_id, t]));
  const letters = new Map(doc.translations.map(t => [t.feature_id, translationLetters(t)]));
  const frames = options.showFrames ? doc.frames.map(frame => ({ frame, letters: frameLetters(frame) })) : [];
  const bases: HTMLElement[][] = Array.from({ length }, () => []);

  for (let start = 0; start < length; start += columns) {
    const end = Math.min(start + columns, length);
    const block = document.createElement('section'); block.className = 'sequence-block'; block.dataset.rowStart = String(start);
    const line = (label: string, className: string, title = '') => {
      const row = document.createElement('div'); row.className = `sequence-line ${className}`;
      const caption = document.createElement('span'); caption.className = 'line-caption'; caption.textContent = label;
      if (title) caption.title = title;
      const grid = document.createElement('div'); grid.className = 'base-grid';
      row.append(caption, grid); block.append(row); return grid;
    };
    const aminoRow = (grid: HTMLElement, items: Letter[], className: string) => {
      for (const item of items) {
        if (item.middle < start || item.middle >= end) continue;
        const cell = document.createElement('span');
        cell.className = `${className}${item.amino === '*' ? ' stop' : ''}${item.codon % 2 ? ' odd' : ''}`;
        cell.dataset.testid = 'amino-acid'; cell.dataset.codonIndex = String(item.codon); cell.dataset.codonMiddle = String(item.middle);
        const three = options.aminoAcids === 'three' && item.span && item.span[0] >= start && item.span[1] <= end;
        cell.textContent = options.aminoAcids === 'three' ? (THREE_LETTER[item.amino] ?? item.amino) : item.amino;
        cell.style.gridRow = '1'; // reverse-strand codons arrive right to left
        cell.style.gridColumn = three ? `${item.span![0] - start + 1} / ${item.span![1] - start + 1}` : `${item.middle - start + 1}`;
        if (options.aminoAcids === 'three' && !three) cell.classList.add('compact');
        cell.title = `codon ${item.codon + 1} · ${THREE_LETTER[item.amino] ?? item.amino}`;
        grid.append(cell);
      }
    };

    const ruler = line('position', 'coordinate-line');
    for (let pos = start; pos < end; pos += 10) {
      const tick = document.createElement('span'); tick.textContent = String(pos);
      tick.style.gridColumn = `${pos - start + 1} / span ${Math.min(10, end - pos)}`; ruler.append(tick);
    }
    for (const { frame, letters: frameItems } of frames.filter(f => f.frame.strand === 'forward')) {
      const grid = line(`frame +${frame.offset + 1}`, 'frame-line'); grid.dataset.testid = 'frame-row'; grid.dataset.frame = `+${frame.offset + 1}`;
      aminoRow(grid, frameItems, 'amino');
    }
    for (const [label, sequence, strand] of [['5′ → 3′', doc.sequence, 'forward'], ['3′ → 5′', doc.aligned_complement_3to5, 'complement']]) {
      const grid = line(label, 'strand-line'); grid.dataset.strand = strand;
      for (let pos = start; pos < end; pos++) {
        const base = document.createElement(highlighted(pos) ? 'mark' : 'span');
        base.dataset.position = String(pos); base.textContent = sequence[pos];
        base.title = `Reference position ${pos}`; grid.append(base); bases[pos].push(base);
      }
    }
    for (const { frame, letters: frameItems } of frames.filter(f => f.frame.strand === 'reverse')) {
      const grid = line(`frame −${frame.offset + 1}`, 'frame-line'); grid.dataset.testid = 'frame-row'; grid.dataset.frame = `-${frame.offset + 1}`;
      aminoRow(grid, frameItems, 'amino');
    }
    for (const feature of doc.features) {
      const spans = feature.parts.flatMap((part, partIndex) => rowSpans(part, start, end, length).map(span => ({ ...span, partIndex })));
      if (!spans.length) continue;
      const color = featureColor(feature);
      const grid = line(feature.strand === 'reverse' ? '← feature' : feature.strand === 'forward' ? 'feature →' : 'feature', 'annotation-line');
      for (const span of spans) {
        const button = document.createElement('button');
        button.className = 'feature-track'; button.dataset.testid = 'sequence-track';
        button.dataset.featureId = feature.id; button.dataset.partIndex = String(span.partIndex);
        button.classList.toggle('selected', feature.id === activeFeature?.id);
        if (feature.strand === 'forward' && span.sourceEnd) button.classList.add('arrow-right');
        if (feature.strand === 'reverse' && span.sourceStart) button.classList.add('arrow-left');
        button.style.gridColumn = `${span.start + 1} / ${span.end + 1}`;
        button.style.gridRow = '1';
        button.style.setProperty('--feature-color', color);
        button.style.setProperty('--feature-text', textOn(color));
        button.textContent = feature.label || feature.kind;
        button.title = `${feature.label || feature.kind} · ${feature.strand} · ${feature.parts.map(p => `[${p.start}, ${p.start + p.length})`).join(', ')}`;
        button.setAttribute('aria-label', button.title);
        button.setAttribute('aria-pressed', String(feature.id === activeFeature?.id));
        button.onclick = event => context.select(feature.id, event.shiftKey);
        grid.append(button);
      }
      const translation = byFeature.get(feature.id);
      const items = letters.get(feature.id);
      if (translation && items && items.some(item => item.middle >= start && item.middle < end)) {
        const warn = translation.warnings.length ? ' ⚠' : '';
        const grid2 = line(`aa${warn}`, 'translation-line', translation.warnings.map(w => w.message).join('\n'));
        grid2.dataset.testid = 'translation-row'; grid2.dataset.featureId = feature.id;
        grid2.style.setProperty('--feature-color', color);
        aminoRow(grid2, items, 'amino');
      }
    }
    for (const orf of orfs) {
      const spans = rowSpans({ start: orf.start, length: orf.length }, start, end, length);
      if (!spans.length) continue;
      const grid = line(orf.strand === 'reverse' ? '← ORF' : 'ORF →', 'orf-line');
      for (const span of spans) {
        const button = document.createElement('button');
        button.className = 'orf-track'; button.dataset.testid = 'orf-track'; button.dataset.orfId = orf.id;
        button.classList.toggle('selected', orf.id === activeOrf?.id);
        if (orf.strand === 'forward' && span.sourceEnd) button.classList.add('arrow-right');
        if (orf.strand === 'reverse' && span.sourceStart) button.classList.add('arrow-left');
        button.style.gridColumn = `${span.start + 1} / ${span.end + 1}`;
        button.textContent = `ORF ${orf.codons} aa`;
        button.title = `ORF · ${orf.strand} · [${orf.start}, ${orf.start + orf.length}) · ${orf.codons} aa`;
        button.setAttribute('aria-pressed', String(orf.id === activeOrf?.id));
        button.onclick = () => context.selectOrf(orf.id);
        grid.append(button);
      }
    }
    target.append(block);
  }
  return bases;
}

/**
 * Translation of a selected range, sliced from the engine's whole-molecule frames:
 * forward codons start at `start`; reverse codons end at `end - 1`. Only codons wholly
 * inside the range are used, matching `dnagent translate --range`.
 */
export function rangeTranslation(doc: Document, start: number, end: number, strand: 'forward' | 'reverse'): string {
  const frame = doc.frames.find(f => f.strand === strand && (strand === 'forward'
    ? (start - f.first) % 3 === 0 && start >= f.first
    : (f.first - (end - 1)) % 3 === 0 && end - 1 <= f.first));
  if (!frame) return '';
  const first = strand === 'forward' ? (start - frame.first) / 3 : (frame.first - (end - 1)) / 3;
  const count = Math.floor((end - start) / 3);
  return frame.protein.slice(first, first + count);
}

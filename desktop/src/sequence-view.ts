import type { Document, Feature } from './bindings';
import { contains, featureColor } from './map-layout';
import { rowSpans } from './sequence-layout';

export function renderSequence(target: HTMLElement, doc: Document, active: Feature | undefined, select: (id: string) => void) {
  target.replaceChildren();
  const columns = 60;
  for (let start = 0; start < doc.sequence.length; start += columns) {
    const end = Math.min(start + columns, doc.sequence.length);
    const block = document.createElement('section'); block.className = 'sequence-block'; block.dataset.rowStart = String(start);
    const line = (label: string, className: string) => {
      const row = document.createElement('div'); row.className = `sequence-line ${className}`;
      const caption = document.createElement('span'); caption.className = 'line-caption'; caption.textContent = label;
      const grid = document.createElement('div'); grid.className = 'base-grid';
      row.append(caption,grid); block.append(row); return grid;
    };
    const ruler = line('position', 'coordinate-line');
    for (let pos = start; pos < end; pos += 10) {
      const tick = document.createElement('span'); tick.textContent = String(pos);
      tick.style.gridColumn = `${pos - start + 1} / span ${Math.min(10,end - pos)}`; ruler.append(tick);
    }
    for (const [label,sequence,strand] of [['5′ → 3′',doc.sequence,'forward'],['3′ → 5′',doc.aligned_complement_3to5,'complement']]) {
      const grid = line(label,'strand-line'); grid.dataset.strand = strand;
      for (let pos = start; pos < end; pos++) {
        const base = document.createElement(active && contains(active,pos,doc.sequence.length) ? 'mark' : 'span');
        base.dataset.position = String(pos); base.textContent = sequence[pos];
        base.title = `Reference position ${pos}`; grid.append(base);
      }
    }
    for (const feature of doc.features) {
      const spans = feature.parts.flatMap((part,partIndex) => rowSpans(part,start,end,doc.sequence.length).map(span => ({...span,partIndex})));
      if (!spans.length) continue;
      const grid = line(feature.strand === 'reverse' ? '← feature' : feature.strand === 'forward' ? 'feature →' : 'feature', 'annotation-line');
      for (const span of spans) {
        const button = document.createElement('button');
        button.className = 'feature-track'; button.dataset.testid = 'sequence-track';
        button.dataset.featureId = feature.id; button.dataset.partIndex = String(span.partIndex);
        button.classList.toggle('selected',feature.id === active?.id);
        if (feature.strand === 'forward' && span.sourceEnd) button.classList.add('arrow-right');
        if (feature.strand === 'reverse' && span.sourceStart) button.classList.add('arrow-left');
        button.style.gridColumn = `${span.start + 1} / ${span.end + 1}`;
        button.style.gridRow = '1';
        button.style.setProperty('--feature-color',featureColor(feature));
        button.textContent = feature.label || feature.kind;
        button.title = `${feature.label || feature.kind} · ${feature.strand} · ${feature.parts.map(p => `[${p.start}, ${p.start+p.length})`).join(', ')}`;
        button.setAttribute('aria-label',button.title);
        button.setAttribute('aria-pressed',String(feature.id === active?.id));
        button.onclick = () => select(feature.id);
        grid.append(button);
      }
    }
    target.append(block);
  }
}

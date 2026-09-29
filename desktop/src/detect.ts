// Detect features panel: lists the engine's library matches (bindings.ts) with a tick
// box each. Which proposals start ticked is a display choice made here: new ones that
// are not inside a longer proposal. No matching happens in TypeScript.
import type { DetectionResult, FeatureRequest, Proposal } from './bindings';
import { featureColor } from './map-layout';

export interface Detection { documentId: number; result: DetectionResult; checked: Set<string> }

/** Ticked by default: not annotated yet, and not nested in a longer proposal. */
export function defaultChecked(result: DetectionResult): Set<string> {
  return new Set(result.proposals.filter(p => p.annotated_as.length === 0 && p.contained_in === null).map(p => p.key));
}

/** Feature requests for the ticked proposals, in proposal order. */
export function requestsFor(detection: Detection, length: number): FeatureRequest[] {
  return detection.result.proposals.filter(p => detection.checked.has(p.key)).map(p => ({
    start: p.start,
    end: (p.start + p.length) % length || length,
    strand: p.strand,
    kind: p.kind,
    label: p.name,
    color: p.color,
    translate: p.translate ? { table: p.translate[0], codon_start: p.translate[1] } : null,
    note: `Detected with the DNAgent feature library (entry ${p.library_id})`,
  }));
}

const where = (p: Proposal) => `${p.start.toLocaleString()}–${(p.start + p.length).toLocaleString()} · ${p.strand === 'forward' ? '→' : p.strand === 'reverse' ? '←' : '·'}`;

export function renderDetection(target: HTMLElement, detection: Detection, handlers: { toggle: (key: string) => void; select: (p: Proposal) => void }) {
  const { result, checked } = detection;
  const byKey = new Map(result.proposals.map(p => [p.key, p]));
  target.replaceChildren(...result.proposals.map(p => {
    const row = document.createElement('div');
    row.className = `detect-row${p.annotated_as.length ? ' annotated' : ''}`;
    row.dataset.testid = 'detect-row'; row.dataset.key = p.key; row.dataset.libraryId = String(p.library_id);
    row.dataset.start = String(p.start); row.dataset.length = String(p.length); row.dataset.strand = p.strand;
    const box = document.createElement('input'); box.type = 'checkbox'; box.dataset.testid = 'detect-check';
    box.checked = checked.has(p.key); box.setAttribute('aria-label', `Add ${p.name}`);
    box.onchange = () => handlers.toggle(p.key);
    const label = document.createElement('button'); label.type = 'button'; label.className = 'detect-label'; label.dataset.testid = 'detect-select';
    label.style.borderLeftColor = featureColor({ id: p.key, label: p.name, kind: p.kind, color: p.color, strand: p.strand, parts: [] });
    const name = document.createElement('span'); name.className = 'name'; name.dataset.testid = 'detect-name'; name.textContent = p.name;
    const detail = document.createElement('span'); detail.className = 'detail';
    const outer = p.contained_in ? byKey.get(p.contained_in) : undefined;
    detail.textContent = [p.kind, `${p.length.toLocaleString()} bp`, where(p),
      p.annotated_as.length ? 'already annotated' : '', outer ? `inside ${outer.name}` : ''].filter(Boolean).join(' · ');
    label.append(name, detail);
    label.title = 'Select on the map and sequence';
    label.onclick = () => handlers.select(p);
    row.append(box, label);
    return row;
  }));
}

import { invoke } from '@tauri-apps/api/core';
import type { Document, Diagnostic, Feature } from './bindings';
import './style.css';

const element = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
let current: Document | null = null;
let selected: string | null = null;
let revision = 0;
const ns = 'http://www.w3.org/2000/svg';

function contains(feature: Feature, base: number, length: number): boolean {
  return feature.parts.some(part => (base - part.start + length) % length < part.length);
}

function select(id: string) {
  selected = id;
  render();
  document.querySelector('#sequence mark')?.scrollIntoView({ block: 'nearest' });
}

function render() {
  if (!current) return;
  const doc = current;
  const length = doc.sequence.length;
  const active = doc.features.find(f => f.id === selected);
  element('title').textContent = `${doc.name} · ${length.toLocaleString()} bp · ${doc.circular ? 'circular' : 'linear'}`;
  element('selection').textContent = active
    ? `${active.label} · ${active.strand} strand · source parts ${active.parts.map(p => `[${p.start}, ${p.start + p.length})`).join(', ')} (zero-based; circular parts may wrap)`
    : 'Select a feature. Sequence is always shown in the forward reference orientation.';
  const list = element('features'); list.replaceChildren();
  for (const feature of doc.features) {
    const button = document.createElement('button');
    button.textContent = `${feature.label || feature.kind} (${feature.strand})`;
    button.classList.toggle('selected', feature.id === selected);
    button.setAttribute('aria-pressed', String(feature.id === selected));
    button.onclick = () => select(feature.id);
    list.append(button);
  }
  const map = document.getElementById('map')!; map.replaceChildren();
  const shape = (tag: string, attributes: Record<string, string>) => {
    const node = document.createElementNS(ns, tag);
    for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, value);
    map.append(node); return node;
  };
  if (doc.circular) shape('circle', { cx:'300', cy:'170', r:'110', fill:'none', stroke:'#aab6c3', 'stroke-width':'2' });
  else shape('path', { d:'M40 170 L560 170', stroke:'#aab6c3', 'stroke-width':'2' });
  doc.features.forEach((feature, index) => {
    for (const part of feature.parts) {
      // Sample the interval so a full-circle feature and origin crossings remain visible.
      const steps = Math.max(2, Math.ceil(part.length / length * 180));
      const radius = 100 + (index % 4) * 9;
      const points = Array.from({length: steps + 1}, (_, i) => {
        const fraction = (part.start + part.length * i / steps) / length;
        if (!doc.circular) return `${40 + fraction * 520},${145 + (index % 5) * 12}`;
        const angle = fraction * Math.PI * 2 - Math.PI / 2;
        return `${300 + Math.cos(angle) * radius},${170 + Math.sin(angle) * radius}`;
      });
      const node = shape('polyline', { points:points.join(' '), fill:'none', stroke:feature.id === selected ? '#db6b26' : '#267c91', 'stroke-width':feature.id === selected ? '8' : '5', tabindex:'0', role:'button', 'aria-label':`${feature.label}, ${feature.strand} strand` });
      node.addEventListener('click', () => select(feature.id));
      node.addEventListener('keydown', event => { if ((event as KeyboardEvent).key === 'Enter') select(feature.id); });
      const title = document.createElementNS(ns, 'title'); title.textContent = `${feature.label} (${feature.strand})`; node.append(title);
    }
  });
  const sequence = element('sequence'); sequence.replaceChildren();
  for (let start = 0; start < length; start += 80) {
    const row = document.createElement('div');
    const label = document.createElement('span'); label.className = 'offset'; label.textContent = String(start).padStart(6); row.append(label);
    for (let pos = start; pos < Math.min(start + 80, length); pos++) {
      const base = document.createElement(active && contains(active, pos, length) ? 'mark' : 'span');
      base.textContent = doc.sequence[pos];
      base.dataset.position = String(pos);
      row.append(base);
    }
    sequence.append(row);
  }
}

element('sequence').onclick = event => {
  const position = (event.target as HTMLElement).dataset.position;
  if (position === undefined || !current) return;
  const matches = current.features.filter(f => contains(f, Number(position), current!.sequence.length));
  if (matches.length) select(matches[(matches.findIndex(f => f.id === selected) + 1) % matches.length].id);
};

element('open').onsubmit = async event => {
  event.preventDefault();
  const request = ++revision;
  element('status').textContent = 'Loading…';
  try {
    const result = await invoke<Document>('open_document', { path: element<HTMLInputElement>('path').value });
    if (request !== revision) return;
    current = result; selected = null;
    element('warnings').textContent = result.warnings.map(w => `${w.code}: ${w.message}`).join('\n');
    element('status').textContent = 'Read-only preview. Click sequence bases to select overlapping features; repeat clicks cycle overlaps.';
    render();
  } catch (error) {
    if (request !== revision) return;
    const diagnostic = error as Partial<Diagnostic>;
    element('status').textContent = `Open failed: ${diagnostic.message ?? String(error)}. Previous document remains unchanged.`;
  }
};

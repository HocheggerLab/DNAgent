import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { Document, Diagnostic, Feature } from './bindings';
import { contains, displayEndpoints, featureColor, labelPositions } from './map-layout';
import './style.css';

const element = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
let current: Document | null = null;
let selected: string | null = null;
let revision = 0;
const ns = 'http://www.w3.org/2000/svg';

function select(id: string) {
  selected = id;
  render();
  document.querySelector('#sequence mark')?.scrollIntoView({ block: 'nearest' });
}

function renderMap(doc: Document) {
  const map = document.getElementById('map')!; map.replaceChildren();
  const length = doc.sequence.length;
  const height = Math.max(440, doc.features.length * 20 + 60);
  const cy = height / 2;
  map.setAttribute('viewBox', `0 0 900 ${height}`);
  const shape = (tag: string, attributes: Record<string, string>, parent: Element = map) => {
    const node = document.createElementNS(ns, tag);
    for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, value);
    parent.append(node); return node;
  };
  const interactive = (node: Element, feature: Feature) => {
    node.setAttribute('tabindex', '0'); node.setAttribute('role', 'button');
    node.setAttribute('aria-label', `${feature.label || feature.kind}, ${feature.strand} strand`);
    node.addEventListener('click', () => select(feature.id));
    node.addEventListener('keydown', event => {
      if (['Enter', ' '].includes((event as KeyboardEvent).key)) { event.preventDefault(); select(feature.id); }
    });
    const title = shape('title', {}, node); title.textContent = `${feature.label} (${feature.strand})`;
  };
  const defs = shape('defs', {});
  if (doc.circular) {
    shape('circle', {cx:'450',cy:String(cy),r:'145',fill:'none',stroke:'#aab6c3','stroke-width':'2'});
    const title = shape('text', {x:'450',y:String(cy),'text-anchor':'middle',class:'map-center'});
    title.textContent = `${length.toLocaleString()} bp`;
  } else shape('path', {d:`M280 ${cy} L620 ${cy}`,stroke:'#aab6c3','stroke-width':'2'});
  const anchors: {feature: Feature; x:number; y:number; right:boolean; color:string}[] = [];
  doc.features.forEach((feature, index) => {
    const color = featureColor(feature);
    const radius = 132 + (index % 4) * 11;
    const point = (base: number): [number, number] => {
      if (!doc.circular) return [280 + base / length * 340, cy - 30 + (index % 5) * 14];
      const angle = base / length * Math.PI * 2 - Math.PI / 2;
      return [450 + Math.cos(angle) * radius, cy + Math.sin(angle) * radius];
    };
    const marker = shape('marker', {id:`arrow-${index}`,viewBox:'0 0 10 10',refX:'9',refY:'5',markerWidth:'8',markerHeight:'8',orient:'auto',markerUnits:'userSpaceOnUse'}, defs);
    shape('path', {d:'M0 0 L10 5 L0 10 Z',fill:color}, marker);
    for (const part of feature.parts) {
      const steps = Math.max(2, Math.ceil(part.length / length * 180));
      const [start, end] = displayEndpoints(part.start, part.length, feature.strand === 'reverse');
      const points = Array.from({length:steps + 1}, (_, i) => point(start + (end - start) * i / steps).join(',')).join(' ');
      if (feature.id === selected) shape('polyline', {points,fill:'none',stroke:'#203344','stroke-width':'10',opacity:'0.3'});
      const node = shape('polyline', {points,fill:'none',stroke:color,'stroke-width':'5',...(feature.strand === 'unknown' ? {} : {'marker-end':`url(#arrow-${index})`})});
      interactive(node, feature);
    }
    // One leader per feature, anchored to its longest part, not an inferred joined CDS.
    const longest = feature.parts.reduce((a, b) => b.length > a.length ? b : a);
    const [x,y] = point(longest.start + longest.length / 2);
    anchors.push({feature,x,y,right:x >= 450,color});
  });
  for (const right of [false,true]) {
    const side = anchors.filter(anchor => anchor.right === right);
    const positions = labelPositions(side.map(anchor => anchor.y),height);
    side.forEach((anchor,index) => {
      const x = right ? 655 : 245;
      const y = positions[index];
      shape('polyline', {points:`${anchor.x},${anchor.y} ${right ? 635 : 265},${y} ${x},${y}`,fill:'none',stroke:anchor.color,'stroke-width':'1',opacity:'0.65'});
      const label = shape('text', {x:String(x + (right ? 4 : -4)),y:String(y + 4),'text-anchor':right ? 'start' : 'end',class:`map-label${anchor.feature.id === selected ? ' active' : ''}`,fill:anchor.color});
      const name = anchor.feature.label || anchor.feature.kind;
      label.textContent = name.length > 28 ? name.slice(0,27) + '…' : name;
      interactive(label,anchor.feature);
    });
  }
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
    button.style.borderLeft = `5px solid ${featureColor(feature)}`;
    button.classList.toggle('selected', feature.id === selected);
    button.setAttribute('aria-pressed', String(feature.id === selected));
    button.onclick = () => select(feature.id);
    list.append(button);
  }
  renderMap(doc);
  const sequence = element('sequence'); sequence.replaceChildren();
  const ruler = document.createElement('div'); ruler.className = 'ruler';
  ruler.textContent = 'offset  ' + Array.from({length:8}, (_,i) => String(i * 10).padEnd(11)).join('');
  sequence.append(ruler);
  for (let start = 0; start < length; start += 80) {
    const row = document.createElement('div');
    const label = document.createElement('span'); label.className = 'offset'; label.textContent = String(start).padStart(6); row.append(label);
    for (let pos = start; pos < Math.min(start + 80, length); pos++) {
      if (pos > start && (pos - start) % 10 === 0) row.append(document.createTextNode(' '));
      const base = document.createElement(active && contains(active, pos, length) ? 'mark' : 'span');
      base.textContent = doc.sequence[pos]; base.dataset.position = String(pos); row.append(base);
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

async function load(path: string) {
  const request = ++revision;
  element('status').textContent = 'Loading…';
  try {
    const result = await invoke<Document>('open_document', { path });
    if (request !== revision) return;
    current = result; selected = null;
    element('warnings').replaceChildren();
    if (result.warnings.length) {
      const details = document.createElement('details');
      const summary = document.createElement('summary');
      summary.textContent = `${result.warnings.length} import-fidelity warnings — review details`;
      const explanation = document.createElement('p');
      explanation.textContent = 'Some source content is not interpreted by DNAagent. These warnings do not by themselves indicate a sequence error. Retained packets are not exposed in this viewer or guaranteed to survive derived exports.';
      const messages = document.createElement('pre');
      messages.textContent = result.warnings.map(w => `${w.code}: ${w.message}`).join('\n');
      details.append(summary,explanation,messages); element('warnings').append(details);
    }
    element('status').textContent = 'Read-only preview. Arrows indicate strand direction for each source part; unknown strands have no arrows. Click sequence bases to cycle overlapping features.';
    render();
  } catch (error) {
    if (request !== revision) return;
    const diagnostic = error as Partial<Diagnostic>;
    element('status').textContent = `Open failed: ${diagnostic.message ?? String(error)}. Previous document remains unchanged.`;
  }
}

element('open').onsubmit = event => { event.preventDefault(); void load(element<HTMLInputElement>('path').value); };
element('browse').onclick = async () => {
  try {
    const path = await open({multiple:false,directory:false,filters:[{name:'DNA constructs',extensions:['dna','fa','fasta','fna']}]});
    if (typeof path === 'string') { element<HTMLInputElement>('path').value = path; await load(path); }
  } catch (error) { element('status').textContent = `File picker failed: ${String(error)}`; }
};

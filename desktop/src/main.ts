import type { Document, Diagnostic } from './bindings';
import { contains, featureColor } from './map-layout';
import { openDocument, pickConstructPath } from './ipc';
import { renderMap } from './map-view';
import { columnsFor, rangeTranslation, renderSequence, type SequenceOptions } from './sequence-view';
import { initTheme } from './theme';
import './style.css';

const element = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
let current: Document | null = null;
let selected: string | null = null;
/** At most one of selected / selectedOrf / range is set. */
let selectedOrf: string | null = null;
let range: { start: number; end: number } | null = null;
let baseIndex: HTMLElement[][] = [];
const OPTIONS_KEY = 'dnagent.viewOptions';
const options: SequenceOptions = { aminoAcids: 'one', showFrames: false, showOrfs: false, orfMinCodons: 75 };
try { Object.assign(options, JSON.parse(localStorage.getItem(OPTIONS_KEY) ?? '{}')); } catch { /* defaults */ }
let revision = 0;
let activeTab: 'map' | 'sequence' = 'map';
/** Features the last map render could not label, for the notice and list badges. */
let unlabelled: { doc: Document | null; ids: Set<string> } = { doc: null, ids: new Set() };
initTheme(element<HTMLSelectElement>('theme'));

function select(id: string) {
  selected = id; selectedOrf = null; range = null;
  render();
  if (activeTab === 'sequence') revealSelection();
}

function selectOrf(id: string) {
  selectedOrf = id; selected = null; range = null;
  render();
  if (activeTab === 'sequence') revealSelection();
}

function revealSelection() {
  document.querySelector('#sequence mark')?.scrollIntoView({ block: 'center', inline: 'nearest' });
}

function saveOptions() {
  try { localStorage.setItem(OPTIONS_KEY, JSON.stringify(options)); } catch { /* not persisted */ }
}

function bindOptions() {
  const orfs = element<HTMLInputElement>('opt-orfs');
  const minimum = element<HTMLSelectElement>('opt-orf-min');
  const frames = element<HTMLInputElement>('opt-frames');
  const amino = element<HTMLSelectElement>('opt-aa');
  orfs.checked = options.showOrfs; frames.checked = options.showFrames;
  minimum.value = String(options.orfMinCodons); amino.value = options.aminoAcids;
  orfs.onchange = () => { options.showOrfs = orfs.checked; if (!orfs.checked) selectedOrf = null; saveOptions(); render(); };
  minimum.onchange = () => { options.orfMinCodons = Number(minimum.value); saveOptions(); render(); };
  frames.onchange = () => { options.showFrames = frames.checked; saveOptions(); render(); };
  amino.onchange = () => { options.aminoAcids = amino.value === 'three' ? 'three' : 'one'; saveOptions(); render(); };
  element<HTMLSelectElement>('range-strand').onchange = () => render();
}
bindOptions();

/** Monospace column width in px (1.2ch at the sequence font), for responsive rows. */
function sequenceColumns(): number {
  const probe = document.createElement('span');
  probe.style.cssText = 'position:absolute;visibility:hidden;font:14px ui-monospace,SFMono-Regular,Menlo,monospace;width:12ch';
  document.body.append(probe);
  const basePx = probe.getBoundingClientRect().width / 10;
  probe.remove();
  return columnsFor(element('panel-sequence').clientWidth, basePx, basePx / 1.2 * 11);
}

function showTab(tab: 'map' | 'sequence') {
  activeTab = tab;
  for (const name of ['map', 'sequence'] as const) {
    element(`panel-${name}`).hidden = name !== tab;
    element(`tab-${name}`).setAttribute('aria-selected', String(name === tab));
    element(`tab-${name}`).tabIndex = name === tab ? 0 : -1;
  }
  render();
  if (tab === 'sequence') revealSelection();
}

for (const name of ['map', 'sequence'] as const) {
  element(`tab-${name}`).onclick = () => showTab(name);
  element(`tab-${name}`).onkeydown = event => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === 'Home' ? 'map' : event.key === 'End' ? 'sequence' : name === 'map' ? 'sequence' : 'map';
    showTab(next); element(`tab-${next}`).focus();
  };
}

function render() {
  if (!current) return;
  const doc = current;
  const length = doc.sequence.length;
  const active = doc.features.find(f => f.id === selected);
  const activeOrf = selectedOrf === null ? undefined : doc.orfs.find(o => o.id === selectedOrf);
  document.body.dataset.tab = activeTab;
  element('title').textContent = `${doc.name} · ${length.toLocaleString()} bp · ${doc.circular ? 'circular' : 'linear'}`;
  element('selection').textContent = active
    ? `${active.label} · ${active.strand} strand · source parts ${active.parts.map(p => `[${p.start}, ${p.start + p.length})`).join(', ')} (zero-based; circular parts may wrap)`
    : activeOrf
      ? `ORF · ${activeOrf.strand} strand · [${activeOrf.start}, ${activeOrf.start + activeOrf.length}) · ${activeOrf.codons} aa + stop (computational, not an annotated gene)`
      : range
        ? `Selection [${range.start}, ${range.end}) · ${range.end - range.start} bp`
        : 'Select a feature, ORF or drag across bases. Sequence is always shown in the forward reference orientation.';
  const rangePanel = element('range-panel');
  rangePanel.hidden = range === null;
  if (range) {
    const strand = element<HTMLSelectElement>('range-strand').value === 'reverse' ? 'reverse' : 'forward';
    element('range-protein').textContent = rangeTranslation(doc, range.start, range.end, strand) || '(shorter than one codon)';
  }
  if (activeTab === 'map') {
    const report = renderMap(element<HTMLElement>('map') as unknown as SVGSVGElement, doc, selected, select,
      { orfs: options.showOrfs ? doc.orfs.filter(o => o.codons >= options.orfMinCodons) : [], selectedOrf, selectOrf });
    unlabelled = { doc, ids: new Set(report.unlabelled) };
    const notice = element('map-notice');
    const count = unlabelled.ids.size;
    const orfCount = report.undrawnOrfs.length;
    notice.hidden = count === 0 && orfCount === 0;
    notice.dataset.undrawnOrfs = String(orfCount);
    notice.textContent = [
      count ? `${count} ${count === 1 ? 'label' : 'labels'} not shown on the map · Show in list` : '',
      orfCount ? `${orfCount} ORF${orfCount === 1 ? '' : 's'} not drawn (no room; listed in Sequence)` : '',
    ].filter(Boolean).join(' · ');
    notice.onclick = showUnlabelled;
  }
  const unlabelledIds = unlabelled.doc === doc ? unlabelled.ids : new Set<string>();
  const list = element('features'); const listScroll = list.scrollTop; list.replaceChildren();
  for (const feature of doc.features) {
    const button = document.createElement('button');
    const name = document.createElement('span'); name.dataset.testid = 'feature-name';
    name.textContent = feature.label || feature.kind;
    button.append(name, ` (${feature.strand})`);
    if (unlabelledIds.has(feature.id)) {
      const badge = document.createElement('span'); badge.className = 'badge'; badge.dataset.testid = 'feature-unlabelled';
      badge.textContent = 'not labelled on map'; button.append(badge);
    }
    button.dataset.testid = 'feature-item'; button.dataset.featureId = feature.id;
    button.style.borderLeftColor = featureColor(feature);
    button.classList.toggle('selected', feature.id === selected);
    button.setAttribute('aria-pressed', String(feature.id === selected));
    button.onclick = () => select(feature.id);
    list.append(button);
  }
  list.scrollTop = listScroll;
  if (activeTab === 'sequence') {
    baseIndex = renderSequence(element('sequence'), doc, { activeFeature: active, activeOrf, range, options, columns: sequenceColumns(), select, selectOrf });
  }
}

function setListCollapsed(collapsed: boolean) {
  document.querySelector('.workspace')!.classList.toggle('list-collapsed', collapsed);
  const toggle = element('toggle-features');
  toggle.setAttribute('aria-expanded', String(!collapsed));
  toggle.textContent = collapsed ? '›' : '‹';
  toggle.title = collapsed ? 'Show feature list' : 'Hide feature list';
  try { localStorage.setItem('dnagent.featureList', collapsed ? 'collapsed' : 'open'); } catch { /* not persisted */ }
}
element('toggle-features').onclick = () => setListCollapsed(element('toggle-features').getAttribute('aria-expanded') === 'true');
try { if (localStorage.getItem('dnagent.featureList') === 'collapsed') setListCollapsed(true); } catch { /* default open */ }

function showUnlabelled() {
  setListCollapsed(false);
  const items = [...document.querySelectorAll<HTMLElement>('[data-testid="feature-item"]')]
    .filter(item => item.querySelector('[data-testid="feature-unlabelled"]'));
  items[0]?.scrollIntoView({ block: 'nearest' });
  for (const item of items) { item.classList.remove('flash'); void item.offsetWidth; item.classList.add('flash'); }
}

// Re-lay out the map when its panel changes size (window resize, list collapse).
let resizeFrame = 0;
new ResizeObserver(() => {
  cancelAnimationFrame(resizeFrame);
  resizeFrame = requestAnimationFrame(() => { if (activeTab === 'map') render(); });
}).observe(element('panel-map'));

// Drag across bases to select a range; a click without movement cycles features.
let drag: { anchor: number; last: number } | null = null;
let suppressClick = false;
const positionOf = (target: EventTarget | null) => {
  const value = (target as HTMLElement | null)?.dataset?.position;
  return value === undefined ? null : Number(value);
};
function paintDrag(lo: number, hi: number, on: boolean) {
  for (let pos = lo; pos <= hi; pos++) for (const base of baseIndex[pos] ?? []) base.classList.toggle('dragging', on);
}
element('sequence').addEventListener('mousedown', event => {
  const position = positionOf(event.target);
  if (position === null || event.button !== 0) return;
  event.preventDefault();
  drag = { anchor: position, last: position };
});
element('sequence').addEventListener('mouseover', event => {
  const position = positionOf(event.target);
  if (!drag || position === null || position === drag.last) return;
  paintDrag(Math.min(drag.anchor, drag.last), Math.max(drag.anchor, drag.last), false);
  drag.last = position;
  paintDrag(Math.min(drag.anchor, position), Math.max(drag.anchor, position), true);
});
window.addEventListener('mouseup', () => {
  if (!drag) return;
  const { anchor, last } = drag;
  drag = null;
  if (anchor === last || !current) return;
  // The browser may or may not follow this mouseup with a click; ignore one only in this turn.
  suppressClick = true;
  setTimeout(() => { suppressClick = false; });
  range = { start: Math.min(anchor, last), end: Math.max(anchor, last) + 1 };
  selected = null; selectedOrf = null;
  render();
});

let sequenceFrame = 0;
new ResizeObserver(() => {
  cancelAnimationFrame(sequenceFrame);
  sequenceFrame = requestAnimationFrame(() => { if (activeTab === 'sequence') render(); });
}).observe(element('panel-sequence'));

element('sequence').onclick = event => {
  if (suppressClick) { suppressClick = false; return; }
  const position = (event.target as HTMLElement).dataset.position;
  if (position === undefined || !current) return;
  const matches = current.features.filter(f => contains(f, Number(position), current!.sequence.length));
  if (matches.length) select(matches[(matches.findIndex(f => f.id === selected) + 1) % matches.length].id);
};

async function load(path: string) {
  const request = ++revision;
  element('status').textContent = 'Loading…';
  try {
    const result = await openDocument(path);
    if (request !== revision) return;
    current = result; selected = null; selectedOrf = null; range = null;
    element('primer-summary').textContent = `Imported primers: ${result.unplaced_primers.length} (unplaced)`;
    element('primer-list').replaceChildren();
    for (const primer of result.unplaced_primers) {
      const item = document.createElement('div'); item.className = 'primer-item'; item.dataset.testid = 'primer-item';
      const name = document.createElement('strong'); name.textContent = primer.name;
      const sequence = document.createElement('code'); sequence.textContent = primer.sequence_5to3;
      const description = document.createElement('p'); description.textContent = primer.description ?? '';
      item.append(name,sequence,description); element('primer-list').append(item);
    }
    element('warnings').replaceChildren();
    if (result.warnings.length) {
      const details = document.createElement('details'); details.dataset.testid = 'warnings-panel';
      const summary = document.createElement('summary'); summary.dataset.testid = 'warnings-summary';
      summary.textContent = `${result.warnings.length} import-fidelity warnings — review details`;
      const explanation = document.createElement('p');
      explanation.textContent = 'Some source content is not interpreted by DNAagent. These warnings do not by themselves indicate a sequence error. Retained packets are not exposed in this viewer or guaranteed to survive derived exports.';
      const messages = document.createElement('pre');
      result.warnings.forEach((w, index) => {
        const line = document.createElement('span'); line.dataset.testid = 'warning-item';
        line.textContent = `${index ? '\n' : ''}${w.code}: ${w.message}`; messages.append(line);
      });
      details.append(summary,explanation,messages); element('warnings').append(details);
    }
    element('status').textContent = 'Read-only · Map and Sequence share feature selection · Unknown strands have no arrows · Click sequence bases to cycle overlapping features.';
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
    const path = await pickConstructPath();
    if (path !== null) { element<HTMLInputElement>('path').value = path; await load(path); }
  } catch (error) { element('status').textContent = `File picker failed: ${String(error)}`; }
};

if (import.meta.env.MODE === 'e2e') {
  void import('./testing/automation').then(automation => automation.install(() => ({current, selected, selectedOrf, range, options, activeTab})));
}

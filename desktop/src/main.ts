import type { Diagnostic, Document, DocumentState, EditState } from './bindings';
import { bindFeatureDialog, openFeatureDialog } from './feature-dialog';
import { contains, featureColor } from './map-layout';
import { openDocument, pickConstructPath, pickSavePath, redo, removeFeature, saveGenbank, undo } from './ipc';
import { renderMap } from './map-view';
import { columnsFor, rangeTranslation, renderSequence, type SequenceOptions } from './sequence-view';
import { initTheme } from './theme';
import './style.css';

const element = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
let current: Document | null = null;
let edit: EditState | null = null;
/** Where a shift-click extends from: the last plainly selected feature, or a range start. */
let anchor: { feature: string } | { base: number } | null = null;
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

/** Forward span from the anchor's start to the furthest end of anchor and target (wrapping on circles). */
function extendedRange(target: string): { start: number; end: number } | null {
  if (!current || !anchor) return null;
  const doc = current;
  const length = doc.sequence.length;
  const feature = (id: string) => doc.features.find(f => f.id === id);
  const b = feature(target);
  if (!b) return null;
  const a = 'feature' in anchor ? feature(anchor.feature) : undefined;
  const start = 'base' in anchor ? anchor.base : a?.parts[0].start;
  if (start === undefined) return null;
  const parts = [...(a?.parts ?? []), ...b.parts];
  if (!doc.circular) {
    const low = Math.min(start, ...parts.map(p => p.start));
    return { start: low, end: Math.max(...parts.map(p => p.start + p.length), range && 'base' in anchor ? range.end : 0) };
  }
  // Offsets of every part end, measured forward from the anchor start.
  const reach = Math.max(...parts.map(p => ((p.start + p.length - start - 1 + length) % length) + 1),
    range && 'base' in anchor ? ((range.end - start - 1 + length) % length) + 1 : 0);
  return reach >= length ? null : { start, end: (start + reach) % length };
}

function select(id: string, extend = false) {
  const extended = extend ? extendedRange(id) : null;
  if (extended) {
    range = extended; selected = null; selectedOrf = null;
  } else {
    selected = id; selectedOrf = null; range = null; anchor = { feature: id };
  }
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
        ? `Selection [${range.start}, ${range.end}) · ${((range.end - range.start + length) % length || length).toLocaleString()} bp${range.end < range.start ? ' · crosses the origin' : ''}`
        : 'Select a feature, ORF or drag across bases. Sequence is always shown in the forward reference orientation.';
  const rangePanel = element('range-panel');
  rangePanel.hidden = range === null;
  if (range) {
    const strand = element<HTMLSelectElement>('range-strand').value === 'reverse' ? 'reverse' : 'forward';
    element('range-protein').textContent = range.end < range.start
      ? '(crosses the origin — use New feature… to preview its translation)'
      : rangeTranslation(doc, range.start, range.end, strand) || '(shorter than one codon)';
  }
  const added = new Set(edit?.added_feature_ids ?? []);
  element('new-feature').hidden = range === null;
  element('delete-feature').hidden = !(selected && added.has(selected));
  element('dirty').hidden = !edit?.dirty;
  document.title = `DNAagent — ${doc.name}${edit?.dirty ? ' •' : ''}`;
  element<HTMLButtonElement>('undo').disabled = !edit?.can_undo;
  element<HTMLButtonElement>('redo').disabled = !edit?.can_redo;
  element<HTMLButtonElement>('save').disabled = !edit;
  element<HTMLButtonElement>('save-as').disabled = !edit;
  if (activeTab === 'map') {
    const report = renderMap(element<HTMLElement>('map') as unknown as SVGSVGElement, doc, selected, select,
      { orfs: options.showOrfs ? doc.orfs.filter(o => o.codons >= options.orfMinCodons) : [], selectedOrf, selectOrf, range });
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
    if (added.has(feature.id)) {
      const badge = document.createElement('span'); badge.className = 'badge added'; badge.dataset.testid = 'feature-added';
      badge.textContent = 'added'; button.append(badge);
    }
    if (unlabelledIds.has(feature.id)) {
      const badge = document.createElement('span'); badge.className = 'badge'; badge.dataset.testid = 'feature-unlabelled';
      badge.textContent = 'not labelled on map'; button.append(badge);
    }
    button.dataset.testid = 'feature-item'; button.dataset.featureId = feature.id;
    button.style.borderLeftColor = featureColor(feature);
    button.classList.toggle('selected', feature.id === selected);
    button.setAttribute('aria-pressed', String(feature.id === selected));
    button.onclick = event => select(feature.id, event.shiftKey);
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

function shiftAnchor(next: { base: number }) { anchor = next; }

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
  shiftAnchor({ base: range.start });
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

const errorMessage = (error: unknown) => (error as Partial<Diagnostic>).message ?? String(error);

/** Adopt a new session state (after an edit, undo/redo or save), keeping a valid selection. */
function applyState(state: DocumentState, selectId?: string) {
  current = state.document; edit = state.edit;
  if (selectId !== undefined) { selected = selectId; selectedOrf = null; range = null; anchor = { feature: selectId }; }
  if (selected && !current.features.some(f => f.id === selected)) selected = null;
  if (selectedOrf && !current.orfs.some(o => o.id === selectedOrf)) selectedOrf = null;
  render();
}

async function runEdit(label: string, action: () => Promise<DocumentState>) {
  try {
    applyState(await action());
    element('status').textContent = label;
  } catch (error) {
    element('status').textContent = `${label} failed: ${errorMessage(error)}`;
  }
}

function defaultSavePath(): string {
  const source = edit?.saved_path ?? edit?.source_path ?? 'construct.dna';
  return source.replace(/\.(dna|fa|fasta|fna|gbk|genbank|gb)$/i, '') + '.gb';
}

async function save(as: boolean) {
  if (!edit) return;
  const path = !as && edit.saved_path ? edit.saved_path : await pickSavePath(defaultSavePath());
  if (!path) return;
  try {
    const result = await saveGenbank(edit.document_id, path);
    applyState(result.state);
    const notes = result.warnings.map(w => `${w.code}: ${w.message}`).join('; ');
    element('status').textContent = `Saved GenBank to ${path}${notes ? ` — ${notes}` : ''}`;
  } catch (error) {
    element('status').textContent = `Save failed: ${errorMessage(error)}`;
  }
}

function newFeature() {
  if (!current || !edit || !range) return;
  const documentId = edit.document_id;
  openFeatureDialog({
    documentId, start: range.start, end: range.end, length: current.sequence.length,
    previousAdded: edit.added_feature_ids,
    onAdded: (state, featureId) => { applyState(state, featureId); element('status').textContent = `Added ${featureId} (unsaved; Save writes GenBank)`; },
  });
}

function deleteSelected() {
  if (!edit || !selected || !edit.added_feature_ids.includes(selected)) return;
  const [documentId, featureId] = [edit.document_id, selected];
  void runEdit(`Deleted ${featureId}`, () => removeFeature(documentId, featureId));
}

element('undo').onclick = () => { if (edit?.can_undo) void runEdit('Undone', () => undo(edit!.document_id)); };
element('redo').onclick = () => { if (edit?.can_redo) void runEdit('Redone', () => redo(edit!.document_id)); };
element('save').onclick = () => void save(false);
element('save-as').onclick = () => void save(true);
element('new-feature').onclick = newFeature;
element('delete-feature').onclick = deleteSelected;
bindFeatureDialog(message => { element('status').textContent = `Add feature failed: ${message}`; });
document.addEventListener('keydown', event => {
  const typing = event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement || event.target instanceof HTMLTextAreaElement;
  if ((element<HTMLDialogElement>('feature-dialog')).open) return;
  const command = event.metaKey || event.ctrlKey;
  const key = event.key.toLowerCase();
  if (command && key === 'z') { event.preventDefault(); (event.shiftKey ? element('redo') : element('undo')).click(); }
  else if (command && key === 'y') { event.preventDefault(); element('redo').click(); }
  else if (command && key === 's') { event.preventDefault(); void save(event.shiftKey); }
  else if (!typing && !command && (event.key === 'Delete' || event.key === 'Backspace')) deleteSelected();
});

async function load(path: string) {
  if (edit?.dirty && !window.confirm('Discard unsaved changes to the current construct?')) return;
  const request = ++revision;
  element('status').textContent = 'Loading…';
  try {
    const state = await openDocument(path);
    if (request !== revision) return;
    const result = state.document;
    current = result; edit = state.edit; selected = null; selectedOrf = null; range = null; anchor = null;
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
    element('status').textContent = 'Click a feature, shift-click another to select the span between, or drag across bases · New feature… adds an annotation · Save writes GenBank.';
    render();
  } catch (error) {
    if (request !== revision) return;
    element('status').textContent = `Open failed: ${errorMessage(error)}. Previous document remains unchanged.`;
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
  void import('./testing/automation').then(automation => automation.install(() => ({current, edit, selected, selectedOrf, range, options, activeTab})));
}

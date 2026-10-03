import type { Diagnostic, Document, DocumentState, EditState, EnzymeCatalogueInfo, EnzymeCount, Fragment, Site, ViewReport } from './bindings';
import { enzymesInSet, loadChoice, openChooser, renderDigest, saveChoice, type EnzymeSet } from './enzymes';
import { defaultChecked, renderDetection, requestsFor, type Detection } from './detect';
import { bindFeatureDialog, openFeatureDialog } from './feature-dialog';
import { contains, featureColor } from './map-layout';
import {
  addFeatures, closeDocument, defaultWorkspace, detectFeatures, digest, enzymeCatalogue, enzymeCounts, findSites, openDocument, pickConstructPath, pickSavePath,
  pickSvgPath, pickWorkspace, pollFiles, redo, removeFeature, reportView, saveGenbank, undo, writeHandoff, writeSvg,
} from './ipc';
import { isoformUi, renderIsoformDetail, renderIsoforms, zoom, zoomTo } from './isoform-view';
import { renderMap, type MapProposal, type MapSite } from './map-view';
import { serializeSvg, themeBackground } from './svg-export';
import { columnsFor, rangeTranslation, renderSequence, type SequenceOptions } from './sequence-view';
import { initTheme } from './theme';
import './style.css';

const element = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
let current: Document | null = null;
let edit: EditState | null = null;
/** Where a shift-click extends from: the last plainly selected feature, or a range start. */
let anchor: { feature: string } | { base: number } | null = null;

/** One open construct. The globals above always mirror the active tab. */
interface DocTab {
  current: Document; edit: EditState; path: string;
  selected: string | null; selectedOrf: string | null; range: { start: number; end: number } | null;
  anchor: { feature: string } | { base: number } | null;
}
const docTabs: DocTab[] = [];
let activeDoc = -1;

function stash() {
  const tab = docTabs[activeDoc];
  if (tab && current && edit) Object.assign(tab, { current, edit, selected, selectedOrf, range, anchor });
}
let selected: string | null = null;
/** At most one of selected / selectedOrf / range is set. */
let selectedOrf: string | null = null;
let range: { start: number; end: number } | null = null;
let baseIndex: HTMLElement[][] = [];
const OPTIONS_KEY = 'dnagent.viewOptions';
const options: SequenceOptions = { aminoAcids: 'one', showFrames: false, showOrfs: false, orfMinCodons: 75 };
try { Object.assign(options, JSON.parse(localStorage.getItem(OPTIONS_KEY) ?? '{}')); } catch { /* defaults */ }
let revision = 0;
type ViewTab = 'map' | 'sequence' | 'isoforms';
const VIEW_TABS: ViewTab[] = ['map', 'sequence', 'isoforms'];
let activeTab: ViewTab = 'map';
/** Transcript ids in drawn order from the last isoform render (for the e2e accounting). */
let isoformsDrawn: string[] = [];
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

/** View tabs the active document offers (Isoforms only for a gene locus). */
const offeredTabs = (): ViewTab[] => VIEW_TABS.filter(name => name !== 'isoforms' || current?.locus);

function showTab(tab: ViewTab) {
  activeTab = offeredTabs().includes(tab) ? tab : 'map';
  syncTabs();
  render();
  if (activeTab === 'sequence') revealSelection();
}

function syncTabs() {
  element('tab-isoforms').hidden = !current?.locus;
  if (!offeredTabs().includes(activeTab)) activeTab = 'map';
  const tab = activeTab;
  for (const name of VIEW_TABS) {
    element(`panel-${name}`).hidden = name !== tab;
    element(`tab-${name}`).setAttribute('aria-selected', String(name === tab));
    element(`tab-${name}`).tabIndex = name === tab ? 0 : -1;
  }
}

for (const name of VIEW_TABS) {
  element(`tab-${name}`).onclick = () => showTab(name);
  element(`tab-${name}`).onkeydown = event => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const tabs = offeredTabs();
    const at = tabs.indexOf(name);
    const next = event.key === 'Home' ? tabs[0] : event.key === 'End' ? tabs[tabs.length - 1]
      : tabs[(at + (event.key === 'ArrowRight' ? 1 : tabs.length - 1)) % tabs.length];
    showTab(next); element(`tab-${next}`).focus();
  };
}

// ------------------------------------------------------------------ restriction enzymes

let catalogue: EnzymeCatalogueInfo | null = null;
const choice = loadChoice();
/** Per document: site counts for every catalogue enzyme (sequence never changes in a session). */
const countsByDoc = new Map<number, EnzymeCount[]>();
const sitesCache = new Map<string, Site[]>();
const requested = new Set<string>();
let digestResult: { documentId: number; enzymes: string[]; fragments: Fragment[] } | null = null;

/** Fetch once per key, then re-render; failures are reported, not retried in a loop. */
function fetchOnce<T>(key: string, request: () => Promise<T>, store: (value: T) => void) {
  if (requested.has(key)) return;
  requested.add(key);
  request().then(value => { store(value); render(); })
    .catch(error => { element('status').textContent = `Restriction sites unavailable: ${errorMessage(error)}`; });
}

/** Enzymes and sites to show for the active document; null while engine results are pending. */
function shownEnzymes(): { names: string[]; sites: Site[] } | null {
  if (!edit || choice.set === 'none') return { names: [], sites: [] };
  const documentId = edit.document_id;
  // Large loci: site counts for the whole catalogue are too slow, so only chosen enzymes are scanned.
  if (current?.sequence_window != null) return choice.set === 'custom' ? chosenSites(documentId) : { names: [], sites: [] };
  const counts = countsByDoc.get(documentId);
  if (!catalogue || !counts) {
    fetchOnce(`counts:${documentId}`, () => enzymeCounts(documentId), value => countsByDoc.set(documentId, value));
    return null;
  }
  const names = enzymesInSet(choice.set, catalogue, counts, choice.custom);
  const key = `${documentId}|${names.join(',')}`;
  const sites = sitesCache.get(key);
  if (!sites) {
    if (!names.length) return { names, sites: [] };
    fetchOnce(`sites:${key}`, () => findSites(documentId, names), value => sitesCache.set(key, value));
    return null;
  }
  return { names, sites };
}

function chosenSites(documentId: number): { names: string[]; sites: Site[] } | null {
  const names = [...choice.custom].sort((a, b) => a.localeCompare(b));
  if (!names.length) return { names, sites: [] };
  const key = `${documentId}|chosen|${names.join(',')}`;
  const sites = sitesCache.get(key);
  if (!sites) {
    fetchOnce(`sites:${key}`, () => findSites(documentId, names), value => sitesCache.set(key, value));
    return null;
  }
  // Like the other sets: only enzymes that cut are listed as shown.
  return { names: names.filter(name => sites.some(site => site.enzyme === name)), sites };
}

/** Half-open range of `length` bases from `start`, wrapping only past the end of a circle. */
function spanRange(start: number, length: number): { start: number; end: number } {
  const size = current?.sequence.length ?? 0;
  return { start, end: current?.circular && start + length > size ? start + length - size : start + length };
}

function selectRange(start: number, length: number) {
  if (!current || length <= 0 || length >= current.sequence.length) return;
  range = spanRange(start, length); selected = null; selectedOrf = null; anchor = { base: start };
  render();
  if (activeTab === 'sequence') revealSelection();
}

const selectSite = (site: Site) => selectRange(site.start, site.length);

/** One map mark per top-strand cut position, naming every enzyme that cuts there. */
function mapSites(sites: Site[]): MapSite[] {
  const byCut = new Map<number, Site[]>();
  for (const site of sites) if (site.top_cut !== null) byCut.set(site.top_cut, [...(byCut.get(site.top_cut) ?? []), site]);
  return [...byCut].sort((a, b) => a[0] - b[0]).map(([cut, group]) => ({
    cut, names: [...new Set(group.map(site => site.enzyme))].sort((a, b) => a.localeCompare(b)), select: () => selectSite(group[0]),
  }));
}

function renderEnzymeControls(shown: { names: string[]; sites: Site[] } | null) {
  const set = element<HTMLSelectElement>('enzyme-set');
  set.value = choice.set;
  set.title = catalogue ? `Enzymes from ${catalogue.source} ${catalogue.version} (${catalogue.enzymes.length})` : 'Loading enzymes…';
  const large = current?.sequence_window != null;
  element('enzyme-summary').textContent = choice.set === 'none' ? '' : large && choice.set !== 'custom' ? 'not computed for records over 100 kb — Choose… enzymes instead'
    : shown === null ? 'finding sites…'
    : `${shown.names.length} enzyme${shown.names.length === 1 ? '' : 's'}, ${shown.sites.length} site${shown.sites.length === 1 ? '' : 's'}`;
  const run = element<HTMLButtonElement>('digest-run');
  run.disabled = !shown?.names.length;
  run.title = shown?.names.length ? `Digest with ${shown.names.join(', ')}` : 'Show some enzymes first';
  const result = digestResult && edit && digestResult.documentId === edit.document_id ? digestResult : null;
  element('digest-summary').textContent = result
    ? `${result.enzymes.join(' + ')}: ${result.fragments.length} fragment${result.fragments.length === 1 ? '' : 's'}`
    : 'Digest with the enzymes shown on the map.';
  if (result) renderDigest(element('digest-fragments'), result.fragments, fragment => selectRange(fragment.start, fragment.length));
  else element('digest-fragments').replaceChildren();
}

async function runDigest() {
  const shown = shownEnzymes();
  if (!edit || !shown?.names.length) return;
  const [documentId, enzymes] = [edit.document_id, shown.names];
  try {
    digestResult = { documentId, enzymes, fragments: await digest(documentId, enzymes) };
    element('status').textContent = `Digested with ${enzymes.join(', ')}`;
  } catch (error) {
    digestResult = null;
    element('status').textContent = `Digest failed: ${errorMessage(error)}`;
  }
  render();
}

function setEnzymeSet(set: EnzymeSet) {
  choice.set = set; saveChoice(choice);
  if (set === 'custom' && !choice.custom.length) chooseEnzymes(); else render();
}

function chooseEnzymes() {
  if (!catalogue) return;
  const counts = edit ? countsByDoc.get(edit.document_id) ?? null : null;
  openChooser(element<HTMLDialogElement>('enzyme-dialog'), catalogue, counts, choice.custom, names => {
    choice.custom = names; choice.set = 'custom'; saveChoice(choice); render();
  });
}

element<HTMLSelectElement>('enzyme-set').onchange = event => setEnzymeSet((event.target as HTMLSelectElement).value as EnzymeSet);
element('enzyme-choose').onclick = chooseEnzymes;
element('digest-run').onclick = () => void runDigest();
void enzymeCatalogue().then(value => { catalogue = value; render(); })
  .catch(error => { element('status').textContent = `Enzyme catalogue unavailable: ${errorMessage(error)}`; });

// ------------------------------------------------------------------ detect features

/** Open detection per document; the panel shows the active document's. */
const detections = new Map<number, Detection>();

async function runDetection(announce = true) {
  if (!edit) return;
  const documentId = edit.document_id;
  const previous = detections.get(documentId);
  try {
    const result = await detectFeatures(documentId);
    // Keep the user's ticks for proposals that are still offered; new ones get the default.
    const defaults = defaultChecked(result);
    const checked = previous
      ? new Set(result.proposals.filter(p => previous.result.proposals.some(q => q.key === p.key) ? previous.checked.has(p.key) && p.annotated_as.length === 0 : defaults.has(p.key)).map(p => p.key))
      : defaults;
    detections.set(documentId, { documentId, result, checked });
    if (announce) {
      element('status').textContent = result.available
        ? `${result.proposals.length} library feature${result.proposals.length === 1 ? '' : 's'} found (${result.proposals.filter(p => p.annotated_as.length).length} already annotated)`
        : result.message;
    }
  } catch (error) {
    element('status').textContent = `Detect features failed: ${errorMessage(error)}`;
  }
  render();
}

function renderDetectPanel() {
  const detection = edit ? detections.get(edit.document_id) : undefined;
  element<HTMLButtonElement>('detect-run').disabled = !edit;
  element('detect-panel').hidden = !detection;
  if (!detection) return;
  const { result, checked } = detection;
  const annotated = result.proposals.filter(p => p.annotated_as.length).length;
  element('detect-summary').textContent = !result.available ? result.message
    : result.proposals.length === 0 ? `No library features found (${result.message}, ≥ ${result.min_length} bp).`
      : `${result.proposals.length} found · ${annotated} already annotated · ${checked.size} ticked`;
  renderDetection(element('detect-list'), detection, {
    toggle: key => { if (!checked.delete(key)) checked.add(key); render(); },
    select: p => selectRange(p.start, p.length),
  });
  const add = element<HTMLButtonElement>('detect-add');
  add.disabled = checked.size === 0;
  add.textContent = `Add ${checked.size} feature${checked.size === 1 ? '' : 's'}`;
}

function mapProposals(): MapProposal[] {
  const detection = edit ? detections.get(edit.document_id) : undefined;
  if (!detection) return [];
  return detection.result.proposals.filter(p => p.annotated_as.length === 0).map(p => ({
    key: p.key, start: p.start, length: p.length, strand: p.strand, checked: detection.checked.has(p.key),
    color: featureColor({ id: p.key, label: p.name, kind: p.kind, color: p.color, strand: p.strand, parts: [] }),
    select: () => selectRange(p.start, p.length),
  }));
}

async function addDetected() {
  if (!edit || !current) return;
  const detection = detections.get(edit.document_id);
  if (!detection || detection.checked.size === 0) return;
  const requests = requestsFor(detection, current.sequence.length);
  const documentId = edit.document_id;
  try {
    applyState(await addFeatures(documentId, requests));
    element('status').textContent = `Added ${requests.length} feature${requests.length === 1 ? '' : 's'} from the library (one undo step; Save writes GenBank)`;
  } catch (error) {
    element('status').textContent = `Add features failed: ${errorMessage(error)}`;
  }
}

element('detect-run').onclick = () => void runDetection();
element('detect-close').onclick = () => { if (edit) detections.delete(edit.document_id); render(); };
element('detect-add').onclick = () => void addDetected();

function render() {
  stash();
  renderDocTabs();
  reportCurrentView();
  element<HTMLButtonElement>('handoff').disabled = docTabs.length === 0;
  if (!current) return;
  const doc = current;
  const length = doc.sequence.length;
  const shown = shownEnzymes();
  renderEnzymeControls(shown);
  renderDetectPanel();
  const active = doc.features.find(f => f.id === selected);
  const activeOrf = selectedOrf === null ? undefined : doc.orfs.find(o => o.id === selectedOrf);
  syncTabs();
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
      : doc.sequence_window !== null ? '(not computed for records over 100 kb — New feature… previews the translation)'
      : rangeTranslation(doc, range.start, range.end, strand) || '(shorter than one codon)';
  }
  const added = new Set(edit?.added_feature_ids ?? []);
  element('new-feature').hidden = range === null;
  element('delete-feature').hidden = !selected;
  element('dirty').hidden = !edit?.dirty;
  document.title = `DNAgent — ${doc.name}${edit?.dirty ? ' •' : ''}`;
  element<HTMLButtonElement>('undo').disabled = !edit?.can_undo;
  element<HTMLButtonElement>('redo').disabled = !edit?.can_redo;
  element<HTMLButtonElement>('save').disabled = !edit;
  element<HTMLButtonElement>('save-as').disabled = !edit;
  if (activeTab === 'map') {
    const report = renderMap(element<HTMLElement>('map') as unknown as SVGSVGElement, doc, selected, select,
      { orfs: options.showOrfs ? doc.orfs.filter(o => o.codons >= options.orfMinCodons) : [], selectedOrf, selectOrf, range, sites: mapSites(shown?.sites ?? []), proposals: mapProposals() });
    unlabelled = { doc, ids: new Set(report.unlabelled) };
    const notice = element('map-notice');
    const count = unlabelled.ids.size;
    const orfCount = report.undrawnOrfs.length;
    const siteCount = report.unlabelledSites.length;
    notice.hidden = count === 0 && orfCount === 0 && siteCount === 0 && report.undrawnProposals.length === 0;
    notice.dataset.unlabelled = String(count);
    notice.dataset.undrawnOrfs = String(orfCount);
    notice.dataset.unlabelledSites = String(siteCount);
    notice.title = siteCount ? `Enzyme sites without a label (ticks are drawn; see the Sequence view): ${report.unlabelledSites.join(', ')}` : '';
    notice.textContent = [
      count ? `${count} ${count === 1 ? 'label' : 'labels'} not shown on the map · Show in list` : '',
      orfCount ? `${orfCount} ORF${orfCount === 1 ? '' : 's'} not drawn (no room; listed in Sequence)` : '',
      siteCount ? `${siteCount} enzyme label${siteCount === 1 ? '' : 's'} not shown (ticks drawn)` : '',
      report.undrawnProposals.length ? `${report.undrawnProposals.length} detected feature${report.undrawnProposals.length === 1 ? '' : 's'} not drawn (listed in Detected)` : '',
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
  if (activeTab === 'isoforms') renderIsoformPanel(doc);
  syncLargeControls(doc);
  if (activeTab === 'sequence') {
    const window = sequenceWindow(doc, active);
    const note = element('sequence-window');
    note.hidden = window === 'full';
    note.textContent = window === 'full' ? '' : window === null
      ? `This record is ${length.toLocaleString()} bp, too long to show whole. Select a feature or isoform, or mark a region (drag here, on the map or in Isoforms), to see its sequence.`
      : `Showing [${window.start.toLocaleString()}, ${window.end.toLocaleString()}) of ${length.toLocaleString()} bp around the selection${window.clipped ? ` — the selection is longer than ${doc.sequence_window!.toLocaleString()} bp; mark a smaller region to see the rest` : ''}.`;
    if (window === null) { element('sequence').replaceChildren(); baseIndex = []; }
    else {
      baseIndex = renderSequence(element('sequence'), doc, { activeFeature: active, activeOrf, range, options, columns: sequenceColumns(), select, selectOrf, sites: shown?.sites ?? [], selectSite,
        window: window === 'full' ? undefined : window });
    }
  }
}

// ------------------------------------------------------------------ isoform view

function selectedIsoform(doc: Document) {
  return selected === null ? undefined : doc.locus?.isoforms.find(i => i.mrna_feature_id === selected || i.cds_feature_id === selected);
}

function renderIsoformPanel(doc: Document) {
  const locus = doc.locus;
  if (!locus || !edit) return;
  const length = doc.sequence.length;
  const ui = isoformUi(edit.document_id, locus, length, doc.sequence_window !== null);
  const quantifier = element<HTMLSelectElement>('iso-quantifier');
  quantifier.replaceChildren(...locus.quantifiers.map(q => {
    const option = document.createElement('option');
    option.value = q.quantifier; option.textContent = `${q.quantifier} (${q.cell_lines.length} cell lines)`;
    return option;
  }));
  quantifier.disabled = locus.quantifiers.length === 0;
  if (ui.quantifier) quantifier.value = ui.quantifier;
  element<HTMLInputElement>('iso-compress').checked = ui.compress;
  const isoform = selectedIsoform(doc);
  element<HTMLButtonElement>('iso-zoom-selection').disabled = !range && !isoform;
  element<HTMLButtonElement>('iso-show-sequence').disabled = !range && !selected;
  const notice = element('iso-notice');
  const missing = locus.missing_features;
  notice.hidden = missing.length === 0 && locus.quantifiers.length > 0;
  notice.textContent = [
    missing.length ? `${missing.length} isoform${missing.length === 1 ? ' is' : 's are'} not drawn because ${missing.length === 1 ? 'its feature was' : 'their features were'} deleted: ${missing.join(', ')}` : '',
    locus.quantifiers.length ? '' : 'This locus bundle has no expression data; isoforms are in annotation order.',
  ].filter(Boolean).join(' · ');
  const detail = element('isoform-detail');
  if (isoform) renderIsoformDetail(detail, locus, isoform, ui.quantifier, () => { selected = null; render(); });
  else { detail.hidden = true; detail.replaceChildren(); }
  const view = range && range.end > range.start ? range : null;
  const drawn = renderIsoforms(element<HTMLElement>('isoforms') as unknown as SVGSVGElement, locus, length, ui, { selectedFeature: selected, range: view }, {
    select: id => select(id),
    selectRange: (start, end) => { range = { start, end }; selected = null; selectedOrf = null; anchor = { base: start }; render(); },
    changed: () => render(),
  });
  isoformsDrawn = drawn.drawn;
}

function isoformAction(action: (ui: ReturnType<typeof isoformUi>, length: number) => void) {
  if (!current?.locus || !edit) return;
  action(isoformUi(edit.document_id, current.locus, current.sequence.length, current.sequence_window !== null), current.sequence.length);
  render();
}

element<HTMLSelectElement>('iso-quantifier').onchange = event => isoformAction(ui => { ui.quantifier = (event.target as HTMLSelectElement).value; });
element<HTMLInputElement>('iso-compress').onchange = event => isoformAction(ui => { ui.compress = (event.target as HTMLInputElement).checked; });
element('iso-zoom-in').onclick = () => isoformAction((ui, length) => zoom(ui, length, 0.5));
element('iso-zoom-out').onclick = () => isoformAction((ui, length) => zoom(ui, length, 2));
element('iso-zoom-fit').onclick = () => isoformAction((ui, length) => { ui.start = 0; ui.end = length; });
element('iso-zoom-selection').onclick = () => isoformAction((ui, length) => {
  const isoform = current ? selectedIsoform(current) : undefined;
  if (range && range.end > range.start) zoomTo(ui, length, range.start, range.end);
  else if (isoform) zoomTo(ui, length, Math.min(...isoform.exons.map(e => e.start)), Math.max(...isoform.exons.map(e => e.start + e.length)));
});
element('iso-show-sequence').onclick = () => showTab('sequence');

/** Export a rendered view as a standalone SVG file chosen in a save dialog. */
async function exportSvg(svgId: string, suffix: string) {
  if (!current) return;
  const svg = element<HTMLElement>(svgId) as unknown as SVGSVGElement;
  const stem = (edit?.saved_path ?? edit?.source_path ?? current.name).replace(/\.(dna|fa|fasta|fna|gbk|genbank|gb|json)$/i, '').replace(/\.locus$/i, '');
  const path = await pickSvgPath(`${stem}.${suffix}.svg`);
  if (!path) return;
  try {
    await writeSvg(path, serializeSvg(svg, themeBackground()));
    element('status').textContent = `Exported ${suffix} to ${path}`;
  } catch (error) {
    element('status').textContent = `Export failed: ${errorMessage(error)}`;
  }
}
element('map-export').onclick = () => void exportSvg('map', 'map');
element('iso-export').onclick = () => void exportSvg('isoforms', 'isoforms');

// ------------------------------------------------------------------ large records (gene loci over 100 kb)

const WINDOW_PAD = 60;

/**
 * Bases the Sequence view shows: 'full' for ordinary records; for large ones the selection
 * (range, else feature) padded and capped at `sequence_window` bases; null without a selection.
 */
function sequenceWindow(doc: Document, active: Document['features'][number] | undefined): 'full' | null | { start: number; end: number; clipped: boolean } {
  const limit = doc.sequence_window;
  if (limit === null) return 'full';
  const length = doc.sequence.length;
  const span = range && range.end > range.start ? range
    : active ? { start: Math.min(...active.parts.map(p => p.start)), end: Math.max(...active.parts.map(p => p.start + p.length)) } : null;
  if (!span) return null;
  const start = Math.max(0, span.start - WINDOW_PAD);
  const end = Math.min(length, span.end + WINDOW_PAD, start + limit);
  return { start, end, clipped: span.end > end };
}

/** ORFs and six-frame translation are not computed for large records; say so on the controls. */
function syncLargeControls(doc: Document) {
  const large = doc.sequence_window !== null;
  for (const id of ['opt-orfs', 'opt-frames']) {
    const box = element<HTMLInputElement>(id);
    box.disabled = large;
    box.closest('label')!.title = large ? 'Not computed for records over 100 kb (use dnagent orfs / translate on a region)' : '';
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
let isoformFrame = 0;
new ResizeObserver(() => {
  cancelAnimationFrame(isoformFrame);
  isoformFrame = requestAnimationFrame(() => { if (activeTab === 'isoforms') render(); });
}).observe(element('panel-isoforms'));

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
  // Edits change which proposals are already annotated.
  if (detections.has(state.edit.document_id)) void runDetection(false);
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
  return source.replace(/\.(dna|fa|fasta|fna|gbk|genbank|gb|json)$/i, '').replace(/\.locus$/i, '') + '.gb';
}

async function save(as: boolean) {
  if (!edit) return;
  const path = !as && edit.saved_path ? edit.saved_path : await pickSavePath(defaultSavePath());
  if (!path) return;
  try {
    const result = await saveGenbank(edit.document_id, path);
    applyState(result.state);
    rememberStamp(path);
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

/** In-app confirmation (native confirm() is unreliable in the desktop web view). Resolves true on OK. */
function confirmAction(title: string, message: string, ok: string): Promise<boolean> {
  const dialog = element<HTMLDialogElement>('confirm-dialog');
  element('confirm-title').textContent = title;
  element('confirm-message').textContent = message;
  element('confirm-ok').textContent = ok;
  return new Promise(resolve => {
    const finish = (answer: boolean) => { dialog.onclose = null; if (dialog.open) dialog.close(); resolve(answer); };
    element('confirm-cancel').onclick = () => finish(false);
    dialog.querySelector('form')!.onsubmit = event => { event.preventDefault(); finish(true); };
    dialog.onclose = () => finish(false); // Escape
    dialog.showModal();
    element('confirm-ok').focus();
  });
}

async function deleteSelected() {
  if (!edit || !selected || !current) return;
  const feature = current.features.find(f => f.id === selected);
  if (!feature) return;
  const [documentId, featureId, name] = [edit.document_id, selected, feature.label || feature.kind];
  const imported = !edit.added_feature_ids.includes(featureId);
  const confirmed = await confirmAction(`Delete “${name}”?`,
    `${imported ? 'This feature came with the file. ' : ''}The ${feature.kind} at ${feature.parts.map(p => `[${p.start}, ${p.start + p.length})`).join(', ')} is removed from this construct; the file changes only when you save. Undo (⌘Z) restores it.`,
    'Delete feature');
  if (!confirmed) { element('status').textContent = `Kept ${name}`; return; }
  await runEdit(`Deleted ${name} (${featureId}) · ⌘Z to undo`, () => removeFeature(documentId, featureId));
}

element('undo').onclick = () => { if (edit?.can_undo) void runEdit('Undone', () => undo(edit!.document_id)); };
element('redo').onclick = () => { if (edit?.can_redo) void runEdit('Redone', () => redo(edit!.document_id)); };
element('save').onclick = () => void save(false);
element('save-as').onclick = () => void save(true);
element('new-feature').onclick = newFeature;
element('delete-feature').onclick = () => void deleteSelected();
bindFeatureDialog(message => { element('status').textContent = `Add feature failed: ${message}`; });
document.addEventListener('keydown', event => {
  const typing = event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement || event.target instanceof HTMLTextAreaElement;
  if (['feature-dialog', 'enzyme-dialog', 'confirm-dialog'].some(id => element<HTMLDialogElement>(id).open)) return;
  const command = event.metaKey || event.ctrlKey;
  const key = event.key.toLowerCase();
  if (command && event.shiftKey && key === 'c') { event.preventDefault(); void handoff(); return; }
  if (command && /^[1-9]$/.test(event.key)) { event.preventDefault(); activateDoc(Number(event.key) - 1); return; }
  if (command && key === 'z') { event.preventDefault(); (event.shiftKey ? element('redo') : element('undo')).click(); }
  else if (command && key === 'y') { event.preventDefault(); element('redo').click(); }
  else if (command && key === 's') { event.preventDefault(); void save(event.shiftKey); }
  else if (!typing && !command && (event.key === 'Delete' || event.key === 'Backspace')) { event.preventDefault(); void deleteSelected(); }
});

function renderDocumentChrome(doc: Document) {
  element('primer-summary').textContent = `Imported primers: ${doc.unplaced_primers.length} (unplaced)`;
  element('primer-list').replaceChildren();
  for (const primer of doc.unplaced_primers) {
    const item = document.createElement('div'); item.className = 'primer-item'; item.dataset.testid = 'primer-item';
    const name = document.createElement('strong'); name.textContent = primer.name;
    const sequence = document.createElement('code'); sequence.textContent = primer.sequence_5to3;
    const description = document.createElement('p'); description.textContent = primer.description ?? '';
    item.append(name, sequence, description); element('primer-list').append(item);
  }
  element('warnings').replaceChildren();
  if (doc.warnings.length) {
    const details = document.createElement('details'); details.dataset.testid = 'warnings-panel';
    const summary = document.createElement('summary'); summary.dataset.testid = 'warnings-summary';
    summary.textContent = `${doc.warnings.length} import-fidelity warnings — review details`;
    const explanation = document.createElement('p');
    explanation.textContent = 'Some source content is not interpreted by DNAgent. These warnings do not by themselves indicate a sequence error. Retained packets are carried into saved GenBank but are not shown in this viewer.';
    const messages = document.createElement('pre');
    doc.warnings.forEach((w, index) => {
      const line = document.createElement('span'); line.dataset.testid = 'warning-item';
      line.textContent = `${index ? '\n' : ''}${w.code}: ${w.message}`; messages.append(line);
    });
    details.append(summary, explanation, messages); element('warnings').append(details);
  }
}

function renderDocTabs() {
  const bar = element('doc-tabs');
  bar.replaceChildren(...docTabs.map((tab, index) => {
    const item = document.createElement('div');
    item.className = `doc-tab${index === activeDoc ? ' active' : ''}`;
    item.dataset.testid = 'doc-tab'; item.dataset.documentId = String(tab.edit.document_id);
    item.setAttribute('role', 'tab'); item.setAttribute('aria-selected', String(index === activeDoc));
    item.title = tab.path;
    const name = document.createElement('span'); name.className = 'name'; name.textContent = tab.current.name;
    item.append(name);
    if (tab.edit.dirty) { const dot = document.createElement('span'); dot.className = 'dot'; dot.textContent = '●'; item.append(dot); }
    const close = document.createElement('button'); close.type = 'button'; close.className = 'close'; close.dataset.testid = 'doc-tab-close';
    close.textContent = '×'; close.title = `Close ${tab.current.name}`;
    close.onclick = event => { event.stopPropagation(); void closeDoc(index); };
    item.append(close);
    item.onclick = () => activateDoc(index);
    return item;
  }));
  bar.hidden = docTabs.length === 0;
}

function activateDoc(index: number) {
  if (index < 0 || index >= docTabs.length) return;
  stash();
  activeDoc = index;
  const tab = docTabs[index];
  ({ current, edit, selected, selectedOrf, range, anchor } = tab);
  element<HTMLInputElement>('path').value = tab.path;
  renderDocumentChrome(tab.current);
  render();
}

function clearView() {
  current = null; edit = null; selected = null; selectedOrf = null; range = null; anchor = null;
  element('title').textContent = 'No construct loaded';
  for (const id of ['features', 'sequence', 'warnings', 'primer-list', 'digest-fragments', 'detect-list']) element(id).replaceChildren();
  element('detect-panel').hidden = true; element<HTMLButtonElement>('detect-run').disabled = true;
  element('enzyme-summary').textContent = ''; element('digest-summary').textContent = '';
  element<HTMLButtonElement>('digest-run').disabled = true;
  (element('map') as unknown as SVGSVGElement).replaceChildren();
  (element('isoforms') as unknown as SVGSVGElement).replaceChildren();
  element('isoform-detail').hidden = true; element('tab-isoforms').hidden = true;
  if (activeTab === 'isoforms') showTab('map');
  for (const id of ['new-feature', 'delete-feature', 'dirty', 'range-panel', 'map-notice']) element(id).hidden = true;
  for (const id of ['undo', 'redo', 'save', 'save-as']) element<HTMLButtonElement>(id).disabled = true;
  element('selection').textContent = 'Open a construct.';
  renderDocTabs();
  reportCurrentView();
  element<HTMLButtonElement>('handoff').disabled = true;
}

async function closeDoc(index: number) {
  const tab = docTabs[index];
  if (!tab) return;
  if (index === activeDoc) stash();
  if (tab.edit.dirty && !await confirmAction(`Close “${tab.current.name}”?`, 'It has unsaved changes, which will be lost.', 'Discard changes')) return;
  try { await closeDocument(tab.edit.document_id); } catch { /* already gone */ }
  docTabs.splice(index, 1);
  if (docTabs.length === 0) { activeDoc = -1; clearView(); return; }
  activeDoc = -1;
  activateDoc(Math.min(index, docTabs.length - 1));
}

async function load(path: string) {
  const existing = docTabs.findIndex(tab => tab.path === path);
  if (existing >= 0) { activateDoc(existing); element('status').textContent = `${docTabs[existing].current.name} is already open`; return; }
  const request = ++revision;
  element('status').textContent = 'Loading…';
  try {
    const state = await openDocument(path);
    if (request !== revision) { void closeDocument(state.edit.document_id).catch(() => undefined); return; }
    stash();
    docTabs.push({ current: state.document, edit: state.edit, path, selected: null, selectedOrf: null, range: null, anchor: null });
    activeDoc = -1;
    activateDoc(docTabs.length - 1);
    rememberStamp(path);
    element('status').textContent = 'Click a feature, shift-click another to select the span between, or drag across bases · New feature… adds an annotation · Save writes GenBank · Hand off to agent shares all tabs.';
  } catch (error) {
    if (request !== revision) return;
    element('status').textContent = `Open failed: ${errorMessage(error)}. Previous document remains unchanged.`;
  }
}

// ------------------------------------------------------------------ agent handoff and workspace

const WORKSPACE_KEY = 'dnagent.workspace';
let workspace = '';
try { workspace = localStorage.getItem(WORKSPACE_KEY) ?? ''; } catch { /* default below */ }
let lastHandoff: { prompt: string; context_path: string } | null = null;

let reportedView = '';
/** Tell Rust what is shown (only when it changed), so agents read the live view through `dnagent mcp`. */
function reportCurrentView() {
  const view: ViewReport = {
    document_ids: docTabs.map(tab => tab.edit.document_id),
    active_document_id: current && edit ? edit.document_id : null,
    view_tab: activeTab,
    selection: current ? range : null,
    selected_feature_id: current ? selected : null,
    workspace,
  };
  const key = JSON.stringify(view);
  if (key === reportedView) return;
  reportedView = key;
  void reportView(view).catch(() => { reportedView = ''; });
}
type Notice = { kind: 'new' | 'changed' | 'conflict'; path: string };
let notices: Notice[] = [];
/** Last seen stamp per watched path; null until the workspace baseline is taken. */
let stamps: Map<string, string> | null = null;

function workspaceLabel() {
  element('workspace').textContent = `Workspace: ${workspace.replace(/^\/Users\/[^/]+/, '~') || '…'}`;
  element('workspace').title = `Workspace folder shared with the agent: ${workspace}`;
}

async function ensureWorkspace() {
  if (!workspace) {
    try { workspace = await defaultWorkspace(); } catch (error) { element('status').textContent = `No workspace folder: ${errorMessage(error)}`; }
  }
  workspaceLabel();
}

function setWorkspace(path: string) {
  workspace = path; stamps = null; notices = []; renderNotices(); reportCurrentView();
  try { localStorage.setItem(WORKSPACE_KEY, path); } catch { /* not persisted */ }
  workspaceLabel();
  void pollWorkspace();
}

const stampKey = (s: { modified_ms: number; size: number }) => `${s.modified_ms}:${s.size}`;

/** Forget a path's stamp so our own write does not look like an outside change. */
function rememberStamp(path: string) {
  void pollFiles(workspace || '.', [path]).then(found => {
    const mine = found.find(s => s.path === path);
    if (mine && stamps) stamps.set(path, stampKey(mine));
  }).catch(() => undefined);
}

function renderNotices() {
  const box = element('file-notices');
  box.replaceChildren(...notices.map(notice => {
    const row = document.createElement('div');
    row.className = `file-notice ${notice.kind}`; row.dataset.testid = 'file-notice'; row.dataset.kind = notice.kind; row.dataset.path = notice.path;
    const text = document.createElement('span');
    const name = notice.path.split('/').pop();
    text.textContent = notice.kind === 'conflict'
      ? `${name} changed on disk, but its tab has unsaved edits.`
      : notice.kind === 'new' ? `New in workspace: ${name}` : `Changed in workspace: ${name}`;
    const button = (label: string, testid: string, action: () => void) => {
      const b = document.createElement('button'); b.type = 'button'; b.textContent = label; b.dataset.testid = testid; b.onclick = action; return b;
    };
    const dismiss = () => { notices = notices.filter(n => n !== notice); renderNotices(); };
    if (notice.kind === 'conflict') {
      row.append(text, button('Reload (discard my edits)', 'notice-reload', () => { dismiss(); void reloadTab(notice.path); }),
        button('Keep mine', 'notice-dismiss', dismiss));
    } else {
      row.append(text, button('Open', 'notice-open', () => { dismiss(); void load(notice.path); }), button('Dismiss', 'notice-dismiss', dismiss));
    }
    return row;
  }));
}

async function reloadTab(path: string) {
  const index = docTabs.findIndex(tab => tab.path === path);
  if (index < 0) return;
  try {
    const state = await openDocument(path);
    const old = docTabs[index];
    void closeDocument(old.edit.document_id).catch(() => undefined);
    const keep = (id: string | null) => (id && state.document.features.some(f => f.id === id) ? id : null);
    if (index === activeDoc) stash();
    Object.assign(docTabs[index], { current: state.document, edit: state.edit, selected: keep(old.selected), selectedOrf: null });
    if (index === activeDoc) { activeDoc = -1; activateDoc(index); } else renderDocTabs();
    element('status').textContent = `Reloaded ${state.document.name} (changed on disk)`;
  } catch (error) {
    element('status').textContent = `Reload failed: ${errorMessage(error)}`;
  }
}

/** Compare workspace and open-file stamps with the last poll; offer or reload changes. */
async function pollWorkspace() {
  await ensureWorkspace();
  stash();
  let found;
  try { found = await pollFiles(workspace, docTabs.map(tab => tab.path)); } catch { return; }
  const first = stamps === null;
  const seen = stamps ?? new Map<string, string>();
  for (const file of found) {
    const key = stampKey(file);
    const before = seen.get(file.path);
    seen.set(file.path, key);
    if (first || before === key) continue;
    const tab = docTabs.find(t => t.path === file.path);
    if (notices.some(n => n.path === file.path)) continue;
    if (tab) {
      if (tab.edit.dirty) notices.push({ kind: 'conflict', path: file.path });
      else await reloadTab(file.path);
    } else {
      notices.push({ kind: before === undefined ? 'new' : 'changed', path: file.path });
    }
  }
  stamps = seen;
  renderNotices();
}

async function handoff() {
  if (!docTabs.length) return;
  await ensureWorkspace();
  stash();
  try {
    const result = await writeHandoff(workspace, docTabs.map((tab, index) => ({
      document_id: tab.edit.document_id, active: index === activeDoc, selection: tab.range, selected_feature_id: tab.selected,
    })));
    lastHandoff = result;
    element('handoff-panel').hidden = false;
    element<HTMLTextAreaElement>('handoff-prompt').value = result.prompt;
    let copied = false;
    try { await navigator.clipboard.writeText(result.prompt); copied = true; } catch { /* shown for manual copy */ }
    element('handoff-note').textContent = `${result.snapshots.length} snapshot${result.snapshots.length === 1 ? '' : 's'} + context.json in ${result.context_path.replace(/\/context\.json$/, '')}${copied ? ' · prompt copied' : ''}`;
    element('status').textContent = `Handed off ${result.snapshots.length} construct${result.snapshots.length === 1 ? '' : 's'} to the agent.`;
  } catch (error) {
    element('status').textContent = `Handoff failed: ${errorMessage(error)}`;
  }
}

element('handoff').onclick = () => void handoff();
element('handoff-close').onclick = () => { element('handoff-panel').hidden = true; };
element('handoff-copy').onclick = async () => {
  const text = element<HTMLTextAreaElement>('handoff-prompt');
  try { await navigator.clipboard.writeText(text.value); } catch { text.select(); document.execCommand('copy'); }
  element('handoff-note').textContent += ' · copied';
};
element('workspace').onclick = async () => {
  await ensureWorkspace();
  const path = await pickWorkspace(workspace).catch(() => null);
  if (path) setWorkspace(path);
};
void ensureWorkspace().then(() => pollWorkspace());
window.setInterval(() => { if (document.visibilityState === 'visible') void pollWorkspace(); }, import.meta.env.MODE === 'e2e' ? 3_600_000 : 2_000);

element('open').onsubmit = event => { event.preventDefault(); void load(element<HTMLInputElement>('path').value); };
element('browse').onclick = async () => {
  try {
    const path = await pickConstructPath();
    if (path !== null) { element<HTMLInputElement>('path').value = path; await load(path); }
  } catch (error) { element('status').textContent = `File picker failed: ${String(error)}`; }
};

if (import.meta.env.MODE === 'e2e') {
  void import('./testing/automation').then(automation => automation.install(
    () => ({ current, edit, selected, selectedOrf, range, options, activeTab, workspace, notices, lastHandoff, isoformsDrawn,
      isoformUi: current?.locus && edit ? { ...isoformUi(edit.document_id, current.locus, current.sequence.length, current.sequence_window !== null) } : null,
      enzymes: { set: choice.set, catalogue, shown: current ? shownEnzymes()?.names ?? null : null,
        digest: digestResult && edit && digestResult.documentId === edit.document_id ? digestResult : null },
      detection: edit ? detections.get(edit.document_id) ?? null : null,
      tabs: docTabs.map((tab, index) => ({ document_id: tab.edit.document_id, name: tab.current.name, path: tab.path, dirty: tab.edit.dirty, active: index === activeDoc })) }),
    { pollWorkspace }));
}

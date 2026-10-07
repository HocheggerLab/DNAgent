// e2e-only automation API (`window.__DNAGENT_TEST__`). Commands dispatch the same
// DOM events a user would; `getState` reports rendered DOM plus a read-only model view.
// Never set application state directly here, or the scenarios prove nothing.
import type { Document, EditState, EnzymeCatalogueInfo, Fragment } from '../bindings';
import { dialogState, previewPending } from '../feature-dialog';
import type { SequenceOptions } from '../sequence-view';
import { pendingRequests } from '../ipc';
import { themeState } from '../theme';
import { queuePick, queueSave, session, setDelay } from './stub-backend';

export interface ModelView {
  current: Document | null;
  edit: EditState | null;
  selected: string | null;
  selectedOrf: string | null;
  range: { start: number; end: number } | null;
  options: SequenceOptions;
  activeTab: 'map' | 'sequence' | 'isoforms';
  /** Transcript ids in the order the last isoform render drew them. */
  isoformsDrawn: string[];
  isoformUi: { start: number; end: number; compress: boolean; quantifier: string | null } | null;
  workspace: string;
  notices: { kind: string; path: string }[];
  lastHandoff: { prompt: string; context_path: string } | null;
  tabs: { document_id: number; name: string; path: string; dirty: boolean; active: boolean }[];
  enzymes: { set: string; catalogue: EnzymeCatalogueInfo | null; shown: string[] | null; digest: { enzymes: string[]; fragments: Fragment[] } | null };
  detection: { result: { available: boolean } } | null;
}

export interface Hooks { pollWorkspace: () => Promise<void> }

export interface Region { strand: string; start: number; length: number }

export interface Part { start: number; length: number }
export type ReconstructedPart = Part | { error: string; positions: number[] };

export interface AppState {
  idle: boolean;
  status: string;
  path_input: string;
  active_tab: 'map' | 'sequence' | 'isoforms' | 'inconsistent';
  visible_panels: string[];
  /** Parsed from the displayed title, so display bugs are caught; null before any load. */
  document: null | { name: string | null; length: number | null; topology: string | null; title: string };
  features: { id: string; name: string; strand: string; selected: boolean; unlabelled: boolean; added: boolean; text: string }[];
  /** Edit session state, plus what the controls show. */
  edit: null | {
    revision: number; can_undo: boolean; can_redo: boolean; dirty: boolean; added_feature_ids: string[]; saved_path: string | null;
    shown: { dirty: boolean; undo: boolean; redo: boolean; new_feature: boolean; delete_feature: boolean };
  };
  /** Number of tabs shown in the tab bar. */
  tab_count: number;
  /** Open construct tabs (model) and what the tab bar shows. */
  tabs: { document_id: number; name: string; path: string; dirty: boolean; active: boolean; shown_dirty: boolean }[];
  workspace: {
    path: string; shown: string;
    notices: { kind: string; path: string }[];
    /** The agent panel: a live agent's message or presented result (`dnagent mcp`). */
    agent: { open: boolean; message: string; highlights: { text: string; active: boolean }[] };
    handoff: { open: boolean; prompt: string; context_path: string | null };
  };
  dialog: { open: boolean; summary: string; protein: string; warnings: string[]; error: string; inputs: { label: string; kind: string; strand: string; translate: boolean } };
  /** View options as shown in the toolbar controls. */
  options: { amino_acids: string; show_frames: boolean; show_orfs: boolean; orf_min_codons: number };
  selection: {
    kind: 'feature' | 'orf' | 'range' | null;
    orf_id: string | null;
    range: { start: number; end: number } | null;
    /** Displayed translation of the selected range, and its strand control. */
    range_translation: { strand: string; protein: string } | null;
    feature_id: string | null;
    map: null | { parts: Part[]; active_labels: string[] };
    sequence: null | {
      /** Displayed CDS translations: letters (or 3-letter names) in codon order, with each codon's column. */
      translations: { feature_id: string; protein: string; codon_count: number; middles: number[]; warned: boolean }[];
      frames: { frame: string; protein: string; middles: number[] }[];
      orf_regions: Region[];
      parts: ReconstructedPart[];
      part_indices: number[];
      highlighted_positions: number[];
      complement_highlighted_positions: number[];
    };
  };
  /** No panel is rendered for warning-free documents: counts are then 0 and `present` false. */
  theme: { preference: string; resolved: 'light' | 'dark' };
  /** Sequence layout is null unless the Sequence tab is visible. */
  layout: {
    feature_list_collapsed: boolean; sequence_columns: number | null; sequence_fits_width: boolean | null;
    /** Rendered base span [first, last + 1) in the Sequence view (null when hidden or empty), and its notice for large records. */
    sequence_rendered: { start: number; end: number } | null; sequence_window_note: string;
    /** Every base of the selected range is rendered (null without a range or when the view is hidden). */
    sequence_shows_range: boolean | null;
    /** ORF and six-frame controls are usable (false for records over 100 kb). */
    orf_controls_enabled: boolean;
    /** Distinct features with an annotation track in the Sequence view, in drawn order;
     * null when that view is hidden. A selected isoform narrows this to its own mRNA and CDS. */
    sequence_feature_ids: string[] | null;
    /** Rendered amino-acid letters whose centre is not over their codon's middle base
     * (CSS geometry, not the model: `middles` can be right while the row is drawn wrong). */
    amino_acids_off_their_codon: number | null;
  };
  /** Rendered map accounting and geometry checks; null unless the Map tab is visible. */
  map: null | {
    width: number; height: number; radius: number; fills_panel: boolean;
    orf_regions: Region[]; undrawn_orfs: number;
    /** The drawn layout was computed for the canvas's current size (resize handled). */
    layout_current: boolean;
    drawn_ids: string[];
    labels: { id: string; mode: string }[];
    unlabelled_ids: string[]; unlabelled_names: string[]; unlabelled_count: number;
    badged_ids: string[]; notice_count: number;
    /** Ids (drawn order) with exactly one label, or none and a list badge. */
    accounted_ids: string[];
    overlapping_labels: number; labels_outside_viewport: number; labels_under_notice: number;
  };
  /** Restriction display. Map and sequence parts are read from the DOM (null when that view is hidden). */
  enzymes: {
    set: string; set_shown: string;
    catalogue: { source: string; count: number; names: string[] } | null;
    /** Enzymes the app chose to show (model), alphabetical; null while engine results are pending. */
    shown: string[] | null;
    map: null | {
      /** Cut positions with a tick, ascending. */
      ticks: number[];
      /** Labels as displayed: "EcoRI, ApoI (396)" read back into names and cut. */
      labels: { cut: number; names: string[] }[];
      unlabelled: number;
      /** Every tick has exactly one label, or is counted in the notice. */
      accounted: boolean;
    };
    sequence: null | {
      /** Recognition regions rebuilt from the drawn site tracks. */
      sites: { enzyme: string; start: number; length: number }[];
      /** Cut boundaries drawn on each strand (position of the base after the cut). */
      cuts: { top: number[]; bottom: number[] };
    };
    digest: { summary: string; fragments: { start: number; length: number }[]; enzymes: string[] | null };
  };
  /** The in-app confirmation dialog (delete feature, discard changes). */
  confirm: { open: boolean; title: string; message: string; ok: string };
  /** Detect features panel, read from the DOM; null while it is closed. */
  detect: null | {
    available: boolean; summary: string; add_label: string;
    /** Rows in panel order: library id, displayed name, span, strand and badges. */
    rows: { library_id: number; name: string; start: number; length: number; strand: string; annotated: boolean; contained: boolean; variants: number }[];
    /** Ticked rows as {library_id, start}. */
    checked: { library_id: number; start: number }[];
    /** Names of rows not yet annotated. */
    new_names: string[];
    /** Names of rows not yet annotated and not ticked (what should stay new after adding). */
    unticked_new_names: string[];
    /** Proposal arcs drawn on the map (Map tab only), by start then longer first. */
    map: { start: number; length: number }[] | null;
    /** Map arcs drawn as ticked, as {start, length}. */
    map_checked: { start: number; length: number }[] | null;
  };
  warnings: { present: boolean; count_shown: number; items: number; codes: string[]; open: boolean };
  /** Isoform view; null unless the document is a gene locus. Rows are read from the DOM (empty while the tab is hidden). */
  isoforms: null | {
    tab_shown: boolean; quantifier: string; compress: boolean;
    window: { start: number; end: number } | null;
    /** Model order of the last render and the rows as drawn, top to bottom. */
    drawn_ids: string[];
    /** Exons and coding stretches rebuilt from the drawn boxes; codon marks with the bases they cover. */
    rows: { transcript: string; display: string; feature_id: string; selected: boolean; exons: { start: number; end: number }[]; cds: { start: number; end: number }[]; codons: { kind: string; position: number; bases: number }[] }[];
    highlighted_exons: { start: number; end: number }[];
    range: { start: number; end: number } | null;
    /** Tick labels (genomic coordinates) as displayed, left to right. */
    tick_labels: number[];
    notice: string;
    detail: null | { transcript: string; display: string; text: string; cells: { cell_line: string; mean: number | null }[]; cells_shown: string[] };
    buttons: { zoom_selection: boolean; show_sequence: boolean };
  };
  primers: { count: number; summary: string };
}

const byTestId = (id: string, root: ParentNode = document) => [...root.querySelectorAll<HTMLElement>(`[data-testid="${id}"]`)];

function required<T extends Element>(node: T | null | undefined, what: string): T {
  if (!node) throw new Error(`e2e: ${what} not found`);
  return node;
}

const visible = (id: string) => !document.getElementById(id)!.hidden;

function isoformState(model: ModelView, panels: string[]): AppState['isoforms'] {
  if (!model.current?.locus) return null;
  const shown = panels.includes('isoforms');
  const svg = document.getElementById('isoforms')!;
  const span = (n: Element) => ({ start: Number((n as SVGElement).dataset.start), end: Number((n as SVGElement).dataset.end) });
  // Abutting drawn pieces form one exon (or one coding stretch).
  const merge = (pieces: { start: number; end: number }[]) => pieces.sort((a, b) => a.start - b.start)
    .reduce<{ start: number; end: number }[]>((out, p) => {
      const last = out[out.length - 1];
      if (last && last.end === p.start) last.end = p.end; else out.push({ ...p });
      return out;
    }, []);
  const rows = shown ? [...svg.querySelectorAll<SVGElement>('[data-testid="isoform-row"]')].map(row => {
    const pieces = [...row.querySelectorAll<SVGElement>('[data-testid="isoform-exon"]')];
    const marks = [...row.querySelectorAll<SVGElement>('[data-testid="isoform-codon"]')];
    return {
      transcript: row.dataset.transcript!, display: row.dataset.display!, feature_id: row.dataset.featureId!,
      selected: row.getAttribute('aria-pressed') === 'true',
      exons: merge(pieces.map(span)),
      cds: merge(pieces.filter(n => n.dataset.coding === 'true').map(span)),
      codons: ['start', 'stop'].flatMap(kind => {
        const blocks = marks.filter(n => n.dataset.kind === kind);
        return blocks.length ? [{ kind, position: Number(blocks[0].dataset.position), bases: blocks.reduce((sum, n) => sum + span(n).end - span(n).start, 0) }] : [];
      }),
    };
  }) : [];
  const band = svg.querySelector('[data-testid="isoform-range"]');
  const detail = document.getElementById('isoform-detail')!;
  return {
    tab_shown: !document.getElementById('tab-isoforms')!.hidden,
    quantifier: (document.getElementById('iso-quantifier') as HTMLSelectElement).value,
    compress: (document.getElementById('iso-compress') as HTMLInputElement).checked,
    window: model.isoformUi && { start: model.isoformUi.start, end: model.isoformUi.end },
    drawn_ids: model.isoformsDrawn,
    rows,
    highlighted_exons: shown ? [...svg.querySelectorAll('[data-testid="isoform-exon-highlight"]')].map(span) : [],
    range: shown && band ? span(band) : null,
    tick_labels: shown ? [...svg.querySelectorAll('.iso-tick-label')].map(n => Number((n.textContent ?? '').replace(/\D/g, ''))) : [],
    notice: document.getElementById('iso-notice')!.hidden ? '' : document.getElementById('iso-notice')!.textContent ?? '',
    detail: !shown || detail.hidden ? null : {
      transcript: detail.dataset.transcript ?? '',
      display: detail.querySelector<HTMLElement>('[data-testid="isoform-detail-state"]')?.dataset.display ?? '',
      text: detail.textContent ?? '',
      cells: [...detail.querySelectorAll<HTMLElement>('[data-testid="isoform-cell"]')].map(row => ({
        cell_line: row.dataset.cellLine!, mean: row.dataset.mean ? Number(row.dataset.mean) : null,
      })),
      cells_shown: [...detail.querySelectorAll<HTMLElement>('[data-testid="isoform-cell"] .iso-value')].map(n => n.textContent ?? ''),
    },
    buttons: {
      zoom_selection: !(document.getElementById('iso-zoom-selection') as HTMLButtonElement).disabled,
      show_sequence: !(document.getElementById('iso-show-sequence') as HTMLButtonElement).disabled,
    },
  };
}

function positionsOf(grid: Element): number[] {
  return [...grid.querySelectorAll<HTMLElement>('mark[data-position]')].map(m => Number(m.dataset.position));
}

/** Rebuild one circular interval per source part from the rendered track spans. */
function reconstructParts(length: number): { parts: ReconstructedPart[]; indices: number[] } {
  const covered = new Map<number, Set<number>>();
  for (const block of document.querySelectorAll<HTMLElement>('#sequence .sequence-block')) {
    const rowStart = Number(block.dataset.rowStart);
    for (const track of block.querySelectorAll<HTMLElement>('[data-testid="sequence-track"][aria-pressed="true"]')) {
      const [from, to] = track.style.gridColumn.split('/').map(v => Number(v.trim()) - 1);
      const index = Number(track.dataset.partIndex);
      const set = covered.get(index) ?? new Set<number>();
      for (let column = from; column < to; column++) set.add(rowStart + column);
      covered.set(index, set);
    }
  }
  const indices = [...covered.keys()].sort((a, b) => a - b);
  const parts = indices.map((index): ReconstructedPart => {
    const set = covered.get(index)!;
    const positions = [...set].sort((a, b) => a - b);
    // Interval start: the covered base whose predecessor (circularly) is uncovered.
    const starts = positions.filter(p => !set.has((p - 1 + length) % length));
    if (starts.length !== 1) return { error: `part ${index} is not one contiguous interval`, positions };
    return { start: starts[0], length: set.size };
  });
  return { parts, indices };
}

/** Text a user sees: all descendants except hover <title> tooltips. */
function visibleText(node: Element): string {
  return [...node.childNodes].map(child => child.nodeType === Node.TEXT_NODE ? child.textContent ?? ''
    : child instanceof Element && child.tagName.toLowerCase() !== 'title' ? visibleText(child) : '').join('');
}

const unique = (ids: string[]) => ids.filter((id, index) => ids.indexOf(id) === index);

function intersects(a: DOMRect, b: DOMRect): boolean {
  return a.left < b.right - 0.5 && b.left < a.right - 0.5 && a.top < b.bottom - 0.5 && b.top < a.bottom - 0.5;
}

function detectState(panels: string[]): AppState['detect'] {
  if (document.getElementById('detect-panel')!.hidden) return null;
  const rows = byTestId('detect-row').map(row => ({
    library_id: Number(row.dataset.libraryId), name: row.querySelector('[data-testid="detect-name"]')!.textContent ?? '',
    start: Number(row.dataset.start), length: Number(row.dataset.length), strand: row.dataset.strand ?? '',
    annotated: row.classList.contains('annotated'),
    contained: (row.querySelector('.detail')?.textContent ?? '').includes(' inside '),
    // As displayed: "+N shorter variants".
    variants: Number(/\+(\d+) shorter variant/.exec(row.querySelector('.detail')?.textContent ?? '')?.[1] ?? 0),
  }));
  const ticked = byTestId('detect-row').map(row => (row.querySelector('input') as HTMLInputElement).checked);
  const arcs = [...document.querySelectorAll<SVGElement>('#map [data-testid="map-proposal"]')]
    .map(n => ({ start: Number(n.dataset.start), length: Number(n.dataset.length), checked: n.classList.contains('checked') }))
    .sort((a, b) => a.start - b.start || b.length - a.length);
  return {
    available: !(document.getElementById('detect-summary')!.textContent ?? '').startsWith('No feature library'),
    summary: document.getElementById('detect-summary')!.textContent ?? '',
    add_label: document.getElementById('detect-add')!.textContent ?? '',
    rows,
    checked: rows.filter((_, i) => ticked[i]).map(r => ({ library_id: r.library_id, start: r.start })),
    new_names: rows.filter(r => !r.annotated).map(r => r.name),
    unticked_new_names: rows.filter((r, i) => !r.annotated && !ticked[i]).map(r => r.name),
    map: panels.includes('map') ? arcs.map(({ start, length }) => ({ start, length })) : null,
    map_checked: panels.includes('map') ? arcs.filter(a => a.checked).map(({ start, length }) => ({ start, length })) : null,
  };
}

function enzymeState(model: ModelView, panels: string[]): AppState['enzymes'] {
  const length = model.current?.sequence.length ?? 0;
  const set = document.getElementById('enzyme-set') as HTMLSelectElement;
  let map: AppState['enzymes']['map'] = null;
  if (model.current && panels.includes('map')) {
    const ticks = [...document.querySelectorAll<SVGElement>('#map [data-testid="map-site-tick"]')].map(n => Number(n.dataset.siteCut)).sort((a, b) => a - b);
    const labels = [...document.querySelectorAll<SVGElement>('#map [data-testid="map-site-label"]')].map(node => {
      const match = /^(.*) \((\d+)\)$/.exec(visibleText(node));
      return match ? { cut: Number(match[2]), names: match[1].split(', ') } : { cut: -1, names: [visibleText(node)] };
    }).sort((a, b) => a.cut - b.cut);
    const unlabelled = Number(document.getElementById('map-notice')!.dataset.unlabelledSites ?? 0);
    const labelled = labels.map(label => label.cut);
    map = { ticks, labels, unlabelled,
      accounted: new Set(labelled).size === labelled.length && labelled.every(cut => ticks.includes(cut)) && labelled.length + unlabelled === ticks.length };
  }
  let sequence: AppState['enzymes']['sequence'] = null;
  if (model.current && panels.includes('sequence')) {
    const covered = new Map<string, { enzyme: string; set: Set<number> }>();
    for (const block of document.querySelectorAll<HTMLElement>('#sequence .sequence-block')) {
      const rowStart = Number(block.dataset.rowStart);
      for (const track of block.querySelectorAll<HTMLElement>('[data-testid="site-track"]')) {
        const key = `${track.dataset.enzyme}@${track.dataset.siteStart}`;
        const entry = covered.get(key) ?? { enzyme: track.dataset.enzyme!, set: new Set<number>() };
        const [from, to] = track.style.gridColumn.split('/').map(v => Number(v.trim()) - 1);
        for (let column = from; column < to; column++) entry.set.add(rowStart + column);
        covered.set(key, entry);
      }
    }
    const sites = [...covered.values()].map(({ enzyme, set: bases }) => ({
      enzyme, start: [...bases].find(p => !bases.has((p - 1 + length) % length)) ?? 0, length: bases.size,
    })).sort((a, b) => a.start - b.start || a.enzyme.localeCompare(b.enzyme));
    const cutsOf = (strand: string) => [...document.querySelectorAll<HTMLElement>(`#sequence [data-strand="${strand}"] .cut-before`)]
      .map(base => Number(base.dataset.position)).sort((a, b) => a - b);
    sequence = { sites, cuts: { top: cutsOf('forward'), bottom: cutsOf('complement') } };
  }
  const catalogue = model.enzymes.catalogue;
  return {
    set: model.enzymes.set, set_shown: set.value,
    catalogue: catalogue && { source: catalogue.source, count: catalogue.enzymes.length, names: catalogue.enzymes.map(e => e.name) },
    shown: model.enzymes.shown,
    map, sequence,
    digest: {
      summary: document.getElementById('digest-summary')!.textContent ?? '',
      fragments: byTestId('digest-fragment').map(node => ({ start: Number(node.dataset.start), length: Number(node.dataset.length) }))
        .sort((a, b) => a.start - b.start || a.length - b.length),
      enzymes: model.enzymes.digest?.enzymes ?? null,
    },
  };
}

function mapState(current: Document): AppState['map'] {
  const svg = document.getElementById('map')!;
  const panel = document.getElementById('panel-map')!;
  const box = svg.getBoundingClientRect();
  const panelBox = panel.getBoundingClientRect();
  const drawn = unique([...svg.querySelectorAll<SVGElement>('[data-testid="map-feature"]')].map(node => node.dataset.featureId!));
  const labelNodes = [...svg.querySelectorAll<SVGElement>('[data-testid="map-label"]')];
  const labels = labelNodes.map(node => ({ id: node.dataset.featureId!, mode: node.dataset.labelMode ?? '' }));
  const unlabelled = drawn.filter(id => !labels.some(label => label.id === id));
  const badged = [...document.querySelectorAll<HTMLElement>('[data-testid="feature-item"]')]
    .filter(item => item.querySelector('[data-testid="feature-unlabelled"]')).map(item => item.dataset.featureId!);
  const notice = document.getElementById('map-notice')!;
  const siteLabels = [...svg.querySelectorAll<SVGElement>('[data-testid="map-site-label"]')];
  const pills = [...labelNodes.filter(node => node.dataset.labelMode === 'outside'), ...siteLabels].map(node => node.getBoundingClientRect());
  let overlapping = 0;
  pills.forEach((a, i) => pills.slice(i + 1).forEach(b => { if (intersects(a, b)) overlapping++; }));
  const outside = [...labelNodes, ...siteLabels].map(node => node.getBoundingClientRect())
    .filter(r => r.left < box.left - 0.5 || r.right > box.right + 0.5 || r.top < box.top - 0.5 || r.bottom > box.bottom + 0.5).length;
  const names = new Map(current.features.map(f => [f.id, f.label || f.kind]));
  return {
    width: Number(svg.dataset.width), height: Number(svg.dataset.height), radius: Number(svg.dataset.radius),
    orf_regions: sortRegions([...svg.querySelectorAll<SVGElement>('[data-testid="map-orf"]')].map(node => ({
      strand: node.dataset.orfStrand!, start: Number(node.dataset.orfStart), length: Number(node.dataset.orfLength) }))),
    undrawn_orfs: Number(notice.dataset.undrawnOrfs ?? 0),
    fills_panel: Math.abs(box.width - panelBox.width) <= 2 && Math.abs(box.bottom - panelBox.bottom) <= 2,
    layout_current: Math.abs(Number(svg.dataset.width) - Math.max(240, svg.clientWidth)) <= 1
      && Math.abs(Number(svg.dataset.height) - Math.max(160, svg.clientHeight)) <= 1,
    drawn_ids: drawn, labels,
    unlabelled_ids: unlabelled, unlabelled_names: unlabelled.map(id => names.get(id) ?? id), unlabelled_count: unlabelled.length,
    badged_ids: badged, notice_count: notice.hidden ? 0 : Number(notice.dataset.unlabelled ?? 0),
    accounted_ids: drawn.filter(id => {
      const count = labels.filter(label => label.id === id).length;
      return count === 1 || (count === 0 && badged.includes(id));
    }),
    overlapping_labels: overlapping, labels_outside_viewport: outside,
    labels_under_notice: notice.hidden ? 0 : [...labelNodes, ...siteLabels].filter(node => intersects(node.getBoundingClientRect(), notice.getBoundingClientRect())).length,
  };
}

/** Read `name · 1,234 bp · topology` back from the rendered title. */
function displayedDocument(title: string): NonNullable<AppState['document']> {
  const match = title.match(/^(.*) · ([\d,.\s]+) bp · (\w+)$/);
  if (!match) return { name: null, length: null, topology: null, title };
  return { name: match[1], length: Number(match[2].replace(/\D/g, '')), topology: match[3], title };
}

function aminoRows(rows: HTMLElement[], three: boolean) {
  const cells = rows.flatMap(row => [...row.querySelectorAll<HTMLElement>('[data-testid="amino-acid"]')]);
  const byCodon = new Map<number, HTMLElement>();
  for (const cell of cells) {
    const codon = Number(cell.dataset.codonIndex);
    if (byCodon.has(codon)) throw new Error(`codon ${codon} drawn twice`);
    byCodon.set(codon, cell);
  }
  const ordered = [...byCodon.entries()].sort((a, b) => a[0] - b[0]).map(([, cell]) => cell);
  const block = (cell: HTMLElement) => Number(cell.closest<HTMLElement>('.sequence-block')!.dataset.rowStart);
  return {
    protein: ordered.map(cell => cell.textContent ?? '').join(three ? ' ' : ''),
    codon_count: ordered.length,
    // Column → reference position: the row start plus the (middle) grid column.
    middles: ordered.map(cell => {
      const [from, to] = cell.style.gridColumn.split('/').map(v => Number(v.trim()) - 1);
      return block(cell) + (to === undefined ? from : Math.floor((from + to - 1) / 2));
    }),
  };
}

function regionsFromTracks(selector: string, length: number): Region[] {
  const covered = new Map<string, { strand: string; set: Set<number> }>();
  for (const blockNode of document.querySelectorAll<HTMLElement>('#sequence .sequence-block')) {
    const rowStart = Number(blockNode.dataset.rowStart);
    for (const track of blockNode.querySelectorAll<HTMLElement>(selector)) {
      const id = track.dataset.orfId!;
      const caption = track.closest('.sequence-line')!.querySelector('.line-caption')!.textContent ?? '';
      const entry = covered.get(id) ?? { strand: caption.startsWith('←') ? 'reverse' : 'forward', set: new Set<number>() };
      const [from, to] = track.style.gridColumn.split('/').map(v => Number(v.trim()) - 1);
      for (let column = from; column < to; column++) entry.set.add(rowStart + column);
      covered.set(id, entry);
    }
  }
  return sortRegions([...covered.values()].map(({ strand, set }) => {
    const start = [...set].find(p => !set.has((p - 1 + length) % length)) ?? 0;
    return { strand, start, length: set.size };
  }));
}

const sortRegions = (regions: Region[]) => regions.sort((a, b) =>
  a.start - b.start || Number(a.strand === 'reverse') - Number(b.strand === 'reverse') || a.length - b.length);

export function getState(model: ModelView): AppState {
  const { current, selected } = model;
  const tabs = (['map', 'sequence', 'isoforms'] as const).filter(name => document.getElementById(`tab-${name}`)!.getAttribute('aria-selected') === 'true');
  const panels = ['map', 'sequence', 'isoforms'].filter(name => visible(`panel-${name}`));
  const consistent = tabs.length === 1 && panels.length === 1 && tabs[0] === panels[0] && tabs[0] === model.activeTab;
  const details = document.querySelector<HTMLDetailsElement>('[data-testid="warnings-panel"]');
  let sequence: AppState['selection']['sequence'] = null;
  if (current && panels.includes('sequence')) {
    const { parts, indices } = reconstructParts(current.sequence.length);
    const three = (document.getElementById('opt-aa') as HTMLSelectElement).value === 'three';
    const translationRows = [...document.querySelectorAll<HTMLElement>('#sequence [data-testid="translation-row"]')];
    // Source order (the feature list), not on-screen order: an origin-spanning CDS appears in row 0.
    const listOrder = [...document.querySelectorAll<HTMLElement>('[data-testid="feature-item"]')].map(item => item.dataset.featureId!);
    const translationIds = unique(translationRows.map(row => row.dataset.featureId!)).sort((a, b) => listOrder.indexOf(a) - listOrder.indexOf(b));
    const frameRows = [...document.querySelectorAll<HTMLElement>('#sequence [data-testid="frame-row"]')];
    sequence = {
      translations: translationIds.map(id => {
        const rows = translationRows.filter(row => row.dataset.featureId === id);
        return { feature_id: id, ...aminoRows(rows, three), warned: rows.some(row => row.parentElement!.querySelector('.line-caption')!.textContent!.includes('⚠')) };
      }),
      frames: unique(frameRows.map(row => row.dataset.frame!)).map(frame => ({ frame, ...(({ protein, middles }) => ({ protein, middles }))(aminoRows(frameRows.filter(row => row.dataset.frame === frame), three)) })),
      orf_regions: regionsFromTracks('[data-testid="orf-track"]', current.sequence.length),
      parts, part_indices: indices,
      highlighted_positions: [...document.querySelectorAll('#sequence [data-strand="forward"]')].flatMap(positionsOf),
      complement_highlighted_positions: [...document.querySelectorAll('#sequence [data-strand="complement"]')].flatMap(positionsOf),
    };
  }
  return {
    idle: pendingRequests() === 0 && !previewPending(),
    status: document.getElementById('status')!.textContent ?? '',
    path_input: document.querySelector<HTMLInputElement>('[data-testid="path-input"]')!.value,
    active_tab: consistent ? model.activeTab : 'inconsistent',
    visible_panels: panels,
    document: current && displayedDocument(document.getElementById('title')!.textContent ?? ''),
    features: byTestId('feature-item').map(button => ({
      id: button.dataset.featureId!,
      name: required(button.querySelector('[data-testid="feature-name"]'), 'feature name').textContent ?? '',
      selected: button.getAttribute('aria-pressed') === 'true',
      unlabelled: button.querySelector('[data-testid="feature-unlabelled"]') !== null,
      added: button.querySelector('[data-testid="feature-added"]') !== null,
      // As displayed: "name (strand)".
      strand: /\((forward|reverse|unknown)\)/.exec(button.textContent ?? '')?.[1] ?? '',
      text: button.textContent ?? '',
    })),
    options: {
      amino_acids: (document.getElementById('opt-aa') as HTMLSelectElement).value,
      show_frames: (document.getElementById('opt-frames') as HTMLInputElement).checked,
      show_orfs: (document.getElementById('opt-orfs') as HTMLInputElement).checked,
      orf_min_codons: Number((document.getElementById('opt-orf-min') as HTMLSelectElement).value),
    },
    selection: {
      kind: selected ? 'feature' : model.selectedOrf ? 'orf' : model.range ? 'range' : null,
      orf_id: model.selectedOrf,
      range: model.range,
      range_translation: document.getElementById('range-panel')!.hidden ? null : {
        strand: (document.getElementById('range-strand') as HTMLSelectElement).value,
        protein: document.getElementById('range-protein')!.textContent ?? '',
      },
      feature_id: selected,
      map: current && panels.includes('map') ? {
        parts: byTestId('map-selection-part').map(node => ({
          start: Number(node.dataset.partStart), length: Number(node.dataset.partLength),
        })),
        active_labels: [...document.querySelectorAll('#map [data-testid="map-label"].active')].map(visibleText),
      } : null,
      sequence,
    },
    // The theme actually applied to the page, not a recomputation of the preference.
    edit: model.edit && {
      revision: model.edit.revision, can_undo: model.edit.can_undo, can_redo: model.edit.can_redo, dirty: model.edit.dirty,
      added_feature_ids: model.edit.added_feature_ids, saved_path: model.edit.saved_path,
      shown: {
        dirty: !document.getElementById('dirty')!.hidden,
        undo: !(document.getElementById('undo') as HTMLButtonElement).disabled,
        redo: !(document.getElementById('redo') as HTMLButtonElement).disabled,
        new_feature: !document.getElementById('new-feature')!.hidden,
        delete_feature: !document.getElementById('delete-feature')!.hidden,
      },
    },
    dialog: dialogState(),
    tab_count: document.querySelectorAll('[data-testid="doc-tab"]').length,
    tabs: model.tabs.map(tab => {
      const node = [...document.querySelectorAll<HTMLElement>('[data-testid="doc-tab"]')].find(n => n.dataset.documentId === String(tab.document_id));
      return { ...tab, shown_dirty: node?.querySelector('.dot') !== null && node !== undefined };
    }),
    workspace: {
      path: model.workspace,
      shown: document.getElementById('workspace')!.textContent ?? '',
      notices: [...document.querySelectorAll<HTMLElement>('[data-testid="file-notice"]')].map(n => ({ kind: n.dataset.kind ?? '', path: n.dataset.path ?? '' })),
      agent: {
        open: !document.getElementById('agent-panel')!.hidden,
        message: document.getElementById('agent-message')!.textContent ?? '',
        highlights: [...document.querySelectorAll<HTMLElement>('[data-testid="agent-highlight"]')].map(b => ({ text: b.textContent ?? '', active: b.classList.contains('active') })),
      },
      handoff: {
        open: !document.getElementById('handoff-panel')!.hidden,
        prompt: (document.getElementById('handoff-prompt') as HTMLTextAreaElement).value,
        context_path: model.lastHandoff?.context_path ?? null,
      },
    },
    theme: { preference: themeState().preference, resolved: document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light' },
    layout: {
      feature_list_collapsed: document.getElementById('toggle-features')!.getAttribute('aria-expanded') === 'false',
      sequence_columns: sequence ? Number(document.getElementById('sequence')!.style.getPropertyValue('--columns')) : null,
      sequence_fits_width: sequence ? (panel => panel.scrollWidth <= panel.clientWidth + 1)(document.getElementById('panel-sequence')!) : null,
      ...(() => {
        const rendered = panels.includes('sequence')
          ? [...document.querySelectorAll<HTMLElement>('#sequence [data-strand="forward"] [data-position]')].map(n => Number(n.dataset.position)) : [];
        // reduce, not spread: a whole 100 kb record would exceed the argument limit.
        const span = rendered.length ? { start: rendered.reduce((a, b) => Math.min(a, b)), end: rendered.reduce((a, b) => Math.max(a, b)) + 1 } : null;
        const r = model.range;
        return {
          sequence_rendered: span,
          sequence_window_note: document.getElementById('sequence-window')!.hidden ? '' : document.getElementById('sequence-window')!.textContent ?? '',
          sequence_shows_range: !panels.includes('sequence') || !r ? null : span !== null && r.start < r.end && span.start <= r.start && span.end >= r.end,
        };
      })(),
      orf_controls_enabled: !(document.getElementById('opt-orfs') as HTMLInputElement).disabled && !(document.getElementById('opt-frames') as HTMLInputElement).disabled,
      sequence_feature_ids: !panels.includes('sequence') ? null
        : [...new Set([...document.querySelectorAll<HTMLElement>('#sequence [data-testid="sequence-track"]')].map(n => n.dataset.featureId!))],
      amino_acids_off_their_codon: !panels.includes('sequence') ? null : (() => {
        let off = 0;
        for (const cell of document.querySelectorAll<HTMLElement>('#sequence [data-testid="amino-acid"]')) {
          const block = cell.closest('.sequence-block');
          const base = block?.querySelector<HTMLElement>(`[data-strand="forward"] [data-position="${cell.dataset.codonMiddle}"]`);
          if (!base) continue;
          const letter = cell.getBoundingClientRect(), column = base.getBoundingClientRect();
          // Half a column: a letter is "on" its codon while it stays within that base's cell.
          if (Math.abs((letter.left + letter.right) / 2 - (column.left + column.right) / 2) > column.width / 2) off++;
        }
        return off;
      })(),
    },
    map: current && panels.includes('map') ? mapState(current) : null,
    warnings: {
      present: details !== null,
      count_shown: details ? Number.parseInt(byTestId('warnings-summary')[0]?.textContent ?? '', 10) : 0,
      items: byTestId('warning-item').length,
      codes: byTestId('warning-item').map(item => (item.textContent ?? '').trim().split(':')[0]),
      open: details?.open ?? false,
    },
    enzymes: enzymeState(model, panels),
    isoforms: isoformState(model, panels),
    detect: detectState(panels),
    confirm: {
      open: (document.getElementById('confirm-dialog') as HTMLDialogElement).open,
      title: document.getElementById('confirm-title')!.textContent ?? '',
      message: document.getElementById('confirm-message')!.textContent ?? '',
      ok: document.getElementById('confirm-ok')!.textContent ?? '',
    },
    primers: { count: byTestId('primer-item').length, summary: document.getElementById('primer-summary')!.textContent ?? '' },
  };
}

export interface AutomationApi {
  /** This page's backend session (its agent socket is named after it). */
  sessionId(): string;
  open(path: string, options?: { delayMs?: number }): void;
  browse(path: string | null): void;
  selectFeature(featureId: string, extend?: boolean): void;
  saveAs(path: string | null): void;
  /** Choose the workspace through the Workspace… button (queued folder picker answer). */
  chooseWorkspace(path: string): void;
  /** Run the same workspace poll the timer runs. */
  pollWorkspace(): Promise<void>;
  selectTab(name: 'map' | 'sequence' | 'isoforms'): void;
  /** Click an isoform row (Isoforms tab). */
  selectIsoform(transcript: string): void;
  /** Double-click exon `exon` (zero-based, locus order) of an isoform. */
  doubleClickExon(transcript: string, exon: number): void;
  /** Drag from the left edge of exon `from` to the right edge of exon `to` of an isoform. */
  dragExons(transcript: string, from: number, to: number): void;
  /** Client rectangle of the drawn piece at the start or end of an isoform's exon (for real pointer input). */
  exonBox(transcript: string, exon: number, edge: 'start' | 'end'): { left: number; right: number; top: number; height: number; width: number };
  /** Click an isoform toolbar button: zoom-in, zoom-out, zoom-fit, zoom-selection, show-sequence. */
  isoformButton(name: string): void;
  /** Choose the ordering quantifier / toggle intron compression through the toolbar controls. */
  setIsoformQuantifier(quantifier: string): void;
  setCompressIntrons(on: boolean): void;
  /** Queue the save dialog's answer, then click Export SVG… on the map or isoform view. */
  exportSvg(view: 'map' | 'isoforms', path: string | null): void;
  clickSequenceBase(index: number): void;
  selectOrf(orfId: string): void;
  /** Click the map label (Map tab) or the first site track (Sequence tab) of `enzyme`. */
  clickSite(enzyme: string): void;
  /** Click the tick box of the first Detected row named `name`. */
  toggleDetection(name: string): void;
  /** Click the first Detected row named `name` (selects it). */
  selectDetection(name: string): void;
  /** Tick exactly these enzymes in the chooser and confirm (opens it via Choose…). */
  chooseEnzymes(names: string[]): void;
  getState(): AppState;
}

function isoformRow(transcript: string): SVGElement {
  if (!visible('panel-isoforms')) throw new Error('e2e: isoform actions require the Isoforms tab');
  const row = [...document.querySelectorAll<SVGElement>('#isoforms [data-testid="isoform-row"]')].find(r => r.dataset.transcript === transcript);
  return required(row, `isoform row ${transcript}`);
}

/** The drawn piece at the start (or end) of an isoform's `exon`-th exon, in locus order. */
function exonNode(transcript: string, exon: number, edge: 'start' | 'end'): SVGElement {
  const pieces = [...isoformRow(transcript).querySelectorAll<SVGElement>('[data-testid="isoform-exon"]')]
    .sort((a, b) => Number(a.dataset.start) - Number(b.dataset.start));
  // Pieces of one exon abut; a gap starts a new exon.
  const exons: SVGElement[][] = [];
  for (const piece of pieces) {
    const last = exons[exons.length - 1];
    if (last && Number(last[last.length - 1].dataset.end) === Number(piece.dataset.start)) last.push(piece); else exons.push([piece]);
  }
  const group = exons[exon];
  if (!group) throw new Error(`e2e: exon ${exon} of ${transcript} not drawn`);
  return edge === 'start' ? group[0] : group[group.length - 1];
}

export function install(model: () => ModelView, hooks: Hooks) {
  const api: AutomationApi = {
    /** Type a path and submit the open form, as a user would. */
    open(path: string, options: { delayMs?: number } = {}) {
      if (options.delayMs) setDelay(path, options.delayMs);
      const input = required(document.querySelector<HTMLInputElement>('[data-testid="path-input"]'), 'path input');
      input.value = path;
      required(document.querySelector<HTMLFormElement>('[data-testid="open-form"]'), 'open form').requestSubmit();
    },
    /** Queue the native picker's answer, then click Browse…. */
    browse(path: string | null) {
      queuePick(path);
      required(byTestId('browse')[0], 'Browse button').click();
    },
    selectFeature(featureId: string, extend = false) {
      const item = byTestId('feature-item').find(button => button.dataset.featureId === featureId);
      required(item, `feature list item ${featureId}`).dispatchEvent(new MouseEvent('click', { bubbles: true, shiftKey: extend }));
    },
    chooseWorkspace(path: string) {
      queuePick(path);
      required(byTestId('workspace-button')[0], 'Workspace button').click();
    },
    pollWorkspace: () => hooks.pollWorkspace(),
    sessionId: () => session,
    /** Queue the save dialog's answer, then click Save as…. */
    saveAs(path: string | null) {
      queueSave(path);
      required(byTestId('save-as')[0], 'Save as button').click();
    },
    selectTab(name: 'map' | 'sequence' | 'isoforms') {
      const tab = required(byTestId(`tab-${name}`)[0], `tab ${name}`);
      if (tab.hidden) throw new Error(`e2e: the ${name} tab is not offered for this document`);
      tab.click();
    },
    selectIsoform(transcript: string) {
      isoformRow(transcript).dispatchEvent(new MouseEvent('click', { bubbles: true }));
    },
    doubleClickExon(transcript: string, exon: number) {
      exonNode(transcript, exon, 'start').dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 1 }));
      document.getElementById('isoforms')!.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 2 }));
    },
    dragExons(transcript: string, from: number, to: number) {
      const a = exonNode(transcript, from, 'start').getBoundingClientRect();
      const b = exonNode(transcript, to, 'end').getBoundingClientRect();
      const y = a.top + a.height / 2;
      const svg = document.getElementById('isoforms')!;
      svg.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: a.left, clientY: y }));
      window.dispatchEvent(new MouseEvent('mousemove', { clientX: (a.left + b.right) / 2, clientY: y }));
      window.dispatchEvent(new MouseEvent('mousemove', { clientX: b.right, clientY: y }));
      window.dispatchEvent(new MouseEvent('mouseup', { button: 0, clientX: b.right, clientY: y }));
    },
    exonBox(transcript: string, exon: number, edge: 'start' | 'end') {
      const node = exonNode(transcript, exon, edge);
      node.scrollIntoView({ block: 'nearest' });
      const r = node.getBoundingClientRect();
      return { left: r.left, right: r.right, top: r.top, height: r.height, width: r.width };
    },
    isoformButton(name: string) {
      const button = required(byTestId(`isoform-${name}`)[0] as HTMLButtonElement | undefined, `isoform button ${name}`);
      if (button.disabled) throw new Error(`e2e: isoform button ${name} is disabled`);
      button.click();
    },
    setIsoformQuantifier(quantifier: string) {
      const select = document.getElementById('iso-quantifier') as HTMLSelectElement;
      if (![...select.options].some(o => o.value === quantifier)) throw new Error(`e2e: quantifier ${quantifier} is not offered`);
      select.value = quantifier;
      select.dispatchEvent(new Event('change', { bubbles: true }));
    },
    setCompressIntrons(on: boolean) {
      const box = document.getElementById('iso-compress') as HTMLInputElement;
      if (box.checked !== on) box.click();
    },
    exportSvg(view: 'map' | 'isoforms', path: string | null) {
      if (!visible(`panel-${view}`)) throw new Error(`e2e: exportSvg('${view}') requires the ${view} tab`);
      queueSave(path);
      required(byTestId(view === 'map' ? 'map-export' : 'isoform-export')[0], `${view} export button`).click();
    },
    clickSequenceBase(index: number) {
      if (!visible('panel-sequence')) throw new Error('e2e: clickSequenceBase requires the Sequence tab');
      const base = document.querySelector<HTMLElement>(`#sequence [data-strand="forward"] [data-position="${index}"]`);
      required(base, `sequence base ${index}`).click();
    },
    /** Click the ORF's track (Sequence tab) or arc (Map tab). */
    selectOrf(orfId: string) {
      const node = [...document.querySelectorAll<HTMLElement | SVGElement>('[data-testid="orf-track"], [data-testid="map-orf"]')]
        .find(item => item.dataset.orfId === orfId && item.getClientRects().length > 0);
      if (!node) throw new Error(`e2e: ORF ${orfId} is not drawn (are ORFs shown, and long enough?)`);
      node.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    },
    clickSite(enzyme: string) {
      const node = [...document.querySelectorAll<HTMLElement | SVGElement>('#map [data-testid="map-site-label"], #sequence [data-testid="site-track"]')]
        .find(item => item.getClientRects().length > 0 && (item.dataset.enzymes ?? item.dataset.enzyme ?? '').split(',').includes(enzyme));
      if (!node) throw new Error(`e2e: no visible site label or track for ${enzyme}`);
      node.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    },
    toggleDetection(name: string) {
      const row = byTestId('detect-row').find(r => r.querySelector('[data-testid="detect-name"]')?.textContent === name);
      required(row, `Detected row ${name}`).querySelector<HTMLInputElement>('input')!.click();
    },
    selectDetection(name: string) {
      const row = byTestId('detect-row').find(r => r.querySelector('[data-testid="detect-name"]')?.textContent === name);
      required(row?.querySelector<HTMLElement>('[data-testid="detect-select"]'), `Detected row ${name}`).click();
    },
    chooseEnzymes(names: string[]) {
      required(byTestId('enzyme-choose')[0], 'Choose… button').click();
      const dialog = document.getElementById('enzyme-dialog') as HTMLDialogElement;
      if (!dialog.open) throw new Error('e2e: enzyme chooser did not open (catalogue not loaded?)');
      const options = byTestId('enzyme-option', dialog);
      for (const name of names) if (!options.some(o => o.dataset.enzyme === name)) throw new Error(`e2e: ${name} is not in the enzyme catalogue`);
      for (const option of options) {
        const box = option.querySelector('input')!;
        if (box.checked !== names.includes(option.dataset.enzyme!)) box.click();
      }
      required(byTestId('enzyme-done', dialog)[0], 'Show these button').click();
    },
    getState: () => getState(model()),
  };
  Object.assign(window, { __DNAGENT_TEST__: api });
}

declare global {
  interface Window { __DNAGENT_TEST__?: AutomationApi }
}

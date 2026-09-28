// e2e-only automation API (`window.__DNAGENT_TEST__`). Commands dispatch the same
// DOM events a user would; `getState` reports rendered DOM plus a read-only model view.
// Never set application state directly here, or the scenarios prove nothing.
import type { Document } from '../bindings';
import { pendingRequests } from '../ipc';
import { themeState } from '../theme';
import { queuePick, setDelay } from './stub-backend';

export interface ModelView {
  current: Document | null;
  selected: string | null;
  activeTab: 'map' | 'sequence';
}

export interface Part { start: number; length: number }
export type ReconstructedPart = Part | { error: string; positions: number[] };

export interface AppState {
  idle: boolean;
  status: string;
  path_input: string;
  active_tab: 'map' | 'sequence' | 'inconsistent';
  visible_panels: string[];
  /** Parsed from the displayed title, so display bugs are caught; null before any load. */
  document: null | { name: string | null; length: number | null; topology: string | null; title: string };
  features: { id: string; name: string; selected: boolean; unlabelled: boolean; text: string }[];
  selection: {
    feature_id: string | null;
    map: null | { parts: Part[]; active_labels: string[] };
    sequence: null | {
      parts: ReconstructedPart[];
      part_indices: number[];
      highlighted_positions: number[];
      complement_highlighted_positions: number[];
    };
  };
  /** No panel is rendered for warning-free documents: counts are then 0 and `present` false. */
  theme: { preference: string; resolved: 'light' | 'dark' };
  layout: { feature_list_collapsed: boolean };
  /** Rendered map accounting and geometry checks; null unless the Map tab is visible. */
  map: null | {
    width: number; height: number; radius: number; fills_panel: boolean;
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
  warnings: { present: boolean; count_shown: number; items: number; codes: string[]; open: boolean };
  primers: { count: number; summary: string };
}

const byTestId = (id: string, root: ParentNode = document) => [...root.querySelectorAll<HTMLElement>(`[data-testid="${id}"]`)];

function required<T extends Element>(node: T | null | undefined, what: string): T {
  if (!node) throw new Error(`e2e: ${what} not found`);
  return node;
}

const visible = (id: string) => !document.getElementById(id)!.hidden;

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
  const pills = labelNodes.filter(node => node.dataset.labelMode === 'outside').map(node => node.getBoundingClientRect());
  let overlapping = 0;
  pills.forEach((a, i) => pills.slice(i + 1).forEach(b => { if (intersects(a, b)) overlapping++; }));
  const outside = labelNodes.map(node => node.getBoundingClientRect())
    .filter(r => r.left < box.left - 0.5 || r.right > box.right + 0.5 || r.top < box.top - 0.5 || r.bottom > box.bottom + 0.5).length;
  const names = new Map(current.features.map(f => [f.id, f.label || f.kind]));
  return {
    width: Number(svg.dataset.width), height: Number(svg.dataset.height), radius: Number(svg.dataset.radius),
    fills_panel: Math.abs(box.width - panelBox.width) <= 2 && Math.abs(box.bottom - panelBox.bottom) <= 2,
    layout_current: Math.abs(Number(svg.dataset.width) - Math.max(320, svg.clientWidth)) <= 1
      && Math.abs(Number(svg.dataset.height) - Math.max(320, svg.clientHeight)) <= 1,
    drawn_ids: drawn, labels,
    unlabelled_ids: unlabelled, unlabelled_names: unlabelled.map(id => names.get(id) ?? id), unlabelled_count: unlabelled.length,
    badged_ids: badged, notice_count: notice.hidden ? 0 : Number.parseInt(notice.textContent ?? '', 10),
    accounted_ids: drawn.filter(id => {
      const count = labels.filter(label => label.id === id).length;
      return count === 1 || (count === 0 && badged.includes(id));
    }),
    overlapping_labels: overlapping, labels_outside_viewport: outside,
    labels_under_notice: notice.hidden ? 0 : labelNodes.filter(node => intersects(node.getBoundingClientRect(), notice.getBoundingClientRect())).length,
  };
}

/** Read `name · 1,234 bp · topology` back from the rendered title. */
function displayedDocument(title: string): NonNullable<AppState['document']> {
  const match = title.match(/^(.*) · ([\d,.\s]+) bp · (\w+)$/);
  if (!match) return { name: null, length: null, topology: null, title };
  return { name: match[1], length: Number(match[2].replace(/\D/g, '')), topology: match[3], title };
}

export function getState(model: ModelView): AppState {
  const { current, selected } = model;
  const tabs = (['map', 'sequence'] as const).filter(name => document.getElementById(`tab-${name}`)!.getAttribute('aria-selected') === 'true');
  const panels = ['map', 'sequence'].filter(name => visible(`panel-${name}`));
  const consistent = tabs.length === 1 && panels.length === 1 && tabs[0] === panels[0] && tabs[0] === model.activeTab;
  const details = document.querySelector<HTMLDetailsElement>('[data-testid="warnings-panel"]');
  let sequence: AppState['selection']['sequence'] = null;
  if (current && panels.includes('sequence')) {
    const { parts, indices } = reconstructParts(current.sequence.length);
    sequence = {
      parts, part_indices: indices,
      highlighted_positions: [...document.querySelectorAll('#sequence [data-strand="forward"]')].flatMap(positionsOf),
      complement_highlighted_positions: [...document.querySelectorAll('#sequence [data-strand="complement"]')].flatMap(positionsOf),
    };
  }
  return {
    idle: pendingRequests() === 0,
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
      text: button.textContent ?? '',
    })),
    selection: {
      feature_id: selected,
      map: current && panels.includes('map') ? {
        parts: byTestId('map-selection-part').map(node => ({
          start: Number(node.dataset.partStart), length: Number(node.dataset.partLength),
        })),
        active_labels: [...document.querySelectorAll('#map [data-testid="map-label"].active')].map(visibleText),
      } : null,
      sequence,
    },
    theme: themeState(),
    layout: { feature_list_collapsed: document.getElementById('toggle-features')!.getAttribute('aria-expanded') === 'false' },
    map: current && panels.includes('map') ? mapState(current) : null,
    warnings: {
      present: details !== null,
      count_shown: details ? Number.parseInt(byTestId('warnings-summary')[0]?.textContent ?? '', 10) : 0,
      items: byTestId('warning-item').length,
      codes: byTestId('warning-item').map(item => (item.textContent ?? '').trim().split(':')[0]),
      open: details?.open ?? false,
    },
    primers: { count: byTestId('primer-item').length, summary: document.getElementById('primer-summary')!.textContent ?? '' },
  };
}

export interface AutomationApi {
  open(path: string, options?: { delayMs?: number }): void;
  browse(path: string | null): void;
  selectFeature(featureId: string): void;
  selectTab(name: 'map' | 'sequence'): void;
  clickSequenceBase(index: number): void;
  getState(): AppState;
}

export function install(model: () => ModelView) {
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
    selectFeature(featureId: string) {
      const item = byTestId('feature-item').find(button => button.dataset.featureId === featureId);
      required(item, `feature list item ${featureId}`).click();
    },
    selectTab(name: 'map' | 'sequence') {
      required(byTestId(`tab-${name}`)[0], `tab ${name}`).click();
    },
    clickSequenceBase(index: number) {
      if (!visible('panel-sequence')) throw new Error('e2e: clickSequenceBase requires the Sequence tab');
      const base = document.querySelector<HTMLElement>(`#sequence [data-strand="forward"] [data-position="${index}"]`);
      required(base, `sequence base ${index}`).click();
    },
    getState: () => getState(model()),
  };
  Object.assign(window, { __DNAGENT_TEST__: api });
}

declare global {
  interface Window { __DNAGENT_TEST__?: AutomationApi }
}

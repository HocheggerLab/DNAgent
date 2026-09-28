// e2e-only automation API (`window.__DNAGENT_TEST__`). Commands dispatch the same
// DOM events a user would; `getState` reports rendered DOM plus a read-only model view.
// Never set application state directly here, or the scenarios prove nothing.
import type { Document } from '../bindings';
import { pendingRequests } from '../ipc';
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
  active_tab: 'map' | 'sequence' | 'inconsistent';
  visible_panels: string[];
  document: null | { name: string; length: number; topology: 'circular' | 'linear'; title: string };
  features: { id: string; name: string; selected: boolean; text: string }[];
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
  warnings: null | { count_shown: number | null; items: number; codes: string[]; open: boolean };
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
    active_tab: consistent ? model.activeTab : 'inconsistent',
    visible_panels: panels,
    document: current && {
      name: current.name,
      length: current.sequence.length,
      topology: current.circular ? 'circular' : 'linear',
      title: document.getElementById('title')!.textContent ?? '',
    },
    features: byTestId('feature-item').map(button => ({
      id: button.dataset.featureId!,
      name: required(button.querySelector('[data-testid="feature-name"]'), 'feature name').textContent ?? '',
      selected: button.getAttribute('aria-pressed') === 'true',
      text: button.textContent ?? '',
    })),
    selection: {
      feature_id: selected,
      map: current && panels.includes('map') ? {
        parts: byTestId('map-selection-part').map(node => ({
          start: Number(node.dataset.partStart), length: Number(node.dataset.partLength),
        })),
        active_labels: [...document.querySelectorAll('#map .map-label.active')].map(node => node.textContent ?? ''),
      } : null,
      sequence,
    },
    warnings: details && {
      count_shown: Number.parseInt(byTestId('warnings-summary')[0]?.textContent ?? '', 10) || null,
      items: byTestId('warning-item').length,
      codes: byTestId('warning-item').map(item => (item.textContent ?? '').trim().split(':')[0]),
      open: details.open,
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

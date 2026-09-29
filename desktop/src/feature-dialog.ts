// New-feature dialog. Every preview and the final add come from the Rust session;
// this module only collects the request and shows the engine's answer.
import type { DocumentState, FeatureRequest } from './bindings';
import { addFeature, previewFeature } from './ipc';

const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export interface DialogContext {
  documentId: number;
  start: number;
  end: number;
  length: number;
  /** Called with the new state and the id of the feature that was added. */
  onAdded: (state: DocumentState, featureId: string) => void;
  previousAdded: string[];
}

let context: DialogContext | null = null;
let previewTimer = 0;
let previewToken = 0;

function request(): FeatureRequest {
  const translate = byId<HTMLInputElement>('feature-translate').checked;
  return {
    start: context!.start,
    end: context!.end,
    strand: byId<HTMLSelectElement>('feature-strand').value as FeatureRequest['strand'],
    kind: translate ? 'CDS' : byId<HTMLSelectElement>('feature-kind').value,
    label: byId<HTMLInputElement>('feature-label').value,
    color: byId<HTMLInputElement>('feature-color').value,
    translate: translate ? {
      table: Number(byId<HTMLSelectElement>('feature-table').value),
      codon_start: Number(byId<HTMLSelectElement>('feature-codon-start').value),
    } : null,
  };
}

function syncControls() {
  const translate = byId<HTMLInputElement>('feature-translate').checked;
  const kind = byId<HTMLSelectElement>('feature-kind');
  if (translate) kind.value = 'CDS';
  kind.disabled = translate;
  const strand = byId<HTMLSelectElement>('feature-strand');
  (strand.querySelector('option[value="unknown"]') as HTMLOptionElement).disabled = translate;
  if (translate && strand.value === 'unknown') strand.value = 'forward';
  byId<HTMLSelectElement>('feature-table').disabled = !translate;
  byId<HTMLSelectElement>('feature-codon-start').disabled = !translate;
}

async function refreshPreview() {
  if (!context) return;
  const token = ++previewToken;
  const summary = byId('preview-summary');
  const protein = byId('preview-protein');
  const warnings = byId('preview-warnings');
  const error = byId('feature-error');
  try {
    const preview = await previewFeature(context.documentId, request());
    if (token !== previewToken) return;
    error.textContent = '';
    byId<HTMLButtonElement>('feature-add').disabled = false;
    summary.textContent = `${preview.length.toLocaleString()} bp · ${preview.parts.map(p => `[${p.start}, ${p.start + p.length})`).join(', ')}`
      + (preview.protein ? ` · ${preview.protein.replace(/\*$/, '').length} aa${preview.protein.endsWith('*') ? ' + stop' : ''}` : '');
    protein.textContent = preview.protein ?? '';
    protein.hidden = !preview.protein;
    warnings.replaceChildren(...preview.warnings.map(w => {
      const item = document.createElement('li'); item.dataset.code = w.code; item.textContent = w.message; return item;
    }));
  } catch (failure) {
    if (token !== previewToken) return;
    error.textContent = (failure as { message?: string }).message ?? String(failure);
    byId<HTMLButtonElement>('feature-add').disabled = true;
    summary.textContent = ''; protein.textContent = ''; warnings.replaceChildren();
  }
}

function schedulePreview() {
  syncControls();
  clearTimeout(previewTimer);
  previewTimer = window.setTimeout(() => void refreshPreview(), 120);
}

export function openFeatureDialog(next: DialogContext) {
  context = next;
  const span = (next.end - next.start + next.length) % next.length || next.length;
  byId('feature-range').textContent = `Selection [${next.start}, ${next.end})${next.end < next.start ? ' (crosses the origin)' : ''} · ${span.toLocaleString()} bp`;
  byId<HTMLInputElement>('feature-label').value = 'New feature';
  byId<HTMLInputElement>('feature-translate').checked = false;
  byId<HTMLSelectElement>('feature-kind').value = 'misc_feature';
  byId<HTMLSelectElement>('feature-strand').value = 'forward';
  byId<HTMLSelectElement>('feature-table').value = '1';
  byId<HTMLSelectElement>('feature-codon-start').value = '1';
  byId('feature-error').textContent = '';
  syncControls();
  byId<HTMLDialogElement>('feature-dialog').showModal();
  byId<HTMLInputElement>('feature-label').select();
  void refreshPreview();
}

export function dialogState() {
  const dialog = byId<HTMLDialogElement>('feature-dialog');
  return {
    open: dialog.open,
    summary: byId('preview-summary').textContent ?? '',
    protein: byId('preview-protein').textContent ?? '',
    warnings: [...byId('preview-warnings').querySelectorAll<HTMLElement>('li')].map(li => li.dataset.code ?? ''),
    error: byId('feature-error').textContent ?? '',
  };
}

export function bindFeatureDialog(onError: (message: string) => void) {
  const form = byId<HTMLFormElement>('feature-form');
  for (const id of ['feature-label', 'feature-kind', 'feature-strand', 'feature-color', 'feature-translate', 'feature-table', 'feature-codon-start']) {
    byId(id).addEventListener('input', schedulePreview);
    byId(id).addEventListener('change', schedulePreview);
  }
  byId('feature-cancel').onclick = () => byId<HTMLDialogElement>('feature-dialog').close();
  form.onsubmit = async event => {
    event.preventDefault();
    if (!context) return;
    const current = context;
    try {
      const state = await addFeature(current.documentId, request());
      const featureId = state.edit.added_feature_ids.find(id => !current.previousAdded.includes(id)) ?? '';
      byId<HTMLDialogElement>('feature-dialog').close();
      current.onAdded(state, featureId);
    } catch (failure) {
      const message = (failure as { message?: string }).message ?? String(failure);
      byId('feature-error').textContent = message;
      onError(message);
    }
  };
}

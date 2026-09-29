// Which enzymes to show, and the enzyme chooser and digest list. Sites, counts and
// fragments are engine output (bindings.ts); this module only picks names and lists results.
import type { EnzymeCatalogueInfo, EnzymeCount, Fragment, FragmentEndInfo } from './bindings';

export type EnzymeSet = 'none' | 'unique6' | 'unique_dual6' | 'unique_any' | 'common' | 'custom';
export const ENZYME_SETS: EnzymeSet[] = ['none', 'unique6', 'unique_dual6', 'unique_any', 'common', 'custom'];

/** A short list of everyday cloning enzymes (shown when they have a site). */
export const COMMON_ENZYMES = [
  'AgeI', 'ApaI', 'AscI', 'AvrII', 'BamHI', 'BglII', 'BsrGI', 'ClaI', 'EcoRI', 'EcoRV', 'HindIII', 'HpaI', 'KpnI', 'MfeI', 'MluI',
  'NcoI', 'NdeI', 'NheI', 'NotI', 'NsiI', 'PacI', 'PstI', 'SacI', 'SacII', 'SalI', 'SmaI', 'SpeI', 'SphI', 'StuI', 'XbaI', 'XhoI', 'XmaI',
];

/** Recognition length not counting N: the "6+ cutter" measure (BglI GCCNNNNNGGC counts 6). */
export const specificity = (site: string) => [...site].filter(base => base !== 'N').length;

/** Enzyme names of a set, alphabetically. Counts are recognition sites in the document. */
export function enzymesInSet(set: EnzymeSet, catalogue: EnzymeCatalogueInfo, counts: EnzymeCount[], custom: string[]): string[] {
  const site = new Map(catalogue.enzymes.map(e => [e.name, e.site]));
  const sites = new Map(counts.map(c => [c.name, c.sites]));
  const long = (name: string) => specificity(site.get(name) ?? '') >= 6;
  const names = counts.map(c => c.name).filter(name => {
    const n = sites.get(name) ?? 0;
    switch (set) {
      case 'unique6': return n === 1 && long(name);
      case 'unique_dual6': return (n === 1 || n === 2) && long(name);
      case 'unique_any': return n === 1;
      case 'common': return n > 0 && COMMON_ENZYMES.includes(name);
      case 'custom': return n > 0 && custom.includes(name);
      default: return false;
    }
  });
  return names.sort((a, b) => a.localeCompare(b));
}

const KEY = 'dnagent.enzymes';
export interface EnzymeChoice { set: EnzymeSet; custom: string[] }

export function loadChoice(): EnzymeChoice {
  const choice: EnzymeChoice = { set: 'unique6', custom: [] };
  try {
    const stored = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<EnzymeChoice>;
    if (stored.set && ENZYME_SETS.includes(stored.set)) choice.set = stored.set;
    if (Array.isArray(stored.custom)) choice.custom = stored.custom.filter(name => typeof name === 'string');
  } catch { /* defaults */ }
  return choice;
}

export function saveChoice(choice: EnzymeChoice) {
  try { localStorage.setItem(KEY, JSON.stringify(choice)); } catch { /* not persisted */ }
}

/**
 * Fill and open the chooser: every catalogue enzyme with its site and site count in the
 * active document. `done` receives the ticked names when the dialog is confirmed.
 */
export function openChooser(dialog: HTMLDialogElement, catalogue: EnzymeCatalogueInfo, counts: EnzymeCount[] | null, chosen: string[], done: (names: string[]) => void) {
  const list = dialog.querySelector<HTMLElement>('#enzyme-options')!;
  const search = dialog.querySelector<HTMLInputElement>('#enzyme-search')!;
  const sites = new Map((counts ?? []).map(c => [c.name, c.sites]));
  dialog.querySelector('#enzyme-source')!.textContent = `${catalogue.source} ${catalogue.version} · ${catalogue.enzymes.length} enzymes`
    + (catalogue.unsupported ? ` (${catalogue.unsupported} not modelled)` : '');
  list.replaceChildren(...catalogue.enzymes.map(enzyme => {
    const label = document.createElement('label'); label.className = 'enzyme-option'; label.dataset.testid = 'enzyme-option'; label.dataset.enzyme = enzyme.name;
    const box = document.createElement('input'); box.type = 'checkbox'; box.value = enzyme.name; box.checked = chosen.includes(enzyme.name);
    const name = document.createElement('strong'); name.textContent = enzyme.name;
    const site = document.createElement('code'); site.textContent = enzyme.site;
    const count = document.createElement('span'); count.className = 'count';
    const n = sites.get(enzyme.name);
    count.textContent = n === undefined ? '' : n === 0 ? 'no sites' : `${n} site${n === 1 ? '' : 's'}`;
    label.append(box, name, site, count);
    return label;
  }));
  search.value = '';
  search.oninput = () => {
    const query = search.value.trim().toLowerCase();
    for (const option of list.children as HTMLCollectionOf<HTMLElement>) option.hidden = query !== '' && !option.dataset.enzyme!.toLowerCase().includes(query);
  };
  dialog.querySelector<HTMLButtonElement>('#enzyme-clear')!.onclick = () => { for (const box of list.querySelectorAll('input')) box.checked = false; };
  dialog.querySelector<HTMLButtonElement>('#enzyme-cancel')!.onclick = () => dialog.close();
  dialog.querySelector<HTMLFormElement>('form')!.onsubmit = event => {
    event.preventDefault();
    done([...list.querySelectorAll<HTMLInputElement>('input:checked')].map(box => box.value));
    dialog.close();
  };
  dialog.showModal();
}

function endText(end: FragmentEndInfo | null): string {
  if (!end || end.original_terminus) return 'end';
  const overhang = end.overhang === 'blunt' ? 'blunt' : `${end.overhang === 'five_prime' ? '5′' : '3′'} ${end.overhang_sequence}`;
  return `${end.enzymes.join('/')} (${overhang})`;
}

/** Fragment list, longest first as on a gel; clicking selects the fragment. */
export function renderDigest(target: HTMLElement, fragments: Fragment[], selectFragment: (fragment: Fragment) => void) {
  const sorted = [...fragments].sort((a, b) => b.length - a.length || a.start - b.start);
  target.replaceChildren(...sorted.map(fragment => {
    const button = document.createElement('button'); button.type = 'button';
    button.className = 'digest-fragment'; button.dataset.testid = 'digest-fragment';
    button.dataset.start = String(fragment.start); button.dataset.length = String(fragment.length);
    const size = document.createElement('strong'); size.textContent = `${fragment.length.toLocaleString()} bp`;
    const ends = document.createElement('span'); ends.textContent = `${endText(fragment.left)} → ${endText(fragment.right)}`;
    button.append(size, ends);
    button.title = `Top strand [${fragment.start}, ${fragment.start + fragment.length}) (zero-based; may wrap)`;
    button.onclick = () => selectFragment(fragment);
    return button;
  }));
}

// Pure format adapters from CLI JSON to the harness's state shapes. Test code only:
// the app never performs these conversions, and none of them is a biological model.
export interface Part { start: number; length: number }
type CliPart = { kind: 'linear'; start: number; end: number } | { kind: 'circular_arc'; start: number; length: number };
interface CliLocation { parts: CliPart[] }
interface CliFeature { id: string; location: CliLocation }

export class TransformError extends Error {}

function location(value: unknown): CliLocation {
  // A single region (e.g. a restriction site's recognition) is a one-part location.
  if (value && typeof value === 'object' && 'kind' in value) return { parts: [value as CliPart] };
  if (!value || typeof value !== 'object' || !Array.isArray((value as CliLocation).parts)) {
    throw new TransformError(`expected a CLI location with parts, got ${JSON.stringify(value)}`);
  }
  return value as CliLocation;
}

/** CLI location → ordered {start,length} parts. Source order is kept; nothing is merged. */
export function parts(value: unknown): Part[] {
  return location(value).parts.map(part => {
    if (part.kind === 'linear') return { start: part.start, length: part.end - part.start };
    if (part.kind === 'circular_arc') return { start: part.start, length: part.length };
    throw new TransformError(`unknown CLI part kind ${JSON.stringify(part)}`);
  });
}

/** Sorted, de-duplicated reference positions covered by a location (circular parts wrap). */
export function positions(value: unknown, moleculeLength: number): number[] {
  const covered = new Set<number>();
  for (const part of parts(value)) {
    for (let offset = 0; offset < part.length; offset++) covered.add((part.start + offset) % moleculeLength);
  }
  return [...covered].sort((a, b) => a - b);
}

/** Ids, in source order, of CLI features whose location covers `base`. */
export function idsCovering(value: unknown, base: number, moleculeLength: number): string[] {
  if (!Array.isArray(value)) throw new TransformError('ids_covering expects the CLI features result array');
  return (value as CliFeature[]).filter(feature => positions(feature.location, moleculeLength).includes(base)).map(f => f.id);
}

export function count(value: unknown): number {
  if (!Array.isArray(value)) throw new TransformError(`count expects an array, got ${JSON.stringify(value)}`);
  return value.length;
}

export function codes(value: unknown): string[] {
  if (!Array.isArray(value)) throw new TransformError('codes expects a warnings array');
  return value.map(item => (item as { code: string }).code);
}

interface CliCodon { positions: number[] }
interface CliOrf { strand: string; start: number; length: number }

/** Middle reference base of each codon; accepts one codon list or a list of them. */
export function codonMiddles(value: unknown): number[] | number[][] {
  if (!Array.isArray(value)) throw new TransformError('codon_middles expects codons');
  if (value.length && Array.isArray(value[0])) return (value as CliCodon[][]).map(list => list.map(c => c.positions[1]));
  return (value as CliCodon[]).map(c => c.positions[1]);
}

const orfOrder = (a: CliOrf, b: CliOrf) => a.start - b.start || Number(a.strand === 'reverse') - Number(b.strand === 'reverse') || a.length - b.length;

/** ORFs → [{strand, start, length}] in (start, forward-first, length) order. */
export function orfRegions(value: unknown): CliOrf[] {
  if (!Array.isArray(value)) throw new TransformError('orf_regions expects the CLI orfs array');
  return (value as CliOrf[]).map(({ strand, start, length }) => ({ strand, start, length })).sort(orfOrder);
}

/** One ORF → its single {start, length} part (for selection-band comparison). */
export function orfParts(value: unknown): Part[] {
  const orf = value as CliOrf;
  if (typeof orf?.start !== 'number') throw new TransformError('orf_parts expects one CLI ORF');
  return [{ start: orf.start, length: orf.length }];
}

/** One ORF → sorted covered positions (wrapping). */
export function orfPositions(value: unknown, moleculeLength: number): number[] {
  const [part] = orfParts(value);
  return Array.from({ length: part.length }, (_, i) => (part.start + i) % moleculeLength).sort((a, b) => a - b);
}

/** Array of arrays/strings → their lengths. */
export function lengths(value: unknown): number[] {
  if (!Array.isArray(value)) throw new TransformError('lengths expects an array');
  return value.map(item => (item as { length: number }).length);
}

interface SpanFeature { label: string; location: { parts: ({ kind: 'linear'; start: number; end: number } | { kind: 'circular_arc'; start: number; length: number })[] } }

/**
 * The documented shift-click rule, from CLI features: start at `from`'s first part start;
 * end at the furthest part end of `from` and `to`, measured forward (wrapping on circular
 * molecules). Linear molecules take the covering span.
 */
export function forwardSpan(value: unknown, from: string, to: string, moleculeLength: number, circular: boolean, as: 'range' | 'parts' = 'range') {
  if (!Array.isArray(value)) throw new TransformError('forward_span expects the CLI features array');
  const find = (label: string) => {
    const matches = (value as SpanFeature[]).filter(f => f.label === label);
    if (matches.length !== 1) throw new TransformError(`forward_span: ${matches.length} features labelled ${JSON.stringify(label)}`);
    return matches[0];
  };
  const spans = [...find(from).location.parts, ...find(to).location.parts].map(p => p.kind === 'linear' ? [p.start, p.end] : [p.start, (p.start + p.length) % moleculeLength]);
  const start = find(from).location.parts[0].start;
  let range: { start: number; end: number };
  if (circular) {
    const reach = Math.max(...spans.map(([, end]) => ((end - start - 1 + moleculeLength) % moleculeLength) + 1));
    range = { start, end: (start + reach) % moleculeLength };
  } else {
    range = { start: Math.min(...spans.map(([s]) => s)), end: Math.max(...spans.map(([, e]) => e)) };
  }
  if (as === 'range') return range;
  return [{ start: range.start, length: ((range.end - range.start + moleculeLength) % moleculeLength) || moleculeLength }];
}

type CliRegion = { kind: 'linear'; start: number; end: number } | { kind: 'circular_arc'; start: number; length: number };
interface CliSite { enzyme: string; recognition: CliRegion; top_cut: number | null; bottom_cut: number | null }
interface CliSites { enzymes: { name: string; recognition_sequence: string }[]; sites: CliSite[] }

function sitesResult(value: unknown): CliSites {
  const result = value as CliSites;
  if (!Array.isArray(result?.sites) || !Array.isArray(result?.enzymes)) throw new TransformError('expected the CLI `sites` result object (path "result")');
  return result;
}

const regionOf = (region: CliRegion) => region.kind === 'linear' ? { start: region.start, length: region.end - region.start } : { start: region.start, length: region.length };
const byName = (a: string, b: string) => a.localeCompare(b);

/**
 * The documented display sets (docs/restriction.md), from a `sites` result over every
 * catalogue enzyme: sites counted per enzyme; "6+" = recognition length without N.
 */
export function enzymeSet(value: unknown, set: 'unique6' | 'unique_dual6' | 'unique_any'): string[] {
  const result = sitesResult(value);
  const counts = new Map<string, number>();
  for (const site of result.sites) counts.set(site.enzyme, (counts.get(site.enzyme) ?? 0) + 1);
  const long = (name: string) => [...result.enzymes.find(e => e.name === name)!.recognition_sequence].filter(b => b !== 'N').length >= 6;
  return [...counts].filter(([name, n]) => set === 'unique_any' ? n === 1 : set === 'unique6' ? n === 1 && long(name) : (n === 1 || n === 2) && long(name))
    .map(([name]) => name).sort(byName);
}

/** Distinct top-strand cut positions, ascending. */
export function siteTicks(value: unknown): number[] {
  return [...new Set(sitesResult(value).sites.flatMap(site => site.top_cut ?? []))].sort((a, b) => a - b);
}

/** One label per top-strand cut position naming every enzyme cutting there (alphabetical). */
export function siteLabels(value: unknown): { cut: number; names: string[] }[] {
  const byCut = new Map<number, Set<string>>();
  for (const site of sitesResult(value).sites) if (site.top_cut !== null) byCut.set(site.top_cut, (byCut.get(site.top_cut) ?? new Set()).add(site.enzyme));
  return [...byCut].sort((a, b) => a[0] - b[0]).map(([cut, names]) => ({ cut, names: [...names].sort(byName) }));
}

/** Recognition regions {enzyme, start, length}, by start then name. */
export function siteRegions(value: unknown): { enzyme: string; start: number; length: number }[] {
  return sitesResult(value).sites.map(site => ({ enzyme: site.enzyme, ...regionOf(site.recognition) }))
    .sort((a, b) => a.start - b.start || byName(a.enzyme, b.enzyme));
}

/** Cut boundaries that have a base after them (a cut at a linear molecule's end has none). */
export function siteCuts(value: unknown, moleculeLength: number): { top: number[]; bottom: number[] } {
  const sites = sitesResult(value).sites;
  const cuts = (pick: (site: CliSite) => number | null) => [...new Set(sites.flatMap(site => pick(site) ?? []).filter(cut => cut < moleculeLength))].sort((a, b) => a - b);
  return { top: cuts(site => site.top_cut), bottom: cuts(site => site.bottom_cut) };
}

/** Half-open {start, end} of `length` bases; `end` wraps only past the end of a circle. */
function spanRange(start: number, length: number, moleculeLength: number, circular: boolean) {
  return { start, end: circular && start + length > moleculeLength ? start + length - moleculeLength : start + length };
}

/** One site → the selection range of its recognition sequence. */
export function recognitionRange(value: unknown, moleculeLength: number, circular: boolean) {
  const site = value as CliSite;
  if (!site?.recognition) throw new TransformError('recognition_range expects one CLI site');
  const { start, length } = regionOf(site.recognition);
  return spanRange(start, length, moleculeLength, circular);
}

interface CliFragment { top: { source_start: number; length: number } }

function fragmentsOf(value: unknown): CliFragment[] {
  if (!Array.isArray(value)) throw new TransformError('expected the CLI digest fragments array (path "result.fragments")');
  return value as CliFragment[];
}

/** Fragments → top-strand {start, length}, by start. */
export function fragmentParts(value: unknown): Part[] {
  return fragmentsOf(value).map(f => ({ start: f.top.source_start, length: f.top.length })).sort((a, b) => a.start - b.start || a.length - b.length);
}

/** The fragment at `rank` in list order (longest first, then by start) → its selection range. */
export function fragmentRange(value: unknown, rank: number, moleculeLength: number, circular: boolean) {
  const ordered = fragmentsOf(value).map(f => f.top).sort((a, b) => b.length - a.length || a.source_start - b.source_start);
  if (rank >= ordered.length) throw new TransformError(`fragment_range: rank ${rank} but only ${ordered.length} fragments`);
  return spanRange(ordered[rank].source_start, ordered[rank].length, moleculeLength, circular);
}

/** Enzymes with at least one site, alphabetical. */
export function siteEnzymes(value: unknown): string[] {
  return [...new Set(sitesResult(value).sites.map(site => site.enzyme))].sort(byName);
}

interface CliMatch { library_id: number; name: string; strand: string; length: number; annotated_as: string[]; location: { parts: CliRegion[] } }

function matchesOf(value: unknown): CliMatch[] {
  if (!Array.isArray(value)) throw new TransformError('expected the detect-features matches array (path "result.matches")');
  return value as CliMatch[];
}

const matchStart = (m: CliMatch) => m.location.parts[0].start;

/** The documented grouping rule: inside the span of a longer match (circular-aware). */
function containedIn(inner: CliMatch, all: CliMatch[], moleculeLength: number): boolean {
  return all.some(outer => outer !== inner && outer.length > inner.length
    && ((matchStart(inner) - matchStart(outer) + moleculeLength) % moleculeLength) + inner.length <= outer.length);
}

/** Matches → panel rows (engine order): id, name, span, strand, annotated, nested. */
export function detectionRows(value: unknown, moleculeLength: number) {
  const all = matchesOf(value);
  return all.map(m => ({ library_id: m.library_id, name: m.name, start: matchStart(m), length: m.length, strand: m.strand,
    annotated: m.annotated_as.length > 0, contained: containedIn(m, all, moleculeLength) }));
}

/** Ticked by default: not annotated and not nested in a longer match → [{library_id, start}]. */
export function defaultDetections(value: unknown, moleculeLength: number) {
  const all = matchesOf(value);
  return all.filter(m => m.annotated_as.length === 0 && !containedIn(m, all, moleculeLength)).map(m => ({ library_id: m.library_id, start: matchStart(m) }));
}

/** Spans of matches not yet annotated, by start then longer first (the map's proposal arcs). */
export function detectionSpansNew(value: unknown) {
  return matchesOf(value).filter(m => m.annotated_as.length === 0).map(m => ({ start: matchStart(m), length: m.length }))
    .sort((a, b) => a.start - b.start || b.length - a.length);
}

/** The first match named `label` → its selection range. */
export function detectionRange(value: unknown, label: string, moleculeLength: number, circular: boolean) {
  const match = matchesOf(value).find(m => m.name === label);
  if (!match) throw new TransformError(`detection_range: no match named ${JSON.stringify(label)}`);
  return spanRange(matchStart(match), match.length, moleculeLength, circular);
}

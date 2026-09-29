// Pure format adapters from CLI JSON to the harness's state shapes. Test code only:
// the app never performs these conversions, and none of them is a biological model.
export interface Part { start: number; length: number }
type CliPart = { kind: 'linear'; start: number; end: number } | { kind: 'circular_arc'; start: number; length: number };
interface CliLocation { parts: CliPart[] }
interface CliFeature { id: string; location: CliLocation }

export class TransformError extends Error {}

function location(value: unknown): CliLocation {
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

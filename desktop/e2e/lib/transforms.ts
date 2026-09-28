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

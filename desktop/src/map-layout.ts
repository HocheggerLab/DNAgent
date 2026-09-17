import type { Feature } from './bindings';

const palette = ['#267c91', '#8754a3', '#b65e27', '#328052', '#b64262', '#4566ad'];
export function featureColor(feature: Feature): string {
  if (feature.color && /^#[\da-f]{6}$/i.test(feature.color)) return feature.color;
  let hash = 0;
  for (const char of feature.id) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return palette[hash % palette.length];
}

export function contains(feature: Feature, base: number, length: number): boolean {
  return feature.parts.some(part => (base - part.start + length) % length < part.length);
}

/** Display traversal only: reverse arrows follow decreasing reference coordinates. */
export function displayEndpoints(start: number, length: number, reverse: boolean): [number, number] {
  return reverse ? [start + length, start] : [start, start + length];
}

/** Equally spaced leader labels, sorted by their anchor to reduce crossings. */
export function labelPositions(anchors: number[], height: number): number[] {
  const result = Array<number>(anchors.length);
  const ordered = anchors.map((y, index) => ({ y, index })).sort((a, b) => a.y - b.y);
  ordered.forEach((item, rank) => {
    result[item.index] = height / 2 + (rank - (ordered.length - 1) / 2) * 20;
  });
  return result;
}

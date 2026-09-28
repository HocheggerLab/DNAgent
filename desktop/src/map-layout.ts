import type { Feature } from './bindings';

// Saturated fallback colours for features without a valid imported colour; they read
// on both light and dark backgrounds and carry outlines on the map.
const palette = ['#4f8fe6', '#e2574c', '#f2b33d', '#3fae72', '#9b6ad6', '#e8839e', '#35b5c9', '#b3b04a'];
export function featureColor(feature: Feature): string {
  if (feature.color && /^#[\da-f]{6}$/i.test(feature.color)) return feature.color;
  let hash = 0;
  for (const char of feature.id) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return palette[hash % palette.length];
}

export function contains(feature: Feature, base: number, length: number): boolean {
  return feature.parts.some(part => (base - part.start + length) % length < part.length);
}

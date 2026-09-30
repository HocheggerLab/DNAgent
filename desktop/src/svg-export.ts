// Standalone SVG export of a rendered view: a copy of the drawing with the theme's
// computed styles inlined, a background, and no interactive attributes, so it opens
// the same in a browser, Illustrator or Inkscape.
const PROPERTIES = [
  'fill', 'fill-opacity', 'stroke', 'stroke-width', 'stroke-opacity', 'stroke-dasharray', 'opacity',
  'font-family', 'font-size', 'font-weight', 'font-style', 'text-anchor', 'dominant-baseline', 'visibility', 'display',
];
const DROP = ['tabindex', 'role', 'aria-pressed', 'aria-label', 'class', 'data-testid'];

export function serializeSvg(svg: SVGSVGElement, background: string): string {
  const copy = svg.cloneNode(true) as SVGSVGElement;
  const originals = [svg, ...svg.querySelectorAll('*')];
  const copies = [copy, ...copy.querySelectorAll('*')];
  originals.forEach((original, index) => {
    const target = copies[index];
    if (!(original instanceof SVGElement) || original.closest('defs') && original.tagName === 'clipPath') return;
    const computed = getComputedStyle(original);
    const style = PROPERTIES.map(p => [p, computed.getPropertyValue(p)] as const)
      .filter(([p, v]) => v && !(p === 'display' && v !== 'none') && !(p === 'visibility' && v === 'visible'))
      .map(([p, v]) => `${p}:${v}`).join(';');
    if (style) target.setAttribute('style', style);
    for (const name of DROP) target.removeAttribute(name);
    for (const name of [...target.getAttributeNames()]) {
      // Theme variables only resolve inside the app; the inlined computed style carries the value.
      if (name.startsWith('data-') || target.getAttribute(name)!.includes('var(--')) target.removeAttribute(name);
    }
  });
  const box = svg.viewBox.baseVal;
  copy.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  copy.setAttribute('width', String(box.width));
  copy.setAttribute('height', String(box.height));
  copy.removeAttribute('id');
  const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
  for (const [k, v] of Object.entries({ x: box.x, y: box.y, width: box.width, height: box.height, fill: background })) rect.setAttribute(k, String(v));
  copy.insertBefore(rect, copy.firstChild);
  return `<?xml version="1.0" encoding="UTF-8"?>\n${new XMLSerializer().serializeToString(copy)}\n`;
}

/** The panel background of the current theme, for the exported canvas. */
export const themeBackground = () => getComputedStyle(document.documentElement).getPropertyValue('--panel').trim() || '#ffffff';

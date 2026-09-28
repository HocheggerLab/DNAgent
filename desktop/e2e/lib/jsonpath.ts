// Deliberately small JSON path subset, so scenario expectations stay predictable:
//   key  .key  [n]  [*]  [?(@.key=='text')]  [?(@.key==3)]
// A path that matches nothing is an error, never an empty pass.

type Token =
  | { kind: 'key'; key: string }
  | { kind: 'index'; index: number }
  | { kind: 'all' }
  | { kind: 'filter'; key: string; value: string | number | boolean | null };

export class PathError extends Error {}

const KEY = /^[A-Za-z_][A-Za-z0-9_]*/;
const FILTER = /^\[\?\(@\.([A-Za-z_][A-Za-z0-9_]*)\s*==\s*('(?:[^'\\]|\\.)*'|-?\d+|true|false|null)\)\]/;

export function parsePath(path: string): Token[] {
  const tokens: Token[] = [];
  let rest = path.trim();
  if (!rest) throw new PathError('empty path');
  let first = true;
  while (rest) {
    let match: RegExpMatchArray | null;
    if (!first && rest.startsWith('.')) {
      rest = rest.slice(1);
      if (!(match = rest.match(KEY))) throw new PathError(`expected a key after '.' in ${JSON.stringify(path)}`);
      tokens.push({ kind: 'key', key: match[0] });
    } else if (first && (match = rest.match(KEY))) {
      tokens.push({ kind: 'key', key: match[0] });
    } else if ((match = rest.match(/^\[(\d+)\]/))) {
      tokens.push({ kind: 'index', index: Number(match[1]) });
    } else if ((match = rest.match(/^\[\*\]/))) {
      tokens.push({ kind: 'all' });
    } else if ((match = rest.match(FILTER))) {
      const literal = match[2];
      const value = literal.startsWith("'") ? literal.slice(1, -1).replace(/\\(.)/g, '$1') : JSON.parse(literal);
      tokens.push({ kind: 'filter', key: match[1], value });
    } else {
      throw new PathError(`unsupported syntax at ${JSON.stringify(rest)} in ${JSON.stringify(path)}`);
    }
    rest = rest.slice(match[0].length);
    first = false;
  }
  return tokens;
}

/** True when the path can yield several values (wildcard or filter). */
export function isMulti(path: string): boolean {
  return parsePath(path).some(token => token.kind === 'all' || token.kind === 'filter');
}

function describe(value: unknown): string {
  const text = JSON.stringify(value);
  return text === undefined ? String(value) : text.length > 120 ? `${text.slice(0, 117)}...` : text;
}

/** All values matched by `path`. Throws with the failing prefix if a step has no match. */
export function queryAll(root: unknown, path: string): unknown[] {
  let current: unknown[] = [root];
  let prefix = '';
  for (const token of parsePath(path)) {
    const next: unknown[] = [];
    for (const value of current) {
      if (token.kind === 'key') {
        if (value !== null && typeof value === 'object' && !Array.isArray(value) && token.key in value) {
          next.push((value as Record<string, unknown>)[token.key]);
        }
      } else if (!Array.isArray(value)) {
        continue;
      } else if (token.kind === 'index') {
        if (token.index < value.length) next.push(value[token.index]);
      } else if (token.kind === 'all') {
        next.push(...value);
      } else {
        next.push(...value.filter(item => item !== null && typeof item === 'object'
          && (item as Record<string, unknown>)[token.key] === token.value));
      }
    }
    const step = token.kind === 'key' ? `${prefix ? '.' : ''}${token.key}`
      : token.kind === 'index' ? `[${token.index}]` : token.kind === 'all' ? '[*]' : `[?(@.${token.key}==${describe(token.value)})]`;
    // Wildcards over an empty array legitimately yield nothing; other steps must match.
    if (!next.length && !(token.kind === 'all' && current.every(Array.isArray))) {
      throw new PathError(`path ${JSON.stringify(path)}: nothing matched at ${JSON.stringify(prefix + step)} (value there: ${current.map(describe).join(' | ')})`);
    }
    prefix += step;
    current = next;
  }
  return current;
}

/** `path` semantics for scenarios: an array for wildcard/filter paths, otherwise the single value. */
export function query(root: unknown, path: string): unknown {
  const values = queryAll(root, path);
  return isMulti(path) ? values : values[0];
}

/** Exactly one match, unwrapped. */
export function queryOne(root: unknown, path: string): unknown {
  const values = queryAll(root, path);
  if (values.length !== 1) throw new PathError(`path ${JSON.stringify(path)} must match exactly one value, matched ${values.length}`);
  return values[0];
}

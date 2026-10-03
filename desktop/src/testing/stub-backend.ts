// e2e-only backend: forwards every desktop command to the live Rust test server
// (`cargo run -p dnagent-agent-mcp --example e2e_server`) through the Vite proxy.
// One server session per page load; pickers answer from queued test paths.
import type { Diagnostic } from '../bindings';

/** One backend session per page load; the e2e server serves agents on a socket named after it. */
export const session = crypto.randomUUID();
const delays = new Map<string, number>();
const picks: (string | null)[] = [];
const saves: (string | null)[] = [];

/** Delay the next open of `path` once, to exercise stale-request handling. */
export function setDelay(path: string, ms: number) { delays.set(path, ms); }
/** Queue the path returned by the next Browse… picker call (null = cancelled). */
export function queuePick(path: string | null) { picks.push(path); }
/** Queue the path returned by the next Save-as dialog (null = cancelled). */
export function queueSave(path: string | null) { saves.push(path); }

const snake = (args: Record<string, unknown>) =>
  Object.fromEntries(Object.entries(args).map(([key, value]) => [key.replace(/[A-Z]/g, c => `_${c.toLowerCase()}`), value]));

export async function invoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const path = typeof args.path === 'string' ? args.path : '';
  if (command === 'open_document' && delays.has(path)) {
    const delay = delays.get(path)!;
    delays.delete(path);
    await new Promise(resolve => setTimeout(resolve, delay));
  }
  const response = await fetch('/__dnagent/invoke', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ session, command, args: snake(args) }),
  });
  const body = await response.json() as { ok: boolean; value?: T; error?: Diagnostic };
  if (!body.ok) throw body.error ?? { code: 'e2e_server', message: `HTTP ${response.status}` };
  return body.value as T;
}

export async function pickConstructPath(): Promise<string | null> {
  if (!picks.length) throw new Error('e2e: Browse… clicked without a queued picker result');
  return picks.shift()!;
}

export async function pickSavePath(): Promise<string | null> {
  if (!saves.length) throw new Error('e2e: Save as… clicked without a queued save path');
  return saves.shift()!;
}

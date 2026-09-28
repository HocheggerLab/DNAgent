// e2e-only backend: replays `open_document` results serialised by Rust
// (`cargo run -p dnagent-desktop-api --example export_recordings`). Never hand-edit
// the recordings; `cargo test -p dnagent-desktop-api` fails when they are stale.
import type { Diagnostic, Document } from '../bindings';
import recordingsJson from '../../e2e/fixtures/recordings.json';

type Recording = { ok: Document } | { err: Diagnostic };
const recordings = recordingsJson as unknown as Record<string, Recording>;
const delays = new Map<string, number>();
const picks: (string | null)[] = [];

/** Delay the next open of `path` once, to exercise stale-request handling. */
export function setDelay(path: string, ms: number) { delays.set(path, ms); }

/** Queue the path returned by the next Browse… picker call (null = cancelled). */
export function queuePick(path: string | null) { picks.push(path); }

export async function openDocument(path: string): Promise<Document> {
  const delay = delays.get(path) ?? 0;
  delays.delete(path);
  if (delay) await new Promise(resolve => setTimeout(resolve, delay));
  const recording = recordings[path];
  if (!recording) {
    const known = Object.keys(recordings).join(', ');
    throw { code: 'e2e_recording_missing', message: `no recording for ${path}; known: ${known}` } satisfies Diagnostic;
  }
  if ('err' in recording) throw structuredClone(recording.err);
  return structuredClone(recording.ok);
}

export async function pickConstructPath(): Promise<string | null> {
  if (!picks.length) throw new Error('e2e: Browse… clicked without a queued picker result');
  return picks.shift()!;
}

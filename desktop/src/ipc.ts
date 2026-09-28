// The only module that talks to the native shell. In the e2e Vite mode it answers
// from Rust-generated recordings instead; production builds drop that branch.
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { Document } from './bindings';

const stub = import.meta.env.MODE === 'e2e' ? import('./testing/stub-backend') : null;
let pending = 0;

/** Native requests not yet settled; used by the test harness to wait deterministically. */
export const pendingRequests = () => pending;

async function tracked<T>(request: () => Promise<T>): Promise<T> {
  pending++;
  try { return await request(); } finally { pending--; }
}

export function openDocument(path: string): Promise<Document> {
  return tracked(async () => stub
    ? (await stub).openDocument(path)
    : invoke<Document>('open_document', { path }));
}

export function pickConstructPath(): Promise<string | null> {
  return tracked(async () => {
    if (stub) return (await stub).pickConstructPath();
    const path = await open({multiple:false,directory:false,filters:[{name:'DNA constructs',extensions:['dna','fa','fasta','fna']}]});
    return typeof path === 'string' ? path : null;
  });
}

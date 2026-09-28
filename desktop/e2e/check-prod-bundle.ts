// Proves the production bundle contains no automation API or recorded responses,
// and that the same markers are detectable in an e2e-mode build (so the check is live).
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const desktop = resolve(import.meta.dirname, '..');
const markers = ['__DNAGENT_TEST__', 'e2e_recording_missing', 'fixtures/formats/snapgene/synthetic_multipart_origin.dna'];

function files(dir: string): string[] {
  return readdirSync(dir).flatMap(name => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? files(path) : [path];
  });
}

function found(dir: string): string[] {
  const text = files(dir).map(file => readFileSync(file, 'utf8')).join('\n');
  return markers.filter(marker => text.includes(marker));
}

const vite = (args: string[]) => execFileSync('npx', ['vite', 'build', '--logLevel', 'error', ...args], { cwd: desktop, stdio: 'inherit' });
vite([]);
const leaked = found(join(desktop, 'dist'));
const probe = mkdtempSync(join(tmpdir(), 'dnagent-e2e-build-'));
try {
  vite(['--mode', 'e2e', '--outDir', probe, '--emptyOutDir']);
  const present = found(probe);
  if (present.length !== markers.length) throw new Error(`marker check is not live: e2e build only contains ${JSON.stringify(present)}`);
} finally { rmSync(probe, { recursive: true, force: true }); }
if (leaked.length) {
  console.error(`production bundle contains test-only code: ${JSON.stringify(leaked)}`);
  process.exit(1);
}
console.log(`production bundle is free of test-only code (${markers.length} markers checked)`);

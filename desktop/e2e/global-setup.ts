import { execFileSync } from 'node:child_process';
import { mkdirSync, rmSync } from 'node:fs';
import { dirname } from 'node:path';
import { assertBinary, BINARY, FEATURE_LIBRARY, LARGE_LOCUS, REPO_ROOT } from './lib/cli.ts';

export default function globalSetup() {
  assertBinary();
  console.log(`e2e ground truth: ${BINARY}`);
  // A feature library from the public fixtures only; the e2e server and the CLI oracle both use it.
  rmSync(FEATURE_LIBRARY, { force: true });
  mkdirSync(dirname(FEATURE_LIBRARY), { recursive: true });
  execFileSync(BINARY, ['library', '--db', FEATURE_LIBRARY, 'import', 'fixtures/formats'], { cwd: REPO_ROOT, stdio: 'pipe' });
  // A ~1.2 Mb gene locus (too large to commit), generated deterministically from the synthetic one.
  execFileSync('python3', ['fixtures/formats/locus/generate_synthetic_locus.py', '--large', LARGE_LOCUS], { cwd: REPO_ROOT, stdio: 'pipe' });
}

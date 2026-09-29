import { execFileSync } from 'node:child_process';
import { mkdirSync, rmSync } from 'node:fs';
import { dirname } from 'node:path';
import { assertBinary, BINARY, FEATURE_LIBRARY, REPO_ROOT } from './lib/cli.ts';

export default function globalSetup() {
  assertBinary();
  console.log(`e2e ground truth: ${BINARY}`);
  // A feature library from the public fixtures only; the e2e server and the CLI oracle both use it.
  rmSync(FEATURE_LIBRARY, { force: true });
  mkdirSync(dirname(FEATURE_LIBRARY), { recursive: true });
  execFileSync(BINARY, ['library', '--db', FEATURE_LIBRARY, 'import', 'fixtures/formats'], { cwd: REPO_ROOT, stdio: 'pipe' });
}

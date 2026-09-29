// Ground truth: the validated dnagent CLI, run at test time and cached per process.
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

export const REPO_ROOT = resolve(import.meta.dirname, '../../..');
export const BINARY = process.env.DNAGENT_BINARY ?? resolve(REPO_ROOT, 'target/debug/dnagent');
export const CLI_COMMANDS = ['inspect', 'features', 'primers', 'translate', 'orfs'] as const;
export type CliCommand = (typeof CLI_COMMANDS)[number];

const cache = new Map<string, unknown>();

export function assertBinary(): void {
  if (!existsSync(BINARY)) {
    throw new Error(`dnagent binary not found at ${BINARY}. Run \`cargo build -p dnagent-cli --locked\` from the repository root, or set DNAGENT_BINARY.`);
  }
}

/** The JSON envelope of `dnagent <command> <fixture> --output json` (failure envelopes included). */
export function cli(command: CliCommand, fixture: string, args: string[] = []): Record<string, unknown> {
  const key = [command, fixture, ...args].join(' ');
  // Only committed fixtures are immutable; saved files and workspace files change during a scenario.
  if (!fixture.startsWith('fixtures/')) cache.delete(key);
  if (!cache.has(key)) {
    assertBinary();
    let stdout: string;
    try {
      stdout = execFileSync(BINARY, [command, fixture, ...args, '--output', 'json'], { cwd: REPO_ROOT, encoding: 'utf8' });
    } catch (error) {
      // Runtime failures still print a JSON envelope on stdout with a nonzero exit.
      stdout = String((error as { stdout?: string }).stdout ?? '');
      if (!stdout.trim()) throw new Error(`dnagent ${key} failed without JSON output: ${String(error)}`);
    }
    cache.set(key, JSON.parse(stdout));
  }
  return cache.get(key) as Record<string, unknown>;
}

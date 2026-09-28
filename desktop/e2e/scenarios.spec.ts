// One generic spec: every e2e/scenarios/*.json becomes a test. Malformed files fail
// as their own test with the validator's message; they are never skipped.
import { test } from '@playwright/test';
import { basename } from 'node:path';
import { runScenario } from './lib/runner.ts';
import { loadScenario, scenarioFiles, type Scenario } from './lib/scenario.ts';

const files = scenarioFiles();
test('scenario directory is not empty', () => {
  if (!files.length) throw new Error('no scenarios found in desktop/e2e/scenarios');
});

for (const file of files) {
  let scenario: Scenario | undefined;
  let invalid: Error | undefined;
  try { scenario = loadScenario(file); } catch (error) { invalid = error as Error; }
  test(scenario ? `${scenario.id}: ${scenario.description}` : basename(file), async ({ page }) => {
    if (invalid) throw invalid;
    await runScenario(page, scenario!);
  });
}

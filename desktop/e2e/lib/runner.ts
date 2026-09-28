// Executes one validated scenario against the e2e-mode frontend.
import type { Page } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { cli } from './cli.ts';
import { query, queryOne } from './jsonpath.ts';
import type { Assertion, CliExpectation, Scenario, Step } from './scenario.ts';
import { ACTIONS, E2E_DIR } from './scenario.ts';
import { codes, count, idsCovering, parts, positions } from './transforms.ts';

const IDLE_TIMEOUT_MS = 5_000;
const show = (value: unknown) => JSON.stringify(value, null, 2)?.replace(/\n\s*/g, ' ') ?? String(value);

/** Resolve a CLI-derived expectation. `fixture` defaults to the most recently opened one. */
export function resolveCli(expectation: CliExpectation, defaultFixture: string): unknown {
  const fixture = expectation.fixture ?? defaultFixture;
  const envelope = cli(expectation.command, fixture);
  let value = expectation.one !== undefined ? queryOne(envelope, expectation.one) : query(envelope, expectation.path!);
  const transform = expectation.transform;
  const moleculeLength = () => queryOne(cli('inspect', fixture), 'result.length') as number;
  if (transform === 'parts') value = parts(value);
  else if (transform === 'positions') value = positions(value, moleculeLength());
  else if (transform === 'count') value = count(value);
  else if (transform === 'codes') value = codes(value);
  else if (transform?.name === 'ids_covering') value = idsCovering(value, transform.base, moleculeLength());
  if (expectation.index !== undefined) {
    if (!Array.isArray(value) || expectation.index >= value.length) {
      throw new Error(`index ${expectation.index} out of range for ${show(value)}`);
    }
    value = value[expectation.index];
  }
  return value;
}

function describeExpected(assertion: Assertion, fixture: string): string {
  if (assertion.equals_cli) {
    const e = assertion.equals_cli;
    return `dnagent ${e.command} ${e.fixture ?? fixture} → ${e.one ? `one(${e.one})` : e.path}${e.transform ? ` | ${typeof e.transform === 'string' ? e.transform : `${e.transform.name}(${e.transform.base})`}` : ''}${e.index !== undefined ? ` [${e.index}]` : ''}`;
  }
  return assertion.equals_state !== undefined ? `state ${assertion.equals_state}` : 'literal';
}

export async function runScenario(page: Page, scenario: Scenario): Promise<void> {
  const artifacts = resolve(E2E_DIR, 'artifacts', scenario.id);
  mkdirSync(artifacts, { recursive: true });
  const shot = (name: string) => page.screenshot({ path: resolve(artifacts, `${name}.png`), fullPage: true });
  const pageErrors: string[] = [];
  page.on('pageerror', error => pageErrors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') pageErrors.push(message.text()); });

  const waitIdle = () => page.waitForFunction(() => window.__DNAGENT_TEST__!.getState().idle, null, { timeout: IDLE_TIMEOUT_MS })
    .catch(() => { throw new Error(`app did not become idle within ${IDLE_TIMEOUT_MS} ms`); });
  const state = () => page.evaluate(() => window.__DNAGENT_TEST__!.getState());
  // Two animation frames: the map re-lays out on the frame after a resize.
  const settle = () => page.evaluate(() => new Promise<void>(done => requestAnimationFrame(() => requestAnimationFrame(() => done()))));
  const ready = () => page.waitForFunction(() => window.__DNAGENT_TEST__ !== undefined, null, { timeout: IDLE_TIMEOUT_MS })
    .catch(() => { throw new Error('window.__DNAGENT_TEST__ is missing: is Vite running with --mode e2e?'); });
  await page.goto('/');
  await ready();
  let fixture = scenario.fixture;

  const checkOpened = async (expectError: boolean) => {
    const status = (await state()).status;
    const failed = status.startsWith('Open failed');
    if (failed !== expectError) throw new Error(expectError ? `expected the open to fail, status: ${status}` : `open failed: ${status}`);
  };

  const run = async (step: Step): Promise<void> => {
    if ('open' in step) {
      fixture = step.open.fixture ?? scenario.fixture;
      await page.evaluate(([path, delayMs]) => window.__DNAGENT_TEST__!.open(path, { delayMs }), [fixture, step.open.delay_ms] as const);
      if (step.open.wait !== false) { await waitIdle(); await checkOpened(step.open.expect_error ?? false); }
    } else if ('browse' in step) {
      fixture = step.browse.fixture ?? scenario.fixture;
      await page.evaluate(path => window.__DNAGENT_TEST__!.browse(path), fixture);
      await waitIdle(); await checkOpened(false);
    } else if ('select_feature' in step) {
      const target = step.select_feature;
      const id = 'id' in target ? target.id
        : queryOne(cli('features', fixture), `result[?(@.label=='${target.label.replace(/['\\]/g, '\\$&')}')].id`) as string;
      // Wait for the list item to be rendered (not for idle), so a selection can be made
      // while an older, stale request is still in flight.
      await page.waitForFunction(featureId => [...document.querySelectorAll<HTMLElement>('[data-testid="feature-item"]')]
        .some(item => item.dataset.featureId === featureId), id, { timeout: IDLE_TIMEOUT_MS })
        .catch(() => { throw new Error(`feature ${id} never appeared in the feature list`); });
      // Selection, tab and base clicks issue no requests and render synchronously,
      // so they do not wait for idle (which would also mask stale-request races).
      await page.evaluate(featureId => window.__DNAGENT_TEST__!.selectFeature(featureId), id);
    } else if ('select_tab' in step) {
      await page.evaluate(tab => window.__DNAGENT_TEST__!.selectTab(tab), step.select_tab);
    } else if ('click_sequence_base' in step) {
      await page.evaluate(base => window.__DNAGENT_TEST__!.clickSequenceBase(base), step.click_sequence_base);
    } else if ('click' in step) {
      // A trusted Playwright pointer click, not a synthetic element.click().
      await page.getByTestId(step.click.testid).nth(step.click.index ?? 0).click({ timeout: IDLE_TIMEOUT_MS });
      await waitIdle();
      await settle();
    } else if ('wait_idle' in step) {
      await waitIdle();
    } else if ('set_viewport' in step) {
      await page.setViewportSize(step.set_viewport);
      await settle();
    } else if ('set_color_scheme' in step) {
      await page.emulateMedia({ colorScheme: step.set_color_scheme });
      await settle();
    } else if ('select_option' in step) {
      await page.getByTestId(step.select_option.testid).selectOption(step.select_option.value, { timeout: IDLE_TIMEOUT_MS });
      await settle();
    } else if ('reload' in step) {
      await page.reload();
      await ready();
    } else if ('expect' in step) {
      const snapshot = await state();
      const failures: string[] = [];
      for (const [item, assertion] of step.expect.entries()) {
        const label = `expect[${item}]${assertion.message ? ` (${assertion.message})` : ''} ${assertion.state}`;
        try {
          const actual = query(snapshot, assertion.state);
          const expected = assertion.equals_cli ? resolveCli(assertion.equals_cli, fixture)
            : assertion.equals_state !== undefined ? query(snapshot, assertion.equals_state) : assertion.equals;
          if (!isDeepStrictEqual(actual, expected)) {
            failures.push(`${label}\n      GUI state: ${show(actual)}\n      expected:  ${show(expected)}\n      from:      ${describeExpected(assertion, fixture)}`);
          }
        } catch (error) {
          failures.push(`${label}: ${(error as Error).message}`);
        }
      }
      if (failures.length) throw new Error(failures.join('\n    '));
    } else {
      throw new Error(`unhandled step ${JSON.stringify(step)}`);
    }
  };

  for (const [index, step] of scenario.steps.entries()) {
    const action = ACTIONS.find(name => name in step)!;
    try {
      await run(step);
      if (step.screenshot) await shot(`${String(index).padStart(2, '0')}-${action}`);
    } catch (error) {
      await shot(`failure-step-${String(index).padStart(2, '0')}`).catch(() => undefined);
      throw new Error(`scenario ${scenario.id}, step ${index} (${action}${step.note ? `: ${step.note}` : ''}):\n    ${(error as Error).message}`);
    }
  }
  await shot('final');
  if (pageErrors.length) throw new Error(`scenario ${scenario.id}: page errors:\n    ${pageErrors.join('\n    ')}`);
}

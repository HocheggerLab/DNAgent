// Executes one validated scenario against the e2e-mode frontend.
import type { Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { BINARY, cli, REPO_ROOT } from './cli.ts';
import { query, queryOne } from './jsonpath.ts';
import type { Assertion, CliExpectation, Scenario, Step } from './scenario.ts';
import { ACTIONS, E2E_DIR } from './scenario.ts';
import {
  codes, codonMiddles, count, enzymeSet, forwardSpan, fragmentParts, fragmentRange, idsCovering, lengths, orfParts, orfPositions, orfRegions, parts,
  positions, recognitionRange, siteCuts, siteEnzymes, siteLabels, siteRegions, siteTicks,
} from './transforms.ts';

const IDLE_TIMEOUT_MS = 5_000;
const show = (value: unknown) => JSON.stringify(value, null, 2)?.replace(/\n\s*/g, ' ') ?? String(value);

/** Resolve a CLI-derived expectation. `fixture` defaults to the most recently opened one. */
export function resolveCli(expectation: CliExpectation, defaultFixture: string, savedFile: string | null = null, memory = new Map<string, unknown>()): unknown {
  if (expectation.saved && !savedFile) throw new Error('equals_cli.saved used before any save_as step');
  const fixture = expectation.file ?? (expectation.saved ? savedFile! : expectation.fixture ?? defaultFixture);
  const args = (expectation.args ?? []).map(arg => {
    if (typeof arg === 'string') return arg;
    if (!memory.has(arg.memory)) throw new Error(`nothing remembered as ${arg.memory}`);
    const value = memory.get(arg.memory);
    return Array.isArray(value) ? value.join(',') : String(value);
  });
  const envelope = cli(expectation.command, fixture, args);
  let value = expectation.one !== undefined ? queryOne(envelope, expectation.one) : query(envelope, expectation.path!);
  const transform = expectation.transform;
  const moleculeLength = () => queryOne(cli('inspect', fixture), 'result.length') as number;
  const circular = () => queryOne(cli('inspect', fixture), 'result.topology') === 'circular';
  if (transform === 'parts') value = parts(value);
  else if (transform === 'positions') value = positions(value, moleculeLength());
  else if (transform === 'count') value = count(value);
  else if (transform === 'codes') value = codes(value);
  else if (transform === 'codon_middles') value = codonMiddles(value);
  else if (transform === 'orf_regions') value = orfRegions(value);
  else if (transform === 'orf_parts') value = orfParts(value);
  else if (transform === 'orf_positions') value = orfPositions(value, moleculeLength());
  else if (transform === 'lengths') value = lengths(value);
  else if (transform === 'site_ticks') value = siteTicks(value);
  else if (transform === 'site_labels') value = siteLabels(value);
  else if (transform === 'site_enzymes') value = siteEnzymes(value);
  else if (transform === 'site_regions') value = siteRegions(value);
  else if (transform === 'site_cuts') value = siteCuts(value, moleculeLength());
  else if (transform === 'recognition_range') value = recognitionRange(value, moleculeLength(), circular());
  else if (transform === 'fragment_parts') value = fragmentParts(value);
  else if (transform?.name === 'enzyme_set') value = enzymeSet(value, transform.set);
  else if (transform?.name === 'fragment_range') value = fragmentRange(value, transform.rank, moleculeLength(), circular());
  else if (transform?.name === 'ids_covering') value = idsCovering(value, transform.base, moleculeLength());
  else if (transform?.name === 'forward_span') {
    value = forwardSpan(value, transform.from, transform.to, moleculeLength(), circular(), transform.as);
  }
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
    const args = (e.args ?? []).map(arg => typeof arg === 'string' ? arg : `<remembered ${arg.memory}>`);
    const input = e.command === 'enzymes' ? '' : ` ${e.saved ? '<saved file>' : e.fixture ?? fixture}`;
    const transform = typeof e.transform === 'string' ? e.transform
      : e.transform && `${e.transform.name}(${Object.entries(e.transform).filter(([key]) => key !== 'name').map(([, value]) => String(value)).join(', ')})`;
    return `dnagent ${e.command}${input}${args.length ? ` ${args.join(' ')}` : ''} → ${e.one ? `one(${e.one})` : e.path}${transform ? ` | ${transform}` : ''}${e.index !== undefined ? ` [${e.index}]` : ''}`;
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
  page.on('dialog', dialog => void dialog.accept()); // e.g. "discard unsaved changes?"
  let savedFile: string | null = null;
  const memory = new Map<string, unknown>();

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

  // CLI defaults follow the active tab's file when it is repo-relative (tabs can switch).
  const activeFile = async () => {
    const active = (await state()).tabs.find(tab => tab.active)?.path;
    return active && !active.startsWith('/') ? active : fixture;
  };
  const run = async (step: Step): Promise<void> => {
    if ('open' in step) {
      if (step.open.saved && !savedFile) throw new Error('open.saved used before any save_as step');
      fixture = step.open.file ?? (step.open.saved ? savedFile! : step.open.fixture ?? scenario.fixture);
      await page.evaluate(([path, delayMs]) => window.__DNAGENT_TEST__!.open(path, { delayMs }), [fixture, step.open.delay_ms] as const);
      if (step.open.wait !== false) { await waitIdle(); await checkOpened(step.open.expect_error ?? false); }
    } else if ('browse' in step) {
      fixture = step.browse.fixture ?? scenario.fixture;
      await page.evaluate(path => window.__DNAGENT_TEST__!.browse(path), fixture);
      await waitIdle(); await checkOpened(false);
    } else if ('select_feature' in step) {
      const target = step.select_feature;
      const id = 'id' in target ? target.id
        : queryOne(cli('features', await activeFile()), `result[?(@.label=='${target.label.replace(/['\\]/g, '\\$&')}')].id`) as string;
      // Wait for the list item to be rendered (not for idle), so a selection can be made
      // while an older, stale request is still in flight.
      await page.waitForFunction(featureId => [...document.querySelectorAll<HTMLElement>('[data-testid="feature-item"]')]
        .some(item => item.dataset.featureId === featureId), id, { timeout: IDLE_TIMEOUT_MS })
        .catch(() => { throw new Error(`feature ${id} never appeared in the feature list`); });
      // Selection, tab and base clicks issue no requests and render synchronously,
      // so they do not wait for idle (which would also mask stale-request races).
      await page.evaluate(([featureId, extend]) => window.__DNAGENT_TEST__!.selectFeature(featureId, extend), [id, target.extend ?? false] as const);
    } else if ('select_tab' in step) {
      await page.evaluate(tab => window.__DNAGENT_TEST__!.selectTab(tab), step.select_tab);
    } else if ('click_sequence_base' in step) {
      await page.evaluate(base => window.__DNAGENT_TEST__!.clickSequenceBase(base), step.click_sequence_base);
    } else if ('click' in step) {
      // A trusted Playwright pointer click, not a synthetic element.click().
      await page.getByTestId(step.click.testid).nth(step.click.index ?? 0).click({ timeout: IDLE_TIMEOUT_MS, modifiers: step.click.modifiers });
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
    } else if ('select_orf' in step) {
      await page.evaluate(id => window.__DNAGENT_TEST__!.selectOrf(id), step.select_orf.id);
      await settle();
    } else if ('drag_bases' in step) {
      // A real pointer drag across forward-strand bases.
      const base = (position: number) => page.locator(`#sequence [data-strand="forward"] [data-position="${position}"]`);
      await base(step.drag_bases.from).scrollIntoViewIfNeeded();
      await base(step.drag_bases.from).hover();
      await page.mouse.down();
      await base(step.drag_bases.to).hover();
      await page.mouse.up();
      await settle();
    } else if ('remember' in step) {
      const snapshot = await state();
      memory.set(step.remember.as, step.remember.single ? queryOne(snapshot, step.remember.state) : query(snapshot, step.remember.state));
    } else if ('set_workspace' in step) {
      // Always start from an empty folder (the schema restricts this to desktop/e2e/artifacts).
      rmSync(resolve(REPO_ROOT, step.set_workspace), { recursive: true, force: true });
      mkdirSync(resolve(REPO_ROOT, step.set_workspace), { recursive: true });
      await page.evaluate(path => window.__DNAGENT_TEST__!.chooseWorkspace(path), step.set_workspace);
      await waitIdle();
    } else if ('poll_workspace' in step) {
      await page.evaluate(() => window.__DNAGENT_TEST__!.pollWorkspace());
      await waitIdle();
    } else if ('run_cli' in step) {
      try {
        execFileSync(BINARY, step.run_cli.args, { cwd: REPO_ROOT, encoding: 'utf8', stdio: 'pipe', env: { ...process.env, DNAGENT_ENZYMES: 'builtin' } });
      } catch (error) {
        throw new Error(`agent command failed: dnagent ${step.run_cli.args.join(' ')}\n${String((error as { stdout?: string }).stdout ?? error)}`);
      }
    } else if ('click_site' in step) {
      await page.evaluate(enzyme => window.__DNAGENT_TEST__!.clickSite(enzyme), step.click_site.enzyme);
      await waitIdle();
    } else if ('choose_enzymes' in step) {
      await page.evaluate(names => window.__DNAGENT_TEST__!.chooseEnzymes(names), step.choose_enzymes.names);
      await waitIdle();
    } else if ('fill' in step) {
      await page.getByTestId(step.fill.testid).fill(step.fill.value, { timeout: IDLE_TIMEOUT_MS });
      await settle();
      await waitIdle();
    } else if ('press' in step) {
      await page.keyboard.press(step.press);
      await settle();
      await waitIdle();
    } else if ('save_as' in step) {
      await page.evaluate(path => window.__DNAGENT_TEST__!.saveAs(path), step.save_as.path);
      await waitIdle();
      savedFile = step.save_as.path;
    } else if ('expect' in step) {
      const snapshot = await state();
      const defaultFile = await activeFile();
      const failures: string[] = [];
      for (const [item, assertion] of step.expect.entries()) {
        const label = `expect[${item}]${assertion.message ? ` (${assertion.message})` : ''} ${assertion.state}`;
        try {
          const actual = assertion.single ? queryOne(snapshot, assertion.state) : query(snapshot, assertion.state);
          if (assertion.equals_memory !== undefined && !memory.has(assertion.equals_memory)) throw new Error(`nothing remembered as ${assertion.equals_memory}`);
          const fileJson = (spec: { file: string; path?: string; one?: string }) => {
            const data = JSON.parse(readFileSync(resolve(REPO_ROOT, spec.file), 'utf8'));
            return spec.one !== undefined ? queryOne(data, spec.one) : query(data, spec.path!);
          };
          const expected = assertion.equals_json_file ? fileJson(assertion.equals_json_file)
            : assertion.equals_memory !== undefined ? memory.get(assertion.equals_memory)
            : assertion.equals_cli ? resolveCli(assertion.equals_cli, defaultFile, savedFile, memory)
            : assertion.equals_state !== undefined ? query(snapshot, assertion.equals_state) : assertion.equals;
          if (!isDeepStrictEqual(actual, expected)) {
            failures.push(`${label}\n      GUI state: ${show(actual)}\n      expected:  ${show(expected)}\n      from:      ${describeExpected(assertion, defaultFile)}`);
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

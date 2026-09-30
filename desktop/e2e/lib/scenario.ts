// Load and validate scenario files. Structural errors come from the JSON Schema;
// semantic rules keep biological expectations tied to the CLI.
import { Ajv, type ErrorObject } from 'ajv';
import { readdirSync, readFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import type { CliCommand } from './cli.ts';
import { parsePath, PathError } from './jsonpath.ts';

export const E2E_DIR = resolve(import.meta.dirname, '..');
export const SCENARIO_DIR = resolve(E2E_DIR, 'scenarios');
const schema = JSON.parse(readFileSync(resolve(E2E_DIR, 'scenario.schema.json'), 'utf8'));
const validate = new Ajv({ allErrors: true, strict: false }).compile(schema);

export const ACTIONS = ['open', 'browse', 'select_feature', 'select_tab', 'click_sequence_base', 'click', 'wait_idle', 'expect',
  'set_viewport', 'set_color_scheme', 'select_option', 'reload', 'select_orf', 'drag_bases', 'fill', 'press', 'save_as', 'remember',
  'set_workspace', 'poll_workspace', 'run_cli', 'click_site', 'choose_enzymes', 'toggle_detection', 'select_detection',
  'select_isoform', 'drag_exons', 'double_click_exon', 'export_svg'] as const;
export type Transform = 'parts' | 'positions' | 'count' | 'codes' | 'codon_middles' | 'orf_regions' | 'orf_parts' | 'orf_positions' | 'lengths'
  | 'site_ticks' | 'site_enzymes' | 'site_labels' | 'site_regions' | 'site_cuts' | 'recognition_range' | 'fragment_parts'
  | 'detection_rows' | 'default_detections' | 'detection_spans_new' | 'spans' | 'isoform_displays' | 'codon_marks'
  | { name: 'detection_range'; label: string } | { name: 'cell_means'; transcript: string }
  | { name: 'ids_covering'; base: number } | { name: 'forward_span'; from: string; to: string; as?: 'range' | 'parts' }
  | { name: 'enzyme_set'; set: 'unique6' | 'unique_dual6' | 'unique_any' } | { name: 'fragment_range'; rank: number };
export type CliArg = string | { memory: string };
export interface CliExpectation { command: CliCommand; fixture?: string; saved?: boolean; file?: string; args?: CliArg[]; path?: string; one?: string; transform?: Transform; index?: number }
export interface Assertion { state: string; single?: boolean; equals?: unknown; equals_cli?: CliExpectation; equals_state?: string; equals_memory?: string;
  equals_json_file?: { file: string; path?: string; one?: string }; message?: string }
export type Step = { screenshot?: boolean; note?: string } & (
  | { open: { fixture?: string; saved?: boolean; file?: string; delay_ms?: number; wait?: boolean; expect_error?: boolean } }
  | { browse: { fixture?: string } }
  | { select_feature: ({ id: string } | { label: string }) & { extend?: boolean } }
  | { select_tab: 'map' | 'sequence' | 'isoforms' }
  | { click_sequence_base: number }
  | { click: { testid: string; index?: number; modifiers?: ('Shift' | 'Alt' | 'Control' | 'Meta')[] } }
  | { wait_idle: true }
  | { expect: Assertion[] }
  | { set_viewport: { width: number; height: number } }
  | { set_color_scheme: 'light' | 'dark' }
  | { select_option: { testid: string; value: string } }
  | { reload: true }
  | { select_orf: { id: string } }
  | { drag_bases: { from: number; to: number } }
  | { fill: { testid: string; value: string } }
  | { press: string }
  | { save_as: { path: string } }
  | { remember: { state: string; as: string; single?: boolean } }
  | { set_workspace: string }
  | { poll_workspace: true }
  | { run_cli: { args: string[] } }
  | { click_site: { enzyme: string } }
  | { choose_enzymes: { names: string[] } }
  | { toggle_detection: { name: string } }
  | { select_detection: { name: string } }
  | { select_isoform: { transcript: string } }
  | { drag_exons: { transcript: string; from: number; to: number } }
  | { double_click_exon: { transcript: string; exon: number } }
  | { export_svg: { view: 'map' | 'isoforms'; path: string } });
export interface Scenario { id: string; description: string; fixture: string; steps: Step[] }

/**
 * State paths whose values are biological or engine-derived. Literal `equals` is
 * rejected here: expectations must come from the CLI (or another state path).
 */
const BIOLOGICAL = [
  /^document(\.(name|length|topology|title))?$/,
  /^features/,
  /^selection\.(map|sequence)(\.|$)/,
  /^warnings(\.(count_shown|items|codes)|$)/,
  /^primers\.count/,
  /^map\.orf_regions/,
  /^selection\.range_translation\.protein/,
  /^dialog\.protein/,
  /^detect\.(rows|checked|new_names|unticked_new_names|map)/,
  /^isoforms\.(rows|drawn_ids|highlighted_exons|range|tick_labels|detail\.(cells|display|transcript))/,
  /^enzymes\.(shown|catalogue\.(names|count)|map\.(ticks|labels)|sequence|digest\.(fragments|enzymes))/,
];

export class ScenarioError extends Error {}

function formatAjv(errors: ErrorObject[], data: unknown): string[] {
  // A step's `oneOf` reports every non-matching action branch; keep only the branch
  // for the action the step actually names (schema branches follow ACTIONS order).
  const relevant = errors.filter(error => {
    // Ajv reports $ref'd schema paths relative to the step definition.
    const branch = error.schemaPath.match(/^#\/oneOf\/(\d+)\//);
    const step = error.instancePath.match(/^\/steps\/(\d+)(\/[a-z_]+)?$/);
    if (!branch || !step) return true;
    const value = (data as { steps: Record<string, unknown>[] }).steps[Number(step[1])];
    return ACTIONS[Number(branch[1])] in value;
  });
  const specific = relevant.filter(error => error.keyword !== 'oneOf');
  return (specific.length ? specific : relevant)
    .map(error => `  ${error.instancePath || '/'}: ${error.message}${'additionalProperty' in error.params ? ` (${String(error.params.additionalProperty)})` : ''}${'allowedValues' in error.params ? ` ${JSON.stringify(error.params.allowedValues)}` : ''}`)
    .filter((line, index, all) => all.indexOf(line) === index);
}

export function checkScenario(data: unknown, source: string): Scenario {
  const problems: string[] = [];
  // Friendlier than a oneOf failure: each step needs exactly one known action.
  const steps = (data as { steps?: unknown })?.steps;
  if (Array.isArray(steps)) {
    steps.forEach((step, index) => {
      const actions = step && typeof step === 'object' ? Object.keys(step).filter(key => !['screenshot', 'note'].includes(key)) : [];
      if (actions.length !== 1 || !(ACTIONS as readonly string[]).includes(actions[0])) {
        problems.push(`  /steps/${index}: expected exactly one action of ${ACTIONS.join(', ')}; got ${JSON.stringify(actions)}`);
      }
    });
  }
  if (!problems.length && !validate(data)) problems.push(...formatAjv(validate.errors ?? [], data));
  if (!problems.length) {
    const scenario = data as Scenario;
    if (`${scenario.id}.json` !== basename(source)) problems.push(`  /id: ${JSON.stringify(scenario.id)} must match the file name`);
    let expectations = 0;
    scenario.steps.forEach((step, index) => {
      if (!('expect' in step)) return;
      step.expect.forEach((assertion, item) => {
        expectations++;
        const where = `  /steps/${index}/expect/${item}`;
        for (const path of [assertion.state, assertion.equals_state, assertion.equals_cli?.path, assertion.equals_cli?.one]) {
          if (path === undefined) continue;
          try { parsePath(path); } catch (error) { problems.push(`${where}: ${(error as PathError).message}`); }
        }
        // null marks "nothing loaded/selected/rendered", which is UI state, not biology.
        const nullable = ['document', 'selection.feature_id', 'selection.map', 'selection.sequence'];
        if ('equals' in assertion && !(assertion.equals === null && nullable.includes(assertion.state))) {
          if (assertion.state.startsWith('selection.feature_id')) {
            problems.push(`${where}: selection.feature_id may only be compared literally with null; resolve ids with equals_cli`);
          } else if (BIOLOGICAL.some(pattern => pattern.test(assertion.state))) {
            problems.push(`${where}: literal "equals" is not allowed on ${assertion.state}; use equals_cli so the engine is the source of truth`);
          }
        }
        const cliExpectation = assertion.equals_cli;
        if (cliExpectation?.one && cliExpectation.index !== undefined) problems.push(`${where}: "index" needs "path", not "one"`);
      });
    });
    if (!expectations) problems.push('  /steps: a scenario needs at least one expect step');
  }
  if (problems.length) throw new ScenarioError(`Invalid scenario ${source}:\n${problems.join('\n')}`);
  return data as Scenario;
}

export function loadScenario(file: string): Scenario {
  let data: unknown;
  try { data = JSON.parse(readFileSync(file, 'utf8')); } catch (error) {
    throw new ScenarioError(`Invalid scenario ${file}:\n  not valid JSON: ${(error as Error).message}`);
  }
  return checkScenario(data, file);
}

export function scenarioFiles(): string[] {
  return readdirSync(SCENARIO_DIR).filter(name => name.endsWith('.json')).sort().map(name => resolve(SCENARIO_DIR, name));
}

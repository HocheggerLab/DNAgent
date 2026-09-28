import assert from 'node:assert/strict';
import test from 'node:test';
import { checkScenario, loadScenario, scenarioFiles, ScenarioError } from './scenario.ts';

const base = {
  id: 'sample', description: 'd', fixture: 'fixtures/formats/snapgene/synthetic_linear.dna',
  steps: [{ open: {} }, { expect: [{ state: 'active_tab', equals: 'map' }] }],
};
const check = (overrides: object) => checkScenario({ ...base, ...overrides }, 'sample.json');

test('a well-formed scenario passes', () => { assert.equal(check({}).id, 'sample'); });

test('committed scenarios are valid and have unique ids', () => {
  const ids = scenarioFiles().map(file => loadScenario(file).id);
  assert.ok(ids.length > 0);
  assert.equal(new Set(ids).size, ids.length);
});

test('malformed scenarios fail with a located message', () => {
  assert.throws(() => check({ steps: [{ opne: {} }] }), /\/steps\/0: expected exactly one action/);
  assert.throws(() => check({ steps: [{ open: {}, select_tab: 'map' }] }), /\/steps\/0: expected exactly one action/);
  assert.throws(() => check({ steps: [{ select_tab: 'maps' }, { expect: [{ state: 'idle', equals: true }] }] }), /\/steps\/0\/select_tab/);
  assert.throws(() => check({ fixture: '/Users/private/lab.dna' }), /\/fixture/);
  assert.throws(() => check({ steps: [{ open: {} }] }), /at least one expect/);
  assert.throws(() => check({ id: 'other' }), /must match the file name/);
  assert.throws(() => check({ steps: [{ expect: [{ state: 'idle', equals_cli: { command: 'inspect' } }] }] }), ScenarioError);
  assert.throws(() => check({ steps: [{ expect: [{ state: 'features[', equals_state: 'idle' }] }] }), /unsupported syntax/);
});

test('biological literals are rejected', () => {
  for (const state of ['document.length', 'selection.map.parts', 'features[*].name', 'warnings.count_shown', 'selection.sequence.highlighted_positions']) {
    assert.throws(() => check({ steps: [{ expect: [{ state, equals: 1 }] }] }), /not allowed/, state);
  }
  assert.throws(() => check({ steps: [{ expect: [{ state: 'selection.feature_id', equals: 'feature-0001' }] }] }), /only be compared literally with null/);
  assert.equal(check({ steps: [{ expect: [{ state: 'selection.feature_id', equals: null }] }] }).id, 'sample');
});

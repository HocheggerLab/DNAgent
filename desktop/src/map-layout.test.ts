import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { contains, displayEndpoints, featureColor, labelPositions } from './map-layout.ts';

const feature = { id:'f1', label:'test', kind:'CDS', color:null, strand:'reverse' as const, parts:[{start:90,length:20}] };
test('origin-spanning highlighting preserves reference frame', () => {
  assert.ok(contains(feature, 95, 100));
  assert.ok(contains(feature, 5, 100));
  assert.ok(!contains(feature, 10, 100));
  assert.deepEqual(displayEndpoints(90, 20, true), [110, 90]);
  assert.deepEqual(displayEndpoints(90, 20, false), [90, 110]);
});
test('imported colors are validated before use in SVG', () => {
  assert.equal(featureColor({...feature,color:'#FF0000'}), '#FF0000');
  assert.equal(featureColor({...feature,color:'url(https://example.org)'}), featureColor(feature));
});
test('labels retain anchor order without vertical collisions', () => {
  const positions = labelPositions([200, 199, 200, 50], 400);
  assert.equal(new Set(positions).size, 4);
  assert.ok(positions[3] < positions[1]);
  assert.ok(positions[1] < positions[0]);
});

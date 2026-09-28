import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { contains, featureColor } from './map-layout.ts';

const feature = { id:'f1', label:'test', kind:'CDS', color:null, strand:'reverse' as const, parts:[{start:90,length:20}] };
test('origin-spanning highlighting preserves reference frame', () => {
  assert.ok(contains(feature, 95, 100));
  assert.ok(contains(feature, 5, 100));
  assert.ok(!contains(feature, 10, 100));
});
test('imported colors are validated before use in SVG', () => {
  assert.equal(featureColor({...feature,color:'#FF0000'}), '#FF0000');
  assert.equal(featureColor({...feature,color:'url(https://example.org)'}), featureColor(feature));
});

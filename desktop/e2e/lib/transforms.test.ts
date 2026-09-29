import assert from 'node:assert/strict';
import test from 'node:test';
import { enzymeSet, fragmentParts, fragmentRange, idsCovering, parts, positions, recognitionRange, siteCuts, siteLabels, siteRegions, siteTicks } from './transforms.ts';

const wrap = { parts: [{ kind: 'circular_arc', start: 10, length: 4 }, { kind: 'linear', start: 4, end: 6 }] };

test('parts keep source order and circular arcs', () => {
  assert.deepEqual(parts(wrap), [{ start: 10, length: 4 }, { start: 4, length: 2 }]);
});

test('positions wrap the origin', () => {
  assert.deepEqual(positions(wrap, 12), [0, 1, 4, 5, 10, 11]);
});

test('ids covering a base follow source order', () => {
  const features = [
    { id: 'a', location: { parts: [{ kind: 'linear', start: 0, end: 3 }] } },
    { id: 'b', location: wrap },
    { id: 'c', location: { parts: [{ kind: 'linear', start: 1, end: 2 }] } },
  ];
  assert.deepEqual(idsCovering(features, 1, 12), ['a', 'b', 'c']);
  assert.deepEqual(idsCovering(features, 7, 12), []);
});

const sites = {
  enzymes: [{ name: 'EcoRI', recognition_sequence: 'GAATTC' }, { name: 'ApoI', recognition_sequence: 'RAATTY' }, { name: 'Sau3AI', recognition_sequence: 'GATC' }],
  sites: [
    { enzyme: 'EcoRI', recognition: { kind: 'circular_arc', start: 23, length: 6 }, top_cut: 24, bottom_cut: 2 },
    { enzyme: 'ApoI', recognition: { kind: 'circular_arc', start: 23, length: 6 }, top_cut: 24, bottom_cut: 2 },
    { enzyme: 'Sau3AI', recognition: { kind: 'linear', start: 5, end: 9 }, top_cut: 5, bottom_cut: 9 },
    { enzyme: 'Sau3AI', recognition: { kind: 'linear', start: 12, end: 16 }, top_cut: 12, bottom_cut: 16 },
  ],
};

test('site transforms group, sort and wrap', () => {
  assert.deepEqual(enzymeSet(sites, 'unique_any'), ['ApoI', 'EcoRI']);
  assert.deepEqual(enzymeSet(sites, 'unique6'), ['ApoI', 'EcoRI']);
  assert.deepEqual(siteLabels(sites), [{ cut: 5, names: ['Sau3AI'] }, { cut: 12, names: ['Sau3AI'] }, { cut: 24, names: ['ApoI', 'EcoRI'] }]);
  assert.deepEqual(siteTicks(sites), [5, 12, 24]);
  assert.deepEqual(siteCuts(sites, 26), { top: [5, 12, 24], bottom: [2, 9, 16] });
  assert.deepEqual(siteCuts(sites, 16), { top: [5, 12], bottom: [2, 9] }, 'no base after a cut at a linear end');
  assert.deepEqual(recognitionRange(sites.sites[0], 26, true), { start: 23, end: 3 });
  assert.deepEqual(siteRegions(sites)[0], { enzyme: 'Sau3AI', start: 5, length: 4 });
});

test('fragment_range ranks longest first', () => {
  const fragments = [{ top: { source_start: 0, length: 5 } }, { top: { source_start: 5, length: 9 } }, { top: { source_start: 14, length: 9 } }];
  assert.deepEqual(fragmentRange(fragments, 0, 23, false), { start: 5, end: 14 });
  assert.deepEqual(fragmentRange(fragments, 1, 23, false), { start: 14, end: 23 });
  assert.deepEqual(fragmentParts(fragments), [{ start: 0, length: 5 }, { start: 5, length: 9 }, { start: 14, length: 9 }]);
});

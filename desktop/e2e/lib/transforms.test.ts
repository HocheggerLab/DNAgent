import assert from 'node:assert/strict';
import test from 'node:test';
import { idsCovering, parts, positions } from './transforms.ts';

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

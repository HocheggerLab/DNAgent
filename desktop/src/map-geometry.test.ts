import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { angleOf, assignLanes, blockArrowPath, niceStep, placeCircularLabels, placeRowLabels, spread, textArcPath, textOn, ticks } from './map-geometry.ts';

test('origin is at the top and angles run clockwise', () => {
  assert.equal(angleOf(0, 100), -Math.PI / 2);
  assert.ok(Math.abs(angleOf(25, 100)) < 1e-12); // three o'clock
});

test('nice tick steps', () => {
  assert.equal(niceStep(9175), 1000);
  assert.equal(niceStep(2686), 500);
  assert.equal(niceStep(12), 2);
  assert.equal(niceStep(3), 1);
  assert.deepEqual(ticks(2686), [500, 1000, 1500, 2000, 2500]);
  assert.deepEqual(ticks(1), []);
});

test('lanes separate overlaps, keep multipart features together and wrap the origin', () => {
  const lanes = assignLanes([
    { id: 'big', parts: [{ start: 0, length: 60 }] },
    { id: 'inside', parts: [{ start: 10, length: 5 }] },
    { id: 'clear', parts: [{ start: 70, length: 10 }] },
    { id: 'wrap', parts: [{ start: 95, length: 10 }, { start: 72, length: 2 }] },
  ], 100, true);
  assert.equal(lanes.get('big'), 0);
  assert.equal(lanes.get('inside'), 1);
  assert.equal(lanes.get('clear'), 0);
  // 'wrap' covers [95,100)+[0,5) (overlaps big) and [72,74) (overlaps clear) → lane 1 or above.
  assert.ok((lanes.get('wrap') ?? 0) >= 1);
  assert.notEqual(lanes.get('wrap'), undefined);
});

test('minimum drawn length and padding make tiny neighbours separate', () => {
  const lanes = assignLanes([{ id: 'a', parts: [{ start: 10, length: 1 }] }, { id: 'b', parts: [{ start: 12, length: 1 }] }], 1000, false, 6);
  assert.notEqual(lanes.get('a'), lanes.get('b'));
});

test('block arrows are closed paths; full circles have no head', () => {
  for (const arrow of ['forward', 'reverse', 'none'] as const) {
    const d = blockArrowPath(0, 0, 100, 10, 0, 1, arrow, 12);
    assert.match(d, /^M .* Z$/);
  }
  assert.equal(blockArrowPath(0, 0, 100, 10, 0, Math.PI * 2, 'forward', 12).match(/M /g)?.length, 2);
});

test('text arcs are flipped on the lower half so labels stay upright', () => {
  assert.equal(textArcPath(0, 0, 100, -2, -1).flipped, false); // upper half
  assert.equal(textArcPath(0, 0, 100, 1, 2).flipped, true);
});

test('circular labels keep order, never overlap and report what does not fit', () => {
  const requests = Array.from({ length: 6 }, (_, i) => ({ id: `r${i}`, anchorX: 150, anchorY: 90 + i, width: 40, priority: i }));
  const { placed, hidden } = placeCircularLabels(requests, 100, 100, 60, 0, 200, 20);
  assert.equal(hidden.length, 0);
  const ys = placed.map(p => p.y);
  for (let i = 1; i < ys.length; i++) assert.ok(ys[i] - ys[i - 1] >= 20 - 1e-9);
  assert.deepEqual(placed.map(p => p.id), requests.map(r => r.id));
  const clamped = placeCircularLabels([{ id: 'wide', anchorX: 190, anchorY: 100, width: 80, priority: 1 }], 100, 100, 60, 0, 200, 20, 0, 200);
  assert.ok(clamped.placed[0].x + 80 <= 200);
  const crowded = placeCircularLabels(requests, 100, 100, 60, 0, 40, 20); // room for 3 labels
  assert.equal(crowded.placed.length, 3);
  assert.deepEqual(crowded.hidden.sort(), ['r0', 'r1', 'r2']); // lowest priority dropped
});

test('row labels pack without collisions and hide the overflow', () => {
  const requests = [0, 1, 2].map(i => ({ id: `l${i}`, centre: 50, width: 40, priority: 3 - i }));
  const { placed, hidden } = placeRowLabels(requests, 0, 200, 2, 4);
  assert.deepEqual(placed.map(p => p.row), [0, 1]);
  assert.deepEqual(hidden, ['l2']);
});

test('label text contrasts with the fill', () => {
  assert.equal(textOn('#ffffff'), '#111111');
  assert.equal(textOn('#ffef86'), '#111111');
  assert.equal(textOn('#993366'), '#ffffff');
});

test('crowded labels spread both ways around their anchors and stay in bounds', () => {
  const ys = [100, 100, 100];
  spread(ys, 0, 400, 20);
  assert.deepEqual(ys, [80, 100, 120]);
  const edge = [2, 3, 4];
  spread(edge, 0, 400, 20);
  assert.deepEqual(edge, [0, 20, 40]);
  const apart = [10, 200];
  spread(apart, 0, 400, 20);
  assert.deepEqual(apart, [10, 200]);
});

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { axisScale, codonBlocks, exonPieces, genomicTicks, tickStep, uncovered } from './isoform-geometry.ts';

const exons = [{ start: 100, length: 50 }, { start: 1000, length: 100 }, { start: 120, length: 10 }];

test('uncovered stretches are the introns and flanks', () => {
  assert.deepEqual(uncovered(2000, exons), [{ start: 0, end: 100 }, { start: 150, end: 1000 }, { start: 1100, end: 2000 }]);
});

test('true scale is linear and invertible', () => {
  const s = axisScale(2000, exons, false, 0, 2000, 0, 1000);
  assert.equal(s.toX(1000), 500);
  assert.equal(s.fromX(250), 500);
  assert.deepEqual(s.breaks, []);
});

test('compressed introns shrink to the gap width and exons keep their length', () => {
  const s = axisScale(2000, exons, true, 0, 2000, 0, 1000, 60);
  // Units: flank 60, exon 50, intron 60, exon 100, flank 60 = 330.
  const perUnit = 1000 / 330;
  assert.ok(Math.abs(s.toX(150) - s.toX(100) - 50 * perUnit) < 1e-9, 'exon drawn to scale');
  assert.ok(Math.abs(s.toX(1000) - s.toX(150) - 60 * perUnit) < 1e-9, 'intron compressed');
  for (const p of [0, 125, 600, 1050, 1999]) assert.ok(Math.abs(s.fromX(s.toX(p)) - p) < 1e-6, `round trip at ${p}`);
  assert.equal(s.breaks.length, 3);
});

test('zooming maps the window onto the full width', () => {
  const s = axisScale(2000, exons, false, 1000, 1100, 0, 500);
  assert.equal(s.toX(1000), 0);
  assert.equal(s.toX(1100), 500);
});

test('ticks fall on round genomic coordinates', () => {
  assert.equal(tickStep(34000), 5000);
  assert.deepEqual(genomicTicks(9_089_594, 0, 12_000, 4).slice(0, 2), [9_090_000 - 9_089_594, 9_095_000 - 9_089_594]);
});

test('codons follow the exons across a junction on either strand', () => {
  const ex = [{ start: 10, length: 5 }, { start: 30, length: 5 }];
  assert.deepEqual(codonBlocks(ex, 11, false), [{ start: 11, end: 14 }]);
  assert.deepEqual(codonBlocks(ex, 13, false), [{ start: 13, end: 15 }, { start: 30, end: 31 }]);
  assert.deepEqual(codonBlocks(ex, 31, true), [{ start: 14, end: 15 }, { start: 30, end: 32 }]);
});

test('exons split into coding and untranslated pieces', () => {
  assert.deepEqual(exonPieces([{ start: 0, length: 10 }, { start: 20, length: 10 }], [{ start: 5, length: 5 }, { start: 20, length: 3 }]), [
    { start: 0, end: 5, coding: false }, { start: 5, end: 10, coding: true },
    { start: 20, end: 23, coding: true }, { start: 23, end: 30, coding: false },
  ]);
});

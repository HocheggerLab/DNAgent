import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { enzymesInSet, specificity } from './enzymes.ts';

const enzyme = (name: string, site: string) => ({ name, site, top_cut_offset: 1, bottom_cut_offset: 5, overhang: 'five_prime', overhang_length: 4 });
const catalogue = { source: 'test', version: '1', unsupported: 0,
  enzymes: [enzyme('EcoRI', 'GAATTC'), enzyme('PvuII', 'CAGCTG'), enzyme('Sau3AI', 'GATC'), enzyme('BglI', 'GCCNNNNNGGC'), enzyme('NotI', 'GCGGCCGC'), enzyme('XhoI', 'CTCGAG')] };
const counts = [['EcoRI', 1], ['PvuII', 2], ['Sau3AI', 1], ['BglI', 1], ['NotI', 3], ['XhoI', 0]].map(([name, sites]) => ({ name: name as string, sites: sites as number, cuts: sites as number }));

test('specificity ignores N', () => {
  assert.equal(specificity('GCCNNNNNGGC'), 6);
  assert.equal(specificity('GATC'), 4);
});

test('display sets follow site counts and 6+ specificity', () => {
  assert.deepEqual(enzymesInSet('unique6', catalogue, counts, []), ['BglI', 'EcoRI']);
  assert.deepEqual(enzymesInSet('unique_dual6', catalogue, counts, []), ['BglI', 'EcoRI', 'PvuII']);
  assert.deepEqual(enzymesInSet('unique_any', catalogue, counts, []), ['BglI', 'EcoRI', 'Sau3AI']);
  assert.deepEqual(enzymesInSet('common', catalogue, counts, []), ['EcoRI', 'NotI']);
  assert.deepEqual(enzymesInSet('custom', catalogue, counts, ['XhoI', 'Sau3AI']), ['Sau3AI'], 'chosen enzymes without a site are not shown');
  assert.deepEqual(enzymesInSet('none', catalogue, counts, []), []);
});

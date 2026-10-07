import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import type { Feature, Isoform, LocusView } from './bindings.ts';
import { orderedFeatures } from './locus-order.ts';

const feature = (id: string): Feature =>
  ({ id, label: id, kind: 'mRNA', color: null, strand: 'reverse', parts: [], notes: [] }) as unknown as Feature;

const isoform = (transcript_id: string, mrna: string, cds: string | null): Isoform =>
  ({ transcript_id, mrna_feature_id: mrna, cds_feature_id: cds }) as unknown as Isoform;

/** Three transcripts in accession order, ranked by expression in the reverse of it. */
const locus = (order: string[], quantifier = 'bambu_lr'): LocusView =>
  ({
    isoforms: [isoform('T1', 'f-1', 'f-2'), isoform('T2', 'f-3', 'f-4'), isoform('T3', 'f-5', null)],
    quantifiers: [{ quantifier, reports_zeros: true, cell_lines: ['A549'], order }],
  }) as unknown as LocusView;

const ids = (features: Feature[]) => features.map(f => f.id);
const annotationOrder = ['f-1', 'f-2', 'f-3', 'f-4', 'f-5'].map(feature);

test('the most expressed transcript leads, each as mRNA then CDS', () => {
  const ordered = orderedFeatures(annotationOrder, locus(['T3', 'T2', 'T1']), 'bambu_lr');
  assert.deepEqual(ids(ordered), ['f-5', 'f-3', 'f-4', 'f-1', 'f-2']);
});

test('a bundle with no panel for the quantifier keeps annotation order', () => {
  assert.deepEqual(ids(orderedFeatures(annotationOrder, locus(['T3']), 'salmon_lr')), ids(annotationOrder));
  assert.deepEqual(ids(orderedFeatures(annotationOrder, locus(['T3']), null)), ids(annotationOrder));
});

test('features of no transcript come first and nothing is lost', () => {
  const withOwn = [...annotationOrder, feature('f-9')];
  const ordered = orderedFeatures(withOwn, locus(['T2', 'T1', 'T3']), 'bambu_lr');
  assert.deepEqual(ids(ordered), ['f-9', 'f-3', 'f-4', 'f-1', 'f-2', 'f-5']);
  assert.equal(ordered.length, withOwn.length);
});

test('a transcript the panel does not rank is kept, after the ranked ones', () => {
  const ordered = orderedFeatures(annotationOrder, locus(['T3', 'T1']), 'bambu_lr');
  assert.deepEqual(ids(ordered), ['f-5', 'f-1', 'f-2', 'f-3', 'f-4']);
});

test('a deleted feature is skipped rather than listed as a hole', () => {
  const remaining = annotationOrder.filter(f => f.id !== 'f-2');
  const ordered = orderedFeatures(remaining, locus(['T1', 'T2', 'T3']), 'bambu_lr');
  assert.deepEqual(ids(ordered), ['f-1', 'f-3', 'f-4', 'f-5']);
});

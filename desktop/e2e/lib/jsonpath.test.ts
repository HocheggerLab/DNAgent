import assert from 'node:assert/strict';
import test from 'node:test';
import { PathError, query, queryOne } from './jsonpath.ts';

const envelope = { result: [{ id: 'a', label: 'x', parts: [1, 2] }, { id: 'b', label: "it's", parts: [] }], warnings: [] };

test('keys, indices, wildcards and filters', () => {
  assert.deepEqual(query(envelope, 'result[*].id'), ['a', 'b']);
  assert.equal(query(envelope, 'result[1].label'), "it's");
  assert.equal(queryOne(envelope, "result[?(@.label=='it\\'s')].id"), 'b');
  assert.deepEqual(query(envelope, 'warnings[*]'), []);
  assert.deepEqual(query(envelope, 'result[*].parts[*]'), [1, 2]);
});

test('no match is an error, never an empty pass', () => {
  assert.throws(() => query(envelope, "result[?(@.label=='missing')]"), PathError);
  assert.throws(() => query(envelope, 'result[5]'), /nothing matched at "result\[5\]"/);
  assert.throws(() => query(envelope, 'nope'), PathError);
  assert.throws(() => queryOne(envelope, 'result[*]'), /exactly one/);
  assert.throws(() => query(envelope, 'result[?(@.label~"x")]'), /unsupported syntax/);
});

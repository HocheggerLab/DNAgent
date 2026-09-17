import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { rowSpans } from './sequence-layout.ts';

test('line clipping does not create biological ends', () => {
  assert.deepEqual(rowSpans({start:10,length:100},0,60,150), [{start:10,end:60,sourceStart:true,sourceEnd:false}]);
  assert.deepEqual(rowSpans({start:10,length:100},60,120,150), [{start:0,end:50,sourceStart:false,sourceEnd:true}]);
});
test('origin wrapping retains real ends on the correct piece', () => {
  assert.deepEqual(rowSpans({start:90,length:20},0,60,100), [{start:0,end:10,sourceStart:false,sourceEnd:true}]);
  assert.deepEqual(rowSpans({start:90,length:20},60,100,100), [{start:30,end:40,sourceStart:true,sourceEnd:false}]);
});
test('multipart gaps and exclusive boundaries remain unfilled', () => {
  const parts = [{start:10,length:10},{start:30,length:10}];
  assert.deepEqual(parts.flatMap(part => rowSpans(part,20,30,100)), []);
  assert.deepEqual(rowSpans({start:0,length:100},0,60,100), [{start:0,end:60,sourceStart:true,sourceEnd:false}]);
});

import type { Segment } from './bindings';

export interface DisplaySpan {
  start: number;
  end: number;
  sourceStart: boolean;
  sourceEnd: boolean;
}

/** Clip a source part into one forward-reference display row without filling gaps.
 * Source ends are retained separately from row/origin breaks for honest arrowheads.
 */
export function rowSpans(part: Segment, rowStart: number, rowEnd: number, moleculeLength: number): DisplaySpan[] {
  const end = part.start + part.length;
  const pieces = end <= moleculeLength
    ? [{start:part.start,end,sourceStart:true,sourceEnd:true}]
    : [{start:part.start,end:moleculeLength,sourceStart:true,sourceEnd:false},
       {start:0,end:end - moleculeLength,sourceStart:false,sourceEnd:true}];
  return pieces.flatMap(piece => {
    const start = Math.max(piece.start,rowStart);
    const end = Math.min(piece.end,rowEnd);
    return start < end ? [{start:start - rowStart,end:end - rowStart,
      sourceStart:piece.sourceStart && start === piece.start,
      sourceEnd:piece.sourceEnd && end === piece.end}] : [];
  });
}

import { describe, expect, test } from 'bun:test';
import { encodeTrailingData, trailingByteSequences } from '../src/azw3/tbs';

describe('trailing byte sequences', () => {
  test('a flat TOC over three records, worked by hand from tbs.py', () => {
    const tbs = trailingByteSequences([
      { index: 0, start: 300, length: 500, depth: 0 }, // completes in record 1
      { index: 1, start: 800, length: 6000, depth: 0 }, // starts in 1, ends in 2
      { index: 2, start: 6800, length: 2200, depth: 0 }, // starts in 2, ends in 3
    ], [4096, 4096, 1000]);
    // One sequence per record: index (minus the parent, none here) shifted
    // over a 3-bit flag field; flag 0b10 carries the type (8), 0b100 the
    // count of consecutive siblings.
    expect(tbs.map((b) => Array.from(b))).toEqual([
      [(0 << 3 | 6) | 0x80, 0x88, 2],
      [(1 << 3 | 6) | 0x80, 0x88, 2],
      [(2 << 3 | 2) | 0x80, 0x88],
    ]);
  });

  test('a record no entry touches gets an empty sequence', () => {
    expect(trailingByteSequences([{ index: 0, start: 10, length: 5, depth: 0 }], [100, 100])[1]).toEqual(new Uint8Array(0));
  });

  test('an entry spanning a whole record is flagged as spanning', () => {
    const [, middle] = trailingByteSequences([{ index: 0, start: 0, length: 9000, depth: 0 }], [4096, 4096, 4096]);
    // index 0, flags 0b10 (type) | 0b1 (spans), then the type, then 0.
    expect(Array.from(middle)).toEqual([(0 << 3 | 3) | 0x80, 0x88, 0x80]);
  });

  test('a trailing entry ends with its own length, read backward', () => {
    expect(Array.from(encodeTrailingData(new Uint8Array(0)))).toEqual([0x81]);
    expect(Array.from(encodeTrailingData(new Uint8Array([1, 2, 3])))).toEqual([1, 2, 3, 0x84]);
    const long = encodeTrailingData(new Uint8Array(200));
    expect(long.length).toBe(202);
    expect(Array.from(long.slice(200))).toEqual([0x81, 0x4a]);
  });
});

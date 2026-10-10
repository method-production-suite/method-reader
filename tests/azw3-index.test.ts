import { describe, expect, test } from 'bun:test';
import { Cncx, chunkIndex, encint, guideIndex, ncxIndex, skelIndex, toBase } from '../src/azw3/index';
import { decint, readIndex } from './helpers/kf8-reader';

describe('the numbers KF8 indices are written in', () => {
  test('encint round-trips both ways, and matches utils.py\'s worked example', () => {
    // utils.py's docstring: 0x11111 forward is 04 22 91, backward 84 22 11.
    expect(Array.from(encint(0x11111))).toEqual([0x04, 0x22, 0x91]);
    expect(Array.from(encint(0x11111, false))).toEqual([0x84, 0x22, 0x11]);
    for (const n of [0, 1, 127, 128, 16383, 16384, 2 ** 31 - 1, 2 ** 32 + 5]) {
      expect(decint(encint(n))).toEqual([n, encint(n).length]);
      expect(decint(encint(n, false), false)).toEqual([n, encint(n, false).length]);
    }
  });

  test('toBase is base 32, upper case, zero-padded', () => {
    expect(toBase(0, 32, 4)).toBe('0000');
    expect(toBase(1, 32, 4)).toBe('0001');
    expect(toBase(32, 32, 4)).toBe('0010');
    expect(toBase(1_000_000, 32)).toBe('UGI0');
    expect(toBase(31, 32, 10)).toBe('000000000V');
  });

  test('CNCX offsets point at each string, deduplicated, and roll over at 64 KiB', () => {
    const many = Array.from({ length: 300 }, (_, i) => `${i} `.padEnd(400, 'x'));
    const cncx = new Cncx(['a', 'b', 'a', ...many]);
    expect(cncx.records.length).toBe(2);
    expect(cncx.offsetOf('a')).toBe(0);
    expect(cncx.offsetOf('b')).toBe(2);
    expect(cncx.offsetOf(many[299]) >= 0x10000).toBe(true);
  });
});

describe('the four indices read back', () => {
  test('skeleton: names, counts twice, geometry twice', () => {
    const records = skelIndex([
      { fileNumber: 0, chunkCount: 1, startPos: 0, length: 300 },
      { fileNumber: 1, chunkCount: 3, startPos: 900, length: 280 },
    ]);
    expect(records.length).toBe(2); // header + one record, no strings
    const t = readIndex(records, 0);
    expect([...t.entries.keys()]).toEqual(['SKEL0000000000', 'SKEL0000000001']);
    expect(t.entries.get('SKEL0000000001')!.get(1)).toEqual([3, 3]);
    expect(t.entries.get('SKEL0000000001')!.get(6)).toEqual([900, 280, 900, 280]);
  });

  test('fragments: selectors through CNCX, and positions as ten-digit names', () => {
    const records = chunkIndex([
      { insertPos: 214, selector: "P-//*[@aid='0']", fileNumber: 0, sequenceNumber: 0, startPos: 0, length: 4000 },
      { insertPos: 4214, selector: "S-//*[@aid='1F']", fileNumber: 0, sequenceNumber: 1, startPos: 4000, length: 60 },
    ]);
    const t = readIndex(records, 0);
    expect([...t.entries.keys()]).toEqual(['0000000214', '0000004214']);
    const second = t.entries.get('0000004214')!;
    expect(t.cncx.get(second.get(2)![0])).toBe("S-//*[@aid='1F']");
    expect(second.get(3)).toEqual([0]);
    expect(second.get(4)).toEqual([1]);
    expect(second.get(6)).toEqual([4000, 60]);
  });

  test('guide and NCX', () => {
    const guide = readIndex(guideIndex([{ type: 'text', title: 'Begin Reading', posFid: [3, 0] }]), 0);
    expect(guide.cncx.get(guide.entries.get('text')!.get(1)![0])).toBe('Begin Reading');
    expect(guide.entries.get('text')!.get(6)).toEqual([3, 0]);

    const ncx = readIndex(ncxIndex([
      { index: 0, offset: 300, length: 500, label: 'INT. ROOM - DAY', depth: 0, posFid: [1, 0] },
      { index: 1, offset: 800, length: 90, label: 'EXT. ROAD - NIGHT', depth: 0, posFid: [2, 14] },
    ]), 0);
    expect([...ncx.entries.keys()]).toEqual(['00', '01']);
    const e = ncx.entries.get('01')!;
    expect(e.get(1)).toEqual([800]);
    expect(e.get(2)).toEqual([90]);
    expect(ncx.cncx.get(e.get(3)![0])).toBe('EXT. ROAD - NIGHT');
    expect(e.get(4)).toEqual([0]);
    expect(e.get(6)).toEqual([2, 14]);
  });

  test('a large index rolls over into more records and still reads as one', () => {
    const table = Array.from({ length: 6000 }, (_, i) => ({
      insertPos: i * 9000, selector: `P-//*[@aid='${i}']`, fileNumber: 0, sequenceNumber: i, startPos: i * 8000, length: 8000,
    }));
    const records = chunkIndex(table);
    const t = readIndex(records, 0);
    expect(t.recordCount).toBeGreaterThan(1);
    expect(t.entries.size).toBe(6000);
    for (const r of records) expect(r.length).toBeLessThanOrEqual(0x10000);
  });
});

// Reads a built MOBI's picture records back out of its bytes, the way a
// reader finds them: the PDB's record list for where each record sits, and
// the MOBI header's fields 108 (the first image record, absolute) and 194
// (the last content record, which reaches the last picture). And its EXTH
// metadata, which is where the cover is named. Shared by the tests that
// check what a whole route hands the MOBI; tests/mobi.test.ts pins the
// header fields themselves and keeps its own reader for those.

const u16 = (b: Uint8Array, o: number) => (b[o] << 8) | b[o + 1];
const u32 = (b: Uint8Array, o: number) =>
  ((b[o] << 24) | (b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3]) >>> 0;

/** The picture records, in record order, each its own bytes. */
export function mobiPictures(b: Uint8Array): Uint8Array[] {
  const count = u16(b, 76);
  const offsets = Array.from({ length: count }, (_, i) => u32(b, 78 + i * 8));
  const r0 = offsets[0];
  const first = u32(b, r0 + 108);
  if (first === 0xffffffff) return [];
  const last = u16(b, r0 + 194);
  const out: Uint8Array[] = [];
  for (let i = first; i <= last; i++) out.push(b.slice(offsets[i], offsets[i + 1]));
  return out;
}

/**
 * The book's markup: its text records, which the PalmDOC header counts and
 * which start at record 1, joined and decoded, cut at the stated text length.
 */
export function mobiHtml(b: Uint8Array): string {
  const count = u16(b, 76);
  const offsets = Array.from({ length: count }, (_, i) => u32(b, 78 + i * 8));
  const r0 = offsets[0];
  const length = u32(b, r0 + 4);
  const textRecords = u16(b, r0 + 8);
  const parts = Array.from({ length: textRecords }, (_, i) => b.slice(offsets[i + 1], offsets[i + 2]));
  const joined = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  parts.reduce((at, p) => (joined.set(p, at), at + p.length), 0);
  return new TextDecoder().decode(joined.slice(0, length));
}

/** One EXTH record: its type number and its data, the 8-byte head left off. */
export interface ExthRecord {
  type: number;
  data: Uint8Array;
}

/**
 * Record 0's EXTH block, walked the way a reader walks it: from the end of
 * the MOBI header (whose length is the header's own field at 20), one record
 * after another by each record's stated length. Throws where the block
 * disagrees with itself (the header flag at 128 unset, the magic missing, or
 * a walk that does not end exactly at the block's stated length or after its
 * stated count), since a reader that trusted either number would lose its
 * place in the records.
 */
export function exthRecords(b: Uint8Array): ExthRecord[] {
  const r0 = u32(b, 78);
  if ((u32(b, r0 + 128) & 0x40) === 0) throw new Error('the MOBI header says there is no EXTH block');
  const at = r0 + 16 + u32(b, r0 + 20);
  const magic = String.fromCharCode(...b.slice(at, at + 4));
  if (magic !== 'EXTH') throw new Error(`expected EXTH at ${at}, found ${JSON.stringify(magic)}`);
  const length = u32(b, at + 4);
  const count = u32(b, at + 8);
  const out: ExthRecord[] = [];
  let p = at + 12;
  for (let i = 0; i < count; i++) {
    const size = u32(b, p + 4);
    out.push({ type: u32(b, p), data: b.slice(p + 8, p + size) });
    p += size;
  }
  if (p - at !== length) throw new Error(`the EXTH block says ${length} bytes, its ${count} records walk ${p - at}`);
  return out;
}

/** The data of the one EXTH record of this type, or undefined if none. */
export function exth(b: Uint8Array, type: number): Uint8Array | undefined {
  const found = exthRecords(b).filter((r) => r.type === type);
  if (found.length > 1) throw new Error(`EXTH ${type} is written ${found.length} times`);
  return found[0]?.data;
}

/**
 * A copy with the PDB header's creation and modification dates zeroed. They
 * are the only bytes of a build that depend on when it ran (pinned in
 * tests/mobi.test.ts), so two builds of one book compare equal through this.
 * Copied with the constructor rather than `slice`, which on a Buffer from
 * readFileSync is a view and would zero the caller's bytes.
 */
export function maskPdbDates(b: Uint8Array): Uint8Array {
  const c = new Uint8Array(b);
  c.fill(0, 36, 44);
  return c;
}

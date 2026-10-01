// Reads a built MOBI's picture records back out of its bytes, the way a
// reader finds them: the PDB's record list for where each record sits, and
// the MOBI header's fields 108 (the first image record, absolute) and 194
// (the last content record, which reaches the last picture). Shared by the
// tests that check what a whole route hands the MOBI; tests/mobi.test.ts
// pins the fields themselves and keeps its own reader for that.

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

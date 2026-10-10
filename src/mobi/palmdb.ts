// The PalmDB ("BOOKMOBI") container and the small byte tools both Kindle
// writers share: the engine's MOBI 6 (mobi/writer.ts) and its AZW3, KF8
// (azw3/writer.ts). One copy, so the two books are laid out by the same
// code: a 78-byte database header, a record list of offsets and ids, a
// 2-byte gap, then the records back to back.

export class ByteWriter {
  private chunks: number[] = [];

  get length(): number {
    return this.chunks.length;
  }
  u8(v: number): this {
    this.chunks.push(v & 0xff);
    return this;
  }
  u16(v: number): this {
    return this.u8(v >>> 8).u8(v);
  }
  u32(v: number): this {
    return this.u16(v >>> 16).u16(v & 0xffff);
  }
  ascii(s: string): this {
    for (const c of s) this.u8(c.charCodeAt(0));
    return this;
  }
  bytes(b: Uint8Array): this {
    for (const v of b) this.chunks.push(v);
    return this;
  }
  zeros(n: number): this {
    for (let i = 0; i < n; i++) this.chunks.push(0);
    return this;
  }
  fill(n: number, v: number): this {
    for (let i = 0; i < n; i++) this.chunks.push(v & 0xff);
    return this;
  }
  toUint8Array(): Uint8Array {
    return new Uint8Array(this.chunks);
  }
}

/** One EXTH record: type, length including this 8-byte head, data. */
export function exthRecord(type: number, data: Uint8Array): ByteWriter {
  return new ByteWriter().u32(type).u32(data.length + 8).bytes(data);
}

/** An EXTH record holding one big-endian 32-bit number. */
export function exthNumber(type: number, value: number): ByteWriter {
  return exthRecord(type, new ByteWriter().u32(value).toUint8Array());
}

/** Deterministic 32-bit hash for the unique-id fields. */
export function hash32(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

/** Concatenate byte arrays. */
export function concatBytes(parts: readonly Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/**
 * The whole file: the PalmDB header for `title`, then `records` in order.
 * `uid` seeds the database's unique-id field.
 */
export function palmDatabase(title: string, uid: number, records: readonly Uint8Array[]): Uint8Array {
  const headerSize = 78 + records.length * 8 + 2; // header + info list + 2-byte pad
  const pdb = new ByteWriter();

  const dbName = title.replace(/[^\x20-\x7e]/g, '_').slice(0, 31);
  pdb.ascii(dbName).zeros(32 - dbName.length);
  pdb.u16(0); // attributes
  pdb.u16(0); // version
  const now = Math.floor(Date.now() / 1000);
  pdb.u32(now).u32(now).u32(0); // created, modified, backup
  pdb.u32(0); // modification number
  pdb.u32(0); // app info
  pdb.u32(0); // sort info
  pdb.ascii('BOOK').ascii('MOBI');
  pdb.u32(uid); // unique id seed
  pdb.u32(0); // next record list
  pdb.u16(records.length);
  let offset = headerSize;
  records.forEach((record, i) => {
    pdb.u32(offset);
    // attributes + 3-byte unique id: the full index, not its low byte —
    // a long script tops 128 text records and single-byte ids collide.
    pdb.u8(0).u8((i >> 16) & 0xff).u8((i >> 8) & 0xff).u8(i & 0xff);
    offset += record.length;
  });
  pdb.zeros(2); // traditional gap before record data

  // The records are copied in as typed arrays, not through ByteWriter, which
  // holds one JS number per byte. Text records never made that matter;
  // pictures do: 60 of them totalling 16.5 MB, the corpus's heaviest picture
  // document, cost about 390 MB of transient memory that way (measured
  // 2026-10-01). `offset` has run to the end of the last record, so it is
  // the size of the whole file.
  const head = pdb.toUint8Array();
  const file = new Uint8Array(offset);
  file.set(head);
  let at = head.length;
  for (const record of records) {
    file.set(record, at);
    at += record.length;
  }
  return file;
}

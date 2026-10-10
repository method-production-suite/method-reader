// A structural reader for KF8 (AZW3) files, for tests: it proves a book is
// well formed with no Calibre on the machine, which is CI.
//
// It reads the way Calibre's reader does, and parts are ported from it
// (src/calibre/ebooks/mobi/reader/index.py, reader/mobi8.py and
// utils.py's decint and get_trailing_data; calibre 9.11.0, GPL-3, Kovid
// Goyal), kept independent of src/azw3/ so a test of the writer is not the
// writer checking itself. It is STRICTER than Calibre's reader: where that
// one warns and carries on (unprocessed index bytes, a fragment whose insert
// position lands inside a tag, a short trailing entry), this throws, because
// a Kindle may not be as forgiving.

import { palmdocDecompress } from './palmdoc-decode';

const u16 = (b: Uint8Array, o: number) => (b[o] << 8) | b[o + 1];
const u32 = (b: Uint8Array, o: number) => ((b[o] << 24) | (b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3]) >>> 0;
const ascii = (b: Uint8Array, o: number, n: number) => String.fromCharCode(...b.slice(o, o + n));
const dec = new TextDecoder('utf-8', { fatal: true });
export const NULL_INDEX = 0xffffffff;

/** A variable-width integer and how many bytes it took (utils.py decint). */
export function decint(raw: Uint8Array, forward = true): [number, number] {
  const src = forward ? raw : raw.slice().reverse();
  const bytes: number[] = [];
  for (const b of src) {
    bytes.push(b & 0x7f);
    if (b & 0x80) break;
  }
  if (!forward) bytes.reverse();
  let val = 0;
  for (const b of bytes) val = val * 128 + b;
  return [val, bytes.length];
}

/** The PalmDB's records, each its own bytes, after checking the record list
 * is in order, inside the file, and covers it to the end. */
export function pdbRecords(file: Uint8Array): Uint8Array[] {
  if (ascii(file, 60, 8) !== 'BOOKMOBI') throw new Error(`not a BOOKMOBI database: ${ascii(file, 60, 8)}`);
  const count = u16(file, 76);
  const offsets = Array.from({ length: count }, (_, i) => u32(file, 78 + i * 8));
  const ids = Array.from({ length: count }, (_, i) => u32(file, 78 + i * 8 + 4) & 0xffffff);
  if (new Set(ids).size !== ids.length) throw new Error('two records share a unique id');
  const headerEnd = 78 + count * 8;
  if (offsets[0] < headerEnd) throw new Error('the first record overlaps the record list');
  for (let i = 1; i < count; i++) {
    if (offsets[i] < offsets[i - 1]) throw new Error(`record ${i} starts before record ${i - 1}`);
  }
  if (offsets[count - 1] > file.length) throw new Error('the last record starts past the end of the file');
  return offsets.map((o, i) => file.slice(o, i + 1 < count ? offsets[i + 1] : file.length));
}

// ── INDX ──

export interface IndexTable {
  /** name → tag number → values, in entry order. */
  entries: Map<string, Map<number, number[]>>;
  /** offset → string. */
  cncx: Map<number, string>;
  /** How many INDX entry records the header announced, and how many CNCX. */
  recordCount: number;
  cncxCount: number;
}

interface TagX { tag: number; valuesPerEntry: number; mask: number; eof: number }

function countBits(n: number): number {
  let c = 0;
  for (; n; n >>= 1) c += n & 1;
  return c;
}

function readCncx(records: Uint8Array[]): Map<number, string> {
  const out = new Map<number, string>();
  records.forEach((raw, r) => {
    let pos = 0;
    while (pos < raw.length) {
      const [length, consumed] = decint(raw.slice(pos));
      if (length > 0) out.set(r * 0x10000 + pos, dec.decode(raw.slice(pos + consumed, pos + consumed + length)));
      pos += consumed + length;
    }
  });
  return out;
}

function tagMap(controlByteCount: number, tagx: TagX[], data: Uint8Array): Map<number, number[]> {
  let control = Array.from(data.slice(0, controlByteCount));
  let rest = data.slice(controlByteCount);
  const parsed: { tag: number; count: number | null; bytes: number | null; per: number }[] = [];
  for (const x of tagx) {
    if (x.eof === 1) {
      control = control.slice(1);
      continue;
    }
    let value = control[0] & x.mask;
    if (value === 0) continue;
    if (value === x.mask) {
      if (countBits(x.mask) > 1) {
        const [bytes, consumed] = decint(rest);
        rest = rest.slice(consumed);
        parsed.push({ tag: x.tag, count: null, bytes, per: x.valuesPerEntry });
      } else {
        parsed.push({ tag: x.tag, count: 1, bytes: null, per: x.valuesPerEntry });
      }
    } else {
      let mask = x.mask;
      while ((mask & 1) === 0) {
        mask >>= 1;
        value >>= 1;
      }
      parsed.push({ tag: x.tag, count: value, bytes: null, per: x.valuesPerEntry });
    }
  }
  const out = new Map<number, number[]>();
  for (const p of parsed) {
    const values: number[] = [];
    if (p.count !== null) {
      for (let k = 0; k < p.count * p.per; k++) {
        const [v, consumed] = decint(rest);
        rest = rest.slice(consumed);
        values.push(v);
      }
    } else {
      let total = 0;
      while (total < p.bytes!) {
        const [v, consumed] = decint(rest);
        rest = rest.slice(consumed);
        total += consumed;
        values.push(v);
      }
      if (total !== p.bytes) throw new Error(`a tag should take ${p.bytes} bytes but took ${total}`);
    }
    out.set(p.tag, values);
  }
  if (rest.some((b) => b !== 0)) throw new Error('index bytes left over after the last tag');
  return out;
}

/** One index: its header record at `idx`, its entry records after it, then
 * its CNCX records (reader/index.py's read_index). */
export function readIndex(records: Uint8Array[], idx: number): IndexTable {
  const head = records[idx];
  if (ascii(head, 0, 4) !== 'INDX') throw new Error(`record ${idx} is not an INDX header`);
  const headerLength = u32(head, 4);
  if (headerLength !== 192) throw new Error(`INDX header length ${headerLength}`);
  const recordCount = u32(head, 24);
  const totalEntries = u32(head, 36);
  const cncxCount = u32(head, 52);
  const tagxAt = u32(head, 180);
  if (ascii(head, tagxAt, 4) !== 'TAGX') throw new Error('no TAGX where the header says');
  const tagxLength = u32(head, tagxAt + 4);
  const controlByteCount = u32(head, tagxAt + 8);
  const tagx: TagX[] = [];
  for (let i = tagxAt + 12; i < tagxAt + tagxLength; i += 4) {
    tagx.push({ tag: head[i], valuesPerEntry: head[i + 1], mask: head[i + 2], eof: head[i + 3] });
  }
  const cncx = readCncx(records.slice(idx + recordCount + 1, idx + recordCount + 1 + cncxCount));

  // The header's own geometry: each entry record's last name and count.
  const headerIdxt = u32(head, 20);
  if (ascii(head, headerIdxt, 4) !== 'IDXT') throw new Error('no IDXT in the INDX header where it says');

  const entries = new Map<string, Map<number, number[]>>();
  let seen = 0;
  for (let r = idx + 1; r <= idx + recordCount; r++) {
    const data = records[r];
    if (ascii(data, 0, 4) !== 'INDX') throw new Error(`record ${r} is not an INDX record`);
    const idxtPos = u32(data, 20);
    const count = u32(data, 24);
    if (ascii(data, idxtPos, 4) !== 'IDXT') throw new Error(`record ${r} has no IDXT where it says`);
    const positions = Array.from({ length: count }, (_, j) => u16(data, idxtPos + 4 + 2 * j));
    positions.push(idxtPos);
    for (let j = 0; j < count; j++) {
      const rec = data.slice(positions[j], positions[j + 1]);
      const nameLength = rec[0];
      const name = dec.decode(rec.slice(1, 1 + nameLength));
      if (entries.has(name)) throw new Error(`two index entries are named ${name}`);
      entries.set(name, tagMap(controlByteCount, tagx, rec.slice(1 + nameLength)));
    }
    // The header's geometry entry for this record: its last name and count.
    const geomAt = u16(head, headerIdxt + 4 + 2 * (r - idx - 1));
    const lastName = dec.decode(head.slice(geomAt + 1, geomAt + 1 + head[geomAt]));
    const lastCount = u16(head, geomAt + 1 + head[geomAt]);
    if (lastCount !== count) throw new Error(`the INDX header says record ${r} holds ${lastCount}, it holds ${count}`);
    if (lastName !== [...entries.keys()].at(-1)) throw new Error(`the INDX header names ${lastName} last in record ${r}`);
    seen += count;
  }
  if (seen !== totalEntries) throw new Error(`the INDX header says ${totalEntries} entries, the records hold ${seen}`);
  return { entries, cncx, recordCount, cncxCount };
}

// ── The whole book ──

export interface Kf8Book {
  records: Uint8Array[];
  header: {
    compression: number;
    textLength: number;
    textRecordCount: number;
    recordSize: number;
    headerLength: number;
    bookType: number;
    encoding: number;
    version: number;
    firstNonText: number;
    title: string;
    language: number;
    firstResource: number;
    exthFlags: number;
    fdst: number;
    fdstCount: number;
    fcis: number;
    flis: number;
    extraDataFlags: number;
    ncx: number;
    chunk: number;
    skel: number;
    datp: number;
    guide: number;
  };
  exth: { type: number; data: Uint8Array }[];
  /** The whole decompressed text: flow 0 then the CSS flows. */
  text: Uint8Array;
  flows: Uint8Array[];
  skel: { name: string; chunkCount: number; start: number; length: number }[];
  chunks: { insertPos: number; selector: string; file: number; seq: number; start: number; length: number }[];
  ncx: { name: string; offset: number; length: number; label: string; depth: number; pos: number; fid: number }[];
  guide: { type: string; title: string; pos: number; fid: number }[];
  /** Each file rebuilt from its skeleton and fragments, as a reader does. */
  parts: string[];
  /** Resource records, from the first resource to the last before FDST. */
  resources: Uint8Array[];
  /** What each text record's trailing data held: [multibyte overlap, TBS]. */
  trailing: { overlap: Uint8Array; tbs: Uint8Array | null }[];
}

export function exthValue(book: Kf8Book, type: number): Uint8Array[] {
  return book.exth.filter((r) => r.type === type).map((r) => r.data);
}
export const exthString = (book: Kf8Book, type: number) => exthValue(book, type).map((d) => dec.decode(d));
export const exthNumber = (book: Kf8Book, type: number) => exthValue(book, type).map((d) => u32(d, 0));

/** Split a text record into its data and its trailing entries
 * (utils.py's get_trailing_data). */
function trailingData(record: Uint8Array, flags: number) {
  let rec = record;
  let tbs: Uint8Array | null = null;
  let f = flags >> 1;
  let num = 0;
  while (f) {
    num++;
    if (f & 1) {
      const [size, consumed] = decint(rec, false);
      if (size < consumed || size > rec.length) throw new Error('a trailing entry larger than its record');
      if (num === 1) tbs = rec.slice(rec.length - size, rec.length - consumed);
      rec = rec.slice(0, rec.length - size);
    }
    f >>= 1;
  }
  let overlap = new Uint8Array(0);
  if (flags & 1) {
    const size = (rec[rec.length - 1] & 0b11) + 1;
    overlap = rec.slice(rec.length - size, rec.length - 1);
    rec = rec.slice(0, rec.length - size);
  }
  return { data: rec, overlap, tbs };
}

export function readKf8(file: Uint8Array): Kf8Book {
  const records = pdbRecords(file);
  const r0 = records[0];
  if (ascii(r0, 16, 4) !== 'MOBI') throw new Error('record 0 has no MOBI header');
  const headerLength = u32(r0, 20);
  const header: Kf8Book['header'] = {
    compression: u16(r0, 0),
    textLength: u32(r0, 4),
    textRecordCount: u16(r0, 8),
    recordSize: u16(r0, 10),
    headerLength,
    bookType: u32(r0, 24),
    encoding: u32(r0, 28),
    version: u32(r0, 36),
    firstNonText: u32(r0, 80),
    title: dec.decode(r0.slice(u32(r0, 84), u32(r0, 84) + u32(r0, 88))),
    language: u32(r0, 92),
    firstResource: u32(r0, 108),
    exthFlags: u32(r0, 128),
    fdst: u32(r0, 192),
    fdstCount: u32(r0, 196),
    fcis: u32(r0, 200),
    flis: u32(r0, 208),
    extraDataFlags: u32(r0, 240),
    ncx: u32(r0, 244),
    chunk: u32(r0, 248),
    skel: u32(r0, 252),
    datp: u32(r0, 256),
    guide: u32(r0, 260),
  };
  if (header.version !== 8) throw new Error(`MOBI version ${header.version}, not 8`);
  if (headerLength !== 264) throw new Error(`MOBI header length ${headerLength}, not 264`);
  if (header.encoding !== 65001) throw new Error('not UTF-8');

  // EXTH, straight after the MOBI header.
  if ((header.exthFlags & 0x40) === 0) throw new Error('the header says there is no EXTH');
  const exthAt = 16 + headerLength;
  if (ascii(r0, exthAt, 4) !== 'EXTH') throw new Error('no EXTH after the MOBI header');
  const exthLength = u32(r0, exthAt + 4);
  const exthCount = u32(r0, exthAt + 8);
  const exth: Kf8Book['exth'] = [];
  let p = exthAt + 12;
  for (let i = 0; i < exthCount; i++) {
    const size = u32(r0, p + 4);
    exth.push({ type: u32(r0, p), data: r0.slice(p + 8, p + size) });
    p += size;
  }
  if (p - exthAt !== exthLength) throw new Error(`EXTH says ${exthLength} bytes, its records walk ${p - exthAt}`);
  if (u32(r0, 84) < p) throw new Error('the title overlaps the EXTH block');

  // Text: decompress each record after cutting its trailing entries.
  const trailing: Kf8Book['trailing'] = [];
  const pieces: Uint8Array[] = [];
  for (let i = 1; i <= header.textRecordCount; i++) {
    const t = trailingData(records[i], header.extraDataFlags);
    trailing.push({ overlap: t.overlap, tbs: t.tbs });
    let data = t.data;
    if (header.compression === 2) data = palmdocDecompress(data);
    else if (header.compression !== 1) throw new Error(`compression ${header.compression}`);
    if (i < header.textRecordCount && data.length !== header.recordSize) {
      throw new Error(`text record ${i} decompresses to ${data.length} bytes, not ${header.recordSize}`);
    }
    pieces.push(data);
  }
  const text = new Uint8Array(pieces.reduce((n, x) => n + x.length, 0));
  pieces.reduce((at, x) => (text.set(x, at), at + x.length), 0);
  if (text.length !== header.textLength) throw new Error(`text is ${text.length} bytes, the header says ${header.textLength}`);

  // FDST: the flows.
  const fdst = records[header.fdst];
  if (ascii(fdst, 0, 4) !== 'FDST') throw new Error('no FDST where the header says');
  const flowCount = u32(fdst, 8);
  if (flowCount !== header.fdstCount) throw new Error('FDST and the header disagree on the flow count');
  const flows: Uint8Array[] = [];
  let expectStart = 0;
  for (let i = 0; i < flowCount; i++) {
    const start = u32(fdst, u32(fdst, 4) + i * 8);
    const end = u32(fdst, u32(fdst, 4) + i * 8 + 4);
    if (start !== expectStart || end < start) throw new Error(`flow ${i} runs ${start} to ${end}`);
    flows.push(text.slice(start, end));
    expectStart = end;
  }
  if (expectStart !== text.length) throw new Error('the flows do not cover the text');
  if (ascii(records[header.flis], 0, 4) !== 'FLIS') throw new Error('no FLIS where the header says');
  if (ascii(records[header.fcis], 0, 4) !== 'FCIS') throw new Error('no FCIS where the header says');
  const eof = records[records.length - 1];
  if (eof.length !== 4 || u32(eof, 0) !== 0xe98e0d0a) throw new Error('no EOF record at the end');

  // The indices.
  const skelTable = readIndex(records, header.skel);
  const skel = [...skelTable.entries].map(([name, tags]) => {
    const [c1, c2] = tags.get(1)!;
    const [s1, l1, s2, l2] = tags.get(6)!;
    if (c1 !== c2 || s1 !== s2 || l1 !== l2) throw new Error(`skeleton ${name}'s repeated values differ`);
    return { name, chunkCount: c1, start: s1, length: l1 };
  });
  const chunkTable = readIndex(records, header.chunk);
  const chunks = [...chunkTable.entries].map(([name, tags]) => ({
    insertPos: Number(name),
    selector: chunkTable.cncx.get(tags.get(2)![0])!,
    file: tags.get(3)![0],
    seq: tags.get(4)![0],
    start: tags.get(6)![0],
    length: tags.get(6)![1],
  }));
  let ncx: Kf8Book['ncx'] = [];
  if (header.ncx !== NULL_INDEX) {
    const t = readIndex(records, header.ncx);
    ncx = [...t.entries].map(([name, tags]) => ({
      name,
      offset: tags.get(1)![0],
      length: tags.get(2)![0],
      label: t.cncx.get(tags.get(3)![0])!,
      depth: tags.get(4)![0],
      pos: tags.get(6)![0],
      fid: tags.get(6)![1],
    }));
  }
  let guide: Kf8Book['guide'] = [];
  if (header.guide !== NULL_INDEX) {
    const t = readIndex(records, header.guide);
    guide = [...t.entries].map(([type, tags]) => ({
      type,
      title: t.cncx.get(tags.get(1)![0])!,
      pos: tags.get(6)![0],
      fid: tags.get(6)![1],
    }));
  }

  // Rebuild each file the way reader/mobi8.py's build_parts does, but
  // refuse a fragment whose insert point lands inside a tag.
  const flow0 = flows[0];
  const parts: string[] = [];
  let chunkAt = 0;
  for (const s of skel) {
    let skeleton = flow0.slice(s.start, s.start + s.length);
    let base = s.start + s.length;
    for (let i = 0; i < s.chunkCount; i++) {
      const c = chunks[chunkAt++];
      if (!c) throw new Error(`skeleton ${s.name} counts more fragments than the table has`);
      if (c.seq !== chunkAt - 1) throw new Error(`fragment ${chunkAt - 1} has sequence number ${c.seq}`);
      if (c.file !== skel.indexOf(s)) throw new Error(`fragment ${c.seq} names file ${c.file}`);
      const part = flow0.slice(base, base + c.length);
      const insert = c.insertPos - s.start;
      const head = dec.decode(skeleton.slice(0, insert));
      const tail = dec.decode(skeleton.slice(insert));
      if (head.lastIndexOf('>') < head.lastIndexOf('<') || (tail.indexOf('>') >= 0 && tail.indexOf('>') < tail.indexOf('<'))) {
        throw new Error(`fragment ${c.seq} would be inserted inside a tag`);
      }
      const next = new Uint8Array(skeleton.length + part.length);
      next.set(skeleton.slice(0, insert));
      next.set(part, insert);
      next.set(skeleton.slice(insert), insert + part.length);
      skeleton = next;
      base += c.length;
    }
    parts.push(dec.decode(skeleton));
  }
  if (chunkAt !== chunks.length) throw new Error(`${chunks.length - chunkAt} fragments belong to no skeleton`);
  const last = skel[skel.length - 1];
  const lastEnd = last ? last.start + last.length + chunks.filter((c) => c.file === skel.length - 1).reduce((n, c) => n + c.length, 0) : 0;
  if (lastEnd !== flow0.length) throw new Error(`the files end at ${lastEnd}, flow 0 at ${flow0.length}`);

  const resources = header.firstResource === NULL_INDEX ? [] : records.slice(header.firstResource, header.fdst);

  return { records, header, exth, text, flows, skel, chunks, ncx, guide, parts, resources, trailing };
}

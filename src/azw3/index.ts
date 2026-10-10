// The KF8 index records (INDX), their string tables (CNCX), and the number
// encodings both use.
//
// Ported from Calibre 9.11.0's src/calibre/ebooks/mobi/writer8/index.py and
// the helpers it takes from src/calibre/ebooks/mobi/utils.py (encint,
// to_base, CNCX, align_block, utf8_text). Calibre is GPL-3, Copyright Kovid
// Goyal; this port is part of Screepub, AGPL-3.0-or-later (see
// THIRD-PARTY-NOTICES.md).
//
// An index is a header record, then one or more records of entries, then
// the CNCX records holding its strings. Each entry is a short name, control
// bytes saying which tags it carries, and the tags' values as
// variable-width integers. A KF8 book carries four: the skeleton (one entry
// per XHTML file), the fragment or "chunk" table (one per piece of body cut
// out of a file), the guide, and the NCX (the table of contents).

import { ByteWriter, concatBytes } from '../mobi/palmdb';

const enc = new TextEncoder();

/** A variable-width integer: 7 bits per byte, big-endian. Forward (the
 * default) marks the LAST byte with the high bit, so a reader going forward
 * knows where it ends; backward marks the first, for a reader coming from
 * the end of a record (utils.py's encint). */
export function encint(value: number, forward = true): Uint8Array {
  if (value < 0 || !Number.isInteger(value)) throw new Error(`Cannot encode ${value} as a variable-width integer.`);
  const bytes: number[] = [];
  let v = value;
  do {
    bytes.push(v & 0x7f);
    v = Math.floor(v / 128);
  } while (v > 0);
  // Collected least significant first, so index 0 is the last byte once
  // reversed: the same order of operations as utils.py.
  bytes[forward ? 0 : bytes.length - 1] |= 0x80;
  return new Uint8Array(bytes.reverse());
}

const DIGITS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ';

/** `num` in `base`, upper case, left-padded with zeros to `minDigits`
 * (utils.py's to_base). Record references in KF8 are base 32. */
export function toBase(num: number, base = 32, minDigits?: number): string {
  let s = '';
  let n = Math.abs(num);
  do {
    s = DIGITS[n % base] + s;
    n = Math.floor(n / base);
  } while (n > 0);
  if (minDigits !== undefined && s.length < minDigits) s = '0'.repeat(minDigits - s.length) + s;
  return num < 0 ? `-${s}` : s;
}

/** Pad to a multiple of 4 with zero bytes (utils.py's align_block). */
export function alignBlock(raw: Uint8Array, multiple = 4): Uint8Array {
  const extra = raw.length % multiple;
  return extra === 0 ? raw : concatBytes([raw, new Uint8Array(multiple - extra)]);
}

/** A string as KF8 stores it: trimmed, NFC, UTF-8, never empty
 * (utils.py's utf8_text, which writes "Unknown" for an empty one). */
export function utf8Text(text: string): Uint8Array {
  const t = text.trim();
  return enc.encode(t ? t.normalize('NFC') : 'Unknown');
}

/** The strings an index points at, packed into records of at most 64 KiB
 * less kindlegen's 1024-byte margin. An offset is the string's place in its
 * record plus 0x10000 for each record before it. */
export class Cncx {
  readonly records: Uint8Array[] = [];
  private readonly offsets = new Map<string, number>();
  static readonly MAX_STRING_LENGTH = 500;

  constructor(strings: Iterable<string> = []) {
    const RECORD_LIMIT = 0x10000 - 1024;
    let buf: Uint8Array[] = [];
    let size = 0;
    let offset = 0;
    for (const s of strings) {
      if (this.offsets.has(s)) continue;
      const utf8 = utf8Text([...s].slice(0, Cncx.MAX_STRING_LENGTH).join(''));
      const raw = concatBytes([encint(utf8.length), utf8]);
      if (size + raw.length > RECORD_LIMIT) {
        this.records.push(alignBlock(concatBytes(buf)));
        buf = [];
        size = 0;
        offset = this.records.length * 0x10000;
      }
      buf.push(raw);
      size += raw.length;
      this.offsets.set(s, offset);
      offset += raw.length;
    }
    if (size > 0) this.records.push(alignBlock(concatBytes(buf)));
  }

  offsetOf(s: string): number {
    const at = this.offsets.get(s);
    if (at === undefined) throw new Error(`The string table holds no "${s}".`);
    return at;
  }
}

/** One tag an index's entries may carry. `endFlag` entries separate control
 * bytes and carry nothing (index.py's TagMeta). */
export interface TagMeta {
  name: string;
  number: number;
  valuesPerEntry: number;
  bitmask: number;
  endFlag: 0 | 1;
}

const tag = (name: string, number: number, valuesPerEntry: number, bitmask: number): TagMeta =>
  ({ name, number, valuesPerEntry, bitmask, endFlag: 0 });
const END_TAG: TagMeta = { name: 'eof', number: 0, valuesPerEntry: 0, bitmask: 0, endFlag: 1 };

/** How far a value is shifted under its mask (index.py's mask_to_bit_shifts). */
function maskShift(mask: number): number {
  let shift = 0;
  while (mask && (mask & 1) === 0) {
    mask >>= 1;
    shift++;
  }
  return shift;
}

/** An entry's tag values: a number, or a list of numbers. */
export type TagValues = Record<string, number | readonly number[]>;
export interface IndexEntry {
  name: string | Uint8Array;
  tags: TagValues;
}

const HEADER_LENGTH = 192;

function valuesOf(v: number | readonly number[] | undefined): readonly number[] {
  if (v === undefined) return [];
  return typeof v === 'number' ? [v] : v;
}

/**
 * Every record of one index: the header, the entry records, then its CNCX
 * records (index.py's Index.__call__).
 */
export function buildIndex(
  tagTypes: readonly TagMeta[],
  entries: readonly IndexEntry[],
  cncx: Cncx = new Cncx(),
  controlByteCount = 1,
): Uint8Array[] {
  // Control bytes: for each tag, how many entries' worth of values it has,
  // shifted under its mask; an end tag closes one control byte.
  const controlBytes = entries.map((entry) => {
    const cbs: number[] = [];
    let ans = 0;
    for (const t of tagTypes) {
      if (t.endFlag === 1) {
        cbs.push(ans);
        ans = 0;
        continue;
      }
      const v = entry.tags[t.name];
      const nvals = v === undefined ? 0 : typeof v === 'number' ? 1 : v.length;
      ans |= t.bitmask & (Math.floor(nvals / t.valuesPerEntry) << maskShift(t.bitmask));
    }
    if (cbs.length !== controlByteCount) throw new Error(`The index entry ${String(entry.name)} is invalid.`);
    return cbs;
  });

  const RECORD_LIMIT = 0x10000 - HEADER_LENGTH - 1048; // kindlegen's margin
  const indexBlocks: Uint8Array[][] = [[]];
  const idxtBlocks: number[][] = [[]];
  const blockSizes = [0];
  const recordCounts = [0];
  const lastIndices: Uint8Array[] = [new Uint8Array(0)];

  entries.forEach((entry, i) => {
    const name = typeof entry.name === 'string' ? enc.encode(entry.name) : entry.name;
    const parts: Uint8Array[] = [new Uint8Array([name.length]), name, new Uint8Array(controlBytes[i])];
    for (const t of tagTypes) {
      for (const val of valuesOf(entry.tags[t.name])) parts.push(encint(val));
    }
    const raw = concatBytes(parts);
    let offset = blockSizes[blockSizes.length - 1];
    let idxtPos = idxtBlocks[idxtBlocks.length - 1].length * 2;
    if (offset + idxtPos + raw.length + 2 > RECORD_LIMIT) {
      indexBlocks.push([]);
      idxtBlocks.push([]);
      blockSizes.push(0);
      recordCounts.push(0);
      lastIndices.push(new Uint8Array(0));
      offset = idxtPos = 0;
    }
    const last = recordCounts.length - 1;
    recordCounts[last]++;
    idxtBlocks[last].push(HEADER_LENGTH + offset);
    indexBlocks[last].push(raw);
    blockSizes[last] += raw.length;
    lastIndices[last] = name;
  });

  const indexRecords = indexBlocks.map((blocks, r) => {
    const indexBlock = alignBlock(concatBytes(blocks));
    const idxt = new ByteWriter().ascii('IDXT');
    for (const p of idxtBlocks[r]) idxt.u16(p);
    const idxtBlock = alignBlock(idxt.toUint8Array());
    const head = new ByteWriter()
      .ascii('INDX')
      .u32(HEADER_LENGTH)
      .zeros(4)
      .u32(1) // header type: 1 for an index record
      .zeros(4)
      .u32(HEADER_LENGTH + indexBlock.length) // where IDXT starts
      .u32(recordCounts[r])
      .fill(8, 0xff)
      .zeros(156);
    const record = concatBytes([head.toUint8Array(), indexBlock, idxtBlock]);
    if (record.length > 0x10000) throw new Error('An index record outgrew 64 KiB.');
    return record;
  });

  // TAGX: each tag's number, values per entry, mask and end flag.
  const tagx = new ByteWriter().ascii('TAGX').u32(12 + tagTypes.length * 4).u32(controlByteCount);
  for (const t of tagTypes) tagx.u8(t.number).u8(t.valuesPerEntry).u8(t.bitmask).u8(t.endFlag);
  const tagxBytes = tagx.toUint8Array();

  // The header's own entries: each record's last name and entry count,
  // pointed at by the header's IDXT.
  const geometry = new ByteWriter();
  const headerIdxt = new ByteWriter().ascii('IDXT');
  let pos = HEADER_LENGTH + tagxBytes.length;
  lastIndices.forEach((name, r) => {
    const start = geometry.length;
    headerIdxt.u16(pos);
    geometry.u8(name.length).bytes(name).u16(recordCounts[r]);
    pos += geometry.length - start;
  });

  const tagxAligned = alignBlock(tagxBytes);
  const geometryAligned = alignBlock(geometry.toUint8Array());
  const idxtAligned = alignBlock(headerIdxt.toUint8Array());
  const header = new ByteWriter()
    .ascii('INDX')
    .u32(HEADER_LENGTH)
    .zeros(8)
    .u32(2) // index type, as Calibre writes it
    .u32(HEADER_LENGTH + tagxAligned.length + geometryAligned.length) // IDXT offset
    .u32(indexRecords.length)
    .u32(65001) // UTF-8
    .u32(0xffffffff)
    .u32(recordCounts.reduce((a, b) => a + b, 0))
    .u32(0) // ORDT offset
    .u32(0) // LIGT offset
    .u32(0) // ORDT/LIGT entries
    .u32(cncx.records.length)
    .zeros(124)
    .u32(HEADER_LENGTH) // TAGX offset
    .zeros(8)
    .bytes(tagxAligned)
    .bytes(geometryAligned)
    .bytes(idxtAligned);

  return [header.toUint8Array(), ...indexRecords, ...cncx.records];
}

// ── The four KF8 indices ──

export interface SkelEntry {
  fileNumber: number;
  chunkCount: number;
  startPos: number;
  length: number;
}

/** One entry per XHTML file: its fragment count and where its skeleton sits
 * in the text. Calibre: "Don't ask me why these entries have to be repeated
 * twice"; kindlegen writes them so. */
export function skelIndex(table: readonly SkelEntry[]): Uint8Array[] {
  const types = [tag('chunk_count', 1, 1, 3), tag('geometry', 6, 2, 12), END_TAG];
  return buildIndex(types, table.map((s) => ({
    name: `SKEL${String(s.fileNumber).padStart(10, '0')}`,
    tags: {
      chunk_count: [s.chunkCount, s.chunkCount],
      geometry: [s.startPos, s.length, s.startPos, s.length],
    },
  })));
}

export interface ChunkEntry {
  insertPos: number;
  selector: string;
  fileNumber: number;
  sequenceNumber: number;
  startPos: number;
  length: number;
}

/** One entry per fragment: where it goes into its skeleton, the selector
 * naming the element it belongs to, and where its bytes sit. */
export function chunkIndex(table: readonly ChunkEntry[]): Uint8Array[] {
  const types = [
    tag('cncx_offset', 2, 1, 1),
    tag('file_number', 3, 1, 2),
    tag('sequence_number', 4, 1, 4),
    tag('geometry', 6, 2, 8),
    END_TAG,
  ];
  const cncx = new Cncx(table.map((c) => c.selector));
  return buildIndex(types, table.map((c) => ({
    name: String(c.insertPos).padStart(10, '0'),
    tags: {
      cncx_offset: cncx.offsetOf(c.selector),
      file_number: c.fileNumber,
      sequence_number: c.sequenceNumber,
      geometry: [c.startPos, c.length],
    },
  })), cncx);
}

export interface GuideEntry {
  type: string;
  title: string;
  posFid: readonly [number, number];
}

/** The guide: named places (where reading starts, the title page). The
 * Kindle wants it sorted by type (main.py sorts before indexing). */
export function guideIndex(table: readonly GuideEntry[]): Uint8Array[] {
  const types = [tag('title', 1, 1, 1), tag('pos_fid', 6, 2, 2), END_TAG];
  const cncx = new Cncx(table.map((g) => g.title));
  return buildIndex(types, table.map((g) => ({
    name: g.type,
    tags: { title: cncx.offsetOf(g.title), pos_fid: [...g.posFid] },
  })), cncx);
}

export interface NcxEntry {
  index: number;
  offset: number;
  length: number;
  label: string;
  depth: number;
  parent?: number;
  firstChild?: number;
  lastChild?: number;
  posFid: readonly [number, number];
}

/** The table of contents. Entry names are the entry's index in hex, at
 * least two digits, all the same width. */
export function ncxIndex(table: readonly NcxEntry[]): Uint8Array[] {
  const types = [
    tag('offset', 1, 1, 1),
    tag('length', 2, 1, 2),
    tag('label', 3, 1, 4),
    tag('depth', 4, 1, 8),
    tag('parent', 21, 1, 16),
    tag('first_child', 22, 1, 32),
    tag('last_child', 23, 1, 64),
    tag('pos_fid', 6, 2, 128),
    END_TAG,
  ];
  const cncx = new Cncx(table.map((e) => e.label));
  const largest = table.reduce((m, e) => Math.max(m, e.index), 0);
  const width = Math.max(2, largest.toString(16).length);
  return buildIndex(types, table.map((e) => {
    const tags: TagValues = {
      offset: e.offset,
      length: e.length,
      label: cncx.offsetOf(e.label),
      depth: e.depth,
      pos_fid: [...e.posFid],
    };
    if (e.parent !== undefined) tags.parent = e.parent;
    if (e.firstChild !== undefined) tags.first_child = e.firstChild;
    if (e.lastChild !== undefined) tags.last_child = e.lastChild;
    return { name: e.index.toString(16).toUpperCase().padStart(width, '0'), tags };
  }), cncx);
}

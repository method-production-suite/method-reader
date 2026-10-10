// Screepub's own AZW3 (KF8) writer: the EPUB in, a Kindle book out, with no
// Calibre. BETA: tried in our tests and in Calibre's reader, not yet on a
// Kindle (docs/formatting-options-log.md, the in-house AZW3 entry).
//
// Ported from Calibre 9.11.0's src/calibre/ebooks/mobi/writer8/ (main.py's
// text records, FDST, indices and guide; mobi.py's record 0 and record
// layout; exth.py), with writer2/resources.py's resource records. Calibre
// is GPL-3, Copyright Kovid Goyal; this port is part of Screepub,
// AGPL-3.0-or-later (see THIRD-PARTY-NOTICES.md).
//
// The file, record by record:
//   0                 the PalmDOC header, the 264-byte MOBI header (version
//                     8), EXTH, the full title, 8192 bytes of padding
//   1..n              text records: 4096 bytes of text each, compressed,
//                     then trailing entries (UTF-8 overlap, TBS)
//   (pad)             zeros, when the text records do not end on 4 bytes
//   fragment INDX, skeleton INDX, guide INDX, NCX INDX, each with its CNCX
//   resources         the pictures, in manifest order
//   FDST, FLIS, FCIS, EOF
//
// Where this departs from Calibre, on purpose (each in the registry entry):
// no generated inline Table of Contents page; pictures pass through as they
// are (Calibre re-encodes them to JPEG); no thumbnail record; the
// stylesheet passes through as the EPUB has it (Calibre rewrites it); the
// landmarks' bodymatter becomes the guide's start-reading entry; a text run
// too long for one fragment is never cut inside an entity (chunk.ts).

import { ByteWriter, concatBytes, exthNumber, exthRecord, hash32, palmDatabase } from '../mobi/palmdb';
import { idKey, kf8Markup, type ResourceRef } from './chunk';
import { readEpub, splitFragment, type EpubBook, type EpubTocEntry } from './epub';
import { chunkIndex, guideIndex, ncxIndex, skelIndex, utf8Text, type GuideEntry, type NcxEntry } from './index';
import { palmdocCompress } from './palmdoc';
import { encodeTrailingData, trailingByteSequences } from './tbs';

const RECORD_SIZE = 4096;
const NULL_INDEX = 0xffffffff;
const enc = new TextEncoder();

/** Calibre's stand-in for a picture nothing shows (writer2/resources.py). */
const PLACEHOLDER_GIF = new Uint8Array([
  ...enc.encode('GIF89a'), 0x01, 0x00, 0x01, 0x00, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff,
  0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x21, 0xfe, 0x20,
  ...enc.encode('calibre-placeholder-gif-for-azw3'), 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
  0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
]);

const FLIS = new Uint8Array([
  ...enc.encode('FLIS'), 0, 0, 0, 8, 0, 0x41, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff,
  0, 1, 0, 3, 0, 0, 0, 3, 0, 0, 0, 1, 0xff, 0xff, 0xff, 0xff,
]);

function fcis(textLength: number): Uint8Array {
  return new ByteWriter()
    .ascii('FCIS').u32(0x14).u32(0x10).u32(0x02).u32(0)
    .u32(textLength)
    .u32(0).u32(0x28).u32(0).u32(0x28).u32(8).u16(1).u16(1).u32(0)
    .toUint8Array();
}

const EOF = new Uint8Array([0xe9, 0x8e, 0x0d, 0x0a]);

/** What a picture's bytes are, read from its first bytes (Calibre asks
 * imghdr the same question). */
export function imageMime(bytes: Uint8Array): string | null {
  if (bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47) return 'image/png';
  if (bytes[0] === 0xff && bytes[1] === 0xd8) return 'image/jpeg';
  if (bytes[0] === 0x47 && bytes[1] === 0x49 && bytes[2] === 0x46) return 'image/gif';
  return null;
}

/** The MOBI language field: Calibre's iana2mobi for English, the only
 * language our EPUB is written in. Anything else is 0, "neutral"; EXTH 524
 * still says the language. */
function languageCode(lang: string): number {
  return /^en(-|$)/i.test(lang) ? 9 : 0;
}

/** Cut the text into 4096-byte records. A record that ends inside a UTF-8
 * character carries the rest of that character after its data, so the
 * record decodes on its own (utils.py's create_text_record). */
function textRecords(text: Uint8Array): { data: Uint8Array; overlap: Uint8Array }[] {
  const out: { data: Uint8Array; overlap: Uint8Array }[] = [];
  for (let start = 0; start < text.length; start += RECORD_SIZE) {
    const end = Math.min(start + RECORD_SIZE, text.length);
    let overlap = new Uint8Array(0);
    if (end < text.length) {
      // Find the start of the last character in this record.
      let lead = end - 1;
      while (lead > start && (text[lead] & 0xc0) === 0x80) lead--;
      const b = text[lead];
      const size = b >= 0xf0 ? 4 : b >= 0xe0 ? 3 : b >= 0xc0 ? 2 : 1;
      if (lead + size > end) overlap = text.slice(end, lead + size);
    }
    out.push({ data: text.slice(start, end), overlap });
  }
  return out;
}

interface FlatToc {
  entry: EpubTocEntry;
  index: number;
  depth: number;
  parent?: number;
  children: number[];
}

/** The TOC depth first, as Calibre's toc.iterdescendants() walks it. */
function flatten(toc: readonly EpubTocEntry[]): FlatToc[] {
  const out: FlatToc[] = [];
  const walk = (entries: readonly EpubTocEntry[], depth: number, parent?: number) => {
    for (const entry of entries) {
      const me: FlatToc = { entry, index: out.length, depth, parent, children: [] };
      out.push(me);
      if (parent !== undefined) out[parent].children.push(me.index);
      walk(entry.children, depth + 1, me.index);
    }
  };
  walk(toc, 0);
  return out;
}

/** Build the AZW3 for an EPUB already read. The book's documents are
 * changed in place. */
export function buildAzw3FromBook(book: EpubBook): Uint8Array {
  // ── Resources: every picture, numbered from 1 in manifest order. ──
  const resources = new Map<string, ResourceRef>();
  const resourceBytes: Uint8Array[] = [];
  for (const img of book.images) {
    const mime = imageMime(img.bytes);
    if (!mime) throw new Error(`The picture ${img.href} is not a PNG, JPEG or GIF.`);
    resourceBytes.push(img.bytes);
    resources.set(img.href, { index: resourceBytes.length, mime });
  }
  const coverOffset = book.coverHref !== undefined ? resources.get(book.coverHref)!.index - 1 : undefined;

  const markup = kf8Markup(book, resources);
  // A picture nothing shows becomes Calibre's placeholder; the cover always
  // counts as shown (resources.py's serialize).
  book.images.forEach((img, i) => {
    if (!markup.usedImages.has(img.href) && img.href !== book.coverHref) resourceBytes[i] = PLACEHOLDER_GIF;
  });

  // ── Text records ──
  const text = concatBytes(markup.flows);
  const records: Uint8Array[] = [new Uint8Array(0)]; // record 0 comes last
  const uncompressedLengths: number[] = [];
  let recordsSize = 0;
  for (const { data, overlap } of textRecords(text)) {
    uncompressedLengths.push(data.length);
    const rec = concatBytes([palmdocCompress(data), overlap, new Uint8Array([overlap.length])]);
    records.push(rec);
    recordsSize += rec.length;
  }
  const lastTextRecord = records.length - 1;

  // ── FDST: where each flow starts and ends in the text. ──
  const fdst = new ByteWriter().ascii('FDST').u32(12).u32(markup.flows.length);
  let flowStart = 0;
  for (const f of markup.flows) {
    fdst.u32(flowStart).u32(flowStart + f.length);
    flowStart += f.length;
  }

  // ── NCX: the TOC, sorted by (depth, offset) as main.py does. ──
  const flow0Length = markup.flows[0].length;
  const firstChunk = markup.chunkTable[0];
  const flat = flatten(book.toc);
  const placed = flat.map((t) => {
    const [href, frag] = splitFragment(t.entry.href);
    const aid = markup.idMap.get(idKey(href, frag ?? '')) ?? markup.idMap.get(idKey(href, ''));
    const at = aid === undefined ? undefined : markup.aidOffsets.get(aid);
    const [pos, fid, offset] = at ?? [0, 0, firstChunk ? firstChunk.insertPos : 0];
    return { ...t, pos, fid, offset };
  });
  placed.sort((a, b) => a.depth - b.depth || a.offset - b.offset);
  const newIndex = new Map(placed.map((p, i) => [p.index, i]));
  const ncx: NcxEntry[] = placed.map((p, i) => {
    const enders = placed.filter((e) => e.depth <= p.depth && e.offset > p.offset).map((e) => e.offset);
    const next = enders.length > 0 ? Math.min(...enders) : flow0Length;
    const entry: NcxEntry = {
      index: i,
      offset: p.offset,
      length: next - p.offset,
      label: p.entry.title || 'Unknown',
      depth: p.depth,
      posFid: [p.pos, p.fid],
    };
    if (p.children.length > 0) {
      entry.firstChild = newIndex.get(p.children[0]);
      entry.lastChild = newIndex.get(p.children[p.children.length - 1]);
    }
    if (p.parent !== undefined) entry.parent = newIndex.get(p.parent);
    return entry;
  });

  // Trailing bytes: the UTF-8 overlap is already on; the TBS go after it,
  // only in a book with a TOC.
  const hasTbs = ncx.length > 0;
  if (hasTbs) {
    const tbs = trailingByteSequences(
      ncx.map((e) => ({ index: e.index, start: e.offset, length: e.length, depth: e.depth, parent: e.parent })),
      uncompressedLengths,
    );
    tbs.forEach((bytes, i) => {
      records[i + 1] = concatBytes([records[i + 1], encodeTrailingData(bytes)]);
    });
  }
  // Calibre's pad record: (size % 4) zero bytes, from the text records'
  // size before their TBS, exactly as main.py computes it.
  let firstNonText = lastTextRecord + 1;
  if (recordsSize % 4 !== 0) {
    records.push(new Uint8Array(recordsSize % 4));
    firstNonText++;
  }

  // ── Guide: where reading starts. ──
  let startOffset: number | undefined;
  const guide: GuideEntry[] = [];
  for (const mark of book.landmarks) {
    if (mark.type !== 'bodymatter') continue;
    const [href, frag] = splitFragment(mark.href);
    const aid = markup.idMap.get(idKey(href, frag ?? '')) ?? markup.idMap.get(idKey(href, ''));
    const at = aid === undefined ? undefined : markup.aidOffsets.get(aid);
    if (!at) continue;
    startOffset = at[2];
    guide.push({ type: 'text', title: mark.title || 'Begin Reading', posFid: [at[0], at[1]] });
  }
  guide.sort((a, b) => (a.type < b.type ? -1 : a.type > b.type ? 1 : 0));

  // ── Indices, in Calibre's order. ──
  const chunkAt = records.length;
  records.push(...chunkIndex(markup.chunkTable));
  const skelAt = records.length;
  records.push(...skelIndex(markup.skelTable));
  let guideAt = NULL_INDEX;
  if (guide.length > 0) {
    guideAt = records.length;
    records.push(...guideIndex(guide));
  }
  let ncxAt = NULL_INDEX;
  if (ncx.length > 0) {
    ncxAt = records.length;
    records.push(...ncxIndex(ncx));
  }

  // ── Resources, FDST, FLIS, FCIS, EOF. ──
  const firstResource = resourceBytes.length > 0 ? records.length : NULL_INDEX;
  records.push(...resourceBytes);
  const fdstAt = records.length;
  records.push(fdst.toUint8Array());
  const flisAt = records.length;
  records.push(FLIS);
  const fcisAt = records.length;
  records.push(fcis(text.length));
  records.push(EOF);

  // ── EXTH ──
  const id = (book.identifier ?? '').split(':').at(-1) || String(hash32(book.title));
  const date = book.modified ?? new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
  const exth: ByteWriter[] = [];
  for (const author of book.authors) exth.push(exthRecord(100, utf8Text(author)));
  exth.push(exthRecord(503, utf8Text(book.title)));
  exth.push(exthRecord(524, utf8Text(book.language)));
  exth.push(exthRecord(113, enc.encode(id))); // ASIN: the book's own id, as Calibre writes its uuid
  exth.push(exthRecord(501, enc.encode('EBOK')));
  exth.push(exthRecord(106, enc.encode(date)));
  // Calibre's be_kindlegen2: kindlegen 2.9's creator fields, the Linux build
  // (201), so the file is the same whatever system wrote it.
  exth.push(exthNumber(204, 201), exthNumber(205, 2), exthNumber(206, 9), exthNumber(207, 0));
  exth.push(exthRecord(535, enc.encode('0730-890adc2')));
  if (coverOffset !== undefined) exth.push(exthNumber(201, coverOffset), exthNumber(203, 0));
  if (startOffset !== undefined) exth.push(exthNumber(116, startOffset));
  exth.push(exthNumber(125, resourceBytes.length));
  if (resourceBytes.length > 0) exth.push(exthNumber(131, 0));
  exth.push(exthRecord(528, enc.encode('true')));
  const exthBody = concatBytes(exth.map((r) => r.toUint8Array()));
  const exthPad = 4 - (exthBody.length % 4); // always at least one byte
  const exthBlock = new ByteWriter()
    .ascii('EXTH').u32(exthBody.length + 12).u32(exth.length)
    .bytes(exthBody).zeros(exthPad)
    .toUint8Array();

  // ── Record 0 ──
  const fullTitle = utf8Text(book.title);
  const uid = hash32(`${book.identifier ?? ''} ${book.title} ${text.length}`);
  const r0 = new ByteWriter()
    .u16(2) // PalmDOC compression
    .zeros(2)
    .u32(text.length)
    .u16(lastTextRecord)
    .u16(RECORD_SIZE)
    .u16(0) // no encryption
    .u16(0)
    .ascii('MOBI')
    .u32(264) // header length
    .u32(2) // a book
    .u32(65001) // UTF-8
    .u32(uid)
    .u32(8) // file version: KF8
    .u32(NULL_INDEX) // 40 meta orth
    .u32(NULL_INDEX) // 44 meta infl
    .fill(32, 0xff) // 48..80 extra indices
    .u32(firstNonText) // 80
    .u32(16 + 264 + exthBlock.length) // 84 full title offset
    .u32(fullTitle.length) // 88
    .u32(languageCode(book.language)) // 92
    .u32(0) // 96 input language
    .u32(0) // 100 output language
    .u32(8) // 104 min version
    .u32(firstResource) // 108
    .zeros(16) // 112..128 huffman
    .u32(0b1010000) // 128 EXTH flags, as Calibre writes them
    .zeros(32) // 132
    .u32(NULL_INDEX) // 164
    .u32(NULL_INDEX) // 168 DRM offset
    .u32(0).u32(0).u32(0) // 172..184 DRM count, size, flags
    .zeros(8) // 184
    .u32(fdstAt) // 192
    .u32(markup.flows.length) // 196
    .u32(fcisAt) // 200
    .u32(1)
    .u32(flisAt) // 208
    .u32(1)
    .zeros(8) // 216
    .u32(NULL_INDEX) // 224 SRCS
    .u32(0)
    .fill(8, 0xff) // 232
    .u32(hasTbs ? 0b11 : 0b1) // 240 extra data flags: UTF-8 overlap, and TBS
    .u32(ncxAt) // 244
    .u32(chunkAt) // 248
    .u32(skelAt) // 252
    .u32(NULL_INDEX) // 256 DATP
    .u32(guideAt) // 260
    .fill(4, 0xff).zeros(4).fill(4, 0xff).zeros(4) // 264..280
    .bytes(exthBlock)
    .bytes(fullTitle)
    .zeros(8192)
    .toUint8Array();
  records[0] = r0;

  return palmDatabase(book.title, uid, records);
}

/** Build the AZW3 for an EPUB's bytes. */
export async function buildAzw3(epub: Uint8Array): Promise<Uint8Array> {
  return buildAzw3FromBook(await readEpub(epub));
}

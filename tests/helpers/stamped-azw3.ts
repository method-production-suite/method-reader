// The smallest file the AZW3 staleness rule reads as one of ours: a PalmDB
// whose record 0 is a MOBI header with an EXTH block carrying the writer
// revision (src/azw3/revision.ts). Not a book; a stand-in for tests that
// need a "current" .azw3 beside an EPUB without building one. Pass
// `revision: null` for the same file with no revision record at all, which
// is what Calibre's AZW3, and ours from before the stamp, look like.
import { AZW3_REVISION_EXTH, AZW3_WRITER_REVISION } from '../../src/azw3/revision';
import { ByteWriter, exthNumber, exthRecord, palmDatabase } from '../../src/mobi/palmdb';

// One file per revision, made once: the PalmDB header carries the time it
// was made, and a test compares a reused file with this stand-in byte for
// byte.
const made = new Map<number | null, Uint8Array<ArrayBuffer>>();

export function stampedAzw3(revision: number | null = AZW3_WRITER_REVISION): Uint8Array<ArrayBuffer> {
  let file = made.get(revision);
  if (!file) made.set(revision, (file = build(revision)));
  return file;
}

function build(revision: number | null): Uint8Array<ArrayBuffer> {
  const records = [exthRecord(503, new TextEncoder().encode('T'))];
  if (revision !== null) records.push(exthNumber(AZW3_REVISION_EXTH, revision));
  const body = records.map((r) => r.toUint8Array());
  const size = body.reduce((n, b) => n + b.length, 0);
  const exth = new ByteWriter().ascii('EXTH').u32(size + 12).u32(records.length);
  for (const b of body) exth.bytes(b);
  exth.zeros(4 - (size % 4));
  const r0 = new ByteWriter()
    .u16(2).zeros(2).u32(0).u16(0).u16(4096).zeros(4)
    .ascii('MOBI').u32(264).u32(2).u32(65001).u32(1).u32(8)
    .zeros(128 - 40).u32(0x50).zeros(264 - (132 - 16))
    .bytes(exth.toUint8Array());
  return new Uint8Array(palmDatabase('T', 1, [r0.toUint8Array()]));
}

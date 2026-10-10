// Which version of the AZW3 writer made a file. Every AZW3 we write carries
// AZW3_WRITER_REVISION in an EXTH record of its own, and the Kindle
// ladder's staleness rule (export/artifact.ts's rungPlan) counts an .azw3
// as current only when it is newer than its EPUB AND carries this revision
// or a later one. So a fixed writer reaches the files already beside the
// books: the next export rebuilds them. A file with no revision at all is
// Calibre's AZW3 (the ladder's before Screepub wrote its own) or ours from
// before the stamp, and is rebuilt too.

import { closeSync, openSync, readSync } from 'node:fs';

/** BUMP THIS whenever the writer's output changes in a way a device could
 * notice: what the book shows, where it opens, how it navigates, whether
 * it opens at all. Every .azw3 written by an earlier revision is then
 * rebuilt on its next export. A change that leaves the bytes as they were
 * (a refactor, a comment) does not bump it. */
export const AZW3_WRITER_REVISION = 1;

/** The EXTH record that holds it, as one big-endian 32-bit number. No
 * reader acts on it: Amazon's and Calibre's numbers run 1-4, 100-209, 300,
 * 401-453 and 501-544 (KindleUnpack's table of every one it has seen), and
 * Calibre's reader and KindleUnpack both skip a number they do not know.
 * 0x5350 is "SP", for Screepub, far from all of them. */
export const AZW3_REVISION_EXTH = 0x5350;

function u32(b: Uint8Array, at: number): number {
  return ((b[at] << 24) | (b[at + 1] << 16) | (b[at + 2] << 8) | b[at + 3]) >>> 0;
}

/** The writer revision in an AZW3's bytes (only the first record is read,
 * so the head of the file is enough), or null when it has none or is not a
 * MOBI-family file at all. */
export function azw3Revision(file: Uint8Array): number | null {
  if (file.length < 86) return null;
  const r0 = u32(file, 78);
  const has = (at: number, n: number) => at >= 0 && at + n <= file.length;
  if (!has(r0, 132)) return null;
  if (String.fromCharCode(...file.subarray(r0 + 16, r0 + 20)) !== 'MOBI') return null;
  if ((u32(file, r0 + 128) & 0x40) === 0) return null;
  const exth = r0 + 16 + u32(file, r0 + 20);
  if (!has(exth, 12) || String.fromCharCode(...file.subarray(exth, exth + 4)) !== 'EXTH') return null;
  const count = u32(file, exth + 8);
  let at = exth + 12;
  let found: number | null = null;
  for (let i = 0; i < count; i++) {
    if (!has(at, 8)) return null;
    const type = u32(file, at);
    const size = u32(file, at + 4);
    if (size < 8 || !has(at, size)) return null;
    if (type === AZW3_REVISION_EXTH && size === 12) found = u32(file, at + 8);
    at += size;
  }
  return found;
}

/** The writer revision of the .azw3 at `path`, or null when it has none or
 * cannot be read. Reads the PalmDB header to find record 0, then record 0
 * and no further: never the text or the pictures. */
export function azw3RevisionOfFile(path: string): number | null {
  let fd: number | undefined;
  try {
    fd = openSync(path, 'r');
    const at = new Uint8Array(4);
    if (readSync(fd, at, 0, 4, 78) < 4) return null;
    // Record 0: 16 + 264 header bytes, an EXTH block of well under a
    // kilobyte, the title and 8192 bytes of padding. 32 KiB past its start
    // holds all of it.
    // A record list long enough to push record 0 past 1 MiB is no book of
    // ours (that is over 130,000 records), and a garbage offset must not
    // decide how much memory this asks for.
    const r0 = u32(at, 0);
    if (r0 > 1024 * 1024) return null;
    const head = new Uint8Array(r0 + 32 * 1024);
    const n = readSync(fd, head, 0, head.length, 0);
    return azw3Revision(head.subarray(0, n));
  } catch {
    return null;
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
}

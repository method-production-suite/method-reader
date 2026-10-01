// tools/mobi-picture-probe.ts builds the books a device pass copies to an
// older Kindle to learn how large a picture its .mobi may carry (registry
// #19a, torture manifest row 19a): one with all three pictures, and one per
// picture. Run here as a real process, the way the pass runs it, and judged
// by the files it writes.
import { afterAll, beforeAll, describe, expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import JSZip from 'jszip';
import { isPng } from '../src/parser/png';
import { mobiPictures } from './helpers/mobi-records';
import {
  EPUB_FILE,
  MOBI_FILE,
  plainSize,
  probePictures,
  singleMobiFile,
} from '../tools/mobi-picture-probe';

/** The single-picture books' files, smallest first, spelled out rather than
 * derived: these are the names the device check tells a person to copy. */
const SINGLE_FILES = [
  'screepub-picture-probe-2kb.mobi',
  'screepub-picture-probe-200kb.mobi',
  'screepub-picture-probe-1400kb.mobi',
];

const ROOT = join(import.meta.dir, '..');
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-mobi-picture-probe-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

interface Run {
  exitCode: number;
  stdout: string;
  stderr: string;
  mobi: Uint8Array;
  epub: Uint8Array;
  /** The single-picture books, in SINGLE_FILES order. */
  singles: Uint8Array[];
}

async function runProbe(out: string): Promise<Run> {
  const proc = Bun.spawn(['bun', join(ROOT, 'tools', 'mobi-picture-probe.ts'), '--out', out], {
    cwd: ROOT,
    env: process.env,
    stdout: 'pipe',
    stderr: 'pipe',
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return {
    exitCode,
    stdout,
    stderr,
    mobi: new Uint8Array(readFileSync(join(out, MOBI_FILE))),
    epub: new Uint8Array(readFileSync(join(out, EPUB_FILE))),
    singles: SINGLE_FILES.map((file) => new Uint8Array(readFileSync(join(out, file)))),
  };
}

/** The PDB name: the title, as the writer stores it in the first 32 bytes. */
const pdbName = (b: Uint8Array) => new TextDecoder().decode(b.slice(0, 32)).replace(/\0+$/, '');
/** The PDB header's unique id seed, which the writer hashes from the title. */
const uidSeed = (b: Uint8Array) => new DataView(b.buffer, b.byteOffset, b.byteLength).getUint32(68);

/** The .mobi's text records joined: the markup, read the way a reader reads
 * it, from the PDB's record list and the PalmDOC header's record count. */
function mobiMarkup(b: Uint8Array): string {
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const count = dv.getUint16(76);
  const offsets = Array.from({ length: count }, (_, i) => dv.getUint32(78 + i * 8));
  const textRecords = dv.getUint16(offsets[0] + 8);
  return new TextDecoder().decode(b.slice(offsets[1], offsets[1 + textRecords]));
}

describe('the picture probe', () => {
  let run: Run;
  beforeAll(async () => {
    run = await runProbe(join(SCRATCH, 'first'));
  }, 60000);

  test('writes the .mobi, the .epub and a .mobi per size, and prints the files and the three sizes', () => {
    expect(run.exitCode, run.stderr).toBe(0);
    expect(run.stdout).toContain(MOBI_FILE);
    expect(run.stdout).toContain(EPUB_FILE);
    for (const file of SINGLE_FILES) expect(run.stdout).toContain(file);
    expect(probePictures().map(singleMobiFile)).toEqual(SINGLE_FILES);
    for (const size of ['about 2 KB', 'about 200 KB', 'about 1.4 MB']) expect(run.stdout).toContain(size);
  });

  // If one record too large makes the Kindle refuse the whole book, the
  // combined book can only say that something failed. Each size alone in a
  // book of its own still answers.
  test('each single book holds its one picture, the same bytes as the combined book’s', () => {
    const combined = mobiPictures(run.mobi);
    run.singles.forEach((book, i) => {
      expect(mobiPictures(book), SINGLE_FILES[i]).toEqual([combined[i]]);
      const html = mobiMarkup(book);
      const line = html.indexOf(`<p>The next picture is about ${plainSize(combined[i].length)}.`);
      const img = html.indexOf('<img recindex="00001"');
      expect(line, SINGLE_FILES[i]).toBeGreaterThanOrEqual(0);
      expect(img, SINGLE_FILES[i]).toBeGreaterThan(line);
    });
    // The plate is a plate in its own book too.
    expect(mobiMarkup(run.singles[2])).toContain(
      '<mbp:pagebreak/>\n<p align="center"><img recindex="00001" alt="Illustration"/></p>\n<mbp:pagebreak/>',
    );
  });

  test('the four books carry four titles and four ids, so a library tells them apart', () => {
    const books = [run.mobi, ...run.singles];
    expect(books.map(pdbName)).toEqual([
      'Picture Probe', 'Picture Probe 2 KB', 'Picture Probe 200 KB', 'Picture Probe 1.4 MB',
    ]);
    expect(new Set(books.map(uidSeed)).size).toBe(4);
  });

  test('the pictures land where the device check needs them: tiny, past the 127 KB cap, and a real plate’s size', () => {
    const sizes = mobiPictures(run.mobi).map((p) => p.length);
    expect(sizes).toHaveLength(3);
    // About 2 KB, about 200 KB and about 1.4 MB, held to a range rather than
    // a byte, since a different zlib could move each a little. The middle
    // one's floor is the point of it: past the cap with room to spare, in
    // binary kilobytes as well as decimal.
    expect(sizes[0]).toBeGreaterThanOrEqual(1_500);
    expect(sizes[0]).toBeLessThan(2_500);
    expect(sizes[1]).toBeGreaterThanOrEqual(150_000);
    expect(sizes[1]).toBeGreaterThan(127 * 1024);
    expect(sizes[1]).toBeLessThan(250_000);
    expect(sizes[2]).toBeGreaterThanOrEqual(1_200_000);
    expect(sizes[2]).toBeLessThan(1_600_000);
  });

  test('the .mobi’s records hold the three PNGs in the order the book shows them, each the EPUB’s own', async () => {
    const records = mobiPictures(run.mobi);
    for (const picture of records) expect(isPng(picture)).toBe(true);
    const zip = await JSZip.loadAsync(run.epub);
    const shipped = await Promise.all(
      ['p001-01.png', 'p002-01.png', 'p003-01.png'].map((n) => zip.file(`OEBPS/images/${n}`)!.async('uint8array')),
    );
    expect(records).toEqual(shipped);
  });

  test('each picture follows the line that names its size, and the largest is a full-page plate', () => {
    const html = mobiMarkup(run.mobi);
    const records = mobiPictures(run.mobi);
    let from = 0;
    records.forEach((picture, i) => {
      const line = html.indexOf(`<p>The next picture is about ${plainSize(picture.length)}.`, from);
      const img = html.indexOf(`<img recindex="0000${i + 1}"`, from);
      expect(line, `picture ${i + 1}`).toBeGreaterThanOrEqual(from);
      expect(img, `picture ${i + 1}`).toBeGreaterThan(line);
      from = img;
    });
    expect(html).toContain('<mbp:pagebreak/>\n<p align="center"><img recindex="00003" alt="Illustration"/></p>\n<mbp:pagebreak/>');
    expect(html).not.toMatch(/<img recindex="0000[12]"[^>]*><\/p>\n<mbp:pagebreak\/>/);
  });

  test('every run writes the same bytes, and the pictures are the ones the tool draws', async () => {
    const again = await runProbe(join(SCRATCH, 'second'));
    expect(again.mobi).toEqual(run.mobi);
    expect(again.epub).toEqual(run.epub);
    expect(again.singles).toEqual(run.singles);
    expect(mobiPictures(run.mobi)).toEqual(probePictures().map((p) => p.bytes));
  }, 60000);

  const EPUBCHECK = Bun.which('epubcheck');
  const checked = EPUBCHECK ? test : test.skip;
  const SKIPPED = EPUBCHECK ? '' : ' (skipped: epubcheck is not on PATH; brew install epubcheck)';

  checked(`the .epub validates clean${SKIPPED}`, () => {
    const check = Bun.spawnSync([EPUBCHECK!, join(SCRATCH, 'first', EPUB_FILE)], { stdout: 'pipe', stderr: 'pipe' });
    const out = `${check.stdout.toString()}\n${check.stderr.toString()}`;
    expect(check.exitCode, out).toBe(0);
    expect(out).toContain('No errors or warnings detected');
  }, 120000);
});

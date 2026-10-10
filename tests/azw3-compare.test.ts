// Our AZW3 beside Calibre's AZW3 of the same EPUB: their structure, and how
// long each takes to build. Calibre's is made with the recipe the Kindle
// ladder used before our writer (ebook-convert with CALIBRE_FORMAT_GUARDS),
// under bun test's Calibre isolation. Runs only where Calibre is installed;
// the largest real script joins in where root /fixtures/ exists (or
// SCREEPUB_REAL_FIXTURES names it). Prints a comparison; asserts only what
// must hold: our structural reader opens BOTH files, and the two tables of
// contents agree. Counts and times only, never a script's name.
import { afterAll, expect, test } from 'bun:test';
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildAzw3 } from '../src/azw3/writer';
import { convertPdf } from '../src/convert';
import { CALIBRE_FORMAT_GUARDS } from '../src/export/calibre';
import { ebookConvert, runCalibreTool } from './helpers/calibre-read';
import { fixtureEpub } from './helpers/fixture-epubs';
import { readKf8, type Kf8Book } from './helpers/kf8-reader';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-azw3-compare-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));
const PRIVATE = process.env.SCREEPUB_REAL_FIXTURES ?? new URL('../fixtures/', import.meta.url).pathname;

const withCalibre = ebookConvert ? test : test.skip;

function shape(b: Kf8Book) {
  return {
    records: b.records.length,
    textRecords: b.header.textRecordCount,
    textBytes: b.header.textLength,
    flows: b.flows.length,
    skeletons: b.skel.length,
    fragments: b.chunks.length,
    ncx: b.ncx.length,
    guide: b.guide.map((g) => g.type).join(','),
    resources: b.resources.length,
    exth: b.exth.length,
  };
}

async function compare(label: string, epub: Uint8Array) {
  const dir = mkdtempSync(join(SCRATCH, 'book-'));
  const epubPath = join(dir, 'book.epub');
  writeFileSync(epubPath, epub);

  const t0 = performance.now();
  const ours = await buildAzw3(epub);
  const oursMs = performance.now() - t0;

  const theirsPath = join(dir, 'calibre.azw3');
  const t1 = performance.now();
  const run = runCalibreTool(ebookConvert!, [epubPath, theirsPath, ...CALIBRE_FORMAT_GUARDS], dir);
  const theirsMs = performance.now() - t1;
  expect(run.code, run.out).toBe(0);
  const theirs = new Uint8Array(readFileSync(theirsPath));

  const a = readKf8(ours);
  const b = readKf8(theirs);
  console.log(`AZW3 ${label}: ours ${oursMs.toFixed(0)} ms, ${ours.length} bytes; Calibre ${theirsMs.toFixed(0)} ms, ${theirs.length} bytes`);
  console.log(`  ours   ${JSON.stringify(shape(a))}`);
  console.log(`  calibre ${JSON.stringify(shape(b))}`);
  // The same table of contents, and Calibre's one more file is its
  // generated inline Table of Contents page.
  expect(a.ncx.map((n) => n.label)).toEqual(b.ncx.map((n) => n.label));
  expect(a.skel.length).toBe(b.skel.length - 1);
}

withCalibre('field-station: ours beside Calibre\'s', async () => {
  await compare('field-station', await fixtureEpub('field-station'));
}, 180_000);

withCalibre('images: ours beside Calibre\'s', async () => {
  await compare('images', await fixtureEpub('images'));
}, 180_000);

const largest = existsSync(PRIVATE)
  ? readdirSync(PRIVATE).filter((f) => f.toLowerCase().endsWith('.pdf'))
    .sort((x, y) => statSync(join(PRIVATE, y)).size - statSync(join(PRIVATE, x)).size)
  : [];

(ebookConvert && largest.length > 0 ? test : test.skip)('the largest real script that converts: ours beside Calibre\'s', async () => {
  for (const file of largest) {
    let epub: Uint8Array;
    try {
      epub = (await convertPdf(new Uint8Array(readFileSync(join(PRIVATE, file))))).epub;
    } catch {
      continue; // refused by a guard; take the next largest
    }
    await compare('largest real script', epub);
    return;
  }
}, 600_000);

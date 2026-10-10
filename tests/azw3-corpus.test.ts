// The real scripts in root /fixtures/ (gitignored, never committed), each
// converted to AZW3 in scratch and read back: by the structural reader
// always, and by Calibre's reader where Calibre is installed (under bun
// test's isolation, tests/helpers/calibre-read.ts). Runs only when the
// folder exists. Reports COUNTS ONLY: no title, author or character name may
// leave this file. SCREEPUB_REAL_FIXTURES points at the folder from a
// worktree, whose root has none of its own.
import { afterAll, expect, test } from 'bun:test';
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { convertPdf, NotAScreenplayError, ScannedPdfError } from '../src/convert';
import { buildAzw3 } from '../src/azw3/writer';
import { calibreDebug, calibreToEpub, ebookConvert, viewOfCalibreEpub, viewOfOurEpub } from './helpers/calibre-read';
import { readKf8 } from './helpers/kf8-reader';

const PRIVATE = process.env.SCREEPUB_REAL_FIXTURES ?? new URL('../fixtures/', import.meta.url).pathname;
const pdfs = existsSync(PRIVATE) ? readdirSync(PRIVATE).filter((f) => f.toLowerCase().endsWith('.pdf')).sort() : [];
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-azw3-corpus-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const withCorpus = pdfs.length > 0 ? test : test.skip;

withCorpus('every real script makes an AZW3 that reads back whole', async () => {
  const counts = { scripts: pdfs.length, refused: 0, built: 0, structural: 0, calibre: 0, calibreSkipped: 0, pictures: 0 };
  const failures: string[] = [];
  for (const [i, file] of pdfs.entries()) {
    let epub: Uint8Array;
    try {
      epub = (await convertPdf(new Uint8Array(readFileSync(join(PRIVATE, file))))).epub;
    } catch (e) {
      if (e instanceof ScannedPdfError || e instanceof NotAScreenplayError) {
        counts.refused++;
        continue;
      }
      throw e;
    }
    // Numbered, never named.
    const label = `script ${i + 1}`;
    const azw3 = await buildAzw3(epub);
    counts.built++;
    try {
      const book = readKf8(azw3);
      counts.structural++;
      counts.pictures += book.resources.length;
    } catch (e) {
      failures.push(`${label}: structural reader: ${(e as Error).message}`);
      continue;
    }
    if (!ebookConvert || !calibreDebug) {
      counts.calibreSkipped++;
      continue;
    }
    const dir = mkdtempSync(join(SCRATCH, 'book-'));
    const path = join(dir, 'book.azw3');
    writeFileSync(path, azw3);
    const read = calibreToEpub(path);
    if (!read.epub) {
      failures.push(`${label}: Calibre could not read it`);
      continue;
    }
    const ours = await viewOfOurEpub(epub);
    const theirs = await viewOfCalibreEpub(read.epub);
    const same = theirs.text === ours.text && theirs.toc.join('\n') === ours.toc.join('\n')
      && theirs.blocks.length === ours.blocks.length && theirs.images.length === ours.images.length;
    if (same) counts.calibre++;
    else failures.push(`${label}: Calibre read it back different (text ${theirs.text === ours.text}, toc ${theirs.toc.length}/${ours.toc.length}, blocks ${theirs.blocks.length}/${ours.blocks.length}, pictures ${theirs.images.length}/${ours.images.length})`);
  }
  console.log('AZW3 corpus:', JSON.stringify(counts));
  expect(failures).toEqual([]);
}, 900_000);

// Our AZW3, read by Calibre's own KF8 reader: the strongest check there is
// short of a Kindle. Runs only where Calibre is installed (skips in CI),
// and only here, under bun test, where tests/isolate-calibre-config.ts keeps
// Calibre away from its real settings folder (tests/helpers/calibre-read.ts
// passes process.env to every spawn).
import { afterAll, describe, expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildAzw3 } from '../src/azw3/writer';
import { calibreDebug, calibreToEpub, ebookConvert, inspectMobi, viewOfCalibreEpub, viewOfOurEpub } from './helpers/calibre-read';
import { fixtureEpub, type FixtureName } from './helpers/fixture-epubs';
import { readKf8 } from './helpers/kf8-reader';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-azw3-calibre-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const withCalibre = ebookConvert && calibreDebug ? test : test.skip;

describe("Calibre's reader opens our AZW3", () => {
  for (const name of ['field-station', 'images', 'torture', 'screenplay'] as FixtureName[]) {
    withCalibre(`${name}: the text, its order, the TOC, the metadata and the pictures come back`, async () => {
      const dir = mkdtempSync(join(SCRATCH, `${name}-`));
      const epub = await fixtureEpub(name);
      const azw3Bytes = await buildAzw3(epub);
      const azw3 = join(dir, `${name}.azw3`);
      writeFileSync(azw3, azw3Bytes);

      const read = calibreToEpub(azw3);
      expect(read.epub, read.out).not.toBeNull();
      // Calibre's reader warns, and carries on, where a file is off; a
      // clean file draws none of its KF8 warnings.
      expect(read.out).not.toMatch(/incorrect insert positions|Could not find|invalid|unprocessed index bytes/i);
      const ours = await viewOfOurEpub(epub);
      const theirs = await viewOfCalibreEpub(read.epub!);

      expect(theirs.title).toBe(ours.title);
      expect(theirs.authors).toEqual(ours.authors);
      expect(theirs.language).toBe(ours.language);
      expect(theirs.toc).toEqual(ours.toc);
      expect(theirs.blocks).toEqual(ours.blocks);
      expect(theirs.text).toBe(ours.text);
      // Every picture, byte for byte, and the same cover.
      const sorted = (a: Uint8Array[]) => a.map((b) => Buffer.from(b).toString('base64')).sort();
      expect(sorted(theirs.images)).toEqual(sorted(ours.images));
      expect(theirs.cover === null ? null : Buffer.from(theirs.cover).toString('base64'))
        .toBe(ours.cover === null ? null : Buffer.from(ours.cover).toString('base64'));

      // And its debugging inspector reads every index, with the counts our
      // structural reader finds.
      const inspected = inspectMobi(azw3);
      expect(inspected.code, inspected.out).toBe(0);
      const book = readKf8(azw3Bytes);
      expect(inspected.entries).toEqual({
        skel: book.skel.length,
        chunks: book.chunks.length,
        ncx: book.ncx.length,
        guide: book.guide.length,
      });
    }, 120_000);
  }
});

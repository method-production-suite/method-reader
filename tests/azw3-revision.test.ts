// The writer revision stamped into every AZW3 we write, and the staleness
// rule that reads it: an .azw3 with no revision (Calibre's, or ours from
// before the stamp) or an older one is rebuilt, so a fixed writer reaches
// the files already beside the books.
import { afterAll, describe, expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AZW3_REVISION_EXTH, AZW3_WRITER_REVISION, azw3Revision, azw3RevisionOfFile } from '../src/azw3/revision';
import { buildAzw3 } from '../src/azw3/writer';
import { buildMobi } from '../src/mobi/writer';
import { azw3Sibling, freshKindleArtifact, rungPlan } from '../src/export/artifact';
import { convertFountain } from '../src/convert';
import { DEFAULT_FORMAT_OPTIONS } from '../src/options';
import { exthNumber, readKf8 } from './helpers/kf8-reader';
import { fixtureEpub } from './helpers/fixture-epubs';
import { stampedAzw3 } from './helpers/stamped-azw3';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-azw3-revision-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

describe('the writer revision', () => {
  test('is a positive whole number, in an EXTH record no Kindle, Calibre or KindleUnpack reads', () => {
    expect(Number.isInteger(AZW3_WRITER_REVISION) && AZW3_WRITER_REVISION >= 1).toBe(true);
    // Amazon's and Calibre's numbers run 1-4, 100-209, 300, 401-453 and
    // 501-544 (KindleUnpack's table; Calibre's reader reads 100-209 and
    // 501-544). Ours sits far outside every one of them.
    expect(AZW3_REVISION_EXTH).toBeGreaterThan(1000);
  });

  test('every AZW3 we write carries it, and it reads back from the bytes', async () => {
    const azw3 = await buildAzw3(await fixtureEpub('screenplay'));
    expect(exthNumber(readKf8(azw3), AZW3_REVISION_EXTH)).toEqual([AZW3_WRITER_REVISION]);
    expect(azw3Revision(azw3)).toBe(AZW3_WRITER_REVISION);
  });

  test('a file with no revision, or that is no MOBI at all, reads as none', async () => {
    expect(azw3Revision(stampedAzw3(null))).toBeNull();
    expect(azw3Revision(stampedAzw3(7))).toBe(7);
    expect(azw3Revision(new TextEncoder().encode('not a book'))).toBeNull();
    expect(azw3Revision(new Uint8Array(0))).toBeNull();
    expect(azw3Revision(stampedAzw3().slice(0, 300))).toBeNull();
    // The engine's MOBI 6 has an EXTH block too, but no revision.
    const mobi = buildMobi({ title: 'T', html: '<p>x</p>' });
    expect(azw3Revision(mobi)).toBeNull();
    expect(azw3RevisionOfFile(join(SCRATCH, 'missing.azw3'))).toBeNull();
  });
});

describe('the staleness rule reads it', () => {
  async function bookWith(azw3: Uint8Array | null): Promise<string> {
    const dir = mkdtempSync(join(SCRATCH, 'book-'));
    const epub = join(dir, 'Script.epub');
    writeFileSync(epub, (await convertFountain('Title: Test\n\nINT. ROOM - DAY\n\nA line.\n')).epub);
    const past = new Date(Date.now() - 20_000);
    utimesSync(epub, past, past);
    if (azw3 !== null) writeFileSync(azw3Sibling(epub), azw3); // newer than the EPUB
    return epub;
  }

  test('newer than the EPUB and stamped with this revision: current', async () => {
    expect(rungPlan('azw3', await bookWith(stampedAzw3())).fresh).toBe(true);
  });

  test('newer than the EPUB but with no revision (Calibre\'s, or ours before the stamp): stale', async () => {
    expect(rungPlan('azw3', await bookWith(stampedAzw3(null))).fresh).toBe(false);
  });

  test('newer than the EPUB but an older revision: stale', async () => {
    expect(rungPlan('azw3', await bookWith(stampedAzw3(AZW3_WRITER_REVISION - 1))).fresh).toBe(false);
  });

  test('the ladder rebuilds an unstamped .azw3 into a stamped one', async () => {
    const epub = await bookWith(stampedAzw3(null));
    const stages: string[] = [];
    const out = await freshKindleArtifact({
      epub, fountainPath: null, format: DEFAULT_FORMAT_OPTIONS, calibreAvailable: false, kfxReady: false,
      onStage: (s) => stages.push(s),
    });
    expect(stages).toEqual(['building the AZW3 for Kindle…']);
    expect(azw3Revision(new Uint8Array(readFileSync(out)))).toBe(AZW3_WRITER_REVISION);
    expect(rungPlan('azw3', epub).fresh).toBe(true);
  });

  test('the KFX and MOBI rungs keep the date rule alone', async () => {
    const epub = await bookWith(null);
    writeFileSync(join(epub, '..', 'Script.mobi'), 'no revision in a mobi');
    expect(rungPlan('mobi', epub).fresh).toBe(true);
  });
});

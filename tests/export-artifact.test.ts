import { afterAll, afterEach, describe, test, expect, spyOn } from 'bun:test';
import { MOBI_CARRIES_PICTURES } from '../src/mobi/html';
import {
  mkdirSync, mkdtempSync, writeFileSync, readFileSync, readdirSync, existsSync, utimesSync, rmSync, statSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import JSZip from 'jszip';
import { DEFAULT_FORMAT_OPTIONS } from '../src/options';
import { convertFountain } from '../src/convert';
import { encodePng } from '../src/parser/png';
import { mobiPictures } from './helpers/mobi-records';
// A namespace, not a named import, because the spy below replaces the binding
// on the module itself: that is what the code under test calls through.
import * as libraryImages from '../src/library-images';
import {
  mobiSibling,
  azw3Sibling,
  availableFormats,
  freshArtifactOf,
  freshKindleArtifact,
  kindleArtifactPlan,
  rungPlan,
  CannotRegenerateError,
  RegenerationFailedError,
} from '../src/export/artifact';
import { kfxSibling, KfxToolchainNotReadyError } from '../src/export/kfx';
import { CalibreMissingError, CalibreFailedError } from '../src/export/calibre';
import { readKf8 } from './helpers/kf8-reader';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-export-artifact-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

function scratch(): string {
  return mkdtempSync(join(SCRATCH, 'test-'));
}

test('the MOBI and AZW3 siblings share the EPUB directory and stem', () => {
  expect(mobiSibling(join('/tmp', 'lib', 'Script.epub'))).toBe(join('/tmp', 'lib', 'Script.mobi'));
  expect(azw3Sibling(join('/tmp', 'lib', 'Script.epub'))).toBe(join('/tmp', 'lib', 'Script.azw3'));
  expect(azw3Sibling(join('/tmp', 'lib', 'Script.EPUB'))).toBe(join('/tmp', 'lib', 'Script.azw3'));
});

test('EPUB and a Kindle file are always available: Screepub writes the AZW3 itself', () => {
  expect(availableFormats()).toEqual(['epub', 'kindle']);
});

/** A real EPUB, as the engine writes one, ten seconds old. */
async function realBook(dir: string): Promise<string> {
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, (await convertFountain('Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n\nJO\nA line.\n')).epub);
  const earlier = new Date(Date.now() - 10_000);
  utimesSync(epub, earlier, earlier);
  return epub;
}

test('with no KFX, the ladder writes our own AZW3 beside the EPUB, with no Calibre and the book untouched', async () => {
  const dir = scratch();
  const epub = await realBook(dir);
  const before = readFileSync(epub);
  const stages: string[] = [];
  // calibreAvailable false: the AZW3 is the engine's own work.
  const out = await freshKindleArtifact({
    epub, fountainPath: null, format: DEFAULT_FORMAT_OPTIONS, calibreAvailable: false, kfxReady: false,
    onStage: (s) => stages.push(s),
  });
  expect(out).toBe(azw3Sibling(epub));
  const book = readKf8(new Uint8Array(readFileSync(out)));
  expect(book.header.version).toBe(8);
  expect(book.ncx.map((n) => n.label)).toEqual(['INT. ROOM - DAY']);
  expect(stages).toEqual(['building the AZW3 for Kindle…']);
  expect(readFileSync(epub)).toEqual(before);
  // Written through a temp file that is gone, and current from now on.
  expect(readdirSync(dir).filter((f) => f.endsWith('.tmp'))).toEqual([]);
  expect(kindleArtifactPlan(epub, { calibreAvailable: false, kfxReady: false }).fresh).toBe(true);
});

test('an EPUB the writer cannot read is an honest error, and leaves the AZW3 already there alone', async () => {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'not a zip');
  const azw3 = azw3Sibling(epub);
  writeFileSync(azw3, 'an older build');
  const now = new Date();
  utimesSync(azw3, new Date(now.getTime() - 10_000), new Date(now.getTime() - 10_000));
  utimesSync(epub, now, now);
  await expect(freshKindleArtifact({
    epub, fountainPath: null, format: DEFAULT_FORMAT_OPTIONS, calibreAvailable: true, kfxReady: false,
  })).rejects.toThrow(RegenerationFailedError);
  expect(readFileSync(azw3, 'utf8')).toBe('an older build');
  expect(readdirSync(dir).filter((f) => f.endsWith('.tmp'))).toEqual([]);
});

test('a fresh .mobi beside the EPUB is reused rather than rebuilt', async () => {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'epub');
  const mobi = mobiSibling(epub);
  writeFileSync(mobi, 'existing-mobi');
  // This filesystem's mtime resolution is coarse enough that two
  // back-to-back writeFileSync calls can land on the identical mtime, and
  // per freshness.ts a tie counts as STALE. Force an unambiguous ordering
  // (matching export-freshness.test.ts's convention) so this test exercises
  // "fresh" rather than racing the filesystem clock.
  const now = new Date();
  utimesSync(epub, new Date(now.getTime() - 10_000), new Date(now.getTime() - 10_000));
  utimesSync(mobi, now, now);

  const out = await freshArtifactOf('mobi', {
    epub,
    fountainPath: null,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: false,
    kfxReady: false,
  });
  expect(out).toBe(mobi);
  expect(readFileSync(mobi, 'utf8')).toBe('existing-mobi');
});

test('a missing .mobi with no .fountain to rebuild from is an honest error', async () => {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'epub'); // written AFTER nothing — no .mobi exists at all
  await expect(
    freshArtifactOf('mobi', {
      epub,
      fountainPath: null,
      format: DEFAULT_FORMAT_OPTIONS,
      calibreAvailable: false,
      kfxReady: false,
    }),
  ).rejects.toThrow(CannotRegenerateError);
});

test('the MOBI rung rebuilds from the .fountain, writes a .mobi, and leaves the EPUB as it was', async () => {
  const dir = scratch();
  const fountain = join(dir, 'Script.fountain');
  writeFileSync(fountain, 'Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n');
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'stale');

  const out = await freshArtifactOf('mobi', {
    epub,
    fountainPath: fountain,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: false,
    kfxReady: false,
  });
  expect(out).toBe(mobiSibling(epub));
  expect(existsSync(out)).toBe(true);
  // Before Screepub wrote its own AZW3 the ladder's MOBI rung rewrote the
  // book too; no rung does now.
  expect(readFileSync(epub, 'utf8')).toBe('stale');
});

// The MOBI rung renders from the .fountain, so it has to find that script's
// pictures the way every other render from the text does
// (src/library-images.ts). The .mobi carries them once the safety gate opens
// (MOBI_CARRIES_PICTURES, src/mobi/html.ts); until then it carries none.
describe('the MOBI rebuild and the picture folder', () => {
  /** What the handed-over .mobi carries of `pictures`: all of them once the
   *  gate is open, none while it is shut. */
  const inMobi = (pictures: Uint8Array[]) => (MOBI_CARRIES_PICTURES ? pictures : []);
  const spies: { mockRestore(): void }[] = [];
  afterEach(() => {
    for (const s of spies.splice(0)) s.mockRestore();
  });

  /** A library script whose one note names a picture in its folder. */
  function library() {
    const dir = scratch();
    const fountain = join(dir, 'Script.fountain');
    writeFileSync(
      fountain,
      'Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n\n[[img: p001-01.png w=40]]\n\nAnother line.\n',
    );
    mkdirSync(join(dir, 'Script.images'));
    const plate = encodePng(new Uint8Array([0, 128, 255]), 1, 1);
    writeFileSync(join(dir, 'Script.images', 'p001-01.png'), plate);
    const epub = join(dir, 'Script.epub');
    writeFileSync(epub, 'stale');
    return { fountain, epub, plate };
  }
  const rebuild = (
    fountain: string,
    epub: string,
    format = DEFAULT_FORMAT_OPTIONS,
    onStage?: (stage: string) => void,
  ) => freshArtifactOf('mobi', { epub, fountainPath: fountain, format, calibreAvailable: false, kfxReady: false, onStage });

  /** The pictures an EPUB carries, by entry path. */
  const figuresIn = async (epub: string) =>
    Object.values((await JSZip.loadAsync(readFileSync(epub))).files)
      .filter((f) => !f.dir && f.name.startsWith('OEBPS/images/'))
      .map((f) => f.name)
      .sort();

  /** A library script with two figures, and the book an earlier conversion
   * built from its folder while both pictures were there: dated ten seconds
   * back, as a book converted before this export would be. */
  async function libraryBook() {
    const dir = scratch();
    const fountain = join(dir, 'Script.fountain');
    const text = 'Title: Test\n\nINT. ROOM - DAY\n\n[[img: p001-01.png w=40]]\n\nA line of action.\n\n'
      + '[[img: p002-01.png w=60]]\n\nAnother line.\n';
    writeFileSync(fountain, text);
    const folder = join(dir, 'Script.images');
    mkdirSync(folder);
    const plates = new Map([
      ['p001-01.png', encodePng(new Uint8Array([0, 128, 255]), 1, 1)],
      ['p002-01.png', encodePng(new Uint8Array([255, 128, 0]), 1, 1)],
    ]);
    for (const [name, bytes] of plates) writeFileSync(join(folder, name), bytes);
    const epub = join(dir, 'Script.epub');
    writeFileSync(epub, (await convertFountain(text, { format: DEFAULT_FORMAT_OPTIONS, images: plates })).epub);
    const earlier = new Date(Date.now() - 10_000);
    utimesSync(epub, earlier, earlier);
    // The control, without which "the book is unchanged" proves nothing: it
    // starts out carrying both pictures.
    expect(await figuresIn(epub)).toEqual(['OEBPS/images/p001-01.png', 'OEBPS/images/p002-01.png']);
    return { fountain, folder, epub, plates };
  }

  /** The rung against a damaged folder: the library book is left exactly as
   * it was, date included (the window tells one version of the book from the
   * next by it); the .mobi it writes carries the pictures that could be read
   * (`carried`, in the script's order) and goes without the rest; and that
   * .mobi is current, so the next export reuses it. */
  async function expectBookKept(fountain: string, epub: string, carried: Uint8Array[]) {
    const bytes = readFileSync(epub);
    const date = statSync(epub).mtimeMs;
    const stages: string[] = [];
    const out = await rebuild(fountain, epub, DEFAULT_FORMAT_OPTIONS, (s) => stages.push(s));
    expect(out).toBe(mobiSibling(epub));
    expect(readFileSync(out).length).toBeGreaterThan(0);
    expect(mobiPictures(readFileSync(out))).toEqual(inMobi(carried));
    expect(readFileSync(epub)).toEqual(bytes);
    expect(statSync(epub).mtimeMs).toBe(date);
    expect(statSync(out).mtimeMs).toBeGreaterThan(date);
    expect(rungPlan('mobi', epub).fresh).toBe(true);
    expect(await rebuild(fountain, epub, DEFAULT_FORMAT_OPTIONS, (s) => stages.push(s))).toBe(out);
    expect(stages).toEqual(['rebuilding the Kindle file…']);
  }

  test('from an intact folder, the .mobi carries every picture the gate lets it, in the script’s order', async () => {
    const { fountain, epub, plates } = await libraryBook();
    const out = await rebuild(fountain, epub);
    expect(mobiPictures(readFileSync(out))).toEqual(inMobi([plates.get('p001-01.png')!, plates.get('p002-01.png')!]));
    // And the library book still carries both: the rung never touches it.
    expect(await figuresIn(epub)).toEqual(['OEBPS/images/p001-01.png', 'OEBPS/images/p002-01.png']);
  });

  test('a picture the folder lost leaves the library book as it was, and the .mobi without it', async () => {
    const { fountain, folder, epub, plates } = await libraryBook();
    // An iCloud placeholder: the name is still there, the picture is not.
    writeFileSync(join(folder, 'p002-01.png'), new Uint8Array(0));
    await expectBookKept(fountain, epub, [plates.get('p001-01.png')!]);
  });

  test('a picture folder that is gone leaves the library book as it was, and the .mobi without pictures', async () => {
    const { fountain, folder, epub } = await libraryBook();
    rmSync(folder, { recursive: true });
    await expectBookKept(fountain, epub, []);
  });

  test('with includeImages off, the folder is not read', async () => {
    const reads = spyOn(libraryImages, 'readImages');
    spies.push(reads);
    // The control, without which the zero below proves nothing: with the
    // option on, the spy sees the read.
    const on = library();
    await rebuild(on.fountain, on.epub);
    expect(reads).toHaveBeenCalledTimes(1);

    const off = library();
    await rebuild(off.fountain, off.epub, { ...DEFAULT_FORMAT_OPTIONS, includeImages: false });
    expect(reads).toHaveBeenCalledTimes(1);
  });
});

test('the MOBI rebuild writes through a temp file and leaves none behind', async () => {
  // The .mobi goes to a hidden temp sibling and is renamed into place, so an
  // interrupted rebuild cannot leave a truncated .mobi at the final path,
  // which, being NEWER than the EPUB, the staleness rule would trust as
  // fresh forever. The crash itself isn't reproducible in a test; what is
  // testable is that the mechanism ran and cleaned up after itself.
  const dir = scratch();
  const fountain = join(dir, 'Script.fountain');
  writeFileSync(fountain, 'Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n');
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'stale');

  const out = await freshArtifactOf('mobi', {
    epub,
    fountainPath: fountain,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: false,
    kfxReady: false,
  });

  expect(readdirSync(dir).filter((f) => f.endsWith('.tmp'))).toEqual([]);
  expect(readFileSync(out).length).toBeGreaterThan(0);
});

// --- Ladder PRECEDENCE: kfxReady wins, and our own AZW3 is everything else.
// The KFX branch cannot be driven to a successful conversion here, but
// precedence can still be proven either by a calibre-free reuse of an
// existing fresh artifact, or by an honest Calibre error showing the right
// branch was reached.

test('a fresh .kfx is reused without touching Calibre, even when calibreAvailable is also true', async () => {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'epub');
  const kfx = kfxSibling(epub);
  writeFileSync(kfx, 'existing-kfx');
  // Make sure the .kfx is unambiguously newer than the EPUB regardless of
  // filesystem mtime resolution (this filesystem can give back-to-back
  // writeFileSync calls the identical mtime, which needsRegeneration
  // treats as stale).
  const now = new Date();
  utimesSync(epub, new Date(now.getTime() - 10_000), new Date(now.getTime() - 10_000));
  utimesSync(kfx, now, now);

  const out = await freshKindleArtifact({
    epub,
    fountainPath: null,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: true,
    kfxReady: true,
  });
  // A wrong implementation that ignored kfxReady, or its freshness check,
  // would either build an AZW3 (which throws here: the placeholder is not an
  // EPUB) or call toKfx unconditionally.
  expect(out).toBe(kfx);
  expect(readFileSync(kfx, 'utf8')).toBe('existing-kfx');
});

test('kfxReady with a stale .kfx attempts a KFX rebuild rather than silently building the AZW3 or reusing the stale file', async () => {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'epub');
  // No .kfx at all yet, so the implementation must attempt toKfx, which
  // needs Calibre. Reaching it throws CalibreMissingError (tool absent),
  // KfxToolchainNotReadyError (Calibre present but no Kindle Previewer or
  // plugin) or CalibreFailedError (whole toolchain present, placeholder
  // input rejected as not a zip). Falling through to the AZW3 branch would
  // instead throw RegenerationFailedError: that is the regression this pins.
  const error = await freshKindleArtifact({
    epub,
    fountainPath: null,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: true,
    kfxReady: true,
  }).catch((e) => e);
  expect(error).toBeInstanceOf(Error);
  expect(error).not.toBeInstanceOf(RegenerationFailedError);
  expect(error).not.toBeInstanceOf(CannotRegenerateError);
  expect(
    [CalibreMissingError, CalibreFailedError, KfxToolchainNotReadyError].some(
      (C) => error instanceof C,
    ),
  ).toBe(true);
});

// The AZW3 rung reuses its own previous answer, by the SAME rule as the KFX
// and MOBI rungs (freshness.ts): "Save a Kindle file" runs `export --for
// kindle` to learn the extension and then `route save-kindle`, and without
// this the second one would build again.

test('a fresh .azw3 is reused, not rebuilt', async () => {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'epub');
  const azw3 = azw3Sibling(epub);
  writeFileSync(azw3, 'existing-azw3');
  const now = new Date();
  utimesSync(epub, new Date(now.getTime() - 10_000), new Date(now.getTime() - 10_000));
  utimesSync(azw3, now, now);
  const stages: string[] = [];

  const out = await freshKindleArtifact({
    epub,
    fountainPath: null,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: false,
    kfxReady: false,
    onStage: (stage) => stages.push(stage),
  });
  // A build would have failed on this placeholder EPUB, and announced
  // itself first.
  expect(out).toBe(azw3);
  expect(readFileSync(azw3, 'utf8')).toBe('existing-azw3');
  expect(stages).toEqual([]);
});

test('a stale .azw3 (the EPUB is newer) is rebuilt, not reused', async () => {
  const dir = scratch();
  const epub = await realBook(dir);
  const azw3 = azw3Sibling(epub);
  writeFileSync(azw3, 'stale-azw3');
  const older = new Date(Date.now() - 20_000);
  utimesSync(azw3, older, older);
  const stages: string[] = [];
  const out = await freshKindleArtifact({
    epub, fountainPath: null, format: DEFAULT_FORMAT_OPTIONS, calibreAvailable: true, kfxReady: false,
    onStage: (stage) => stages.push(stage),
  });
  expect(out).toBe(azw3);
  expect(stages).toEqual(['building the AZW3 for Kindle…']);
  expect(readKf8(new Uint8Array(readFileSync(azw3))).header.version).toBe(8);
});

test('a fresh .mobi is never the ladder\'s answer: with no KFX it builds the AZW3', async () => {
  const dir = scratch();
  const epub = await realBook(dir);
  const mobi = mobiSibling(epub);
  writeFileSync(mobi, 'existing-mobi');
  const now = new Date();
  utimesSync(mobi, now, now);
  const out = await freshKindleArtifact({
    epub, fountainPath: null, format: DEFAULT_FORMAT_OPTIONS, calibreAvailable: false, kfxReady: false,
  });
  expect(out).toBe(azw3Sibling(epub));
  expect(readFileSync(mobi, 'utf8')).toBe('existing-mobi');
});

// ---- kindleArtifactPlan: what the ladder would hand over, without building --

/** An EPUB, and a sibling `ext` either newer than it (fresh) or older (stale),
 *  or none at all. Dated ten seconds apart, so filesystem mtime resolution
 *  cannot turn either one into the tie that counts as stale. */
function withSibling(ext: string | null, fresh: boolean): string {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'epub');
  const now = Date.now();
  const epubAt = new Date(now - 5_000);
  utimesSync(epub, epubAt, epubAt);
  if (ext !== null) {
    const sibling = join(dir, `Script.${ext}`);
    writeFileSync(sibling, ext);
    const at = new Date(fresh ? now : now - 10_000);
    utimesSync(sibling, at, at);
  }
  return epub;
}

test('the plan names the rung the ladder takes, where its file lives, what builds it, and the book’s date', () => {
  const epub = withSibling(null, false);
  const bookDate = statSync(epub).mtimeMs;
  expect(kindleArtifactPlan(epub, { calibreAvailable: true, kfxReady: true }))
    .toEqual({ path: kfxSibling(epub), fresh: false, builtBy: 'calibre', bookDate });
  // Calibre or not, the ladder's other rung is our own AZW3.
  expect(kindleArtifactPlan(epub, { calibreAvailable: true, kfxReady: false }))
    .toEqual({ path: azw3Sibling(epub), fresh: false, builtBy: 'screepub', bookDate });
  expect(kindleArtifactPlan(epub, { calibreAvailable: false, kfxReady: false }))
    .toEqual({ path: azw3Sibling(epub), fresh: false, builtBy: 'screepub', bookDate });
  // The MOBI, asked for by name, is the engine's too.
  expect(rungPlan('mobi', epub)).toEqual({ path: mobiSibling(epub), fresh: false, builtBy: 'screepub', bookDate });
});

test('the book’s date moves when the book is rewritten, and is null when it cannot be read', () => {
  // The window builds ahead once per version of the book: this date is what
  // tells one version from the next (a Settings save rewrites the EPUB).
  const epub = withSibling(null, false);
  const state = { calibreAvailable: true, kfxReady: true };
  const before = kindleArtifactPlan(epub, state).bookDate;
  const later = new Date(Date.now() + 60_000);
  utimesSync(epub, later, later);
  expect(kindleArtifactPlan(epub, state).bookDate).toBe(later.getTime());
  expect(kindleArtifactPlan(epub, state).bookDate).not.toBe(before);
  expect(kindleArtifactPlan(join(scratch(), 'gone.epub'), state).bookDate).toBe(null);
});

test('the plan is fresh exactly when the ladder would reuse the file: newer than the EPUB', () => {
  for (const [ext, state] of [
    ['kfx', { calibreAvailable: true, kfxReady: true }],
    ['azw3', { calibreAvailable: true, kfxReady: false }],
    ['azw3', { calibreAvailable: false, kfxReady: false }],
  ] as const) {
    expect(`${ext} newer: ${kindleArtifactPlan(withSibling(ext, true), state).fresh}`).toBe(`${ext} newer: true`);
    expect(`${ext} older: ${kindleArtifactPlan(withSibling(ext, false), state).fresh}`).toBe(`${ext} older: false`);
  }
  // Another rung's file does not count: a fresh .azw3 is not a KFX.
  expect(kindleArtifactPlan(withSibling('azw3', true), { calibreAvailable: true, kfxReady: true }).fresh).toBe(false);
});

test('a fresh plan is what the ladder hands back, untouched', async () => {
  // The two read one rule: whatever the plan calls fresh, the ladder returns
  // as it is, with no converter run (none is installed here to run).
  const epub = withSibling('azw3', true);
  const plan = kindleArtifactPlan(epub, { calibreAvailable: true, kfxReady: false });
  expect(plan.fresh).toBe(true);
  const out = await freshKindleArtifact({
    epub, fountainPath: null, format: DEFAULT_FORMAT_OPTIONS, calibreAvailable: true, kfxReady: false,
  });
  expect(out).toBe(plan.path);
  expect(readFileSync(out, 'utf8')).toBe('azw3');
});

test('an explicit MOBI rung writes the .mobi and leaves the EPUB, and every sibling built from it, alone', async () => {
  // Asking for the MOBI by name must not rewrite the book, or a .kfx or
  // .azw3 already beside it goes stale for nothing.
  const dir = scratch();
  const fountain = join(dir, 'Script.fountain');
  writeFileSync(fountain, 'Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n');
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'the book as it is');
  const azw3 = azw3Sibling(epub);
  writeFileSync(azw3, 'azw3');
  const past = new Date(Date.now() - 60_000);
  utimesSync(epub, past, past);
  const later = new Date(Date.now() - 30_000);
  utimesSync(azw3, later, later);
  const before = statSync(epub).mtimeMs;

  // calibreAvailable is not consulted by a named rung; nothing here can
  // reach Calibre, because the MOBI rung is the engine's own work.
  const out = await freshArtifactOf('mobi', {
    epub,
    fountainPath: fountain,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: true,
    kfxReady: false,
  });

  expect(out).toBe(mobiSibling(epub));
  expect(readFileSync(out).length).toBeGreaterThan(0);
  expect(readFileSync(epub, 'utf8')).toBe('the book as it is');
  expect(statSync(epub).mtimeMs).toBe(before);
  expect(kindleArtifactPlan(epub, { calibreAvailable: true, kfxReady: false }).fresh).toBe(true);
});

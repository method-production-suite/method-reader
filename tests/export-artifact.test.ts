import { afterAll, afterEach, describe, test, expect, spyOn } from 'bun:test';
import {
  mkdirSync, mkdtempSync, writeFileSync, readFileSync, readdirSync, existsSync, utimesSync, rmSync, statSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import JSZip from 'jszip';
import { DEFAULT_FORMAT_OPTIONS } from '../src/options';
import { convertFountain } from '../src/convert';
import { encodePng } from '../src/parser/png';
// A namespace, not a named import, because the spy below replaces the binding
// on the module itself: that is what the code under test calls through.
import * as libraryImages from '../src/library-images';
import {
  mobiSibling,
  availableFormats,
  freshKindleArtifact,
  kindleArtifactPlan,
  CannotRegenerateError,
} from '../src/export/artifact';
import { kfxSibling, KfxToolchainNotReadyError } from '../src/export/kfx';
import { azw3Sibling, CalibreMissingError, CalibreFailedError } from '../src/export/calibre';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-export-artifact-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

function scratch(): string {
  return mkdtempSync(join(SCRATCH, 'test-'));
}

test('the MOBI sibling shares the EPUB directory and stem', () => {
  expect(mobiSibling(join('/tmp', 'lib', 'Script.epub'))).toBe(join('/tmp', 'lib', 'Script.mobi'));
});

test('EPUB is always available; Kindle needs Calibre or an existing .mobi', () => {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'epub');

  expect(availableFormats(epub, false)).toEqual(['epub']);
  expect(availableFormats(epub, true)).toEqual(['epub', 'kindle']);

  writeFileSync(mobiSibling(epub), 'mobi');
  expect(availableFormats(epub, false)).toEqual(['epub', 'kindle']);
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

  const out = await freshKindleArtifact({
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
    freshKindleArtifact({
      epub,
      fountainPath: null,
      format: DEFAULT_FORMAT_OPTIONS,
      calibreAvailable: false,
      kfxReady: false,
    }),
  ).rejects.toThrow(CannotRegenerateError);
});

test('the MOBI branch rebuilds from the .fountain and writes a .mobi', async () => {
  const dir = scratch();
  const fountain = join(dir, 'Script.fountain');
  writeFileSync(fountain, 'Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n');
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'stale');

  const out = await freshKindleArtifact({
    epub,
    fountainPath: fountain,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: false,
    kfxReady: false,
  });
  expect(out).toBe(mobiSibling(epub));
  expect(existsSync(out)).toBe(true);
  // Documented side effect: this branch rewrites the EPUB in place.
  expect(readFileSync(epub, 'utf8')).not.toBe('stale');
});

// The MOBI rung rewrites the library EPUB in place, from the .fountain, so it
// has to find that script's pictures the way every other render from the
// text does (src/library-images.ts), or a Kindle export on a machine without
// Calibre would strip them from the library book. The MOBI itself carries
// none; these are about the EPUB it rewrites on the way.
describe('the MOBI rebuild and the picture folder', () => {
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
  ) => freshKindleArtifact({ epub, fountainPath: fountain, format, calibreAvailable: false, kfxReady: false, onStage });

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
    return { fountain, folder, epub };
  }

  /** The rung against a damaged folder: the library book is left exactly as
   * it was, date included (the window tells one version of the book from the
   * next by it), and the .mobi it writes is current, so the next export
   * reuses it instead of rebuilding against the same loss. */
  async function expectBookKept(fountain: string, epub: string) {
    const bytes = readFileSync(epub);
    const date = statSync(epub).mtimeMs;
    const stages: string[] = [];
    const out = await rebuild(fountain, epub, DEFAULT_FORMAT_OPTIONS, (s) => stages.push(s));
    expect(out).toBe(mobiSibling(epub));
    expect(readFileSync(out).length).toBeGreaterThan(0);
    expect(readFileSync(epub)).toEqual(bytes);
    expect(statSync(epub).mtimeMs).toBe(date);
    expect(statSync(out).mtimeMs).toBeGreaterThan(date);
    expect(kindleArtifactPlan(epub, { calibreAvailable: false, kfxReady: false }).fresh).toBe(true);
    expect(await rebuild(fountain, epub, DEFAULT_FORMAT_OPTIONS, (s) => stages.push(s))).toBe(out);
    expect(stages).toEqual(['rebuilding the Kindle file…']);
  }

  test('a picture the folder lost leaves the library book as it was', async () => {
    const { fountain, folder, epub } = await libraryBook();
    // An iCloud placeholder: the name is still there, the picture is not.
    writeFileSync(join(folder, 'p002-01.png'), new Uint8Array(0));
    await expectBookKept(fountain, epub);
  });

  test('a picture folder that is gone leaves the library book as it was', async () => {
    const { fountain, folder, epub } = await libraryBook();
    rmSync(folder, { recursive: true });
    await expectBookKept(fountain, epub);
  });

  test('the EPUB it rewrites keeps its figure and the picture behind it', async () => {
    const { fountain, epub, plate } = library();
    await rebuild(fountain, epub);
    const zip = await JSZip.loadAsync(readFileSync(epub));
    const body = await zip.file('OEBPS/text/body001.xhtml')!.async('string');
    expect(body).toContain('<p class="figure w40"><img src="../images/p001-01.png" alt="Illustration"/></p>');
    expect([...await zip.file('OEBPS/images/p001-01.png')!.async('uint8array')]).toEqual([...plate]);
  });

  test('with includeImages off, the folder is not read and the EPUB carries no picture', async () => {
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
    const zip = await JSZip.loadAsync(readFileSync(off.epub));
    expect(Object.keys(zip.files).filter((n) => n.startsWith('OEBPS/images/'))).toEqual([]);
  });
});

test('the rebuild writes through temp files and leaves none behind', async () => {
  // Both outputs go to a hidden temp sibling and are renamed into place, so
  // an interrupted rebuild cannot leave a truncated .mobi at the final path
  // — which, being NEWER than the EPUB, the staleness rung would trust as
  // fresh forever. The crash itself isn't reproducible in a test; what is
  // testable is that the mechanism ran and cleaned up after itself.
  const dir = scratch();
  const fountain = join(dir, 'Script.fountain');
  writeFileSync(fountain, 'Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n');
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'stale');

  const out = await freshKindleArtifact({
    epub,
    fountainPath: fountain,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: false,
    kfxReady: false,
  });

  expect(readdirSync(dir).filter((f) => f.endsWith('.tmp'))).toEqual([]);
  // Both outputs arrived complete at their final paths. (Their mtimes can
  // still tie on a coarse-resolution filesystem, which freshness.ts counts
  // as stale — a rebuild, never a truncated file trusted as fresh.)
  expect(readFileSync(epub, 'utf8')).not.toBe('stale');
  expect(readFileSync(out).length).toBeGreaterThan(0);
});

// --- Strengthening tests beyond the brief's five: these exercise ladder
// PRECEDENCE (kfxReady > calibreAvailable > mobi rebuild). Calibre and
// Kindle Previewer are both absent on this Linux machine, so the
// Calibre-dependent branches cannot be driven to a successful conversion
// here — but precedence can still be proven either by a successful,
// calibre-free reuse of an existing fresh artifact, or by an honest
// CalibreMissingError showing the right branch was reached.

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
  // A wrong implementation that checked calibreAvailable before kfxReady, or
  // that ignored kfxReady's freshness check, would either call toAzw3 (which
  // throws here, since Calibre is absent) or call toKfx unconditionally.
  expect(out).toBe(kfx);
  expect(readFileSync(kfx, 'utf8')).toBe('existing-kfx');
});

test('kfxReady with a stale .kfx attempts a KFX rebuild rather than silently reusing calibre or the stale file', async () => {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'epub');
  // No .kfx at all yet, so needsRegeneration must report stale/missing and
  // the implementation must attempt toKfx — which needs Calibre — rather
  // than silently falling through to the calibreAvailable (AZW3) branch or
  // returning a nonexistent .kfx path.
  //
  // Precedence, asserted without depending on what is installed: reaching
  // the Calibre/KFX branch throws CalibreMissingError (tool absent),
  // KfxToolchainNotReadyError (Calibre present but no Kindle Previewer or
  // plugin — every Linux machine, and the toolchain guard toKfx restored
  // from KFXToolchain.convert), or CalibreFailedError (whole toolchain
  // present, placeholder input rejected as not a zip). Falling through to
  // the MOBI branch would instead throw CannotRegenerateError — that is the
  // regression this pins.
  const error = await freshKindleArtifact({
    epub,
    fountainPath: null,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: true,
    kfxReady: true,
  }).catch((e) => e);
  expect(error).toBeInstanceOf(Error);
  expect(error).not.toBeInstanceOf(CannotRegenerateError);
  expect(
    [CalibreMissingError, CalibreFailedError, KfxToolchainNotReadyError].some(
      (C) => error instanceof C,
    ),
  ).toBe(true);
});

// The AZW3 rung reuses its own previous answer, by the SAME rule as the KFX
// and MOBI rungs (freshness.ts): "Save a Kindle file" on a Calibre-only
// machine runs `export --for kindle` to learn the extension and then
// `route save-kindle`, and without this the second one converted again.

test('a fresh .azw3 is reused with no Calibre call, even though Calibre is available', async () => {
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
    calibreAvailable: true,
    kfxReady: false,
    onStage: (stage) => stages.push(stage),
  });
  // A call to Calibre would either have failed on this placeholder EPUB
  // (Calibre installed) or thrown CalibreMissingError (not installed); and
  // either way it would have announced the conversion first.
  expect(out).toBe(azw3);
  expect(readFileSync(azw3, 'utf8')).toBe('existing-azw3');
  expect(stages).toEqual([]);
});

test('a stale .azw3 (the EPUB is newer) is rebuilt, not reused', async () => {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'epub');
  const azw3 = azw3Sibling(epub);
  writeFileSync(azw3, 'stale-azw3');
  const now = new Date();
  utimesSync(azw3, new Date(now.getTime() - 10_000), new Date(now.getTime() - 10_000));
  utimesSync(epub, now, now);
  const stages: string[] = [];

  // Same environment-independent assertion as the precedence tests below:
  // reaching Calibre surfaces as CalibreMissingError (absent) or
  // CalibreFailedError (present, placeholder input rejected). A reuse would
  // instead return the stale path without throwing.
  const error = await freshKindleArtifact({
    epub,
    fountainPath: null,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: true,
    kfxReady: false,
    onStage: (stage) => stages.push(stage),
  }).catch((e) => e);
  expect(error).toBeInstanceOf(Error);
  expect([CalibreMissingError, CalibreFailedError].some((C) => error instanceof C)).toBe(true);
  expect(stages).toEqual(['converting to AZW3 for Kindle…']);
  // A conversion that failed leaves the old file alone: it is still stale,
  // so the next try rebuilds it again.
  expect(readFileSync(azw3, 'utf8')).toBe('stale-azw3');
}, 120_000);

test('calibreAvailable is used over an existing fresh .mobi, not reused as a shortcut', async () => {
  const dir = scratch();
  const epub = join(dir, 'Script.epub');
  writeFileSync(epub, 'epub');
  const mobi = mobiSibling(epub);
  writeFileSync(mobi, 'existing-mobi');
  const now = new Date();
  utimesSync(mobi, now, now);

  // calibreAvailable=true means the AZW3 branch is always taken (and with no
  // .azw3 beside the EPUB, a conversion). A wrong implementation might
  // instead notice a fresh .mobi already satisfies "kindle" and reuse it,
  // skipping Calibre entirely. Same environment-independent precedence
  // assertion as above:
  // the correct branch surfaces as CalibreMissingError or
  // CalibreFailedError, never CannotRegenerateError (which would mean
  // execution fell through to the MOBI branch instead).
  const error = await freshKindleArtifact({
    epub,
    fountainPath: null,
    format: DEFAULT_FORMAT_OPTIONS,
    calibreAvailable: true,
    kfxReady: false,
  }).catch((e) => e);
  expect(error).toBeInstanceOf(Error);
  expect(error).not.toBeInstanceOf(CannotRegenerateError);
  expect(
    [CalibreMissingError, CalibreFailedError, KfxToolchainNotReadyError].some(
      (C) => error instanceof C,
    ),
  ).toBe(true);
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
  expect(kindleArtifactPlan(epub, { calibreAvailable: true, kfxReady: false }))
    .toEqual({ path: azw3Sibling(epub), fresh: false, builtBy: 'calibre', bookDate });
  expect(kindleArtifactPlan(epub, { calibreAvailable: false, kfxReady: false }))
    .toEqual({ path: mobiSibling(epub), fresh: false, builtBy: 'screepub', bookDate });
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
    ['mobi', { calibreAvailable: false, kfxReady: false }],
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

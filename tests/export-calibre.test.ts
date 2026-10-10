import { afterAll, test, expect, test as bunTest } from 'bun:test';
import { accessSync, constants, mkdtempSync, writeFileSync, readFileSync, readdirSync, chmodSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, delimiter } from 'node:path';
import { platform } from 'node:process';
import JSZip from 'jszip';
import {
  CALIBRE_FORMAT_GUARDS,
  calibreTool,
  isCalibreAvailable,
  toKepub,
  runCalibre,
  CONVERSION_TIMEOUT_MS,
  CalibreFailedError,
  CalibreTimedOutError,
  CalibreMissingError,
} from '../src/export/calibre';
import { convertFountain } from '../src/convert';
import { DEFAULT_FORMAT_OPTIONS } from '../src/options';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-export-calibre-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

test('the format guards are exactly the device-verified trio', () => {
  // Device-verified 2026-07-29. Calibre would otherwise insert a page break
  // before every h2 (scene-per-page again) and its remove-fake-margins
  // heuristic would delete the dialogue column's side margins, which is what
  // a screenplay looks like to it. Change them in one place or not at all.
  expect([...CALIBRE_FORMAT_GUARDS]).toEqual([
    '--page-breaks-before=/',
    '--chapter-mark=none',
    '--disable-remove-fake-margins',
  ]);
});

test('isAvailable agrees with toolURL in both directions', () => {
  const tool = calibreTool('ebook-convert');
  if (tool) {
    expect(isCalibreAvailable()).toBe(true);
    expect(() => accessSync(tool, constants.X_OK)).not.toThrow();
  } else {
    expect(isCalibreAvailable()).toBe(false);
  }
});

test('an unknown tool name is not discovered', () => {
  expect(calibreTool('definitely-not-a-calibre-tool')).toBeNull();
});

// darwin's candidate list is fixed app-bundle/homebrew paths and never
// consults PATH (see src/export/calibre.ts candidatePaths); linux and win32
// both fall back to a PATH scan, which is real, controllable behavior we can
// exercise here without Calibre actually being installed.
const describesPathScan = platform !== 'darwin';

test('calibreTool discovers an executable found on PATH', () => {
  if (!describesPathScan) return;
  const dir = mkdtempSync(join(SCRATCH, 'calibre-path-'));
  const name = platform === 'win32' ? 'screepub-fake-calibre-tool.exe' : 'screepub-fake-calibre-tool';
  const fake = join(dir, name);
  writeFileSync(fake, '#!/bin/sh\nexit 0\n');
  chmodSync(fake, 0o755);
  const originalPath = process.env.PATH;
  process.env.PATH = `${dir}${delimiter}${originalPath ?? ''}`;
  try {
    // calibreTool appends the platform's exe suffix itself, so pass the bare name.
    expect(calibreTool('screepub-fake-calibre-tool')).toBe(fake);
  } finally {
    process.env.PATH = originalPath;
  }
});

test('calibreTool refuses a same-named file on PATH that is not executable', () => {
  if (!describesPathScan) return;
  const dir = mkdtempSync(join(SCRATCH, 'calibre-path-'));
  const name = platform === 'win32' ? 'screepub-fake-calibre-tool2.exe' : 'screepub-fake-calibre-tool2';
  const fake = join(dir, name);
  writeFileSync(fake, '#!/bin/sh\nexit 0\n');
  chmodSync(fake, 0o644); // present, but not executable
  const originalPath = process.env.PATH;
  process.env.PATH = `${dir}${delimiter}${originalPath ?? ''}`;
  try {
    expect(calibreTool('screepub-fake-calibre-tool2')).toBeNull();
  } finally {
    process.env.PATH = originalPath;
  }
});

// As the Swift app's own checks did: environment-dependent checks self-skip
// rather than fail, so CI stays green on bare runners.
const withCalibre = calibreTool('ebook-convert') ? bunTest : bunTest.skip;
// The inverse: these assert the missing-tool error path, which is only true
// to test when Calibre is genuinely absent from this machine.
const withoutCalibre = calibreTool('ebook-convert') ? bunTest.skip : bunTest;

withoutCalibre('toKepub throws CalibreMissingError, specifically, when Calibre is absent', async () => {
  await expect(toKepub('/tmp/screepub-does-not-exist.epub')).rejects.toThrow(CalibreMissingError);
});

// ebook-convert selects its OUTPUT FORMAT from the output file's extension,
// so toKepub must convert to `<stem>.kepub` first and only THEN rename to
// `<stem>.kepub.epub` -- writing straight to the double extension would make
// real Calibre see ".epub" and emit a plain EPUB with no koboSpan markup at
// all. A fake ebook-convert that just touches whatever path it's told to
// write lets us prove the two-step shape without Calibre installed: it
// fails closed (CalibreFailedError, "no .kepub") if toKepub ever regresses
// to the single-step form, because the fake would then create `.kepub.epub`
// directly and the intermediate `.kepub` existsSync check would find nothing.
test('toKepub converts to .kepub then renames to .kepub.epub (fake ebook-convert)', async () => {
  if (!describesPathScan) return;
  if (calibreTool('ebook-convert')) return; // never shadow a real install

  const toolDir = mkdtempSync(join(SCRATCH, 'calibre-fake-'));
  const name = platform === 'win32' ? 'ebook-convert.exe' : 'ebook-convert';
  const fakeTool = join(toolDir, name);
  const argvLog = join(toolDir, 'argv.log');
  // Logs the exact argv it received (one per line, so a guard flag
  // containing "=" or "/" round-trips safely), then touches the output path
  // it's told to write, mimicking ebook-convert's own file creation.
  writeFileSync(fakeTool, `#!/bin/sh\nfor a in "$@"; do echo "$a"; done > "${argvLog}"\ntouch "$2"\n`);
  chmodSync(fakeTool, 0o755);

  const originalPath = process.env.PATH;
  process.env.PATH = `${toolDir}${delimiter}${originalPath ?? ''}`;
  try {
    const workDir = mkdtempSync(join(SCRATCH, 'calibre-work-'));
    const epub = join(workDir, 'book.epub');
    writeFileSync(epub, 'fake epub bytes');
    const rawKepub = join(workDir, 'book.kepub');
    const finalKepub = join(workDir, 'book.kepub.epub');
    writeFileSync(finalKepub, 'stale leftover'); // proves the rename replaces what is there

    const out = await toKepub(epub);

    expect(out).toBe(finalKepub);
    expect(out.endsWith('.kepub.epub')).toBe(true);
    expect(existsSync(rawKepub)).toBe(false); // renamed away, not left behind
    expect(readFileSync(out, 'utf8')).not.toBe('stale leftover'); // overwritten

    // toKepub converts to the RAW .kepub path (the extension is what makes
    // ebook-convert emit KEPUB at all) and passes the same guard trio as
    // toKfx. The Swift app's converter omitted the guards here; this port applies
    // them deliberately, because remove-fake-margins is an input-side
    // heuristic that strips the dialogue column's side margins before the
    // output format is even chosen -- see the note on toKepub. A regression
    // back to the Swift's guard-free call fails this.
    const argv = readFileSync(argvLog, 'utf8').split('\n').filter(Boolean);
    expect(argv).toEqual([epub, rawKepub, ...CALIBRE_FORMAT_GUARDS]);
  } finally {
    process.env.PATH = originalPath;
  }
});

async function minimalEpub(): Promise<string> {
  const dir = mkdtempSync(join(SCRATCH, 'test-'));
  const epub = join(dir, 'book.epub');
  const result = await convertFountain('Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n');
  writeFileSync(epub, result.epub);
  return epub;
}

withCalibre('toKepub names its output .kepub.epub AND emits koboSpan markup', async () => {
  // The filename alone proves nothing: the BROKEN single-step form produces
  // a file with this same name -- a plain EPUB with no koboSpan markup at
  // all. koboSpan is the only assertion that discriminates the two, which is
  // why the Swift app's checks asserted both halves and why
  // the fake-tool test above cannot stand in for it: it self-skips on every
  // machine that actually has Calibre.
  const out = await toKepub(await minimalEpub());
  expect(out.endsWith('.kepub.epub')).toBe(true);
  // The Swift check used zipgrep; an EPUB is a deflated zip, so the marker is not
  // in the raw bytes and the entries have to be inflated to see it.
  const zip = await JSZip.loadAsync(readFileSync(out));
  const entries = Object.values(zip.files).filter((f) => !f.dir);
  const texts = await Promise.all(entries.map((f) => f.async('string')));
  expect(texts.some((t) => t.includes('koboSpan'))).toBe(true);
}, 120_000);

// The dialogue column, read back out of a REAL AZW3. The trio test at the
// top pins --disable-remove-fake-margins in the recipe; this checks what
// the flag is for. Calibre's remove-fake-margins heuristic reads a
// screenplay's dialogue column as publisher page margins and deletes them,
// whatever the unit, so a Calibre release that stopped honouring the flag
// would put every speech at full width on a Kindle while every recipe test
// stayed green. Exploding the book with `calibre-debug -x` and reading the
// .dialogue-block rule from the stylesheet its text links is the one check
// that looks at the output.
//
// Ported at piece F3 from the Swift app's own checks ("AZW3 keeps
// dialogue-block side margins"), which ran it weekly against brew's newest
// Calibre until that job went with the app. The engine-calibre job in
// weekly-toolchain.yml runs this file, so the weekly look survives.
//
// Screepub writes its own AZW3 now (src/azw3/), so no Kindle file is made
// this way any more; the check stays because the guards act on how Calibre
// READS the EPUB, before the output format is chosen, and that reading is
// the first half of every KFX build (toKfx passes the same trio). An AZW3 is
// simply the quickest output to explode and look at.
//
// What it does NOT prove, measured 2026-10-01 on Calibre 9.11.0: that the
// flag is still needed. With the flag deleted from the recipe this test
// stayed green, and so did the Swift check's own hand-built book run the
// same way: that Calibre left the 20% margins alone either way. So this is
// an OUTCOME check (the column survives AZW3, whatever Calibre does), and
// the flag stays on the strength of the device report that put it there
// (docs/formatting-options-log.md §2, 2026-07-22), not on this test.
const calibreDebug = calibreTool('calibre-debug');
const withCalibreDebug = calibreTool('ebook-convert') && calibreDebug ? bunTest : bunTest.skip;

function filesUnder(dir: string): string[] {
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((e) => e.isFile())
    .map((e) => join(e.parentPath, e.name));
}

withCalibreDebug('Calibre’s reading of our EPUB keeps the dialogue column’s side margins (real Calibre, an AZW3 exploded)', async () => {
  const dir = mkdtempSync(join(SCRATCH, 'azw3-geometry-'));
  const epub = join(dir, 'book.epub');
  // Two dozen speeches and one line of action, as the Swift check built
  // it: the heuristic looks for side margins on MOST blocks, which is what
  // a screenplay looks like.
  const speech = 'JO\nEnough dialogue that the fake-margin heuristic sees a screenplay.\n\n';
  const result = await convertFountain(`Title: Test\n\nINT. ROOM - DAY\n\nA hallway.\n\n${speech.repeat(24)}`);
  writeFileSync(epub, result.epub);
  const azw3 = join(dir, 'book.azw3');
  await runCalibre(calibreTool('ebook-convert')!, [epub, azw3, ...CALIBRE_FORMAT_GUARDS]);
  const exploded = join(dir, 'exploded');
  await runCalibre(calibreDebug!, ['-x', azw3, exploded]);

  const files = filesUnder(exploded);
  const pages = files.filter((f) => /\.x?html?$/i.test(f) && readFileSync(f, 'utf8').includes('dialogue-block'));
  expect(pages.length, `no exploded content file mentions dialogue-block: ${files.join(', ')}`).toBeGreaterThan(0);

  // Only stylesheets the page actually links, as the device would apply
  // them; a rule sitting in an unlinked file would prove nothing.
  const page = readFileSync(pages[0]!, 'utf8');
  const rules = files
    .filter((f) => f.endsWith('.css') && page.includes(basename(f)))
    .map((f) => readFileSync(f, 'utf8'))
    .flatMap((css) => {
      const at = css.indexOf('.dialogue-block');
      if (at < 0) return [];
      const after = css.slice(at);
      return [after.slice(0, after.indexOf('}') + 1)];
    });
  expect(rules.length, 'no linked stylesheet carries a .dialogue-block rule').toBeGreaterThan(0);
  const margin = `${DEFAULT_FORMAT_OPTIONS.dialogueSideMarginPct}%`;
  expect(rules.join(' '), `the dialogue column lost its ${margin} side margins`).toContain(margin);
}, 120_000);

// A Calibre that never finishes (Kindle Previewer waiting on a first-run
// dialog nobody can see) used to hold the book's turn in the window
// forever. These fakes stand in for it: a script that starts a CHILD that
// sleeps, records the child's pid, and waits on it, the way ebook-convert
// waits on Kindle Previewer.
function hangingTool(): { tool: string; childPid: string } {
  const dir = mkdtempSync(join(SCRATCH, 'hang-'));
  const tool = join(dir, 'ebook-convert');
  const childPid = join(dir, 'child.pid');
  writeFileSync(tool, `#!/bin/sh\nsleep 30 &\necho $! > "${childPid}"\nwait\n`);
  chmodSync(tool, 0o755);
  return { tool, childPid };
}

/** The pid hangingTool's script records for its child, read WITHOUT giving
 * the thread back. That is the point: runWithTimeout's stop is a setTimeout
 * on this same thread, so while this loop holds the thread the stop cannot
 * land, however long /bin/sh takes to start the child (about 160ms on an
 * idle Mac, more under a full suite). Waited for asynchronously, the stop
 * could kill the script before it wrote the file at all (child.pid missing,
 * 2026-10-01). */
function recordedChildPid(childPid: string, boundMs = 5_000): number {
  const deadline = Date.now() + boundMs;
  for (;;) {
    // The shell creates the file before it writes the pid into it, so an
    // empty read means "not yet", not "no pid".
    const text = existsSync(childPid) ? readFileSync(childPid, 'utf8') : '';
    if (text.endsWith('\n')) return Number(text.trim());
    if (Date.now() >= deadline) throw new Error(`the fake tool recorded no child within ${boundMs}ms`);
    Bun.sleepSync(5);
  }
}

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

test('a conversion is given ten minutes, not forever', () => {
  expect(CONVERSION_TIMEOUT_MS).toBe(10 * 60 * 1000);
});

test('a Calibre run that outlives its timeout is stopped, children and all, with a clear error', async () => {
  if (platform === 'win32') return; // the fake tool is a /bin/sh script
  const { tool, childPid } = hangingTool();
  const started = Date.now();
  // A timeout far shorter than the fake tool takes to start its child, held
  // off by recordedChildPid until the child exists: the stop always lands on
  // a tool with a child to kill, and the hold is exercised on every run, not
  // only on a loaded machine. Remove the hold and this fails every time.
  const run = runCalibre(tool, [], process.env, 1);
  const pid = recordedChildPid(childPid);
  expect(alive(pid)).toBe(true);
  await expect(run).rejects.toThrow(CalibreTimedOutError);
  await expect(run).rejects.toThrow(CalibreFailedError);
  await expect(run).rejects.toThrow('still running after');
  expect(Date.now() - started).toBeLessThan(10_000);
  // The grandchild is what actually hangs in real life (Kindle Previewer
  // under Calibre's plugin), so it must not outlive the stop either.
  // Polled rather than one fixed pause: a SIGKILLed process can take more
  // than 50ms to disappear when the whole suite is loading the machine,
  // which made a single check flaky (2026-10-01).
  for (let waited = 0; alive(pid) && waited < 2_000; waited += 25) await Bun.sleep(25);
  expect(alive(pid)).toBe(false);
}, 15_000);

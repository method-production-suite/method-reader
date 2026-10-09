import { afterAll, describe, test, expect } from 'bun:test';
import { mkdtempSync, writeFileSync, readFileSync, chmodSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, delimiter } from 'node:path';
import { platform } from 'node:process';
import {
  installKfxPlugin,
  previewerPath,
  kfxStatus,
  toKfx,
  kfxSibling,
  kfxScratchPath,
  computeReady,
  listsKfxOutput,
  pluginTableHeader,
  KfxToolchainNotReadyError,
  pluginInstalled,
  debugRunner,
  PLUGIN_LIST_TIMEOUT_MS,
  PLUGIN_INSTALL_TIMEOUT_MS,
} from '../src/export/kfx';
import { calibreTool, CALIBRE_FORMAT_GUARDS, CalibreTimedOutError } from '../src/export/calibre';
import { snippetStubs, systemPython, type SnippetScenario } from './kfx-snippet-stubs';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-export-kfx-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

test('Kindle Previewer is never found on Linux — Amazon ships no build', () => {
  if (platform === 'linux') expect(previewerPath()).toBeNull();
});

test('status.ready requires all three pieces', async () => {
  const status = await kfxStatus();
  expect(status.ready).toBe(status.calibre && status.previewer && status.pluginInstalled);
});

// computeReady: the AND behind `ready`, tested directly with synthetic
// booleans. This is what makes the conjunction provable on Linux, where
// `previewer` can never actually be true (Amazon ships no build) so the
// test above can never distinguish a correct three-way AND from a buggy
// two-way one. Each single-false case pins one term; a mutant dropping any
// one term from the AND fails at least one of these.
test('computeReady is true only when calibre, previewer, and pluginInstalled are all true', () => {
  expect(computeReady({ calibre: true, previewer: true, pluginInstalled: true })).toBe(true);
});

test('computeReady is false when calibre is false', () => {
  expect(computeReady({ calibre: false, previewer: true, pluginInstalled: true })).toBe(false);
});

test('computeReady is false when previewer is false', () => {
  expect(computeReady({ calibre: true, previewer: false, pluginInstalled: true })).toBe(false);
});

test('computeReady is false when pluginInstalled is false', () => {
  expect(computeReady({ calibre: true, previewer: true, pluginInstalled: false })).toBe(false);
});

test('status.calibre agrees with Calibre discovery', async () => {
  const status = await kfxStatus();
  expect(status.calibre).toBe(calibreTool('calibre-customize') !== null);
});

test('pluginInstalled is false whenever Calibre is absent', async () => {
  // The plugin lives inside Calibre, so it cannot be installed without it.
  const status = await kfxStatus();
  if (!status.calibre) expect(status.pluginInstalled).toBe(false);
});

test('on Linux the KFX rung is never ready, so the ladder degrades', async () => {
  if (platform === 'linux') expect((await kfxStatus()).ready).toBe(false);
});

test('toKfx refuses outright when Calibre is absent', async () => {
  // The conversion itself cannot be exercised without Kindle Previewer, which
  // Amazon ships for macOS and Windows only — that path is covered by the
  // hardware pass in piece C. What IS testable everywhere is that it fails
  // honestly rather than reporting success with no file.
  if (calibreTool('ebook-convert')) return;
  await expect(toKfx(join(SCRATCH, 'nonexistent.epub'))).rejects.toThrow(
    'ebook-convert was not found',
  );
});

// --- kfxSibling: exported per the controller ruling so a later task (the
// export ladder) imports this derivation instead of repeating the regex.
// Mirrors the Swift app's mobiSibling(for:) precedent.

test('kfxSibling derives same directory, same stem, .kfx extension', () => {
  expect(kfxSibling('/tmp/book.epub')).toBe('/tmp/book.kfx');
});

test('kfxSibling matches the extension case-insensitively', () => {
  expect(kfxSibling('/tmp/Book.EPUB')).toBe('/tmp/Book.kfx');
});

// --- toKfx driven for real against a fake ebook-convert. The tool and the
// status probe are injected rather than PATH-shadowed, because this machine
// (and any machine with Calibre installed) resolves ebook-convert from a
// fixed install path before PATH, and because `ready` can never be true on
// Linux. Everything else — argv, the scratch file, the rename — is real.

function fakeEbookConvert(): { tool: string; argvLog: string; workDir: string } {
  const toolDir = mkdtempSync(join(SCRATCH, 'kfx-fake-'));
  const name = platform === 'win32' ? 'ebook-convert.exe' : 'ebook-convert';
  const tool = join(toolDir, name);
  const argvLog = join(toolDir, 'argv.log');
  // Logs argv one entry per line, then touches the output path it was told
  // to write — mimicking ebook-convert's own file creation.
  writeFileSync(tool, `#!/bin/sh\nfor a in "$@"; do echo "$a"; done > "${argvLog}"\ntouch "$2"\n`);
  chmodSync(tool, 0o755);
  const workDir = mkdtempSync(join(SCRATCH, 'kfx-work-'));
  return { tool, argvLog, workDir };
}

const READY: Awaited<ReturnType<typeof kfxStatus>> = {
  calibre: true,
  previewer: true,
  pluginInstalled: true,
  ready: true,
};

test('toKfx writes to a .kfx scratch path and renames it onto kfxSibling', async () => {
  // Regression pin for the defect that made toKfx unrunnable: the scratch
  // path ended in `.tmp`, and ebook-convert picks its OUTPUT FORMAT from the
  // extension ("ValueError: No plugin to handle output format: tmp").
  // Verified against Calibre 8.7.0. This is the third time in this branch
  // that Calibre's extension rule has bitten, hence a test and not a comment.
  if (platform === 'win32') return; // the fake tool is a /bin/sh script
  const { tool, argvLog, workDir } = fakeEbookConvert();
  const epub = join(workDir, 'book.epub');
  writeFileSync(epub, 'fake epub bytes');
  // An older conversion already there is replaced by the rename itself,
  // with no delete first: a rename that failed after a delete would leave
  // the book with no .kfx at all.
  writeFileSync(kfxSibling(epub), 'an older conversion');

  const out = await toKfx(epub, undefined, { tool: () => tool, status: async () => READY });

  expect(out).toBe(kfxSibling(epub));
  expect(readFileSync(out, 'utf8')).toBe(''); // the fake's output, not the older one
  const argv = readFileSync(argvLog, 'utf8').split('\n').filter(Boolean);
  expect(argv[0]).toBe(epub);
  expect(argv[1].endsWith('.kfx')).toBe(true);
  expect(argv[1]).toBe(kfxScratchPath(epub));
  expect(argv.slice(2)).toEqual([...CALIBRE_FORMAT_GUARDS]);
  expect(existsSync(kfxScratchPath(epub))).toBe(false); // renamed away
  expect(existsSync(out)).toBe(true);
});

test('the scratch path is hidden, same-directory, and keeps the .kfx extension', () => {
  expect(kfxScratchPath('/tmp/dir/Book.EPUB')).toBe('/tmp/dir/.Book.partial.kfx');
});

test('toKfx refuses when the toolchain is not ready, naming what is missing', async () => {
  if (platform === 'win32') return;
  const { tool, workDir } = fakeEbookConvert();
  const epub = join(workDir, 'book.epub');
  writeFileSync(epub, 'fake epub bytes');
  const notReady = { calibre: true, previewer: false, pluginInstalled: false, ready: false };

  const attempt = toKfx(epub, undefined, { tool: () => tool, status: async () => notReady });

  await expect(attempt).rejects.toThrow(KfxToolchainNotReadyError);
  await expect(attempt).rejects.toThrow('KFX conversion needs Kindle Previewer and the KFX plugin.');
  expect(existsSync(kfxSibling(epub))).toBe(false); // and nothing was written
});

// --- Kindle Previewer's leftovers. The plugin hands the conversion to
// Kindle Previewer, and Previewer writes a <uuid>/ folder into $TMPDIR on
// every run (conv_out/, conversionLog.csv, an intermediate .mobi, about
// 250 KB) and never removes it. Measured 2026-09-22 against Calibre, the
// KFX Output plugin and Kindle Previewer 3: one real conversion, one new
// folder, and 189 of them had piled up on the machine that measured it.
// Previewer honours TMPDIR, so the fix is a temp folder per conversion
// that toKfx owns and removes. This fake does what Previewer does to
// whatever $TMPDIR it is handed, and writes down which folder that was.
function leakyEbookConvert(exitCode: number) {
  const toolDir = mkdtempSync(join(SCRATCH, 'kfx-leaky-'));
  const tool = join(toolDir, 'ebook-convert');
  const seen = join(toolDir, 'tmpdir.txt');
  const leftover = crypto.randomUUID();
  writeFileSync(tool, [
    '#!/bin/sh',
    'dir="${TMPDIR:-/tmp}"',
    `printf '%s' "$dir" > "${seen}"`,
    `mkdir -p "$dir/${leftover}/conv_out"`,
    `echo '"Type","Description"' > "$dir/${leftover}/conversionLog.csv"`,
    exitCode === 0 ? 'touch "$2"' : `echo 'Kindle Previewer failed' >&2; exit ${exitCode}`,
    '',
  ].join('\n'));
  chmodSync(tool, 0o755);
  const workDir = mkdtempSync(join(SCRATCH, 'kfx-work-'));
  const epub = join(workDir, 'book.epub');
  writeFileSync(epub, 'fake epub bytes');
  return {
    tool,
    epub,
    /** The temp folder the tool was given, and whether Previewer's folder
     *  is still in it. Removes that one folder if it is, so a failing run
     *  of this test does not add to the pile it is about. */
    after: () => {
      const dir = readFileSync(seen, 'utf8');
      const left = join(dir, leftover);
      const stillThere = { tmp: existsSync(dir), leftover: existsSync(left) };
      rmSync(left, { recursive: true, force: true });
      return stillThere;
    },
  };
}

test('toKfx gives Kindle Previewer its own temp folder and removes it, leftovers and all', async () => {
  if (platform === 'win32') return; // the fake tool is a /bin/sh script
  const fake = leakyEbookConvert(0);

  const out = await toKfx(fake.epub, undefined, { tool: () => fake.tool, status: async () => READY });

  expect(existsSync(out)).toBe(true); // the conversion itself still lands
  expect(fake.after()).toEqual({ tmp: false, leftover: false });
});

test('a failed conversion removes Kindle Previewer’s temp folder too', async () => {
  if (platform === 'win32') return;
  const fake = leakyEbookConvert(1);

  const attempt = toKfx(fake.epub, undefined, { tool: () => fake.tool, status: async () => READY });

  await expect(attempt).rejects.toThrow('Kindle Previewer failed');
  expect(fake.after()).toEqual({ tmp: false, leftover: false });
});

// --- listsKfxOutput: which plugin the listing names, read by column ------
//
// `calibre-customize --list-plugins` prints a table, not "a line per name".
// Captured from calibre 9.11 on macOS, 2026-09-24:
//
//   Type                  Name                                                  Version        Disabled       Site Customization
//
//   Conversion output     KFX Output                                            (2, 20, 1)     False
//   	 Convert e-books to the KFX format
//
//   Metadata writer       Set KFX metadata (from KFX Output)                    (2, 20, 1)     False
//   	 Set metadata in KFX files
//
// calibre builds that with '%-{T+1}s%-{N+1}s%-15s%-15s%s', where T and N are
// the longest type and name it holds, and prints the header through the same
// format. So the widths below are this machine's, not a constant: the
// narrower listing further down is the same table on a calibre with shorter
// names. The companion metadata plugin ships in the same zip, and its NAME
// contains "KFX Output", which is how a bare substring test reported the
// toolchain ready on a machine that could not write a KFX at all.
const calibreTable = (typeWidth: number, nameWidth: number) => {
  const line = (type: string, name: string, version: string, disabled = 'False') =>
    type.padEnd(typeWidth) + name.padEnd(nameWidth) + version.padEnd(15) + disabled.padEnd(15);
  return {
    header: 'Type'.padEnd(typeWidth) + 'Name'.padEnd(nameWidth) + 'Version'.padEnd(15)
      + 'Disabled'.padEnd(15) + 'Site Customization',
    kfxOutput: [line('Conversion output', 'KFX Output', '(2, 20, 1)'), '\t Convert e-books to the KFX format', ''],
    companion: [
      line('Metadata writer', 'Set KFX metadata (from KFX Output)', '(2, 20, 1)'), '\t Set metadata in KFX files', '',
    ],
    epubOutput: [line('Conversion output', 'EPUB Output', '(1, 0, 0)'), '\t Convert e-books to the EPUB format', ''],
    line,
  };
};
const measured = calibreTable(22, 54);
const listingOf = (table: ReturnType<typeof calibreTable>, ...rows: string[][]) =>
  [table.header, '', ...rows.flat()].join('\n');

describe('listsKfxOutput', () => {
  test('the companion metadata plugin alone is not the KFX Output plugin', () => {
    expect(listsKfxOutput(listingOf(measured, measured.epubOutput, measured.companion))).toBe(false);
  });

  test('both plugins, as the one zip installs them, is installed', () => {
    expect(listsKfxOutput(listingOf(measured, measured.kfxOutput, measured.companion, measured.epubOutput)))
      .toBe(true);
  });

  test('the conversion plugin alone is installed', () => {
    expect(listsKfxOutput(listingOf(measured, measured.epubOutput, measured.kfxOutput))).toBe(true);
  });

  test('the columns are read off the header, not assumed, so a narrower table still reads', () => {
    // A calibre whose longest type is "Conversion output" leaves ONE space
    // between the type and the name, and splitting on runs of spaces would
    // then read "output KFX Output" as the name.
    const narrow = calibreTable(18, 36);
    expect(listsKfxOutput(listingOf(narrow, narrow.companion, narrow.kfxOutput))).toBe(true);
    expect(listsKfxOutput(listingOf(narrow, narrow.companion))).toBe(false);
  });

  test('the plugin, or a renamed copy of it, counts; the companion and look-alikes do not', () => {
    // The old Swift app installed renamed copies of the plugin, such as
    // "KFX Output (Fix Traditional Chinese)". They are working KFX
    // converters, so the reader who has one keeps KFX. The rule is the name
    // itself or the name followed by " (": the companion metadata writer,
    // "Set KFX metadata (from KFX Output)", only ENDS in the name, and
    // neither a longer word nor a prefix of another name is a copy.
    const named = (name: string) =>
      listsKfxOutput(listingOf(measured, [measured.line('Conversion output', name, '(2, 12, 0)'), '']));
    expect(named('KFX Output')).toBe(true);
    expect(named('KFX Output (Fix Traditional Chinese)')).toBe(true);
    expect(named('KFX Output (fork)')).toBe(true);
    expect(named('Set KFX metadata (from KFX Output)')).toBe(false);
    expect(named('Legacy KFX Output')).toBe(false);
    expect(named('KFX Outputs')).toBe(false);
    expect(named('KFX Output(fork)')).toBe(false);
    // A renamed copy's own companion is not a converter either.
    expect(named('Set KFX metadata (from KFX Output (Fix Traditional Chinese))')).toBe(false);
  });

  test('a plugin Calibre lists as disabled still counts, because ebook-convert still converts with it', () => {
    // Measured 2026-09-24, calibre 9.11, on a scratch CALIBRE_CONFIG_DIRECTORY
    // holding a copy of KFX Output 2.20.1: after `calibre-customize
    // --disable-plugin "KFX Output"` the Disabled column reads True (False
    // otherwise), and `ebook-convert book.epub book.kfx` still wrote a real
    // KFX (a CONT container). calibre's plugin_for_output_format() does not
    // consult the disabled list; only the calibre window's own format menu
    // (available_output_formats) hides a disabled one. Screepub converts
    // through ebook-convert, so the column says nothing about whether it can.
    const disabled = [measured.line('Conversion output', 'KFX Output', '(2, 20, 1)', 'True'), ''];
    const enabled = [measured.line('Conversion output', 'KFX Output', '(2, 20, 1)', 'False'), ''];
    expect(listsKfxOutput(listingOf(measured, measured.companion, disabled))).toBe(true);
    expect(listsKfxOutput(listingOf(measured, measured.companion, enabled))).toBe(true);
  });

  test('a description line that mentions it is not a plugin row', () => {
    // Descriptions are printed indented by a tab, under their own row.
    const quoting = [
      measured.line('User interface action', 'KFX Helper', '(1, 0, 0)'),
      '\t Works alongside KFX Output',
      `\t${' '.repeat(21)}KFX Output`,
      '',
    ];
    expect(listsKfxOutput(listingOf(measured, quoting))).toBe(false);
  });

  test('Windows line endings read the same', () => {
    const crlf = listingOf(measured, measured.companion, measured.kfxOutput).split('\n').join('\r\n');
    expect(listsKfxOutput(crlf)).toBe(true);
  });

  test('a listing with no header row is not trusted to name anything', () => {
    // Without the header there are no columns to read; "not installed"
    // degrades to AZW3, where a guess could send the ladder into a KFX
    // conversion that cannot run.
    expect(listsKfxOutput(['', ...measured.kfxOutput].join('\n'))).toBe(false);
    expect(listsKfxOutput('')).toBe(false);
  });

  test('this machine’s Calibre, when it has one, still prints the header the columns come from', async () => {
    // The unit tests above pin the parse to a captured table. This pins the
    // capture to the real thing wherever the real thing exists, so a calibre
    // that changes its table fails here rather than quietly dropping every
    // Kindle to AZW3.
    // Found the way listsKfxOutput() finds it, not assumed to be line 0: a
    // calibre that printed a warning first would still parse, and this must
    // not fail on it any more than the parse does.
    // env: process.env carries the suite's scratch copy of Calibre's
    // settings folder; left out, Calibre gets .env.test's guard and fails
    // (tests/isolate-calibre-config.ts).
    const customize = calibreTool('calibre-customize');
    if (customize === null) return;
    const proc = Bun.spawn([customize, '--list-plugins'], { stdout: 'pipe', stderr: 'pipe', env: process.env });
    const [, stdout] = await Promise.all([proc.exited, new Response(proc.stdout).text()]);
    expect(pluginTableHeader(stdout)).toMatch(/^Type +Name +Version +Disabled +Site Customization$/);
  });

  test('the header is found wherever it is, and nowhere it is not', () => {
    const table = listingOf(measured, measured.kfxOutput);
    expect(pluginTableHeader(table)).toBe(measured.header);
    expect(pluginTableHeader(`calibre: a warning printed first\n${table}`)).toBe(measured.header);
    expect(listsKfxOutput(`calibre: a warning printed first\n${table}`)).toBe(true);
    expect(pluginTableHeader(measured.kfxOutput.join('\n'))).toBe(null);
  });
});

// --- pluginInstalled: exercised directly against a fake calibre-customize
// so the "KFX Output" substring check is actually proven, not just assumed.
// Without these, a mutant that hardcodes pluginInstalled to `calibre`
// (always true when Calibre is present) or to `false` (always, regardless
// of what calibre-customize reports) would both still pass every test above
// on this Linux dev machine, because calibre itself is absent here.

const describesPathScan = platform !== 'darwin';

function withFakeCustomize(pluginListing: string, run: (dir: string) => Promise<void>) {
  return async () => {
    if (!describesPathScan) return;
    if (calibreTool('calibre-customize')) return; // never shadow a real install
    const dir = mkdtempSync(join(SCRATCH, 'kfx-customize-'));
    const name = platform === 'win32' ? 'calibre-customize.exe' : 'calibre-customize';
    const fake = join(dir, name);
    writeFileSync(fake, `#!/bin/sh\ncat <<'EOF'\n${pluginListing}\nEOF\n`);
    chmodSync(fake, 0o755);
    const originalPath = process.env.PATH;
    process.env.PATH = `${dir}${delimiter}${originalPath ?? ''}`;
    try {
      await run(dir);
    } finally {
      process.env.PATH = originalPath;
    }
  };
}

test(
  'pluginInstalled is true when calibre-customize lists KFX Output',
  withFakeCustomize(listingOf(measured, measured.kfxOutput, measured.companion), async () => {
    const status = await kfxStatus();
    expect(status.calibre).toBe(true);
    expect(status.pluginInstalled).toBe(true);
  }),
);

test(
  'pluginInstalled is false when calibre-customize lists other plugins but not KFX Output',
  withFakeCustomize(listingOf(measured, measured.epubOutput), async () => {
    const status = await kfxStatus();
    expect(status.calibre).toBe(true);
    expect(status.pluginInstalled).toBe(false);
  }),
);

test(
  'pluginInstalled is false when calibre-customize lists only the companion metadata plugin',
  withFakeCustomize(listingOf(measured, measured.epubOutput, measured.companion), async () => {
    const status = await kfxStatus();
    expect(status.calibre).toBe(true);
    expect(status.pluginInstalled).toBe(false);
    expect(status.ready).toBe(false);
  }),
);

// ── installing jhowell's plugin without shipping a copy of it ────────
//
// The old Swift app carried a 485 KB GPL-3 zip of the KFX Output plugin and
// installed it with `calibre-customize -a`. That copy was pinned at 2.12.0
// AND was a fork, so the thing we shipped was already eight minor versions
// behind the plugin it claimed to be. Vendoring it also meant redistributing
// someone else's GPL-3 binary and keeping THIRD-PARTY-NOTICES honest about
// it.
//
// We do not ship it now. Calibre's own plugin index is the upstream, and
// Calibre's own `add_plugin` is the installer, so the newest version is
// whatever Calibre says it is on the day the user asks. Our part is one
// `calibre-debug -c` call and reading one JSON line back.
//
// This is a WRITE to the user's Calibre and it fetches third-party code over
// the network, so it is never automatic: something has to ask for it.
describe('installKfxPlugin', () => {
  const okLine = (v: string) =>
    `some calibre chatter\nSCREEPUB_RESULT ${JSON.stringify({ ok: true, version: v })}\n`;

  // Every test here injects `run`, so none of them wants Calibre to be
  // FOUND. But installKfxPlugin's second parameter defaults to
  // `calibreTool('calibre-debug')`, real discovery against real paths, and
  // when that answers null the function returns before the injected runner
  // is ever called. So these passed on a machine with Calibre installed and
  // failed on every runner without it, which is what left ci.yml red on
  // main from 2026-09-18 onward: six failures that were entirely about the
  // author's /Applications folder.
  //
  // Worse than the six, and the reason this is a fake PATH rather than a
  // self-skip: 'output with no result line is refused' asserts only that
  // ok is false, which a missing Calibre also produces. It passed on CI
  // while testing nothing at all. A test that cannot tell the difference
  // between the thing working and the thing being absent is the failure
  // mode a skip would have preserved.
  //
  // Not null either. null takes the no-Calibre branch, which is exactly
  // what the 'no Calibre is a named reason' test below is for.
  const TOOL = '/nowhere/calibre-debug';

  test('reports the version Calibre actually installed', async () => {
    const r = await installKfxPlugin(
      async () => ({ code: 0, stdout: okLine('2.20.1'), stderr: '' }),
      TOOL,
    );
    expect(r.ok).toBe(true);
    expect(r.version).toBe('2.20.1');
  });

  test('a conflicting KFX fork is removed, and named in the result', async () => {
    // Found the hard way on a real machine. The old Swift app's vendored
    // copy installs under the name "KFX Output (Fix Traditional Chinese)",
    // a fork. Installing the official plugin ALONGSIDE it does not
    // supersede it: both register the same internal Python package
    // (calibre_plugins.kfx_output), so the new plugin's code imports the
    // OLD fork's kfxlib and KFX conversion dies outright with
    // "cannot import name 'JobLog'". Every unit test passed and the
    // toolchain still reported ready; only converting a real book showed it.
    //
    // So the install has to clear variants, not just add. Leaving the
    // conflict is strictly worse than not installing at all.
    const r = await installKfxPlugin(async () => ({
      code: 0,
      stdout: `SCREEPUB_RESULT ${JSON.stringify({
        ok: true,
        version: '2.20.1',
        removed: ['KFX Output (Fix Traditional Chinese)'],
      })}\n`,
      stderr: '',
    }), TOOL);
    expect(r.ok).toBe(true);
    expect(r.removed).toEqual(['KFX Output (Fix Traditional Chinese)']);
  });

  test('the companion metadata writer is never removed', async () => {
    // The bug in my first cut of the guard, caught by running it for real.
    // "Set KFX metadata (from KFX Output)" ships INSIDE the same zip and is
    // a metadata writer, not a conversion output, so it cannot collide for
    // the .kfx slot. A name-only test matched it and deleted it; it only
    // survived because add_plugin put it straight back. The snippet now
    // asks calibre which plugins are conversion outputs rather than
    // guessing from the name.
    const r = await installKfxPlugin(async () => ({
      code: 0,
      stdout: `SCREEPUB_RESULT ${JSON.stringify({ ok: true, version: '2.20.1', removed: [] })}\n`,
      stderr: '',
    }), TOOL);
    expect(r.removed).not.toContain('Set KFX metadata (from KFX Output)');
  });

  test('a plain upgrade removes nothing', async () => {
    // "KFX Output" replacing "KFX Output" is calibre's own upgrade path and
    // must not be mistaken for a conflict.
    const r = await installKfxPlugin(async () => ({
      code: 0,
      stdout: `SCREEPUB_RESULT ${JSON.stringify({ ok: true, version: '2.20.1', removed: [] })}\n`,
      stderr: '',
    }), TOOL);
    expect(r.ok).toBe(true);
    expect(r.removed).toEqual([]);
  });

  test('no Calibre is a named reason, not a throw', async () => {
    // The ladder still works without KFX — it degrades to AZW3 then MOBI —
    // so a missing toolchain must never take the caller down with it.
    const r = await installKfxPlugin(async () => ({ code: 127, stdout: '', stderr: '' }), null);
    expect(r.ok).toBe(false);
    expect(r.reason).toMatch(/calibre/i);
  });

  test('a failed install carries Calibre’s own words', async () => {
    const r = await installKfxPlugin(async () => ({
      code: 1, stdout: '', stderr: 'urlopen error timed out',
    }), TOOL);
    expect(r.ok).toBe(false);
    expect(r.reason).toMatch(/timed out/);
  });

  test('output with no result line is refused rather than read as success', async () => {
    // exit 0 proves calibre-debug ran, not that the plugin landed.
    const r = await installKfxPlugin(
      async () => ({ code: 0, stdout: 'hello\n', stderr: '' }),
      TOOL,
    );
    expect(r.ok).toBe(false);
  });

  test('a result line saying failure is honoured over the exit code', async () => {
    const r = await installKfxPlugin(async () => ({
      code: 0,
      stdout: `SCREEPUB_RESULT ${JSON.stringify({ ok: false, error: 'size mismatch' })}\n`,
      stderr: '',
    }), TOOL);
    expect(r.ok).toBe(false);
    expect(r.reason).toMatch(/size mismatch/);
  });

  // A failure AFTER the fork-clearing step (add_plugin itself failing) has
  // already taken an older copy out. That is the one failure that changes
  // the reader's Calibre, so the names travel with it: the caller has to be
  // able to say what is now missing.
  test('a failure after an older copy was cleared still names what was removed', async () => {
    const r = await installKfxPlugin(async () => ({
      code: 0,
      stdout: `SCREEPUB_RESULT ${JSON.stringify({
        ok: false,
        error: 'add_plugin failed',
        removed: ['KFX Output (Fix Traditional Chinese)'],
      })}\n`,
      stderr: '',
    }), TOOL);
    expect(r).toEqual({
      ok: false,
      reason: 'add_plugin failed',
      removed: ['KFX Output (Fix Traditional Chinese)'],
    });
  });

  test('a failure before anything was cleared reports an empty removed list', async () => {
    const r = await installKfxPlugin(async () => ({
      code: 0,
      stdout: `SCREEPUB_RESULT ${JSON.stringify({ ok: false, error: 'size mismatch', removed: [] })}\n`,
      stderr: '',
    }), TOOL);
    expect(r.ok).toBe(false);
    expect(r.removed).toEqual([]);
  });

  // The most likely reason either network fetch (the index, or the zip
  // itself) fails is the user being offline, and unwrapped that arrives as
  // raw urllib/http exception text: no user should have to parse
  // "<urlopen error [Errno 8] nodename nor servname provided>". The snippet
  // marks both fetches and tags the result 'offline': true; this is the
  // friendly sentence installKfxPlugin reports instead. It names the SITE,
  // not the index: the second fetch is the plugin's zip, and a failed zip
  // download used to be reported as an unreachable index.
  test("an unreachable plugin site gets a friendly sentence, not urlopen's own text", async () => {
    const r = await installKfxPlugin(async () => ({
      code: 0,
      stdout: `SCREEPUB_RESULT ${JSON.stringify({
        ok: false,
        error: '<urlopen error [Errno 8] nodename nor servname provided>',
        offline: true,
      })}\n`,
      stderr: '',
    }), TOOL);
    expect(r.ok).toBe(false);
    // Its own sentence, because cli-kfx.ts puts "could not install the KFX
    // plugin: " in front of it: a reason that also began "could not reach"
    // read as a stutter.
    expect(r.reason).toBe(
      "Calibre's plugin site could not be reached. Check the internet connection, then try again.",
    );
    expect(r.reason).not.toContain('urlopen');
  });
});

// The install snippet is Python inside a TypeScript string, and nothing
// above runs it: every installKfxPlugin test hands back a canned
// SCREEPUB_RESULT line. These used to check the snippet's TEXT instead
// (which except clause came first, which line sat above which), and a text
// check passes on a snippet that fails the moment Python reads it. So these
// RUN it, under the system python3, against stand-ins for the few calibre
// names it imports (tests/kfx-snippet-stubs.ts), and go through
// installKfxPlugin's own runner seam so our parsing of its answer runs too.
// The real Calibre is never touched. Without python3 they skip.
const PYTHON = systemPython();

describe.skipIf(PYTHON === null)('the install snippet, run under python3 against stub calibre modules', () => {
  const TOOL = '/nowhere/calibre-debug';
  // The Swift app's vendored fork: same package, different NAME, so it
  // collides with the real plugin for the .kfx slot and has to go.
  const FORK = { name: 'KFX Output (Fix Traditional Chinese)', output: true };
  // Ships in the same zip as KFX Output but is a metadata writer, not an
  // output plugin. A name-only test once deleted it.
  const COMPANION = { name: 'Set KFX metadata (from KFX Output)', output: false };
  const REAL = { name: 'KFX Output', output: true };
  const installed = [REAL, FORK, COMPANION];
  const kinds = (events: string[]) => events.map((e) => e.split(' ')[0]);

  async function install(scenario: SnippetScenario) {
    const stubs = snippetStubs(mkdtempSync(join(SCRATCH, 'kfx-snippet-')), PYTHON!, scenario);
    const result = await installKfxPlugin(stubs.run, TOOL);
    const { stdout, stderr } = stubs.last();
    // Every run, failure or not, must end in the JSON line: a traceback
    // with no SCREEPUB_RESULT is the contract broken.
    expect(stdout, `no result line; stderr was:\n${stderr}`).toContain('SCREEPUB_RESULT ');
    return { result, events: stubs.events() };
  }

  test('installs, reports the version, and clears only the fork', async () => {
    const { result, events } = await install({ index: 'ok', zip: 'ok', plugins: installed });
    expect(result).toEqual({ ok: true, version: '2.20.1', removed: [FORK.name] });
    // The order is the point: fetch the index, download and check the zip,
    // and only then clear the fork and add the new copy.
    expect(kinds(events)).toEqual(['index', 'zip', 'remove', 'add']);
    expect(events).toContain(`remove ${FORK.name}`);
    expect(events).toContain('add zip-present');
    expect(events[1]).toBe('zip https://plugins.calibre-ebook.com/kfx_output.zip');
  });

  test('an unreachable index reports offline and leaves every plugin in place', async () => {
    const { result, events } = await install({ index: 'offline', zip: 'ok', plugins: installed });
    expect(result).toEqual({
      ok: false,
      reason: "Calibre's plugin site could not be reached. Check the internet connection, then try again.",
      removed: [],
    });
    expect(kinds(events)).toEqual(['index']);
  });

  test('an unreachable zip download reports offline and leaves every plugin in place', async () => {
    const { result, events } = await install({ index: 'ok', zip: 'offline', plugins: installed });
    expect(result.ok).toBe(false);
    expect(result.reason).toContain('could not be reached');
    expect(result.removed).toEqual([]);
    expect(kinds(events)).toEqual(['index', 'zip']);
  });

  // The fork-clearing used to run before the fetches, so an offline user
  // lost a working fork with nothing in its place. A download whose size
  // disagrees with the index, or that is not a plugin, must leave it too.
  test('a size mismatch is refused before any fork is cleared', async () => {
    const { result, events } = await install({ index: 'ok', zip: 'ok', sizeDelta: 7, plugins: installed });
    expect(result.ok).toBe(false);
    expect(result.reason).toMatch(/^downloaded \d+ bytes, index says \d+$/);
    expect(result.removed).toEqual([]);
    expect(kinds(events)).toEqual(['index', 'zip']);
  });

  test('a download that is not a calibre plugin is refused before any fork is cleared', async () => {
    const { result, events } = await install({ index: 'ok', zip: 'not-plugin', plugins: installed });
    expect(result).toEqual({ ok: false, reason: 'downloaded file is not a calibre plugin', removed: [] });
    expect(kinds(events)).toEqual(['index', 'zip']);
  });

  // The one failure that changes the reader's Calibre: the fork is already
  // gone when add_plugin fails, so the result has to say so.
  test('an add_plugin failure reports what was already removed', async () => {
    const { result, events } = await install({
      index: 'ok', zip: 'ok', plugins: installed, addFails: 'plugin rejected by calibre',
    });
    expect(result).toEqual({ ok: false, reason: 'plugin rejected by calibre', removed: [FORK.name] });
    expect(kinds(events)).toEqual(['index', 'zip', 'remove', 'add']);
  });

  // An index entry with no 'file' is a broken index, not a lost
  // connection: reporting it as offline sends the reader to check a
  // connection that is fine.
  test('an index entry with no file is a plain failure, not offline', async () => {
    const { result, events } = await install({ index: 'no-file', zip: 'ok', plugins: installed });
    expect(result.ok).toBe(false);
    expect(result.reason).not.toContain('could not be reached');
    expect(result.removed).toEqual([]);
    expect(kinds(events)).toEqual(['index']);
  });

  // A calibre that moves one of the names the snippet imports fails before
  // any line inside the try has done anything. The snippet still has to
  // answer with its JSON line rather than a NameError traceback, which is
  // why `class Unreachable` and `removed` are bound above the try.
  test('a failed calibre import still ends in a result line', async () => {
    const { result, events } = await install({ index: 'ok', zip: 'ok', plugins: installed, noIndexUrl: true });
    expect(result.ok).toBe(false);
    expect(result.reason).toContain('INDEX_URL');
    expect(result.removed).toEqual([]);
    expect(events).toEqual([]);
  });
});

// Nothing used to time out Calibre here, so a Kindle Previewer stuck on a
// dialog held the window's turn forever. The real commands are never run:
// a /bin/sh script that just sleeps stands in for each.
describe('every Calibre spawn here has a timeout', () => {
  function sleeper(): string {
    const dir = mkdtempSync(join(SCRATCH, 'sleeper-'));
    const tool = join(dir, 'calibre-tool');
    writeFileSync(tool, '#!/bin/sh\nsleep 5\n');
    chmodSync(tool, 0o755);
    return tool;
  }

  test('the limits: a minute to list plugins, five to install one', () => {
    expect(PLUGIN_LIST_TIMEOUT_MS).toBe(60 * 1000);
    expect(PLUGIN_INSTALL_TIMEOUT_MS).toBe(5 * 60 * 1000);
  });

  test('a plugin listing that hangs reads as not installed, so the ladder degrades', async () => {
    if (platform === 'win32') return;
    const started = Date.now();
    expect(await pluginInstalled(sleeper(), 200)).toBe(false);
    expect(Date.now() - started).toBeLessThan(4000);
  });

  test('an install that hangs is stopped and reported, never left running', async () => {
    if (platform === 'win32') return;
    const tool = sleeper();
    const started = Date.now();
    const result = await installKfxPlugin(debugRunner(200), tool);
    expect(Date.now() - started).toBeLessThan(4000);
    expect(result.ok).toBe(false);
    expect(result.reason).toContain('was stopped');
  });

  test('a KFX conversion that hangs is stopped, and its scratch and temp folder go with it', async () => {
    if (platform === 'win32') return;
    const workDir = mkdtempSync(join(SCRATCH, 'kfx-hang-'));
    const epub = join(workDir, 'book.epub');
    writeFileSync(epub, 'fake epub bytes');
    const tool = sleeper();
    await expect(
      toKfx(epub, undefined, { tool: () => tool, status: async () => READY, timeoutMs: 200 }),
    ).rejects.toThrow(CalibreTimedOutError);
    expect(existsSync(kfxScratchPath(epub))).toBe(false);
    expect(existsSync(kfxSibling(epub))).toBe(false);
  });
});

import { afterAll, beforeAll, describe, test, expect, beforeEach, afterEach } from 'bun:test';
import {
  chmodSync,
  cpSync,
  mkdtempSync,
  mkdirSync,
  rmSync,
  writeFileSync,
  readFileSync,
  readdirSync,
  existsSync,
  statSync,
  utimesSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { exportCommand } from '../src/cli-export';
import { availableFormats, type FreshKindleArtifactOptions } from '../src/export/artifact';
import { isCalibreAvailable } from '../src/export/calibre';
import { kfxStatus, type KfxStatus } from '../src/export/kfx';
import { DEFAULT_FORMAT_OPTIONS, type FormatOptions } from '../src/options';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-export-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const kfxReadyStatus: KfxStatus = { calibre: true, previewer: true, pluginInstalled: true, ready: true };
const calibreOnlyStatus: KfxStatus = { calibre: true, previewer: false, pluginInstalled: false, ready: false };
const noToolchainStatus: KfxStatus = { calibre: false, previewer: false, pluginInstalled: false, ready: false };
// The REAL toolchain's answer on this machine, asked once. It decides
// whether the one real-conversion test below runs at all.
const REAL_KFX_READY = (await kfxStatus()).ready;

let dir: string;
let epub: string;
let fountain: string;

// A real EPUB, produced by the engine, so the ladder has something honest
// to work from rather than a stub that every branch would reject. Built
// ONCE, not before every test: each build is a bun start-up and a full
// conversion, and every test got the same bytes. Each test still gets its
// own folder holding a copy, because tests write siblings (.kfx, .azw3,
// .mobi) and change timestamps beside it.
//
// The build is checked here and not trusted: a failed build used to leave
// no EPUB at all, and every test downstream then failed (or passed) for a
// reason that had nothing to do with what it was testing.
const BUILT = mkdtempSync(join(SCRATCH, 'built-'));
let buildAnswer: { ok?: boolean; epubPath?: string } = {};
beforeAll(() => {
  const source = join(BUILT, 'Script.fountain');
  const built = join(BUILT, 'Script.epub');
  writeFileSync(source, 'INT. ROOM - DAY\n\nMARGO\nHello.\n');
  const proc = Bun.spawnSync(['bun', 'src/cli.ts', source, '--json', '-o', built]);
  const stdout = proc.stdout.toString();
  if (proc.exitCode !== 0 || !existsSync(built)) {
    throw new Error(`the engine did not build the test EPUB (exit ${proc.exitCode}):\n${stdout}\n${proc.stderr.toString()}`);
  }
  buildAnswer = JSON.parse(stdout);
});

beforeEach(() => {
  dir = mkdtempSync(join(SCRATCH, 'export-'));
  epub = join(dir, 'Script.epub');
  fountain = join(dir, 'Script.fountain');
  // Timestamps kept, so the copy keeps the order the build left: the EPUB
  // no older than its .fountain.
  cpSync(join(BUILT, 'Script.fountain'), fountain, { preserveTimestamps: true });
  cpSync(join(BUILT, 'Script.epub'), epub, { preserveTimestamps: true });
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

test('the shared EPUB is a real one the engine built', () => {
  expect(buildAnswer.ok).toBe(true);
  expect(buildAnswer.epubPath).toBe(join(BUILT, 'Script.epub'));
  // An EPUB is a zip whose first entry is the uncompressed mimetype.
  const bytes = readFileSync(epub);
  expect(bytes.subarray(0, 2).toString('latin1')).toBe('PK');
  expect(bytes.toString('latin1', 30, 58)).toBe('mimetypeapplication/epub+zip');
});

describe('exportCommand', () => {
  test('epub is always available and is returned as itself', async () => {
    const result = await exportCommand({ epub, for: 'epub' });
    expect(result.path).toBe(epub);
    expect(result.format).toBe('epub');
    expect(result.extension).toBe('epub');
    // The label is the user-facing sentence; asserting it catches a wrapper
    // that invented its own wording instead of using formats.ts'.
    expect(result.label).toContain('for emailing to Kindle');
    expect(result.available).toContain('epub');
  });

  // The `epub` rung does no ladder work at all — a wrapper that fabricates
  // a stage ("selecting epub…") for a format that never touches the ladder
  // would pass every other assertion here.
  test('epub format does no ladder work, so it reports no stages', async () => {
    const result = await exportCommand({ epub, for: 'epub' });
    expect(result.stages).toEqual([]);
  });

  // The wrapper must not invent or hardcode its own idea of what this
  // machine can produce — it has to be exactly what availableFormats says,
  // computed with the SAME real calibreAvailable this process sees. A
  // wrapper that always answered ['epub', 'kindle'] (or always just
  // ['epub']) regardless of the machine would fail this on whichever
  // environment disagrees with its hardcoded answer.
  test('available is exactly availableFormats for this machine, not a hardcoded list', async () => {
    const result = await exportCommand({ epub, for: 'epub' });
    expect(result.available).toEqual(availableFormats(epub, isCalibreAvailable()));
  });

  // --- Kindle rung selection, driven deterministically through the seam ---
  //
  // freshKindleArtifact's own staleness rules and toolchain-error mapping
  // are already covered by tests/export-artifact.test.ts. What belongs to
  // THIS wrapper is: does it probe the right things, thread the resulting
  // state INTO the ladder, and report the extension/label/stages that state
  // implies? Injecting calibreAvailable/kfxStatus lets every rung be
  // asserted on every machine, regardless of what's actually installed —
  // real Calibre on Linux checks fixed paths before PATH (calibreTool's
  // candidatePaths), so a genuine install can't be shadowed the way the
  // rest of the codebase shadows a missing one.

  test('kindle rung: KFX ready selects the KFX rung', async () => {
    const fakeKfx = join(dir, 'Script.kfx');
    writeFileSync(fakeKfx, 'fake-kfx-bytes');
    const seen: { opts?: FreshKindleArtifactOptions } = {};
    const result = await exportCommand(
      { epub, for: 'kindle', fountain },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => kfxReadyStatus,
        freshKindleArtifact: async (opts) => {
          seen.opts = opts;
          opts.onStage?.('converting to KFX (Kindle Previewer can take ~20s to start)…');
          return fakeKfx;
        },
      },
    );
    // The ladder must be HANDED the ready state, not just told it exists —
    // a wrapper that forgot to thread kfxReady through picks a rung the
    // ladder never agreed to.
    expect(seen.opts?.kfxReady).toBe(true);
    expect(seen.opts?.calibreAvailable).toBe(true);
    expect(result.path).toBe(fakeKfx);
    expect(result.extension).toBe('kfx');
    expect(result.label).toContain('best quality');
    expect(result.stages.join(' ')).toContain('KFX');
  });

  test('kindle rung: Calibre present without KFX selects the AZW3 rung', async () => {
    const fakeAzw3 = join(dir, 'Script.azw3');
    writeFileSync(fakeAzw3, 'fake-azw3-bytes');
    const seen: { opts?: FreshKindleArtifactOptions } = {};
    const result = await exportCommand(
      { epub, for: 'kindle', fountain },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        freshKindleArtifact: async (opts) => {
          seen.opts = opts;
          opts.onStage?.('converting to AZW3 for Kindle…');
          return fakeAzw3;
        },
      },
    );
    expect(seen.opts?.kfxReady).toBe(false);
    expect(seen.opts?.calibreAvailable).toBe(true);
    expect(result.path).toBe(fakeAzw3);
    expect(result.extension).toBe('azw3');
    expect(result.label).not.toContain('best quality');
    expect(result.stages.join(' ')).toContain('AZW3');
  });

  test('kindle rung: neither Calibre nor KFX rebuilds a real MOBI from the .fountain', async () => {
    // The bottom rung needs no external toolchain at all — convertFountain
    // is the engine's own pure-TS code, not Calibre — so this runs through
    // the REAL ladder (no freshKindleArtifact fake) with only the PROBE
    // results forced via the seam, proving the wrapper's own detection
    // actually drives rung selection, not just its plumbing.
    const result = await exportCommand(
      { epub, for: 'kindle', fountain, optionsJson: '{"showSceneNumbers":true}' },
      { calibreAvailable: () => false, kfxStatus: async () => noToolchainStatus },
    );
    expect(result.extension).toBe('mobi');
    expect(result.path).toBe(join(dir, 'Script.mobi'));
    expect(existsSync(result.path)).toBe(true);
    expect(result.stages.join(' ')).toContain('rebuilding');
  });

  test('kindle rung: neither Calibre nor an existing .mobi refuses deterministically', async () => {
    let message = '';
    try {
      await exportCommand(
        { epub, for: 'kindle' }, // no fountain: the bottom rung cannot rebuild
        { calibreAvailable: () => false, kfxStatus: async () => noToolchainStatus },
      );
    } catch (err) {
      message = (err as Error).message;
    }
    expect(message).toMatch(/\.fountain/);
  });

  // Names the exact defect this piece is guarding against: a wrapper (or a
  // ladder call site) that reuses whatever sits beside the EPUB without
  // checking the staleness rule. Forced onto the MOBI rung via the seam, so
  // this asserts on every machine rather than self-skipping wherever real
  // Calibre happens to be installed.
  test('a stale .mobi beside the EPUB is rebuilt, not returned as-is', async () => {
    const mobiPath = join(dir, 'Script.mobi');
    writeFileSync(mobiPath, 'stale-placeholder');
    const past = new Date(Date.now() - 60_000);
    utimesSync(mobiPath, past, past); // older than the epub -> stale

    const result = await exportCommand(
      { epub, for: 'kindle', fountain },
      { calibreAvailable: () => false, kfxStatus: async () => noToolchainStatus },
    );
    expect(result.extension).toBe('mobi');
    expect(result.path).toBe(mobiPath);
    expect(readFileSync(result.path, 'utf8')).not.toBe('stale-placeholder');
    expect(result.stages.join(' ')).toContain('rebuilding');
  });

  // The inverse: a FRESH .mobi must be reused with NO stages at all — a
  // wrapper that fabricates a "reusing…" stage the ladder itself never
  // emits (freshKindleArtifact returns the cached path with no onStage
  // call) would fail this. Also forced deterministic via the seam.
  test('a fresh .mobi beside the EPUB is reused with no invented stages', async () => {
    const mobiPath = join(dir, 'Script.mobi');
    writeFileSync(mobiPath, 'already-built');
    const now = new Date();
    utimesSync(epub, new Date(now.getTime() - 10_000), new Date(now.getTime() - 10_000));
    utimesSync(mobiPath, now, now);

    const result = await exportCommand(
      { epub, for: 'kindle', fountain },
      { calibreAvailable: () => false, kfxStatus: async () => noToolchainStatus },
    );
    expect(result.extension).toBe('mobi');
    expect(result.path).toBe(mobiPath);
    expect(readFileSync(result.path, 'utf8')).toBe('already-built');
    expect(result.stages).toEqual([]);
  });

  // The one test in this file that cannot be made deterministic: whether
  // the REAL, non-injected path (real isCalibreAvailable, real kfxStatus,
  // real freshKindleArtifact) produces a genuine .kfx needs the real
  // toolchain to run and emit bytes. No seam can stand in for that without
  // testing a fake instead of the wiring.
  //
  // So it runs only where the KFX toolchain reports ready, and there it
  // demands a real .kfx. It used to run everywhere and pass on ANY error
  // that mentioned Calibre or Kindle, which a broken toolchain produces
  // too: on the one machine able to check the KFX rung, a KFX failure
  // passed. Where the toolchain is absent it skips, and the injected tests
  // above cover every rung.
  test.skipIf(!REAL_KFX_READY)('kindle rung on the real, ready KFX toolchain: a real .kfx file', async () => {
    const result = await exportCommand({ epub, for: 'kindle', fountain });
    expect(result.extension).toBe('kfx');
    expect(result.path).toBe(join(dir, 'Script.kfx'));
    expect(existsSync(result.path)).toBe(true);
    expect(statSync(result.path).size).toBeGreaterThan(0);
    // 2 minutes, not bun's default 5 seconds, and the difference is not
    // slack: it only runs where the toolchain is present, and there it
    // really converts. Measured 2026-09-14 on an M-series Mac with Calibre,
    // the KFX plugin and Kindle Previewer: 21s to a real .kfx, most of it
    // Previewer's cold start, which Amazon ships x86_64-only so it comes up
    // under Rosetta.
  }, 120_000);

  test('a missing EPUB is unreadable, not a ladder failure', async () => {
    let code = '';
    try {
      await exportCommand({ epub: join(dir, 'nope.epub'), for: 'kindle' });
    } catch (err) {
      code = (err as { code: string }).code;
    }
    expect(code).toBe('unreadable');
  });

  // The file check has to run BEFORE any toolchain probe or ladder call:
  // an implementation that asked Calibre or the ladder first and only
  // mapped ENOENT afterward could still land on 'unreadable' by accident,
  // but it would also spend the ~1s Calibre probe on a plain typo. This
  // pins the message too, so a wrapper that got the right code from the
  // wrong path (e.g. re-threw the ladder's own ENOENT) still has to name
  // the file.
  test('a missing EPUB names the path, not a generic ladder complaint', async () => {
    const missing = join(dir, 'nope.epub');
    let message = '';
    try {
      await exportCommand({ epub: missing, for: 'kindle' });
    } catch (err) {
      message = (err as Error).message;
    }
    expect(message).toContain(missing);
  });

  test('an unknown --for is a usage error naming the formats it accepts', async () => {
    let message = '';
    try {
      await exportCommand({ epub, for: 'pdf' });
    } catch (err) {
      message = (err as Error).message;
    }
    expect(message).toContain('epub');
    expect(message).toContain('kindle');
  });

  // Fix round 1: readFormat's own CliError('bad-options', …) was being
  // evaluated INSIDE the try that wraps the ladder call, so the catch's
  // unconditional `throw new CliError('export-failed', errorMessage(err))`
  // rewrapped it — right message, wrong code, and code is what a UI
  // branches on. Asserts the CODE, not just the message (a message-only
  // check passed against the bug), and proves the ladder is never even
  // reached: an injected fake that recorded a call would fail this if
  // readFormat's throw were still happening too late.
  test('malformed --options-json is bad-options, not export-failed', async () => {
    let ladderCalled = false;
    let error: unknown;
    try {
      await exportCommand(
        { epub, for: 'kindle', fountain, optionsJson: 'not json' },
        {
          calibreAvailable: () => true,
          kfxStatus: async () => calibreOnlyStatus,
          freshKindleArtifact: async () => {
            ladderCalled = true;
            return join(dir, 'Script.azw3');
          },
        },
      );
    } catch (err) {
      error = err;
    }
    expect((error as { code?: string } | undefined)?.code).toBe('bad-options');
    expect((error as Error | undefined)?.message).toContain('--options-json');
    expect(ladderCalled).toBe(false);
  });

  test('--options-json that parses but is not a JSON object is also bad-options', async () => {
    let code = '';
    try {
      await exportCommand({ epub, for: 'kindle', fountain, optionsJson: '[1,2,3]' });
    } catch (err) {
      code = (err as { code: string }).code;
    }
    expect(code).toBe('bad-options');
  });

  // The same rewrap could in principle swallow ANY CliError raised while a
  // kindle export is being resolved. 'unreadable' is thrown before the
  // toolchain is even probed, so it must win regardless of what else about
  // the request is also wrong — a malformed --options-json here must not
  // turn into 'bad-options' OR 'export-failed' ahead of the more
  // fundamental problem: there is no file to export at all.
  test('a missing EPUB is unreadable even when --options-json is also malformed', async () => {
    let code = '';
    try {
      await exportCommand({
        epub: join(dir, 'nope.epub'), for: 'kindle', optionsJson: 'not json',
      });
    } catch (err) {
      code = (err as { code: string }).code;
    }
    expect(code).toBe('unreadable');
  });
});

describe('exportCommand: app-wide defaults underneath --options-json', () => {
  // No app settings file at this path at all: readAppSettings reads a
  // missing file as {}, same as the production test-run guard.
  function noAppSettings(): string {
    return join(dir, 'no-such-settings.json');
  }
  function appSettingsWith(formatDefaults: Record<string, unknown>): string {
    const path = join(dir, `app-settings-${Math.random()}.json`);
    writeFileSync(path, JSON.stringify({ formatDefaults }));
    return path;
  }

  test('no --options-json, no app settings: the ladder gets the shipped defaults', async () => {
    const seen: { format?: FormatOptions } = {};
    await exportCommand(
      { epub, for: 'kindle', fountain },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        appSettingsPath: noAppSettings(),
        freshKindleArtifact: async (opts) => {
          seen.format = opts.format;
          return join(dir, 'Script.azw3');
        },
      },
    );
    expect(seen.format).toEqual(DEFAULT_FORMAT_OPTIONS);
  });

  test('no --options-json, app defaults set: the ladder gets THEM, not the shipped defaults', async () => {
    const seen: { format?: FormatOptions } = {};
    await exportCommand(
      { epub, for: 'kindle', fountain },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        appSettingsPath: appSettingsWith({ dialogueSideMarginPct: 5 }),
        freshKindleArtifact: async (opts) => {
          seen.format = opts.format;
          return join(dir, 'Script.azw3');
        },
      },
    );
    expect(seen.format?.dialogueSideMarginPct).toBe(5);
  });

  // The controller's decision: WITH --options-json too, it overlays on the
  // app defaults rather than on the shipped ones, so the precedence rule
  // (flags > app defaults > shipped) holds for export the same as it does
  // for a conversion.
  test('a PARTIAL --options-json overlays on the app defaults, not the shipped ones', async () => {
    const seen: { format?: FormatOptions } = {};
    await exportCommand(
      { epub, for: 'kindle', fountain, optionsJson: '{"justifyText":true}' },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        appSettingsPath: appSettingsWith({ dialogueSideMarginPct: 5 }),
        freshKindleArtifact: async (opts) => {
          seen.format = opts.format;
          return join(dir, 'Script.azw3');
        },
      },
    );
    // The knob --options-json actually named:
    expect(seen.format?.justifyText).toBe(true);
    // The knob it never mentioned: still the app default, not the shipped
    // one, proving the overlay landed on appDefaults and not on
    // DEFAULT_FORMAT_OPTIONS.
    expect(seen.format?.dialogueSideMarginPct).toBe(5);
  });

  // What the window actually sends (desktop/ui/send-routes.js's optionsJsonFor):
  // a FULL FormatOptions object read back from `screepub settings`, so an
  // app default the ladder would otherwise supply is already baked in and
  // this changes nothing observable for it. Proven here rather than
  // asserted from memory.
  test('a FULL --options-json wins outright, same as before this change', async () => {
    const full = { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 1 };
    const seen: { format?: FormatOptions } = {};
    await exportCommand(
      { epub, for: 'kindle', fountain, optionsJson: JSON.stringify(full) },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        appSettingsPath: appSettingsWith({ dialogueSideMarginPct: 5 }),
        freshKindleArtifact: async (opts) => {
          seen.format = opts.format;
          return join(dir, 'Script.azw3');
        },
      },
    );
    expect(seen.format).toEqual(full);
  });

  test('with no appSettingsPath given, it defaults to the production path (test-guarded)', async () => {
    const seen: { format?: FormatOptions } = {};
    await exportCommand(
      { epub, for: 'kindle', fountain },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        freshKindleArtifact: async (opts) => {
          seen.format = opts.format;
          return join(dir, 'Script.azw3');
        },
      },
    );
    expect(seen.format).toEqual(DEFAULT_FORMAT_OPTIONS);
  });
});

// Review fix: send-panel.js's own comment (ensureSettings) says
// "the Kindle rung that does will fall back to this script's defaults", but
// until now the export command never read the script's sidecar at all, only
// the app-wide ones. When export is told which script it is (--fountain),
// its base is THIS SCRIPT'S settings, resolved over the app defaults, same
// as a conversion's own precedence: flags > sidecar > app defaults >
// shipped. --fountain is optional (a bare `--for epub` never needs it, and
// the window leaves it out only when the conversion wrote no .fountain, as
// with a script converted from Fountain input; a window whose settings
// read failed drops --options-json instead, never --fountain), so when it
// is absent the base stays the app defaults, exactly as before this fix.
describe('exportCommand: the script\'s own sidecar, when --fountain names one', () => {
  function appSettingsWith(formatDefaults: Record<string, unknown>): string {
    const path = join(dir, `app-settings-${Math.random()}.json`);
    writeFileSync(path, JSON.stringify({ formatDefaults }));
    return path;
  }

  test('no --options-json: the ladder gets the sidecar\'s own value, not the app defaults', async () => {
    writeFileSync(join(dir, 'Script.screepub.json'), JSON.stringify({ dialogueSideMarginPct: 13 }));
    const seen: { format?: FormatOptions } = {};
    await exportCommand(
      { epub, for: 'kindle', fountain },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        appSettingsPath: appSettingsWith({ dialogueSideMarginPct: 5 }),
        freshKindleArtifact: async (opts) => {
          seen.format = opts.format;
          return join(dir, 'Script.azw3');
        },
      },
    );
    expect(seen.format?.dialogueSideMarginPct).toBe(13);
  });

  test('a partial --options-json moves the named knob and leaves the sidecar\'s other values standing', async () => {
    writeFileSync(
      join(dir, 'Script.screepub.json'),
      JSON.stringify({ dialogueSideMarginPct: 13, justifyText: true }),
    );
    const seen: { format?: FormatOptions } = {};
    await exportCommand(
      { epub, for: 'kindle', fountain, optionsJson: '{"dialogueSideMarginPct":24}' },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        appSettingsPath: appSettingsWith({ dialogueSideMarginPct: 5 }),
        freshKindleArtifact: async (opts) => {
          seen.format = opts.format;
          return join(dir, 'Script.azw3');
        },
      },
    );
    // The knob --options-json named:
    expect(seen.format?.dialogueSideMarginPct).toBe(24);
    // The sidecar's other value, untouched by either --options-json or the
    // app defaults:
    expect(seen.format?.justifyText).toBe(true);
  });

  test('no sidecar on disk: falls back to the app defaults, not the shipped ones', async () => {
    const seen: { format?: FormatOptions } = {};
    await exportCommand(
      { epub, for: 'kindle', fountain },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        appSettingsPath: appSettingsWith({ dialogueSideMarginPct: 5 }),
        freshKindleArtifact: async (opts) => {
          seen.format = opts.format;
          return join(dir, 'Script.azw3');
        },
      },
    );
    expect(seen.format?.dialogueSideMarginPct).toBe(5);
  });

  test('no --fountain at all: still the app defaults, unaffected by this fix', async () => {
    const seen: { format?: FormatOptions } = {};
    await exportCommand(
      { epub, for: 'kindle' },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        appSettingsPath: appSettingsWith({ dialogueSideMarginPct: 5 }),
        freshKindleArtifact: async (opts) => {
          seen.format = opts.format;
          return join(dir, 'Script.azw3');
        },
      },
    );
    expect(seen.format?.dialogueSideMarginPct).toBe(5);
  });
});

describe('exportCommand --check: which Kindle file, and whether it is built, without building it', () => {
  const never = async (): Promise<string> => {
    throw new Error('the ladder was not expected to run');
  };
  const older = (path: string) => {
    const at = new Date(Date.now() - 60_000);
    utimesSync(path, at, at);
  };

  test('KFX ready and no .kfx yet: kfx, not fresh, built by Calibre, and nothing is built', async () => {
    const result = await exportCommand(
      { epub, for: 'kindle', check: true },
      { calibreAvailable: () => true, kfxStatus: async () => kfxReadyStatus, freshKindleArtifact: never },
    );
    expect(result).toMatchObject({
      path: join(dir, 'Script.kfx'), format: 'kindle', extension: 'kfx', fresh: false, builtBy: 'calibre',
      stages: [], bookDate: statSync(epub).mtimeMs,
    });
    expect(result.label).toContain('best quality');
    expect(existsSync(join(dir, 'Script.kfx'))).toBe(false);
  });

  test('Calibre alone, with an .azw3 newer than the book: azw3, fresh', async () => {
    older(epub);
    writeFileSync(join(dir, 'Script.azw3'), 'azw3');
    const result = await exportCommand(
      { epub, for: 'kindle', check: true },
      { calibreAvailable: () => true, kfxStatus: async () => calibreOnlyStatus, freshKindleArtifact: never },
    );
    expect(result).toMatchObject({ extension: 'azw3', fresh: true, builtBy: 'calibre' });
  });

  test('no toolchain: the engine’s own mobi, which Screepub builds itself', async () => {
    const result = await exportCommand(
      { epub, for: 'kindle', check: true, fountain },
      { calibreAvailable: () => false, kfxStatus: async () => noToolchainStatus, freshKindleArtifact: never },
    );
    expect(result).toMatchObject({ extension: 'mobi', fresh: false, builtBy: 'screepub' });
    expect(existsSync(join(dir, 'Script.mobi'))).toBe(false);
  });

  test('an .azw3 older than the book is not fresh', async () => {
    writeFileSync(join(dir, 'Script.azw3'), 'azw3');
    older(join(dir, 'Script.azw3'));
    const result = await exportCommand(
      { epub, for: 'kindle', check: true },
      { calibreAvailable: () => true, kfxStatus: async () => calibreOnlyStatus, freshKindleArtifact: never },
    );
    expect(result.fresh).toBe(false);
  });

  test('the EPUB is the book itself: always there and current, and nothing builds it', async () => {
    const result = await exportCommand({ epub, for: 'epub', check: true }, { calibreAvailable: () => false });
    expect(result).toMatchObject({ path: epub, extension: 'epub', fresh: true });
    expect(result.builtBy).toBeUndefined();
  });

  test('--check writes nothing, so --out beside it is refused before anything is probed', async () => {
    let probed = 0;
    const deps = {
      calibreAvailable: () => { probed += 1; return true; },
      kfxStatus: async () => { probed += 1; return kfxReadyStatus; },
      freshKindleArtifact: never,
    };
    // Absolute or not: the refusal is about --check taking no --out at all,
    // not about the shape of a path it would never use.
    for (const out of [join(dir, 'Copy.kfx'), 'Copy.kfx']) {
      const err = await exportCommand({ epub, for: 'kindle', check: true, out }, deps)
        .catch((e: unknown) => e as { code?: string; message?: string });
      expect(`${out}: ${(err as { code?: string }).code}`).toBe(`${out}: usage`);
      expect((err as { message?: string }).message).toContain('--check');
    }
    expect(probed).toBe(0);
  });

  test('through the CLI: one object, the extension this machine makes, and no file beside the book', () => {
    const before = readdirSync(dir).sort();
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'export', epub, '--for', 'kindle', '--check', '--json'], {
      env: process.env,
    });
    const lines = proc.stdout.toString().trim().split('\n');
    expect(lines).toHaveLength(1);
    const answer = JSON.parse(lines[0]!);
    expect(answer.ok).toBe(true);
    expect(['kfx', 'azw3', 'mobi']).toContain(answer.extension);
    expect(typeof answer.fresh).toBe('boolean');
    expect(answer.builtBy).toBe(answer.extension === 'mobi' ? 'screepub' : 'calibre');
    expect(answer.bookDate).toBe(statSync(epub).mtimeMs);
    expect(readdirSync(dir).sort()).toEqual(before);
  });

  test('export --help names --check', () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'export', '--help']);
    expect(proc.stdout.toString()).toContain('--check');
  });
});

describe('screepub export (through the CLI)', () => {
  test('--json prints one object with the path and the available formats', () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'export', epub, '--for', 'epub', '--json']);
    const stdout = proc.stdout.toString();
    expect(stdout.trim().split('\n')).toHaveLength(1);
    const answer = JSON.parse(stdout);
    expect(answer.ok).toBe(true);
    expect(answer.path).toBe(epub);
    expect(answer.available).toContain('epub');
  });

  test('a failure is one JSON object with export-failed or unreadable', () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'export',
      join(dir, 'nope.epub'), '--for', 'kindle', '--json']);
    expect(proc.exitCode).toBe(1);
    const stdout = proc.stdout.toString();
    expect(stdout.trim().split('\n')).toHaveLength(1);
    const answer = JSON.parse(stdout);
    expect(answer.ok).toBe(false);
    expect(['unreadable', 'export-failed']).toContain(answer.error.code);
    // The exact contract shape: {code, message} and nothing stray riding
    // along that the app's decoder would choke on.
    expect(Object.keys(answer.error).sort()).toEqual(['code', 'message']);
    expect(typeof answer.error.message).toBe('string');
    expect(answer.error.message.length).toBeGreaterThan(0);
  });

  // The filename-shadowing rule (resolveCommand's, inherited by every verb):
  // a bare word is a verb only when no file of that name exists. Bare verb
  // word as argv[0], cwd pointing at the shadow file's directory — passing
  // an absolute path here would never match the verb string and silently
  // test nothing (the trap this piece has hit twice already).
  test('a file literally named `export` shadows the verb and is not converted', () => {
    const shadow = join(dir, 'export');
    writeFileSync(shadow, 'not a screenplay');
    const cliPath = join(process.cwd(), 'src', 'cli.ts');
    const proc = Bun.spawnSync(['bun', cliPath, 'export', '--json'], { cwd: dir });
    // Dispatched as `convert` on the shadow file, not the export verb: it
    // must NOT fail with export's usage error ("expected exactly one .epub").
    const stdout = proc.stdout.toString();
    const answer = JSON.parse(stdout);
    expect(answer.ok).toBe(false);
    expect(answer.error.code).not.toBe('usage');
  });
});

describe('--options-json validity does not depend on --for', () => {
  // The epub rung ignores the option VALUES — it converts nothing — but a
  // window sending malformed JSON must hear the same code either way.
  // Before this, `--for epub` returned before readFormat ran, so the same
  // argv was accepted on one rung and rejected on the other: a contract that
  // is valid or invalid depending on a second flag is one a UI cannot trust.
  test('malformed JSON is bad-options on the epub rung too', async () => {
    await expect(
      exportCommand({ epub, for: 'epub', optionsJson: '{oops' }),
    ).rejects.toMatchObject({ code: 'bad-options' });
  });

  test('malformed JSON is bad-options on the kindle rung', async () => {
    await expect(
      exportCommand(
        { epub, for: 'kindle', optionsJson: '{oops' },
        { calibreAvailable: () => false, kfxStatus: async () => noToolchainStatus },
      ),
    ).rejects.toMatchObject({ code: 'bad-options' });
  });
});

// --out: the window's save dialog raises the dialog and hands the engine an
// absolute path; the window never writes a file itself. This is the only
// place a copy of the artifact reaches a spot the user actually chose.
describe('exportCommand --out', () => {
  test('epub is copied to a new nested absolute path: parents created, bytes equal, answer path is out', async () => {
    const out = join(dir, 'nested', 'deeper', 'Saved.epub');
    const result = await exportCommand({ epub, for: 'epub', out });
    expect(result.path).toBe(out);
    expect(existsSync(out)).toBe(true);
    expect(readFileSync(out).equals(readFileSync(epub))).toBe(true);
  });

  test('a relative --out is refused on the epub rung, naming "absolute"', async () => {
    // Run with cwd inside SCRATCH: were the absolute check ever bypassed (a
    // regression, or a mutant), a relative path resolves against cwd and
    // would otherwise land in whatever directory happened to be current,
    // which must never be the repo itself.
    const before = process.cwd();
    process.chdir(dir);
    let error: unknown;
    try {
      await exportCommand({ epub, for: 'epub', out: 'relative.epub' });
    } catch (err) {
      error = err;
    } finally {
      process.chdir(before);
    }
    expect((error as { code?: string } | undefined)?.code).toBe('usage');
    expect((error as Error | undefined)?.message).toContain('absolute');
  });

  // The point of this one: the absolute check must fire before Calibre's
  // probe or the ladder ever runs, not merely before the ladder SUCCEEDS.
  // An injected ladder that records its own calls is the only way to prove
  // zero of them happened, rather than merely that the visible answer was
  // an error.
  test('a relative --out is refused before the kindle ladder runs, zero ladder calls', async () => {
    const before = process.cwd();
    process.chdir(dir); // see the note above: keep any stray write inside SCRATCH
    let ladderCalls = 0;
    let calibreCalls = 0;
    let error: unknown;
    try {
      await exportCommand(
        { epub, for: 'kindle', fountain, out: 'relative/path.mobi' },
        {
          calibreAvailable: () => {
            calibreCalls++;
            return false;
          },
          kfxStatus: async () => noToolchainStatus,
          freshKindleArtifact: async () => {
            ladderCalls++;
            return join(dir, 'Script.mobi');
          },
        },
      );
    } catch (err) {
      error = err;
    } finally {
      process.chdir(before);
    }
    expect((error as { code?: string } | undefined)?.code).toBe('usage');
    expect((error as Error | undefined)?.message).toContain('absolute');
    expect(calibreCalls).toBe(0);
    expect(ladderCalls).toBe(0);
  });

  test('the wrong extension for epub is refused before any copy, naming .epub', async () => {
    const out = join(dir, 'Saved.mobi');
    let error: unknown;
    try {
      await exportCommand({ epub, for: 'epub', out });
    } catch (err) {
      error = err;
    }
    expect((error as { code?: string } | undefined)?.code).toBe('usage');
    // The FILE's type is named, not the name the person typed: the book is
    // an EPUB whatever the save box said.
    expect((error as Error | undefined)?.message).toBe(
      'that is an EPUB file: choose a name ending in .epub',
    );
    expect(existsSync(out)).toBe(false);
  });

  test('kindle rung: out ending in the produced extension (.azw3) copies', async () => {
    const fakeAzw3 = join(dir, 'Script.azw3');
    writeFileSync(fakeAzw3, 'fake-azw3-bytes');
    const out = join(dir, 'ForKindle.azw3');
    const result = await exportCommand(
      { epub, for: 'kindle', fountain, out },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        freshKindleArtifact: async () => fakeAzw3,
      },
    );
    expect(result.path).toBe(out);
    expect(readFileSync(out, 'utf8')).toBe('fake-azw3-bytes');
  });

  // The extension is only known once the ladder answers: this is a rung
  // that would have made an .azw3, refused a save name ending .kfx, naming
  // the extension it actually would have produced.
  test('kindle rung: out ending .kfx is refused naming .azw3, after the ladder ran', async () => {
    const fakeAzw3 = join(dir, 'Script.azw3');
    writeFileSync(fakeAzw3, 'fake-azw3-bytes');
    const out = join(dir, 'ForKindle.kfx');
    let ladderCalls = 0;
    let error: unknown;
    try {
      await exportCommand(
        { epub, for: 'kindle', fountain, out },
        {
          calibreAvailable: () => true,
          kfxStatus: async () => calibreOnlyStatus,
          freshKindleArtifact: async () => {
            ladderCalls++;
            return fakeAzw3;
          },
        },
      );
    } catch (err) {
      error = err;
    }
    expect((error as { code?: string } | undefined)?.code).toBe('usage');
    expect((error as Error | undefined)?.message).toBe(
      'that is an AZW3 file: choose a name ending in .azw3',
    );
    // Unlike the relative-path check, the extension cannot be known until
    // the ladder has already run once: it fired exactly once, not zero.
    expect(ladderCalls).toBe(1);
    expect(existsSync(out)).toBe(false);
  });

  test('an existing file at out is replaced', async () => {
    const out = join(dir, 'Existing.epub');
    writeFileSync(out, 'stale-bytes-that-must-go');
    const result = await exportCommand({ epub, for: 'epub', out });
    expect(result.path).toBe(out);
    expect(readFileSync(out).equals(readFileSync(epub))).toBe(true);
  });

  test('no temp file is left beside the destination on success', async () => {
    const destDir = join(dir, 'clean-success');
    mkdirSync(destDir);
    const out = join(destDir, 'Saved.epub');
    await exportCommand({ epub, for: 'epub', out });
    expect(readdirSync(destDir)).toEqual(['Saved.epub']);
  });

  // Forces the copy's own rename to fail (a directory sits where the file
  // must go) so the catch-and-unlink path actually runs, not just the happy
  // path's implicit cleanup-by-rename.
  test('no temp file is left beside the destination on failure, and it answers export-failed', async () => {
    const destDir = join(dir, 'clean-failure');
    mkdirSync(destDir);
    const out = join(destDir, 'Saved.epub');
    mkdirSync(out);
    let error: unknown;
    try {
      await exportCommand({ epub, for: 'epub', out });
    } catch (err) {
      error = err;
    }
    expect((error as { code?: string } | undefined)?.code).toBe('export-failed');
    expect(readdirSync(destDir)).toEqual(['Saved.epub']);
  });

  // The system's own complaint names the hidden temp file the copy was
  // going through (`.<name>.<pid>.tmp`), a file the person never asked for
  // and cannot find. The answer names the path they chose instead.
  test('a folder that cannot be written: says where the copy was going, never the temp file', async () => {
    if (process.platform === 'win32') return; // chmod does not make a Windows folder read-only
    const destDir = join(dir, 'read-only');
    mkdirSync(destDir);
    const out = join(destDir, 'Saved.epub');
    chmodSync(destDir, 0o555);
    let error: { code?: string; message?: string } | undefined;
    try {
      await exportCommand({ epub, for: 'epub', out });
    } catch (err) {
      error = err as typeof error;
    } finally {
      chmodSync(destDir, 0o755);
    }
    expect(error?.code).toBe('export-failed');
    const message = error?.message ?? '';
    expect(message.startsWith(`could not save the copy to ${out}: `)).toBe(true);
    // A reason, not an empty tail.
    expect(message.length).toBeGreaterThan(`could not save the copy to ${out}: `.length);
    expect(message).not.toContain('.tmp');
    expect(message).not.toContain('.Saved.epub.');
    expect(readdirSync(destDir)).toEqual([]);
  });

  test('a copy that fails at the rename names the chosen path too, never the temp file', async () => {
    const destDir = join(dir, 'rename-failure');
    mkdirSync(destDir);
    const out = join(destDir, 'Saved.epub');
    mkdirSync(out); // a folder where the file must go: the copy lands, the rename cannot
    let error: { code?: string; message?: string } | undefined;
    try {
      await exportCommand({ epub, for: 'epub', out });
    } catch (err) {
      error = err as typeof error;
    }
    expect(error?.code).toBe('export-failed');
    expect(error?.message?.startsWith(`could not save the copy to ${out}: `)).toBe(true);
    expect(error?.message).not.toContain('.tmp');
  });

  test('out equal to the epub artifact\'s own path answers without copying', async () => {
    const past = new Date(Date.now() - 60_000);
    utimesSync(epub, past, past);
    const result = await exportCommand({ epub, for: 'epub', out: epub });
    expect(result.path).toBe(epub);
    // A copy-then-rename over itself would still succeed, but it would also
    // give the file a fresh mtime; this is the only way from outside to
    // tell "answered without touching it" from "copied it onto itself".
    expect(statSync(epub).mtime.getTime()).toBe(past.getTime());
  });

  test('kindle rung: out equal to the produced path answers without copying', async () => {
    const fakeAzw3 = join(dir, 'Script.azw3');
    writeFileSync(fakeAzw3, 'fake-azw3-bytes');
    const past = new Date(Date.now() - 60_000);
    utimesSync(fakeAzw3, past, past);
    const result = await exportCommand(
      { epub, for: 'kindle', fountain, out: fakeAzw3 },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => calibreOnlyStatus,
        freshKindleArtifact: async () => fakeAzw3,
      },
    );
    expect(result.path).toBe(fakeAzw3);
    expect(statSync(fakeAzw3).mtime.getTime()).toBe(past.getTime());
  });

  // A behavioral test cannot easily force a mid-copy interruption, so this
  // pins the mechanism at the source: the copy goes through a temp name in
  // the destination folder and a rename, never a direct write onto `out`.
  // Mirrors how tests/cli-kfx.test.ts already pins its own branch's shape.
  test('the copy goes through a temp name and a rename, not a direct write', () => {
    const src = readFileSync(new URL('../src/cli-export.ts', import.meta.url), 'utf8');
    expect(src).toMatch(/\.tmp`\);/);
    expect(src).toMatch(/await rename\(tmp, destination\)/);
  });
});

describe('screepub export --out (through the CLI)', () => {
  test('epub to a new absolute path, through the spawned CLI', () => {
    const out = join(dir, 'cli-nested', 'Saved.epub');
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'export', epub, '--for', 'epub', '--out', out, '--json']);
    const answer = JSON.parse(proc.stdout.toString());
    expect(answer.ok).toBe(true);
    expect(answer.path).toBe(out);
    expect(existsSync(out)).toBe(true);
  });

  test('export --help mentions --out', () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'export', '--help', '--json']);
    const answer = JSON.parse(proc.stdout.toString());
    expect(answer.ok).toBe(true);
    expect(answer.usage).toContain('--out');
  });
});

// --- Any file type on request (spec decision 11) ---------------------------
//
// Each explicit value builds only itself, never the ladder. Every test
// injects the toolchain answers AND the two Calibre builders, so no test
// here can reach a real Calibre even if the code under test is wrong: the
// seams that must not be used throw if they are asked at all.
describe('export --for one named file type', () => {
  const kfxNeverAsked = async (): Promise<KfxStatus> => {
    throw new Error('kfxStatus must not be asked for this value');
  };
  const ladderNeverRun = async (): Promise<string> => {
    throw new Error('the Kindle ladder must not run for this value');
  };
  const neverBuild = {
    kfx: async (): Promise<string> => {
      throw new Error('the KFX builder must not run here');
    },
    azw3: async (): Promise<string> => {
      throw new Error('the AZW3 builder must not run here');
    },
  };
  const safe = { freshKindleArtifact: ladderNeverRun, rungBuilders: neverBuild };
  async function codeAndMessage(run: Promise<unknown>): Promise<{ code?: string; message?: string }> {
    try {
      await run;
    } catch (err) {
      const e = err as { code?: string; message?: string };
      return { code: e.code, message: e.message };
    }
    return {};
  }
  function freshSibling(ext: string): string {
    const path = join(dir, `Script.${ext}`);
    writeFileSync(path, `fake-${ext}-bytes`);
    const later = new Date(statSync(epub).mtimeMs + 60_000);
    utimesSync(path, later, later);
    return path;
  }

  test('fountain returns the script itself and needs no toolchain', async () => {
    const result = await exportCommand(
      { epub, for: 'fountain', fountain },
      { calibreAvailable: () => false, kfxStatus: kfxNeverAsked, ...safe },
    );
    expect(result.path).toBe(fountain);
    expect(result.format).toBe('fountain');
    expect(result.extension).toBe('fountain');
    expect(result.stages).toEqual([]);
  });

  test('fountain with --out writes a byte-for-byte copy there', async () => {
    const out = join(dir, 'saved', 'Copy.fountain');
    const result = await exportCommand(
      { epub, for: 'fountain', fountain, out },
      { calibreAvailable: () => false, kfxStatus: kfxNeverAsked, ...safe },
    );
    expect(result.path).toBe(out);
    expect(readFileSync(out)).toEqual(readFileSync(fountain));
  });

  // A script's .fountain carries Screepub's own notes (registry #18, #19):
  // the cache needs them, a screenwriting app does not.
  const NOTED = [
    'INT. HUT - NIGHT',
    '',
    '[[fmt: sans +1]]',
    'CHYRON: LIVE.',
    '',
    '@MARA',
    '[[fmt: mono]] Read it back.',
    '',
    '[[img: p002-01.png w=71]]',
    '',
    "She looks up. [[a writer's own note]]",
    '',
  ].join('\n');

  test('fountain with --out leaves Screepub\'s own notes behind, and the script beside the book keeps them', async () => {
    writeFileSync(fountain, NOTED);
    const out = join(dir, 'saved', 'Copy.fountain');
    const result = await exportCommand(
      { epub, for: 'fountain', fountain, out },
      { calibreAvailable: () => false, kfxStatus: kfxNeverAsked, ...safe },
    );
    expect(result.path).toBe(out);
    expect(readFileSync(out, 'utf8')).toBe([
      'INT. HUT - NIGHT',
      '',
      'CHYRON: LIVE.',
      '',
      '@MARA',
      'Read it back.',
      '',
      "She looks up. [[a writer's own note]]",
      '',
    ].join('\n'));
    // The cache boundary is untouched.
    expect(readFileSync(fountain, 'utf8')).toBe(NOTED);
    // Nothing left beside the copy but the copy.
    expect(readdirSync(join(dir, 'saved'))).toEqual(['Copy.fountain']);
  });

  test('fountain --out onto the book\'s own script is refused when the script carries notes, and the script is left alone', async () => {
    writeFileSync(fountain, NOTED);
    const answer = await codeAndMessage(exportCommand(
      { epub, for: 'fountain', fountain, out: fountain },
      { calibreAvailable: () => false, kfxStatus: kfxNeverAsked, ...safe },
    ));
    expect(answer.code).toBe('export-failed');
    expect(answer.message).toContain(fountain);
    expect(readFileSync(fountain, 'utf8')).toBe(NOTED);
  });

  test('fountain --out onto the book\'s own script with no notes in it is already there', async () => {
    const before = readFileSync(fountain);
    const result = await exportCommand(
      { epub, for: 'fountain', fountain, out: fountain },
      { calibreAvailable: () => false, kfxStatus: kfxNeverAsked, ...safe },
    );
    expect(result.path).toBe(fountain);
    expect(readFileSync(fountain)).toEqual(before);
  });

  test('fountain without --fountain is usage', async () => {
    const answer = await codeAndMessage(exportCommand(
      { epub, for: 'fountain' },
      { calibreAvailable: () => false, kfxStatus: kfxNeverAsked, ...safe },
    ));
    expect(answer.code).toBe('usage');
  });

  test('fountain --check says the script is current, and builds nothing', async () => {
    const result = await exportCommand(
      { epub, for: 'fountain', fountain, check: true },
      { calibreAvailable: () => false, kfxStatus: kfxNeverAsked, ...safe },
    );
    expect(result).toMatchObject({ path: fountain, extension: 'fountain', fresh: true });
  });

  test('mobi rebuilds the engine MOBI beside the EPUB, never through Calibre', async () => {
    const result = await exportCommand(
      { epub, for: 'mobi', fountain },
      { calibreAvailable: () => true, kfxStatus: kfxNeverAsked, ...safe },
    );
    const mobi = join(dir, 'Script.mobi');
    expect(result.path).toBe(mobi);
    expect(result.format).toBe('mobi');
    expect(result.extension).toBe('mobi');
    // A real MOBI: the PalmDB header's type and creator.
    expect(readFileSync(mobi).toString('latin1', 60, 68)).toBe('BOOKMOBI');
    expect(existsSync(join(dir, 'Script.azw3'))).toBe(false);
    expect(existsSync(join(dir, 'Script.kfx'))).toBe(false);
  });

  test('mobi without --fountain is usage', async () => {
    const answer = await codeAndMessage(exportCommand(
      { epub, for: 'mobi' },
      { calibreAvailable: () => false, kfxStatus: kfxNeverAsked, ...safe },
    ));
    expect(answer.code).toBe('usage');
  });

  test('mobi --check reports the MOBI rung even on a machine that has KFX ready', async () => {
    const result = await exportCommand(
      { epub, for: 'mobi', fountain, check: true },
      { calibreAvailable: () => true, kfxStatus: async () => kfxReadyStatus, ...safe },
    );
    expect(result).toMatchObject({
      path: join(dir, 'Script.mobi'),
      extension: 'mobi',
      fresh: false,
      builtBy: 'screepub',
    });
  });

  test('fountain refuses a --fountain file that is not a .fountain, as usage, and writes nothing', async () => {
    const notScript = join(dir, 'Script.txt');
    writeFileSync(notScript, 'Title: Test\n');
    const out = join(dir, 'saved', 'Copy.fountain');
    const answer = await codeAndMessage(exportCommand(
      { epub, for: 'fountain', fountain: notScript, out },
      { calibreAvailable: () => false, kfxStatus: kfxNeverAsked, ...safe },
    ));
    expect(answer.code).toBe('usage');
    expect(answer.message).toContain('.fountain');
    expect(existsSync(out)).toBe(false);
  });

  test('mobi with a --fountain that cannot be read is unreadable, before --check and before any probe', async () => {
    const gone = join(dir, 'Gone.fountain');
    const calibreNeverAsked = () => {
      throw new Error('Calibre must not be probed for a script that cannot be read');
    };
    for (const check of [false, true]) {
      const before = readdirSync(dir).sort();
      const answer = await codeAndMessage(exportCommand(
        { epub, for: 'mobi', fountain: gone, ...(check ? { check: true } : {}) },
        { calibreAvailable: calibreNeverAsked, kfxStatus: kfxNeverAsked, ...safe },
      ));
      expect(`check=${check}: ${answer.code}`).toBe(`check=${check}: unreadable`);
      expect(answer.message).toBe(`cannot read the script to export: ${gone}`);
      expect(readdirSync(dir).sort()).toEqual(before);
    }
  });

  test('kfx with the toolchain not ready is export-failed, names what is missing, and writes nothing', async () => {
    const before = readdirSync(dir).sort();
    const answer = await codeAndMessage(exportCommand(
      { epub, for: 'kfx', fountain },
      { calibreAvailable: () => true, kfxStatus: async () => calibreOnlyStatus, ...safe },
    ));
    expect(answer.code).toBe('export-failed');
    expect(answer.message).toContain('this computer cannot make a KFX file yet');
    expect(answer.message).toContain('Kindle Previewer');
    expect(answer.message).toContain('the KFX plugin');
    expect(readdirSync(dir).sort()).toEqual(before);
  });

  test('kfx never falls back to AZW3, even with Calibre present', async () => {
    const answer = await codeAndMessage(exportCommand(
      { epub, for: 'kfx', fountain },
      { calibreAvailable: () => true, kfxStatus: async () => noToolchainStatus, ...safe },
    ));
    expect(answer.code).toBe('export-failed');
    expect(answer.message).toContain('Calibre');
    expect(existsSync(join(dir, 'Script.azw3'))).toBe(false);
  });

  test('kfx with the toolchain ready builds through the KFX builder only', async () => {
    const built: string[] = [];
    const fakeKfx = join(dir, 'Script.kfx');
    const result = await exportCommand(
      { epub, for: 'kfx', fountain },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => kfxReadyStatus,
        freshKindleArtifact: ladderNeverRun,
        rungBuilders: {
          ...neverBuild,
          kfx: async (book, onStage) => {
            built.push(book);
            onStage?.('converting to KFX');
            return fakeKfx;
          },
        },
      },
    );
    expect(built).toEqual([epub]);
    expect(result).toMatchObject({ path: fakeKfx, format: 'kfx', extension: 'kfx', stages: ['converting to KFX'] });
  });

  test('azw3 builds through the AZW3 builder even when KFX is ready', async () => {
    const built: string[] = [];
    const fakeAzw3 = join(dir, 'Script.azw3');
    const result = await exportCommand(
      { epub, for: 'azw3', fountain },
      {
        calibreAvailable: () => true,
        kfxStatus: async () => kfxReadyStatus,
        freshKindleArtifact: ladderNeverRun,
        rungBuilders: {
          ...neverBuild,
          azw3: async (book) => {
            built.push(book);
            return fakeAzw3;
          },
        },
      },
    );
    expect(built).toEqual([epub]);
    expect(result).toMatchObject({ path: fakeAzw3, format: 'azw3', extension: 'azw3' });
  });

  test('azw3 without Calibre is export-failed', async () => {
    const answer = await codeAndMessage(exportCommand(
      { epub, for: 'azw3', fountain },
      { calibreAvailable: () => false, kfxStatus: kfxNeverAsked, ...safe },
    ));
    expect(answer.code).toBe('export-failed');
    expect(answer.message).toBe('this computer cannot make an AZW3 file without Calibre');
  });

  test('azw3 --check with a fresh sibling reports it fresh and built by Calibre, even with KFX ready', async () => {
    const azw3 = freshSibling('azw3');
    const result = await exportCommand(
      { epub, for: 'azw3', check: true },
      { calibreAvailable: () => true, kfxStatus: async () => kfxReadyStatus, ...safe },
    );
    expect(result).toMatchObject({ path: azw3, format: 'azw3', extension: 'azw3', fresh: true, builtBy: 'calibre' });
    expect(result.bookDate).toBe(statSync(epub).mtimeMs);
  });

  test('azw3 reuses a fresh sibling without building', async () => {
    const azw3 = freshSibling('azw3');
    const result = await exportCommand(
      { epub, for: 'azw3' },
      { calibreAvailable: () => true, kfxStatus: async () => kfxReadyStatus, ...safe },
    );
    expect(result.path).toBe(azw3);
    expect(readFileSync(azw3, 'utf8')).toBe('fake-azw3-bytes');
  });

  test('kfx --check with a fresh sibling reports it, never the AZW3 beside it', async () => {
    freshSibling('azw3');
    const kfx = freshSibling('kfx');
    const result = await exportCommand(
      { epub, for: 'kfx', check: true },
      { calibreAvailable: () => true, kfxStatus: async () => kfxReadyStatus, ...safe },
    );
    expect(result).toMatchObject({ path: kfx, extension: 'kfx', fresh: true, builtBy: 'calibre' });
  });

  test('an unknown value is usage, and the sentence lists all six', async () => {
    const answer = await codeAndMessage(exportCommand({ epub, for: 'nook' }, safe));
    expect(answer.code).toBe('usage');
    expect(answer.message).toBe('--for takes epub, kindle, kfx, azw3, mobi, fountain or fdx, not "nook"');
  });

  test('--out with the wrong extension is usage for every new value, before any work', async () => {
    for (const value of ['kfx', 'azw3', 'mobi', 'fountain']) {
      const answer = await codeAndMessage(exportCommand(
        { epub, for: value, fountain, out: join(dir, 'Wrong.epub') },
        { calibreAvailable: () => true, kfxStatus: async () => kfxReadyStatus, ...safe },
      ));
      expect(`${value}: ${answer.code} ${answer.message}`).toBe(
        `${value}: usage that is ${value === 'azw3' ? 'an' : 'a'} ${value.toUpperCase()} file: choose a name ending in .${value}`,
      );
    }
    expect(existsSync(join(dir, 'Wrong.epub'))).toBe(false);
    expect(existsSync(join(dir, 'Script.mobi'))).toBe(false);
  });

  test('export --help lists the six values', () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'export', '--help', '--json']);
    const answer = JSON.parse(proc.stdout.toString());
    expect(answer.usage).toContain('epub|kindle|kfx|azw3|mobi|fountain');
  });
});

// The `app-settings` verb: reads and writes `libraryPath` and
// `formatDefaults`, the two app-wide settings piece C owns in the shared
// settings file (`src/settings/app.ts`). Most of this file drives the
// handler IN PROCESS with an injected settingsPath, which never touches a
// real settings file regardless of the test-run guard; the last describe
// spawns the real CLI, guarded by an explicit SCREEPUB_CONFIG_DIR the same
// way tests/cli-app-defaults.test.ts does.
import { afterAll, describe, expect, test } from 'bun:test';
import {
  chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { appSettingsCommand } from '../src/cli-app-settings';
import { DEFAULT_FORMAT_OPTIONS } from '../src/options';
import { READER_DEVICES, readAppSettings, writeAppSettings } from '../src/settings/app';

const ROOT = new URL('..', import.meta.url).pathname;
const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-app-settings-'));

afterAll(() => {
  // The unwritable-folder test leaves a 0o500 directory behind; put it back
  // before rm, or the cleanup fails and the next run inherits it.
  for (const dir of readdirSync(SCRATCH)) {
    try { chmodSync(join(SCRATCH, dir), 0o700); } catch { /* not a directory we locked */ }
  }
  rmSync(SCRATCH, { recursive: true, force: true });
});

let counter = 0;
function scratch(name: string): string {
  const dir = join(SCRATCH, `${name}-${counter++}`);
  mkdirSync(dir, { recursive: true });
  return dir;
}

/** A settings.json path inside its own fresh scratch folder, unwritten. */
function settingsFile(): string {
  return join(scratch('settings'), 'settings.json');
}

// This HOME does not exist, same reasoning as tests/library.test.ts: nothing
// here may depend on, or touch, a real home directory.
const HOME = '/home/ada';
function env(extra: Record<string, string | undefined> = {}): Record<string, string | undefined> {
  return { HOME, ...extra };
}

function codeOf(err: unknown): string {
  return (err as { code?: string }).code ?? '';
}

describe('appSettingsCommand: a plain read', () => {
  test('no settings file: shipped defaults, no chosen folder, not customized', () => {
    const file = settingsFile();
    const result = appSettingsCommand({}, { settingsPath: file, platform: 'linux', env: env() });
    expect(result.file).toBe(file);
    expect(result.library.chosen).toBeNull();
    expect(result.library.path).toBe(join(HOME, 'Documents', 'Screepub'));
    expect(result.library.platformDefault).toBe(result.library.path);
    expect(result.library.fromEnv).toBe(false);
    expect(result.home).toBe(HOME);
    expect(result.formatDefaults).toEqual(DEFAULT_FORMAT_OPTIONS);
    expect(result.shippedDefaults).toEqual(DEFAULT_FORMAT_OPTIONS);
    expect(result.customized).toBe(false);
    // A read must not create the file it reads.
    expect(existsSync(file)).toBe(false);
  });
});

describe('appSettingsCommand: setting and resetting libraryPath', () => {
  test('a good absolute folder is stored, created, and reported back as chosen', () => {
    const file = settingsFile();
    const chosen = join(scratch('chosen-lib'), 'not-yet-made');
    const result = appSettingsCommand(
      { set: JSON.stringify({ libraryPath: chosen }) },
      { settingsPath: file, platform: 'linux', env: env() },
    );
    expect(result.library.chosen).toBe(chosen);
    expect(result.library.path).toBe(chosen);
    expect(readAppSettings(file).libraryPath).toBe(chosen);
    // The check IS making the folder, per the spec.
    expect(existsSync(chosen)).toBe(true);
  });

  test('null resets it: the key is removed, not written as null', () => {
    const file = settingsFile();
    const chosen = scratch('chosen-lib');
    appSettingsCommand({ set: JSON.stringify({ libraryPath: chosen }) }, { settingsPath: file, platform: 'linux', env: env() });

    const result = appSettingsCommand(
      { set: JSON.stringify({ libraryPath: null }) },
      { settingsPath: file, platform: 'linux', env: env() },
    );
    expect(result.library.chosen).toBeNull();
    expect(result.library.path).toBe(join(HOME, 'Documents', 'Screepub'));
    expect('libraryPath' in readAppSettings(file)).toBe(false);
  });
});

describe('appSettingsCommand: setting and resetting formatDefaults', () => {
  test('an object is stored as the FULL resolved options, clamped', () => {
    const file = settingsFile();
    // 999 is well outside dialogueSideMarginPct's 0-30 range, so a clamped
    // 30 in the answer proves resolveFormatOptions ran, not a raw echo.
    const result = appSettingsCommand(
      { set: JSON.stringify({ formatDefaults: { dialogueSideMarginPct: 999 } }) },
      { settingsPath: file, platform: 'linux', env: env() },
    );
    expect(result.formatDefaults.dialogueSideMarginPct).toBe(30);
    expect(Object.keys(result.formatDefaults).sort()).toEqual(Object.keys(DEFAULT_FORMAT_OPTIONS).sort());
    expect(result.customized).toBe(true);

    const onDisk = readAppSettings(file).formatDefaults as Record<string, unknown>;
    expect(onDisk.dialogueSideMarginPct).toBe(30);
    // The FULL object landed on disk, not the one key the caller sent.
    expect(Object.keys(onDisk).sort()).toEqual(Object.keys(DEFAULT_FORMAT_OPTIONS).sort());
  });

  test('null resets it: the key is removed, customized goes back to false', () => {
    const file = settingsFile();
    appSettingsCommand(
      { set: JSON.stringify({ formatDefaults: { justifyText: true } }) },
      { settingsPath: file, platform: 'linux', env: env() },
    );
    const result = appSettingsCommand(
      { set: JSON.stringify({ formatDefaults: null }) },
      { settingsPath: file, platform: 'linux', env: env() },
    );
    expect(result.formatDefaults).toEqual(DEFAULT_FORMAT_OPTIONS);
    expect(result.customized).toBe(false);
    expect('formatDefaults' in readAppSettings(file)).toBe(false);
  });
});

describe('appSettingsCommand: refusals before anything is written', () => {
  test('an unknown key is usage, naming the allowed keys, and writes nothing', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand({ set: JSON.stringify({ theme: 'dark' }) }, { settingsPath: file, platform: 'linux', env: env() });
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('usage');
    expect((err as Error).message).toContain('libraryPath');
    expect((err as Error).message).toContain('formatDefaults');
    expect(existsSync(file)).toBe(false);
  });

  test('lastRoute is refused here: it belongs to piece B, not writable through this verb', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand({ set: JSON.stringify({ lastRoute: 'kindle' }) }, { settingsPath: file, platform: 'linux', env: env() });
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('usage');
    expect(existsSync(file)).toBe(false);
  });

  test('--set that is not valid JSON is bad-settings, nothing written', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand({ set: '{oops' }, { settingsPath: file, platform: 'linux', env: env() });
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect(existsSync(file)).toBe(false);
  });

  test('--set as a JSON array is bad-settings, not silently accepted', () => {
    // typeof [] === 'object', so this has to be checked explicitly.
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand({ set: '[1,2,3]' }, { settingsPath: file, platform: 'linux', env: env() });
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect(existsSync(file)).toBe(false);
  });

  test('libraryPath that is not a string or null is bad-settings', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand({ set: JSON.stringify({ libraryPath: 7 }) }, { settingsPath: file, platform: 'linux', env: env() });
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect(existsSync(file)).toBe(false);
  });

  test('formatDefaults that is not an object or null is bad-settings', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand({ set: JSON.stringify({ formatDefaults: 'kindleEink' }) }, { settingsPath: file, platform: 'linux', env: env() });
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect(existsSync(file)).toBe(false);
  });

  test('a relative libraryPath is refused, and nothing is written', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand({ set: JSON.stringify({ libraryPath: 'Scripts' }) }, { settingsPath: file, platform: 'linux', env: env() });
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect((err as Error).message).toContain('full path');
    expect(existsSync(file)).toBe(false);
  });

  test('a path through a regular file is refused, not a stack trace', () => {
    const dir = scratch('blocker');
    const blocker = join(dir, 'not-a-dir');
    writeFileSync(blocker, 'x');
    const target = join(blocker, 'library');
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand({ set: JSON.stringify({ libraryPath: target }) }, { settingsPath: file, platform: 'linux', env: env() });
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect(existsSync(file)).toBe(false);
  });

  test('a regular file already sitting at the exact libraryPath is refused in plain words, and a good formatDefaults alongside it is not stored either', () => {
    // mkdirSync(recursive) on a path that already exists AS A FILE (not a
    // parent segment, the exact target) fails EEXIST, not ENOTDIR. Node's
    // own text for it ("EEXIST: file already exists, mkdir '...'") must not
    // reach the caller raw.
    //
    // formatDefaults here is GOOD (passes its own pure check), which is the
    // point: this failure happens in PHASE 2, the folder probe, after
    // formatDefaults has already been resolved and sat waiting in `write`.
    // A mutant that stored formatDefaults before probing the folder (rather
    // than only once writeAppSettings itself is reached) would still pass
    // every other test in this file, since none of them combine a good
    // formatDefaults with a libraryPath that fails this late.
    const already = join(scratch('eexist'), 'already-a-file');
    writeFileSync(already, 'x');
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand(
        { set: JSON.stringify({ libraryPath: already, formatDefaults: { justifyText: true } }) },
        { settingsPath: file, platform: 'linux', env: env() },
      );
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect((err as Error).message).toContain('that is a file, not a folder');
    expect((err as Error).message).not.toContain('EEXIST');
    expect(existsSync(file)).toBe(false);
  });

  // Root bypasses ordinary permission bits, and Windows has no chmod-style
  // read-only-directory story that mkdirSync/accessSync would trip on here.
  test.skipIf(process.platform === 'win32' || process.getuid?.() === 0)(
    'an unwritable folder is refused, and nothing is written',
    () => {
      const locked = scratch('locked');
      chmodSync(locked, 0o500);
      const target = join(locked, 'child');
      const file = settingsFile();
      let err: unknown;
      try {
        appSettingsCommand({ set: JSON.stringify({ libraryPath: target }) }, { settingsPath: file, platform: 'linux', env: env() });
      } catch (e) { err = e; }
      expect(codeOf(err)).toBe('bad-settings');
      expect((err as Error).message).toContain('permission denied');
      // The mapped phrase, not the raw code: today's message would pass a
      // bare toContain('permission denied') even with the EACCES mapping
      // deleted, because Node's own "EACCES: permission denied, mkdir ..."
      // happens to contain that same substring. This is the assertion that
      // actually proves the mapping ran.
      expect((err as Error).message).not.toContain('EACCES');
      expect(existsSync(file)).toBe(false);
    },
  );

  // The test above targets a CHILD of the locked folder, so mkdirSync fails
  // on the parent and accessSync's own W_OK check is never reached. This
  // one targets the locked folder ITSELF, which already exists: mkdirSync
  // is then a no-op, and accessSync is what actually refuses it.
  test.skipIf(process.platform === 'win32' || process.getuid?.() === 0)(
    'the write-permission check itself refuses an existing, unwritable folder',
    () => {
      const locked = scratch('locked-existing');
      chmodSync(locked, 0o500);
      const file = settingsFile();
      let err: unknown;
      try {
        appSettingsCommand({ set: JSON.stringify({ libraryPath: locked }) }, { settingsPath: file, platform: 'linux', env: env() });
      } catch (e) { err = e; }
      expect(codeOf(err)).toBe('bad-settings');
      expect((err as Error).message).toContain('permission denied');
      expect((err as Error).message).not.toContain('EACCES');
      expect(existsSync(file)).toBe(false);
    },
  );

  test('good formatDefaults alongside a bad libraryPath stores neither', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand(
        { set: JSON.stringify({ formatDefaults: { justifyText: true }, libraryPath: 'relative' }) },
        { settingsPath: file, platform: 'linux', env: env() },
      );
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect(existsSync(file)).toBe(false);
  });

  test('a good NEW libraryPath alongside a bad formatDefaults creates no folder, stores neither', () => {
    // The order the brief's own bug lived in: libraryPath validated (and,
    // before this fix, CREATED) before formatDefaults' type was even
    // checked. Every pure check must run for BOTH keys before either one
    // touches disk, so this folder must never come into existence.
    const file = settingsFile();
    const newFolder = join(scratch('would-be-lib'), 'not-yet-made');
    let err: unknown;
    try {
      appSettingsCommand(
        { set: JSON.stringify({ libraryPath: newFolder, formatDefaults: 'nope' }) },
        { settingsPath: file, platform: 'linux', env: env() },
      );
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect(existsSync(file)).toBe(false);
    expect(existsSync(newFolder)).toBe(false);
  });

  test('--set with an empty object changes nothing: it answers like a plain read and writes nothing', () => {
    const file = settingsFile();
    const result = appSettingsCommand({ set: '{}' }, { settingsPath: file, platform: 'linux', env: env() });
    expect(result.library.chosen).toBeNull();
    expect(result.customized).toBe(false);
    expect(existsSync(file)).toBe(false);
  });

  test('--set with an empty object does not touch an existing file, even a corrupt one', () => {
    const file = settingsFile();
    writeFileSync(file, 'not json at all');
    appSettingsCommand({ set: '{}' }, { settingsPath: file, platform: 'linux', env: env() });
    expect(readFileSync(file, 'utf8')).toBe('not json at all');
  });

  // Root bypasses ordinary permission bits, and Windows has no chmod-style
  // read-only-directory story that would make the settings file itself
  // unwritable here.
  test.skipIf(process.platform === 'win32' || process.getuid?.() === 0)(
    'a settings file that cannot be saved is bad-settings, not a raw crash, and NAMES the file',
    () => {
      const locked = scratch('locked-config');
      chmodSync(locked, 0o500);
      const file = join(locked, 'settings.json');
      let err: unknown;
      try {
        // libraryPath: null is a pure reset, so the ONLY thing that can fail
        // here is writeAppSettings' own write, not the library-folder check.
        appSettingsCommand({ set: JSON.stringify({ libraryPath: null }) }, { settingsPath: file, platform: 'linux', env: env() });
      } catch (e) { err = e; }
      expect(codeOf(err)).toBe('bad-settings');
      expect((err as Error).message).toContain('permission denied');
      expect((err as Error).message).not.toContain('EACCES');
      // "cannot save the settings file: part of that path is not a folder"
      // names no path at all; a person reading it has no idea WHICH file.
      // The library-folder wrapper already names its path ("cannot use
      // <path> as the library: ..."), and this wrapper must too.
      expect((err as Error).message).toContain(file);
    },
  );
});

describe('appSettingsCommand: a pure check can pass on one platform and not another, and never touches disk either way', () => {
  // "C:\\Books" is absolute for win32 but not for posix, which is exactly
  // why platform has to be threaded through the check rather than asking
  // the host's own node:path. The real hazard this guards against: if
  // phase 2 ever ran anyway on this POSIX host, mkdirSync('C:\\Books', ...)
  // would create a directory literally named "C:\Books" relative to
  // process.cwd() (backslash is just an ordinary character on POSIX), which
  // for `bun test` run from the worktree root is the repo itself.
  const bogusFolder = join(ROOT, 'C:\\Books');

  test('win32: "C:\\\\Books" passes the path check, so a bad formatDefaults is what actually refuses the call, and nothing is created anywhere', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand(
        { set: JSON.stringify({ libraryPath: 'C:\\Books', formatDefaults: 'nope' }) },
        { settingsPath: file, platform: 'win32', env: env() },
      );
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect((err as Error).message).toBe('formatDefaults must be an object or null');
    expect(existsSync(file)).toBe(false);
    expect(existsSync(bogusFolder)).toBe(false);
  });

  test('linux: the SAME input is refused on the path itself: "C:\\\\Books" is not absolute for posix', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand(
        { set: JSON.stringify({ libraryPath: 'C:\\Books', formatDefaults: 'nope' }) },
        { settingsPath: file, platform: 'linux', env: env() },
      );
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect((err as Error).message).toBe('the library folder must be a full path');
    expect(existsSync(file)).toBe(false);
    expect(existsSync(bogusFolder)).toBe(false);
  });
});

describe('appSettingsCommand: one write covers both keys', () => {
  test('both keys land together, in one write, keeping lastRoute', () => {
    const file = settingsFile();
    writeAppSettings({ lastRoute: 'kindle' }, file);
    const chosen = scratch('both-chosen');

    const result = appSettingsCommand(
      { set: JSON.stringify({ libraryPath: chosen, formatDefaults: { justifyText: true } }) },
      { settingsPath: file, platform: 'linux', env: env() },
    );
    expect(result.library.chosen).toBe(chosen);
    expect(result.formatDefaults.justifyText).toBe(true);

    const onDisk = readAppSettings(file);
    expect(onDisk.libraryPath).toBe(chosen);
    expect((onDisk.formatDefaults as Record<string, unknown>).justifyText).toBe(true);
    // lastRoute (piece B's key) survived a write this verb made.
    expect(onDisk.lastRoute).toBe('kindle');
    // Both keys landed together, asserted above from the ONE result and the
    // ONE file read back. This only confirms no stray temp file was left
    // beside settings.json; it does not by itself prove writeAppSettings
    // was called exactly once (nothing here spies on the writer).
    expect(readdirSync(dirname(file))).toEqual(['settings.json']);
  });
});

describe('appSettingsCommand: fromEnv and chosen disagree on purpose', () => {
  test('SCREEPUB_LIBRARY overrides path, but chosen still names the stored choice', () => {
    const file = settingsFile();
    const chosen = scratch('env-chosen');
    appSettingsCommand({ set: JSON.stringify({ libraryPath: chosen }) }, { settingsPath: file, platform: 'linux', env: env() });

    const result = appSettingsCommand(
      {},
      { settingsPath: file, platform: 'linux', env: env({ SCREEPUB_LIBRARY: '/env/lib' }) },
    );
    expect(result.library.fromEnv).toBe(true);
    expect(result.library.path).toBe('/env/lib');
    expect(result.library.chosen).toBe(chosen);
  });

  test('a blank SCREEPUB_LIBRARY does not count as fromEnv', () => {
    const file = settingsFile();
    const result = appSettingsCommand({}, { settingsPath: file, platform: 'linux', env: env({ SCREEPUB_LIBRARY: '   ' }) });
    expect(result.library.fromEnv).toBe(false);
  });
});

describe('appSettingsCommand: chosen is null for anything libraryRoot would not honour', () => {
  test('a stored relative path is not reported as chosen', () => {
    const file = settingsFile();
    writeAppSettings({ libraryPath: 'Scripts' }, file);
    const result = appSettingsCommand({}, { settingsPath: file, platform: 'linux', env: env() });
    expect(result.library.chosen).toBeNull();
    expect(result.library.path).toBe(join(HOME, 'Documents', 'Screepub'));
  });
});

describe('appSettingsCommand: platformDefault', () => {
  test('is correct for darwin with a fake HOME, independent of a chosen folder', () => {
    const file = settingsFile();
    const chosen = scratch('darwin-chosen');
    writeAppSettings({ libraryPath: chosen }, file);

    const result = appSettingsCommand({}, { settingsPath: file, platform: 'darwin', env: env() });
    expect(result.library.platformDefault).toBe(join(HOME, 'Documents', 'Screepub'));
    // The chosen folder still wins for `path`; platformDefault is a separate
    // fact, not what conversions actually use.
    expect(result.library.path).toBe(chosen);
  });
});

describe('appSettingsCommand: home', () => {
  test('is present, and is the same HOME the engine resolves everything else from', () => {
    const file = settingsFile();
    const result = appSettingsCommand({}, { settingsPath: file, platform: 'linux', env: env() });
    expect(result.home).toBe(HOME);
  });
});

// The Settings page's choice (spec 2026-09-24-keep-script-settings-choice-
// design.md): whether a new library conversion pins the settings it
// started from. The third key this verb may write, with the same
// discipline as the other two: checked before anything is written, and
// a write that keeps every other key in the file.
describe('appSettingsCommand: keepScriptSettings', () => {
  test('a plain read answers true when nothing is stored: keeping is the default', () => {
    const file = settingsFile();
    const result = appSettingsCommand({}, { settingsPath: file, platform: 'linux', env: env() });
    expect(result.keepScriptSettings).toBe(true);
    expect(existsSync(file)).toBe(false);
  });

  test('false is stored and answered, and every other key in the file survives the write', () => {
    const file = settingsFile();
    writeAppSettings({ lastRoute: 'kindle', formatDefaults: { justifyText: true } }, file);
    const result = appSettingsCommand(
      { set: JSON.stringify({ keepScriptSettings: false }) },
      { settingsPath: file, platform: 'linux', env: env() },
    );
    expect(result.keepScriptSettings).toBe(false);
    const onDisk = readAppSettings(file);
    expect(onDisk.keepScriptSettings).toBe(false);
    expect(onDisk.lastRoute).toBe('kindle');
    expect((onDisk.formatDefaults as Record<string, unknown>).justifyText).toBe(true);
    // The other answers are untouched by it: customized still reads the
    // stored formatDefaults, not a side effect of this key.
    expect(result.customized).toBe(true);
  });

  test('true is stored as true, and null removes the key, which reads as the default again', () => {
    const file = settingsFile();
    appSettingsCommand({ set: '{"keepScriptSettings":false}' }, { settingsPath: file, platform: 'linux', env: env() });
    const on = appSettingsCommand(
      { set: '{"keepScriptSettings":true}' }, { settingsPath: file, platform: 'linux', env: env() },
    );
    expect(on.keepScriptSettings).toBe(true);
    expect(readAppSettings(file).keepScriptSettings).toBe(true);

    appSettingsCommand({ set: '{"keepScriptSettings":false}' }, { settingsPath: file, platform: 'linux', env: env() });
    const reset = appSettingsCommand(
      { set: '{"keepScriptSettings":null}' }, { settingsPath: file, platform: 'linux', env: env() },
    );
    expect(reset.keepScriptSettings).toBe(true);
    expect('keepScriptSettings' in readAppSettings(file)).toBe(false);
  });

  test('anything but true, false or null is bad-settings, and nothing is written', () => {
    for (const value of ['false', 0, 1, {}, []]) {
      const file = settingsFile();
      let err: unknown;
      try {
        appSettingsCommand(
          { set: JSON.stringify({ keepScriptSettings: value }) },
          { settingsPath: file, platform: 'linux', env: env() },
        );
      } catch (e) { err = e; }
      expect(`${JSON.stringify(value)}: ${codeOf(err)}`).toBe(`${JSON.stringify(value)}: bad-settings`);
      expect((err as Error).message).toContain('keepScriptSettings');
      expect(existsSync(file)).toBe(false);
    }
  });

  test('a good keepScriptSettings alongside a bad formatDefaults stores neither', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand(
        { set: JSON.stringify({ keepScriptSettings: false, formatDefaults: 'kindleEink' }) },
        { settingsPath: file, platform: 'linux', env: env() },
      );
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect(existsSync(file)).toBe(false);
  });

  test('a bad keepScriptSettings alongside a good NEW libraryPath creates no folder, stores neither', () => {
    const file = settingsFile();
    const chosen = join(scratch('keep-lib'), 'not-yet-made');
    let err: unknown;
    try {
      appSettingsCommand(
        { set: JSON.stringify({ libraryPath: chosen, keepScriptSettings: 'no' }) },
        { settingsPath: file, platform: 'linux', env: env() },
      );
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('bad-settings');
    expect(existsSync(chosen)).toBe(false);
    expect(existsSync(file)).toBe(false);
  });

  test('an unknown key is refused naming all three keys this verb may write', () => {
    const file = settingsFile();
    let err: unknown;
    try {
      appSettingsCommand({ set: JSON.stringify({ theme: 'dark' }) }, { settingsPath: file, platform: 'linux', env: env() });
    } catch (e) { err = e; }
    expect(codeOf(err)).toBe('usage');
    expect((err as Error).message).toBe(
      'app-settings --set takes only libraryPath, formatDefaults, keepScriptSettings, readerDevice, animatePage and typewriterSounds, not "theme"',
    );
  });
});

describe('appSettingsCommand: readerDevice, animatePage, typewriterSounds', () => {
  const deps = (file: string) => ({ settingsPath: file, platform: 'linux' as const, env: env() });

  test('a fresh file reports no device, the page animation on, the typewriter sounds off', () => {
    const result = appSettingsCommand({}, deps(settingsFile()));
    expect(result.readerDevice).toBeNull();
    expect(result.animatePage).toBe(true);
    expect(result.typewriterSounds).toBe(false);
  });

  test('a device is stored, reported back, and removed by null', () => {
    const file = settingsFile();
    const set = appSettingsCommand({ set: '{"readerDevice":"kobo"}' }, deps(file));
    expect(set.readerDevice).toBe('kobo');
    expect(readAppSettings(file).readerDevice).toBe('kobo');
    expect(appSettingsCommand({}, deps(file)).readerDevice).toBe('kobo');

    const cleared = appSettingsCommand({ set: '{"readerDevice":null}' }, deps(file));
    expect(cleared.readerDevice).toBeNull();
    expect('readerDevice' in readAppSettings(file)).toBe(false);
  });

  test('every known device is accepted', () => {
    for (const device of READER_DEVICES) {
      const result = appSettingsCommand({ set: JSON.stringify({ readerDevice: device }) }, deps(settingsFile()));
      expect(result.readerDevice).toBe(device);
    }
  });

  test('an unknown device is refused with bad-settings and the file is byte-identical', () => {
    const file = settingsFile();
    writeAppSettings({ lastRoute: 'usb', readerDevice: 'kindle' }, file);
    const before = readFileSync(file, 'utf8');
    for (const bad of ['"nook"', '3', 'true', '"Kobo"']) {
      let caught: unknown;
      try {
        appSettingsCommand({ set: `{"readerDevice":${bad}}` }, deps(file));
      } catch (err) {
        caught = err;
      }
      expect(codeOf(caught)).toBe('bad-settings');
      expect(readFileSync(file, 'utf8')).toBe(before);
    }
  });

  test('both switches are stored and reported', () => {
    const file = settingsFile();
    const result = appSettingsCommand({ set: '{"animatePage":false,"typewriterSounds":true}' }, deps(file));
    expect(result.animatePage).toBe(false);
    expect(result.typewriterSounds).toBe(true);
    const onDisk = readAppSettings(file);
    expect(onDisk.animatePage).toBe(false);
    expect(onDisk.typewriterSounds).toBe(true);
  });

  test('null puts a switch back to its default', () => {
    const file = settingsFile();
    appSettingsCommand({ set: '{"animatePage":false,"typewriterSounds":true}' }, deps(file));
    const result = appSettingsCommand({ set: '{"animatePage":null,"typewriterSounds":null}' }, deps(file));
    expect(result.animatePage).toBe(true);
    expect(result.typewriterSounds).toBe(false);
    expect('animatePage' in readAppSettings(file)).toBe(false);
    expect('typewriterSounds' in readAppSettings(file)).toBe(false);
  });

  test('a non-boolean for either switch is bad-settings and writes nothing', () => {
    const file = settingsFile();
    writeAppSettings({ lastRoute: 'usb' }, file);
    const before = readFileSync(file, 'utf8');
    for (const patch of [
      '{"animatePage":"false"}', '{"typewriterSounds":1}',
      '{"animatePage":false,"typewriterSounds":"yes"}',
    ]) {
      let caught: unknown;
      try {
        appSettingsCommand({ set: patch }, deps(file));
      } catch (err) {
        caught = err;
      }
      expect(codeOf(caught)).toBe('bad-settings');
      expect(readFileSync(file, 'utf8')).toBe(before);
    }
  });

  test('setting one key keeps every other key in the file', () => {
    const file = settingsFile();
    const library = scratch('kept-lib');
    writeAppSettings({ lastRoute: 'usb', libraryPath: library, keepScriptSettings: false }, file);
    appSettingsCommand({ set: '{"readerDevice":"tolino"}' }, deps(file));
    appSettingsCommand({ set: '{"animatePage":false}' }, deps(file));
    const onDisk = readAppSettings(file);
    expect(onDisk.lastRoute).toBe('usb');
    expect(onDisk.libraryPath).toBe(library);
    expect(onDisk.keepScriptSettings).toBe(false);
    expect(onDisk.readerDevice).toBe('tolino');
    expect(onDisk.animatePage).toBe(false);
  });
});

describe('screepub app-settings (through the CLI)', () => {
  async function runCli(args: string[], extraEnv: Record<string, string> = {}) {
    const proc = Bun.spawn(['bun', `${ROOT}src/cli.ts`, ...args], {
      stdout: 'pipe',
      stderr: 'pipe',
      // SCREEPUB_LIBRARY defaults to blank so a developer's own shell
      // variable cannot leak into an assertion about the DEFAULT library
      // location; a test that means to exercise the override sets its own
      // value in extraEnv, which still wins (it is spread last).
      env: { ...process.env, SCREEPUB_LIBRARY: '', ...extraEnv },
    });
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]);
    return { stdout, stderr, exitCode };
  }

  test('--json answer has the documented shape', async () => {
    const configDir = scratch('config');
    const { stdout, exitCode } = await runCli(['app-settings', '--json'], { SCREEPUB_CONFIG_DIR: configDir });
    expect(exitCode).toBe(0);
    const answer = JSON.parse(stdout);
    expect(answer.ok).toBe(true);
    expect(answer.file).toBe(join(configDir, 'settings.json'));
    expect(answer.library.chosen).toBeNull();
    expect(typeof answer.library.path).toBe('string');
    expect(typeof answer.library.platformDefault).toBe('string');
    expect(answer.library.fromEnv).toBe(false);
    expect(typeof answer.home).toBe('string');
    expect(answer.formatDefaults).toBeDefined();
    expect(answer.shippedDefaults).toBeDefined();
    expect(answer.customized).toBe(false);
    expect(answer.keepScriptSettings).toBe(true);
  });

  test('--set round-trips through the CLI', async () => {
    const configDir = scratch('config');
    const chosen = scratch('cli-chosen');
    const setResult = await runCli(
      ['app-settings', '--set', JSON.stringify({ libraryPath: chosen }), '--json'],
      { SCREEPUB_CONFIG_DIR: configDir },
    );
    expect(setResult.exitCode).toBe(0);
    expect(JSON.parse(setResult.stdout).library.chosen).toBe(chosen);

    const readBack = await runCli(['app-settings', '--json'], { SCREEPUB_CONFIG_DIR: configDir });
    expect(readBack.exitCode).toBe(0);
    expect(JSON.parse(readBack.stdout).library.chosen).toBe(chosen);
  });

  test('human output names the folder and, by default, Screepub\'s own defaults', async () => {
    const configDir = scratch('config');
    const { stdout, exitCode } = await runCli(['app-settings'], { SCREEPUB_CONFIG_DIR: configDir });
    expect(exitCode).toBe(0);
    const lines = stdout.trim().split('\n');
    expect(lines[0]).toContain('books are saved in');
    expect(lines[0]).toContain('default folder');
    expect(lines[1]).toBe("new scripts start from: Screepub's defaults");
  });

  test('human output says "your own defaults" once formatDefaults are customized', async () => {
    const configDir = scratch('config');
    await runCli(
      ['app-settings', '--set', JSON.stringify({ formatDefaults: { justifyText: true } }), '--json'],
      { SCREEPUB_CONFIG_DIR: configDir },
    );
    const { stdout } = await runCli(['app-settings'], { SCREEPUB_CONFIG_DIR: configDir });
    expect(stdout.trim().split('\n')[1]).toBe('new scripts start from: your own defaults');
  });

  test('human output says what a converted PDF does: keeps its settings by default, or follows the defaults', async () => {
    const configDir = scratch('config');
    const before = await runCli(['app-settings'], { SCREEPUB_CONFIG_DIR: configDir });
    expect(before.stdout.trim().split('\n')[2]).toBe('when a PDF is converted: keep its settings');

    const set = await runCli(
      ['app-settings', '--set', '{"keepScriptSettings":false}', '--json'],
      { SCREEPUB_CONFIG_DIR: configDir },
    );
    expect(set.exitCode).toBe(0);
    expect(JSON.parse(set.stdout).keepScriptSettings).toBe(false);

    const after = await runCli(['app-settings'], { SCREEPUB_CONFIG_DIR: configDir });
    expect(after.stdout.trim().split('\n')[2]).toBe('when a PDF is converted: follow the defaults');
  });

  test('human output says "the folder you chose" once one is stored', async () => {
    const configDir = scratch('config');
    const chosen = scratch('chosen-human');
    await runCli(
      ['app-settings', '--set', JSON.stringify({ libraryPath: chosen }), '--json'],
      { SCREEPUB_CONFIG_DIR: configDir },
    );
    const { stdout, exitCode } = await runCli(['app-settings'], { SCREEPUB_CONFIG_DIR: configDir });
    expect(exitCode).toBe(0);
    const line = stdout.trim().split('\n')[0];
    expect(line).toContain('books are saved in');
    expect(line).toContain('the folder you chose');
  });

  test('human output says "set by SCREEPUB_LIBRARY" when the env override wins', async () => {
    const configDir = scratch('config');
    const envLib = scratch('env-lib-human');
    const { stdout, exitCode } = await runCli(['app-settings'], { SCREEPUB_CONFIG_DIR: configDir, SCREEPUB_LIBRARY: envLib });
    expect(exitCode).toBe(0);
    const line = stdout.trim().split('\n')[0];
    expect(line).toContain('books are saved in');
    expect(line).toContain('set by SCREEPUB_LIBRARY');
  });

  // Every other verb's flag is refused here too; tests/cli-verb-flags.test.ts
  // checks that for every verb and every flag, owners and all.
  test('refuses a stray positional', async () => {
    const configDir = scratch('config');
    const { stdout, exitCode } = await runCli(['app-settings', 'extra', '--json'], { SCREEPUB_CONFIG_DIR: configDir });
    expect(exitCode).toBe(1);
    const answer = JSON.parse(stdout);
    expect(answer.error.code).toBe('usage');
    expect(answer.error.message).toContain('extra');
  });

  test('--help describes what --set accepts and the SCREEPUB_LIBRARY override', async () => {
    const { stdout, exitCode } = await runCli(['app-settings', '--help']);
    expect(exitCode).toBe(0);
    expect(stdout).toContain('screepub app-settings');
    expect(stdout).toContain('libraryPath');
    expect(stdout).toContain('formatDefaults');
    expect(stdout).toContain('SCREEPUB_LIBRARY');
  });

  test('--help describes keepScriptSettings', async () => {
    const { stdout } = await runCli(['app-settings', '--help']);
    expect(stdout).toContain('keepScriptSettings');
    expect(stdout).not.toContain('\u2014');
  });

  test('--help says formatDefaults REPLACES the stored defaults, unlike settings --set', async () => {
    const { stdout } = await runCli(['app-settings', '--help']);
    expect(stdout).toContain('REPLACES');
    expect(stdout).toContain('settings --set');
  });

  test('the top-level --help usage lists app-settings', async () => {
    const { stdout, exitCode } = await runCli(['--help']);
    expect(exitCode).toBe(0);
    expect(stdout).toContain('screepub app-settings [--set <json>] [--json]');
    expect(stdout).toContain('where books land, and what new scripts start from');
  });
});

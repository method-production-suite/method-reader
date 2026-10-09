import { afterAll, describe, test, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { settingsCommand } from '../src/cli-settings';
import { writeAppSettings } from '../src/settings/app';
import { DEFAULT_FORMAT_OPTIONS } from '../src/options';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-cli-settings-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

let dir: string;
let fountain: string;

beforeEach(() => {
  dir = mkdtempSync(join(SCRATCH, 'settings-'));
  fountain = join(dir, 'Script.fountain');
  writeFileSync(fountain, 'INT. ROOM - DAY\n\nA beat.\n');
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

describe('settingsCommand', () => {
  test('an untouched script reports the shipped defaults and its preset', () => {
    const result = settingsCommand({ fountain });
    expect(result.settings).toEqual(DEFAULT_FORMAT_OPTIONS);
    // Not just "a string": kindleEink IS the defaults, so anything else here
    // means matchingPreset was bypassed or the defaults drifted.
    expect(result.preset).toBe('kindleEink');
    expect(result.sidecar).toBe(join(dir, 'Script.screepub.json'));
    expect(existsSync(result.sidecar)).toBe(false); // reading must not create
  });

  test('it offers exactly the two presets, with their real settings', () => {
    const { presets } = settingsCommand({ fountain });
    expect(presets.map((p) => p.id).sort()).toEqual(['kindleEink', 'phone']);
    const phone = presets.find((p) => p.id === 'phone')!;
    expect(phone.displayName).toBe('Phone / narrow screen');
    expect(phone.settings.dualDialogue).toBe('sequential');
    expect(phone.settings.dialogueSideMarginPct).toBe(10);
  });

  test('--set writes the sidecar and reports the merged result', () => {
    const result = settingsCommand({ fountain, set: '{"justifyText":true}' });
    expect(result.settings.justifyText).toBe(true);
    // The other nineteen must survive: a naive implementation that stored
    // the partial would report a settings object with one key.
    expect(result.settings.contdMode).toBe('auto');
    expect(Object.keys(result.settings)).toHaveLength(20);
    const onDisk = JSON.parse(readFileSync(result.sidecar, 'utf8'));
    expect(onDisk.justifyText).toBe(true);
    expect(onDisk.contdMode).toBe('auto');
  });

  test('a second --set overlays the first rather than resetting it', () => {
    settingsCommand({ fountain, set: '{"justifyText":true}' });
    const result = settingsCommand({ fountain, set: '{"showSceneNumbers":true}' });
    expect(result.settings.justifyText).toBe(true);
    expect(result.settings.showSceneNumbers).toBe(true);
  });

  // Review fix: resolveFormatOptions' dualDialogue merge used to read
  // 'sideBySide' the same as absent, so once a sidecar held 'sequential'
  // nothing could ever move it back through --set either.
  test('--set sequential then --set sideBySide ends sideBySide', () => {
    settingsCommand({ fountain, set: '{"dualDialogue":"sequential"}' });
    const result = settingsCommand({ fountain, set: '{"dualDialogue":"sideBySide"}' });
    expect(result.settings.dualDialogue).toBe('sideBySide');
  });

  test('a tuned script matches no preset', () => {
    const result = settingsCommand({ fountain, set: '{"elementSpacingEm":1.9}' });
    expect(result.preset).toBeNull();
  });

  test('--set clamps rather than trusting', () => {
    const result = settingsCommand({ fountain, set: '{"dialogueSideMarginPct":999}' });
    expect(result.settings.dialogueSideMarginPct).toBe(30);
  });

  test('a missing script is unreadable, not an empty settings object', () => {
    expect(() => settingsCommand({ fountain: join(dir, 'nope.fountain') })).toThrow(
      /cannot read/,
    );
  });

  test('a malformed --set is bad-settings and does not write the sidecar', () => {
    let code = '';
    try {
      settingsCommand({ fountain, set: '{oops' });
    } catch (err) {
      code = (err as { code: string }).code;
    }
    expect(code).toBe('bad-settings');
    expect(existsSync(join(dir, 'Script.screepub.json'))).toBe(false);
  });

  // --- Beyond the brief -----------------------------------------------
  //
  // The standing question for every test here: "would a plausible wrong
  // implementation still pass this?" The brief's suite never (a) reads back
  // a saved sidecar through the NO-`--set` path, (b) feeds the handler a
  // sidecar file that isn't valid JSON, (c) rejects a JSON array as a
  // settings object even though `typeof [] === 'object'`, or (d) points the
  // handler at a path that exists but is not a script file at all. Each of
  // those is a distinct plausible bug, not a restatement of a brief test.

  test('a plain read (no --set) reports what a PRIOR --set actually saved', () => {
    // Guards against a handler whose read branch ignores the sidecar and
    // just hands back DEFAULT_FORMAT_OPTIONS — "inventing" an answer instead
    // of reporting what is really on disk. The brief only ever reads back
    // through another --set call, which goes through loadScriptSettings
    // regardless; a bare read is a separate code path in the brief's own
    // settingsCommand (the `if (options.set !== undefined)` branch is
    // skipped entirely) and deserves its own coverage.
    settingsCommand({ fountain, set: '{"justifyText":true}' });
    const result = settingsCommand({ fountain });
    expect(result.settings.justifyText).toBe(true);
    expect(result.preset).toBeNull();
  });

  test('a malformed sidecar already on disk is treated as absent, not a crash', () => {
    // sidecar.ts's loadScriptSettings swallows a JSON.parse failure and
    // falls back — but that guarantee is only worth something if the verb
    // actually goes through loadScriptSettings rather than reading the file
    // itself (which would throw SyntaxError straight through, an unhandled
    // exception rather than a reported CliError).
    const sidecar = join(dir, 'Script.screepub.json');
    writeFileSync(sidecar, '{ this is not json');
    const result = settingsCommand({ fountain });
    expect(result.settings).toEqual(DEFAULT_FORMAT_OPTIONS);
  });

  test('--set as a JSON array is bad-settings, not silently accepted', () => {
    // typeof [] === 'object', so a check written as `typeof parsed !==
    // 'object'` alone lets an array through into resolveFormatOptions,
    // which would then read numeric/string keys off array indices and
    // quietly do nothing useful. Must be caught explicitly.
    let code = '';
    try {
      settingsCommand({ fountain, set: '[1,2,3]' });
    } catch (err) {
      code = (err as { code: string }).code;
    }
    expect(code).toBe('bad-settings');
    expect(existsSync(join(dir, 'Script.screepub.json'))).toBe(false);
  });

  test('a directory is unreadable, not treated as a script with no sidecar', () => {
    // "A path outside any script": something that exists on disk (so it
    // cannot be confused with the missing-file case above) but is not a
    // script file the sidecar mechanism can be anchored to. A handler that
    // only checked existsSync (rather than isFile()) would sail past this
    // and go on to compute a nonsense sidecar path next to the directory.
    expect(() => settingsCommand({ fountain: dir })).toThrow(/cannot read/);
  });
});

describe('settingsCommand: app-wide defaults underneath the sidecar', () => {
  function appSettings(): string {
    return join(dir, 'app-settings.json');
  }

  test('no app settings file: appDefaults is exactly the shipped defaults', () => {
    const result = settingsCommand({ fountain, appSettingsPath: appSettings() });
    expect(result.appDefaults).toEqual(DEFAULT_FORMAT_OPTIONS);
    // defaults stays Screepub's own regardless of appDefaults, so the app
    // can always show BOTH "your defaults" and "Screepub's defaults".
    expect(result.defaults).toEqual(DEFAULT_FORMAT_OPTIONS);
  });

  test('an untouched script with app defaults set reports THEM, not the shipped defaults', () => {
    const path = appSettings();
    writeAppSettings({ formatDefaults: { dialogueSideMarginPct: 5 } }, path);
    const result = settingsCommand({ fountain, appSettingsPath: path });
    expect(result.settings.dialogueSideMarginPct).toBe(5);
    expect(result.appDefaults.dialogueSideMarginPct).toBe(5);
    // The shipped object never moves, even though the reported settings did.
    expect(result.defaults.dialogueSideMarginPct).toBe(DEFAULT_FORMAT_OPTIONS.dialogueSideMarginPct);
  });

  test('a sidecar already on disk outranks the app defaults', () => {
    const path = appSettings();
    writeAppSettings({ formatDefaults: { dialogueSideMarginPct: 5 } }, path);
    // Write a sidecar directly (not through --set, which would overlay on
    // the CURRENT read rather than proving a PRE-EXISTING one wins).
    const sidecar = join(dir, 'Script.screepub.json');
    writeFileSync(sidecar, JSON.stringify({ dialogueSideMarginPct: 12 }));
    const result = settingsCommand({ fountain, appSettingsPath: path });
    expect(result.settings.dialogueSideMarginPct).toBe(12);
    // Knobs the sidecar never mentioned still come from the app defaults,
    // not the shipped ones: proof the sidecar was overlaid ON TOP of
    // appDefaults rather than resolved straight over DEFAULT_FORMAT_OPTIONS.
    writeAppSettings({ formatDefaults: { dialogueSideMarginPct: 5, justifyText: true } }, path);
    const second = settingsCommand({ fountain, appSettingsPath: path });
    expect(second.settings.justifyText).toBe(true);
  });

  test('--set overlays on the app defaults when there is no sidecar yet', () => {
    const path = appSettings();
    writeAppSettings({ formatDefaults: { dialogueSideMarginPct: 5 } }, path);
    const result = settingsCommand({
      fountain, appSettingsPath: path, set: '{"justifyText":true}',
    });
    expect(result.settings.justifyText).toBe(true);
    expect(result.settings.dialogueSideMarginPct).toBe(5);
  });

  test('a corrupt app settings file changes nothing: shipped defaults throughout', () => {
    const path = appSettings();
    writeFileSync(path, '{ not json');
    const result = settingsCommand({ fountain, appSettingsPath: path });
    expect(result.settings).toEqual(DEFAULT_FORMAT_OPTIONS);
    expect(result.appDefaults).toEqual(DEFAULT_FORMAT_OPTIONS);
  });

  test('the answer carries keepScriptSettings, so the Settings page can draw that choice from this one round trip', () => {
    // true when nothing is stored: keeping a script's settings is the
    // default (spec 2026-09-24-keep-script-settings-choice-design.md).
    expect(settingsCommand({ fountain, appSettingsPath: appSettings() }).keepScriptSettings).toBe(true);
    const path = appSettings();
    writeAppSettings({ keepScriptSettings: false }, path);
    expect(settingsCommand({ fountain, appSettingsPath: path }).keepScriptSettings).toBe(false);
  });

  test('with no appSettingsPath given, it defaults to the production path (test-guarded)', () => {
    // No injected path at all: settingsCommand must fall back to
    // appSettingsPath() itself, which the test-run guard (SCREEPUB_CONFIG_DIR
    // under /dev/null) points away from any real file, so this still reads
    // as the shipped defaults rather than throwing.
    const result = settingsCommand({ fountain });
    expect(result.appDefaults).toEqual(DEFAULT_FORMAT_OPTIONS);
  });
});

describe('screepub settings (through the CLI)', () => {
  test('--json prints one object carrying the settings and the presets', async () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'settings', fountain, '--json']);
    const stdout = proc.stdout.toString();
    expect(stdout.trim().split('\n')).toHaveLength(1);
    const answer = JSON.parse(stdout);
    expect(answer.ok).toBe(true);
    expect(answer.settings.contdMode).toBe('auto');
    expect(answer.presets).toHaveLength(2);
    expect(answer.sidecar).toContain('Script.screepub.json');
  });

  test('--set round-trips through the CLI', async () => {
    Bun.spawnSync(['bun', 'src/cli.ts', 'settings', fountain, '--set',
      '{"showPageMarkers":true}', '--json']);
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'settings', fountain, '--json']);
    expect(JSON.parse(proc.stdout.toString()).settings.showPageMarkers).toBe(true);
  });

  test('a missing script fails as JSON on stdout, exit 1', async () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'settings',
      join(dir, 'nope.fountain'), '--json']);
    expect(proc.exitCode).toBe(1);
    const answer = JSON.parse(proc.stdout.toString());
    expect(answer.ok).toBe(false);
    expect(answer.error.code).toBe('unreadable');
  });

  test('a file named "settings" still converts rather than being stolen', () => {
    // The shadowing rule resolveCommand already owns; this asserts the new
    // verb joined it rather than bypassing it. Fixed from the brief's
    // version, which passed the shadow file's ABSOLUTE path as argv[0] —
    // resolveCommand only ever compares argv[0] against the literal verb
    // strings, so an absolute path never matches 'settings' in the first
    // place and the shadowing branch was never exercised. It also spawned
    // `bun src/cli.ts` with `cwd: dir`, where no such relative file exists,
    // so bun couldn't even find the script (empty stdout, a JSON.parse
    // crash) — the reason this test passed against ANY implementation, wrong
    // or right. Fixed the same way tests/cli-device-commands.test.ts does
    // it: an ABSOLUTE path to cli.ts, and the bare verb word as argv[0],
    // with cwd holding the shadowing file.
    const shadow = join(dir, 'settings');
    writeFileSync(shadow, 'x');
    const cliPath = new URL('../src/cli.ts', import.meta.url).pathname;
    const proc = Bun.spawnSync(['bun', cliPath, 'settings', '--json'], { cwd: dir });
    const answer = JSON.parse(proc.stdout.toString());
    expect(answer.error.code).toBe('unsupported-type');
  });

  test('devices rejects --set: it belongs to settings and app-settings, not every verb', () => {
    // The brief wires this rejection into cli.ts alongside `devices`
    // rejecting `--device`; without a test, a future edit could delete the
    // check and devices would silently ignore --set instead of failing loud.
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'devices', '--set', '{}', '--json']);
    const answer = JSON.parse(proc.stdout.toString());
    expect(answer.ok).toBe(false);
    expect(answer.error.code).toBe('usage');
  });

  // Review fix: a script with no saved settings does not start at
  // DEFAULT_FORMAT_OPTIONS, it starts at the app defaults, which are the
  // shipped ones only until the user chooses otherwise. --help said neither.
  test('--help says a script with no saved settings starts from the app defaults', () => {
    const proc = Bun.spawnSync(['bun', 'src/cli.ts', 'settings', '--help']);
    const stdout = proc.stdout.toString();
    expect(stdout).toContain('starts from the format defaults you chose in the app');
  });
});

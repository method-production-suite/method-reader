// The `app-settings` verb: where books land, and what new scripts start
// from. Reads and writes the three keys of the app settings file that belong
// to piece C: `libraryPath`, `formatDefaults` and `keepScriptSettings` (the
// Settings page's "When a PDF is converted" choice, spec 2026-09-24), plus
// the window's three remembered choices: `readerDevice`, `animatePage` and
// `typewriterSounds` (send-from-convert spec, decisions 3 and 21).
// `lastRoute` (piece B) is not writable from here. Like the other verb
// handlers, this RETURNS a result and never prints; cli.ts owns stdout.
import { accessSync, constants, mkdirSync } from 'node:fs';
import { CliError, plainReason } from './cli-errors';
import {
  chosenLibraryPath, envLibraryOverride, libraryRoot, platformLibraryDefault,
  resolvedIfAbsolute,
} from './library';
import { DEFAULT_FORMAT_OPTIONS, resolveFormatOptions, type FormatOptions } from './options';
import {
  appSettingsPath, homeFolder, READER_DEVICES, readAppSettings, writeAppSettings,
  type AppSettings, type ReaderDevice,
} from './settings/app';
import { appDefaultOptions, appDefaultsCustomized, keepsScriptSettings } from './settings/app-defaults';

type Env = Record<string, string | undefined>;

// The only three keys this verb may write. Anything else, most notably
// `lastRoute` (piece B's key in the same file), is refused before anything
// is touched, so this verb can never step on a setting it does not own.
const WRITABLE_KEYS = [
  'libraryPath', 'formatDefaults', 'keepScriptSettings',
  'readerDevice', 'animatePage', 'typewriterSounds',
] as const;

export interface AppSettingsOptions {
  /** A JSON object with any of `libraryPath`, `formatDefaults` and
   * `keepScriptSettings`; each may be `null` to reset it. Omitted: a plain
   * read. */
  set?: string;
}

export interface AppSettingsDeps {
  /** Where the app settings file lives. Defaults to the production path
   * (`appSettingsPath(platform, env)`); a test points this at a scratch
   * file so it never touches a real one. */
  settingsPath?: string;
  platform?: NodeJS.Platform;
  env?: Env;
}

export interface AppSettingsLibraryInfo {
  /** The folder conversions will actually use. */
  path: string;
  /** The stored choice, resolved, or null: absent, or a value libraryRoot
   * would not honour anyway (relative, not a string), so this never names a
   * folder the engine ignores. */
  chosen: string | null;
  /** The folder with no choice and no SCREEPUB_LIBRARY. */
  platformDefault: string;
  /** True when SCREEPUB_LIBRARY (trimmed) is set: it is overriding `path`
   * regardless of what `chosen` says. */
  fromEnv: boolean;
}

export interface AppSettingsResult {
  file: string;
  library: AppSettingsLibraryInfo;
  /** The user's home folder as the engine sees it, so the window can show
   * the library as `~/...` without inventing the substitution itself. */
  home: string;
  formatDefaults: FormatOptions;
  shippedDefaults: FormatOptions;
  customized: boolean;
  /** Whether a new library conversion keeps the settings it started from
   * (the pin in src/cli.ts): true unless `false` is stored. */
  keepScriptSettings: boolean;
  /** The reader the person said is theirs, or null when none is stored. */
  readerDevice: ReaderDevice | null;
  /** Whether the first page animates in; true unless `false` is stored. */
  animatePage: boolean;
  /** Whether the typewriter sounds play; false unless `true` is stored. */
  typewriterSounds: boolean;
}

/** Pure check for `libraryPath`: no filesystem access at all. `null` reads
 * as Reset and returns `undefined`; a good absolute string comes back
 * resolved, ready for `createdWritableFolder` to create and probe LATER,
 * only once every other key in the same patch has passed its own pure check
 * too. Throws `bad-settings` for anything else. */
function checkedLibraryPath(value: unknown, platform: NodeJS.Platform): string | undefined {
  if (value === null) return undefined;
  if (typeof value !== 'string') {
    throw new CliError('bad-settings', 'libraryPath must be a full path or null');
  }
  const resolved = resolvedIfAbsolute(platform, value);
  if (resolved === null) {
    throw new CliError('bad-settings', 'the library folder must be a full path');
  }
  return resolved;
}

/** The one side effect this handler makes for `libraryPath`: creates
 * `resolved` (recursive) and checks it is writable. Called LAST, after
 * every pure check on the WHOLE patch has already passed, so a call refused
 * for some other reason (a bad formatDefaults alongside a perfectly good,
 * brand new libraryPath) never leaves a folder behind that it did not, in
 * the end, actually choose to store. */
function createdWritableFolder(resolved: string): string {
  try {
    mkdirSync(resolved, { recursive: true });
    accessSync(resolved, constants.W_OK);
  } catch (err) {
    throw new CliError('bad-settings', `cannot use ${resolved} as the library: ${plainReason(err)}`);
  }
  return resolved;
}

/** `formatDefaults` from a validated `--set` patch: `undefined` to remove
 * the key, or the FULL resolved object (every knob present and clamped),
 * never the partial the caller sent. Pure: resolveFormatOptions touches
 * nothing on disk. */
function validatedFormatDefaults(value: unknown): FormatOptions | undefined {
  if (value === null) return undefined;
  if (typeof value !== 'object' || Array.isArray(value)) {
    throw new CliError('bad-settings', 'formatDefaults must be an object or null');
  }
  return resolveFormatOptions(value as Record<string, unknown>, DEFAULT_FORMAT_OPTIONS);
}

/** `keepScriptSettings` from a validated `--set` patch: `undefined` to
 * remove the key (back to the default, which is to keep), or the boolean
 * itself. Pure. Anything else is refused rather than coerced: a string
 * "false" stored as-is would read back as ON (keepsScriptSettings honours
 * only a real `false`), which is the opposite of what the caller asked. */
function validatedKeepScriptSettings(value: unknown): boolean | undefined {
  if (value === null) return undefined;
  if (typeof value !== 'boolean') {
    throw new CliError('bad-settings', 'keepScriptSettings must be true, false or null');
  }
  return value;
}

/** `readerDevice` from a validated `--set` patch: `undefined` to remove the
 * key, or one of READER_DEVICES. Pure. */
function validatedReaderDevice(value: unknown): ReaderDevice | undefined {
  if (value === null) return undefined;
  if (typeof value !== 'string' || !(READER_DEVICES as readonly string[]).includes(value)) {
    throw new CliError('bad-settings', `readerDevice must be one of ${READER_DEVICES.join(', ')} or null`);
  }
  return value as ReaderDevice;
}

/** A boolean switch from a validated `--set` patch: `undefined` to remove
 * the key (back to its default), or the boolean itself. Pure. */
function validatedSwitch(name: string, value: unknown): boolean | undefined {
  if (value === null) return undefined;
  if (typeof value !== 'boolean') {
    throw new CliError('bad-settings', `${name} must be true, false or null`);
  }
  return value;
}

function buildAnswer(file: string, platform: NodeJS.Platform, env: Env): AppSettingsResult {
  const stored = readAppSettings(file);
  const device = stored.readerDevice;
  return {
    file,
    library: {
      path: libraryRoot(platform, env, file),
      chosen: chosenLibraryPath(platform, env, file),
      platformDefault: platformLibraryDefault(platform, env),
      fromEnv: envLibraryOverride(env) !== null,
    },
    home: homeFolder(env),
    formatDefaults: appDefaultOptions(file),
    shippedDefaults: DEFAULT_FORMAT_OPTIONS,
    customized: appDefaultsCustomized(file),
    keepScriptSettings: keepsScriptSettings(file),
    readerDevice:
      typeof device === 'string' && (READER_DEVICES as readonly string[]).includes(device) ? device as ReaderDevice : null,
    animatePage: stored.animatePage !== false,
    typewriterSounds: stored.typewriterSounds === true,
  };
}

export function appSettingsCommand(
  options: AppSettingsOptions,
  deps: AppSettingsDeps = {},
): AppSettingsResult {
  const platform = deps.platform ?? process.platform;
  const env = deps.env ?? process.env;
  const file = deps.settingsPath ?? appSettingsPath(platform, env);

  if (options.set !== undefined) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(options.set);
    } catch {
      throw new CliError('bad-settings', '--set is not valid JSON');
    }
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      throw new CliError('bad-settings', '--set must be a JSON object');
    }
    const patch = parsed as Record<string, unknown>;

    // Every key is checked BEFORE anything is validated or written: an
    // unknown key (most importantly `lastRoute`, which belongs to piece B)
    // must refuse the whole call, not just the key it does not recognise.
    for (const key of Object.keys(patch)) {
      if (!(WRITABLE_KEYS as readonly string[]).includes(key)) {
        throw new CliError(
          'usage',
          `app-settings --set takes only libraryPath, formatDefaults, keepScriptSettings, readerDevice, animatePage and typewriterSounds, not "${key}"`,
        );
      }
    }

    const hasLibraryPath = 'libraryPath' in patch;
    const hasFormatDefaults = 'formatDefaults' in patch;
    const hasKeepScriptSettings = 'keepScriptSettings' in patch;
    const hasReaderDevice = 'readerDevice' in patch;
    const hasAnimatePage = 'animatePage' in patch;
    const hasTypewriterSounds = 'typewriterSounds' in patch;

    // An empty object changes nothing: it answers like a plain read rather
    // than writing (and potentially overwriting an existing, even corrupt,
    // file with) `{}`.
    if (
      !hasLibraryPath && !hasFormatDefaults && !hasKeepScriptSettings &&
      !hasReaderDevice && !hasAnimatePage && !hasTypewriterSounds
    ) {
      return buildAnswer(file, platform, env);
    }

    // Phase 1: every PURE check runs first, for EVERY key, before any one
    // touches disk. A bad formatDefaults or keepScriptSettings must refuse
    // before a good, brand new libraryPath's folder is ever created, and a
    // bad libraryPath must refuse before either of the others is stored:
    // none of the checks below has a side effect, so which one the caller's
    // JSON happened to name first cannot change that.
    const write: Partial<AppSettings> = {};
    const libraryFolder = hasLibraryPath ? checkedLibraryPath(patch.libraryPath, platform) : undefined;
    if (hasFormatDefaults) write.formatDefaults = validatedFormatDefaults(patch.formatDefaults);
    if (hasKeepScriptSettings) {
      write.keepScriptSettings = validatedKeepScriptSettings(patch.keepScriptSettings);
    }

    if (hasReaderDevice) write.readerDevice = validatedReaderDevice(patch.readerDevice);
    if (hasAnimatePage) write.animatePage = validatedSwitch('animatePage', patch.animatePage);
    if (hasTypewriterSounds) {
      write.typewriterSounds = validatedSwitch('typewriterSounds', patch.typewriterSounds);
    }

    // Phase 2: the one side effect (creating and probing the chosen
    // folder), run LAST, only once every pure check above has already
    // passed.
    if (hasLibraryPath) {
      write.libraryPath = libraryFolder === undefined ? undefined : createdWritableFolder(libraryFolder);
    }

    try {
      writeAppSettings(write, file);
    } catch (err) {
      // The settings file's own folder can be unwritable too (a locked
      // config directory), which is not the library-folder check above at
      // all. Wrapped the same way, so a raw fs error never reaches the
      // caller as an uncaught 'internal' failure. `file` is named, the same
      // way createdWritableFolder names the library path above: "cannot
      // save the settings file" alone leaves the reader no way to tell
      // which file, on a machine that may have several.
      throw new CliError('bad-settings', `cannot save the settings file ${file}: ${plainReason(err)}`);
    }
  }

  return buildAnswer(file, platform, env);
}

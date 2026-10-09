// The app-wide settings file: one small JSON file for knobs that belong to
// the app itself, not to any one script. Shared by two pieces built in
// parallel: piece B stores the last-chosen send route (`lastRoute`) here,
// and piece C stores the library folder and app-wide format defaults here.
// Neither owns the file, so a write from one must keep the other's keys.
//
// It lives OUTSIDE the library on purpose. Piece C makes the library
// movable (a user can point SCREEPUB_LIBRARY, or later a setting, somewhere
// else), and a file that recorded where to find the library could not
// itself live inside the thing it locates.
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, isAbsolute, join, posix, resolve, win32 } from 'node:path';
import { writeFileAtomicSync } from '../atomic-write';

type Env = Record<string, string | undefined>;

/** The readers the window can remember as "this is mine". */
export const READER_DEVICES = ['kindle', 'apple', 'kobo', 'tolino', 'remarkable'] as const;
export type ReaderDevice = (typeof READER_DEVICES)[number];

export interface AppSettings {
  lastRoute?: string;
  readerDevice?: ReaderDevice;
  animatePage?: boolean;
  typewriterSounds?: boolean;
  [other: string]: unknown;
}

/** The user's home folder as this engine sees it: HOME first (set on POSIX,
 * and by some Windows shells), then USERPROFILE (Windows' own name for it),
 * then the OS's own answer. The one copy of that rule: the settings file,
 * the library's default folder, `app-settings`'s `home` answer and the Send
 * to Kindle app's ~/Applications all read it here, so none of them can name
 * a different folder `~` for the same environment. It lives in this module,
 * the lowest of those, because library.ts already imports this one. */
export function homeFolder(env: Env): string {
  return env.HOME || env.USERPROFILE || homedir();
}

/** An environment variable naming a folder, as an absolute path, or `null`
 * when it is unset, blank or relative. A relative value would resolve
 * against the engine's working folder, which is `/` when the app was
 * launched from Finder, so it is treated as unset rather than honoured
 * halfway: the same rule a stored libraryPath gets (library.ts's
 * resolvedIfAbsolute). The HOST's path rules, not a platform parameter's:
 * the value comes from this process's own environment. The one copy of
 * that rule, for SCREEPUB_CONFIG_DIR here and SCREEPUB_LIBRARY in
 * library.ts. */
export function absoluteEnvFolder(raw: string | undefined): string | null {
  const value = (raw ?? '').trim();
  return value !== '' && isAbsolute(value) ? resolve(value) : null;
}

/** Folder + 'settings.json'. SCREEPUB_CONFIG_DIR wins everywhere, when it
 * is an absolute path (see absoluteEnvFolder).
 *
 * Platform and env are parameters, not read from the host, for the same
 * reason `src/library.ts`'s `libraryRoot` takes them: it is the only way a
 * single test run can check all three operating systems' answers, and the
 * only way a real run stays out of the tester's actual home directory. */
export function appSettingsPath(
  platform: NodeJS.Platform = process.platform,
  env: Env = process.env,
): string {
  const override = absoluteEnvFolder(env.SCREEPUB_CONFIG_DIR);
  if (override !== null) return join(override, 'settings.json');

  // The PLATFORM's own path rules, not the host's, same as libraryRoot: a
  // path computed for win32 while running on posix (or back) has to use
  // win32's own join, not the host's.
  const path = platform === 'win32' ? win32 : posix;
  const home = homeFolder(env);

  if (platform === 'darwin') {
    return path.join(home, 'Library', 'Application Support', 'Screepub', 'settings.json');
  }

  if (platform === 'win32') {
    const appData = (env.APPDATA ?? '').trim();
    const base = appData !== '' ? appData : path.join(home, 'AppData', 'Roaming');
    return path.join(base, 'Screepub', 'settings.json');
  }

  // Everywhere else: XDG's config-home convention, lower-cased, the way
  // every other well-behaved tool on these systems names its folder.
  const xdgConfigHome = (env.XDG_CONFIG_HOME ?? '').trim();
  const configHome = path.isAbsolute(xdgConfigHome) ? xdgConfigHome : path.join(home, '.config');
  return path.join(configHome, 'screepub', 'settings.json');
}

/** Never throws: missing, unreadable, not JSON, or not an object all read
 * back as `{}` rather than stopping whatever asked. */
export function readAppSettings(path: string = appSettingsPath()): AppSettings {
  let text: string;
  try {
    text = readFileSync(path, 'utf8');
  } catch {
    return {};
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return {};
  }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {};
  return parsed as AppSettings;
}

/** Reads, shallow-merges `patch` over what is there (unknown keys kept, so a
 * write from one piece can never drop a key that belongs to the other),
 * writes atomically (temp file in the same folder, then rename: see
 * src/atomic-write.ts), and creates the folder.
 *
 * A key set to `undefined` in `patch` is removed rather than written as
 * `null`, so callers can delete a key without knowing the rest of the file.
 *
 * A file that is corrupt or unreadable reads as `{}` (readAppSettings), so
 * saving over it REPLACES it with the patch alone and whatever else it held
 * is gone. Intended: the owner chose replace over refusing the save on
 * 2026-10-01, so a damaged file heals on the next save instead of blocking
 * every save after it. tests/app-settings.test.ts pins this.
 *
 * The read and the write are not one atomic operation, so two engine calls
 * landing at the same moment (a route saving `lastRoute` while an
 * `app-settings --set` runs) can still interleave: both read the same
 * starting file, and whichever renames its temp file second wins, silently
 * dropping the other's key. Accepted rather than fixed with a lock: the
 * window is a single JSON read-modify-write, not a long-running job, and
 * both writers are a person doing one thing at a time (choosing a route,
 * opening the settings gear), not a background process that could pile up
 * concurrent calls. */
export function writeAppSettings(
  patch: Partial<AppSettings>,
  path: string = appSettingsPath(),
): AppSettings {
  const merged: AppSettings = { ...readAppSettings(path) };
  for (const [key, value] of Object.entries(patch)) {
    if (value === undefined) {
      delete merged[key];
    } else {
      merged[key] = value;
    }
  }

  const folder = dirname(path);
  if (!existsSync(folder)) mkdirSync(folder, { recursive: true });
  writeFileAtomicSync(path, `${JSON.stringify(merged, null, 2)}\n`);

  return merged;
}

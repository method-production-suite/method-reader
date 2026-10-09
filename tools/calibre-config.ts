// A scratch COPY of Calibre's settings folder, for anything in this repo
// that runs the engine's Calibre code but must not touch the real folder:
// the test suite (tests/isolate-calibre-config.ts, which explains the guard
// in full) and the picture tool (tools/capture/run.ts).
//
// A copy and not an empty folder, so a question like "is the KFX Output
// plugin installed?" gets the answer it would get outside, and the result
// screen in a picture says what it would say on this machine. `caches` stays
// behind; Calibre rebuilds it. Where there is no real folder (CI, any
// machine without Calibre), the copy is an empty folder.
import { cpSync, existsSync, mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

/** What root .env.test gives CALIBRE_CONFIG_DIRECTORY: a path under
 *  /dev/null, where Calibre can neither read nor make a folder.
 *  tests/calibre-config-isolation.test.ts pins that the two agree. */
export const CALIBRE_CONFIG_GUARD = '/dev/null/screepub-test-calibre-guard';

/** The folder Calibre would use, by Calibre's own rule:
 *  CALIBRE_CONFIG_DIRECTORY when the environment already sets it (that is
 *  then the folder Calibre uses, so it is the one to protect, and it is
 *  copied, never used), otherwise ~/Library/Preferences/calibre on macOS,
 *  %APPDATA%\calibre on Windows, and $XDG_CONFIG_HOME/calibre
 *  (~/.config/calibre by default) on Linux. */
export function realCalibreConfig(env: NodeJS.ProcessEnv, os: NodeJS.Platform): string {
  const set = (env.CALIBRE_CONFIG_DIRECTORY ?? '').trim();
  if (set !== '' && set !== CALIBRE_CONFIG_GUARD) return resolve(set);
  if (os === 'darwin') return join(homedir(), 'Library', 'Preferences', 'calibre');
  if (os === 'win32') return join(env.APPDATA ?? join(homedir(), 'AppData', 'Roaming'), 'calibre');
  return join((env.XDG_CONFIG_HOME ?? '').trim() || join(homedir(), '.config'), 'calibre');
}

/** Copies `real` (without its caches) to `<scratch>/calibre` and answers
 *  that path, or makes it empty when there is no real folder. */
export function copyCalibreConfig(real: string, scratch: string): string {
  const copy = join(scratch, 'calibre');
  if (existsSync(real)) {
    const caches = join(real, 'caches');
    // dereference: a symlink inside the real folder is copied as what it
    // points at, so nothing in the copy can lead Calibre back into it.
    cpSync(real, copy, { recursive: true, dereference: true, filter: (src) => src !== caches });
  } else {
    mkdirSync(copy);
  }
  return copy;
}

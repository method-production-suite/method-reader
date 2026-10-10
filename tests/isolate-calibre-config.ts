// Preloaded before every test file by bunfig.toml's [test] section, after
// tests/isolate-app-settings.ts. It does for Calibre's settings folder what
// that file does for Screepub's own settings file. Wherever Calibre is
// installed, the suite runs the real calibre-customize and ebook-convert,
// and an ordinary run of those rewrites files in Calibre's settings folder
// (gui.json, found by the 0.7.3 QA pass). So no test may run Calibre
// against the developer's real folder.
//
// THE CANONICAL EXPLANATION OF THE CALIBRE GUARD LIVES HERE. Root .env.test
// and tests/calibre-config-isolation.test.ts point back to this comment.
//
// What it does: copy the real folder, once per run, into a scratch folder
// in the system temp folder, and point CALIBRE_CONFIG_DIRECTORY at the copy.
// A copy and not an empty folder, so a test that asks whether the KFX
// Output plugin is installed gets the answer it would get outside the
// suite. `caches` stays behind; Calibre rebuilds it. Where there is no real
// folder (CI, any machine without Calibre), the copy is an empty folder.
//
// Which folder is real is Calibre's own rule: CALIBRE_CONFIG_DIRECTORY when
// the shell already sets it (that is then the folder Calibre uses, so it is
// the one to protect, and it is copied, never used), otherwise
// ~/Library/Preferences/calibre on macOS, %APPDATA%\calibre on Windows, and
// $XDG_CONFIG_HOME/calibre (~/.config/calibre by default) on Linux.
//
// A test that sets its own CALIBRE_CONFIG_DIRECTORY in a spawn's env (as
// tests/cli-kfx.test.ts does, per case) keeps it: this runs before any test
// file and changes nothing a test hands a child itself.
//
// THIS FILE IS HALF THE GUARD, for the reason tests/isolate-app-settings.ts
// gives at length: a runtime change to process.env reaches code in this
// process and any spawn that passes process.env as its env, and nothing
// else. The engine's own Calibre spawns (src/export/calibre.ts's
// runCalibre, and kfx.ts's plugin listing and installer) pass it on
// purpose, so an in-process kfxStatus(), toKfx() or toKepub() reaches the
// copy. A Bun.spawn with no env at all still gets the environment bun
// started with. For that, root .env.test sets CALIBRE_CONFIG_DIRECTORY to
// CALIBRE_CONFIG_GUARD (tools/calibre-config.ts): a path under /dev/null, where Calibre can
// neither read nor make a folder. A test that spawns Calibre bare therefore
// fails with NotADirectoryError instead of quietly using the real folder
// (measured on calibre 9.11, 2026-09-24). Keep the two values identical;
// tests/calibre-config-isolation.test.ts pins that they agree.
//
// Removal: an afterAll in a preload runs once, after the last test file of
// the run (checked on bun 1.3.14, 2026-09-24). There is no exit hook as
// well, because process.on('exit') never fires under bun test 1.3.14 (same
// check). A run cut short by --bail or Ctrl-C skips afterAll and leaves the
// copy behind, the same as every other scratch folder in tests/.
// The guard path, Calibre's rule and the copy itself live in
// tools/calibre-config.ts, shared with the picture tool, which runs the
// engine's Calibre probes too.
import { afterAll } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { copyCalibreConfig, realCalibreConfig } from '../tools/calibre-config';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-calibre-config-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

process.env.CALIBRE_CONFIG_DIRECTORY = copyCalibreConfig(realCalibreConfig(process.env, process.platform), SCRATCH);

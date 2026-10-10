// No test reaches the real Calibre settings folder. How the suite keeps it
// away, and why that takes a preload AND a line in root .env.test, is
// explained once, in tests/isolate-calibre-config.ts.
//
// This file does not import that preload. Importing it would run it, so a
// suite that had lost its bunfig.toml line would still pass here. What it
// needs is worked out on its own instead: the real folder by Calibre's own
// rule, the guard path by reading both files that name it.
import { afterAll, describe, expect, test } from 'bun:test';
import {
  chmodSync,
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { platform } from 'node:process';
import { calibreTool, runCalibre } from '../src/export/calibre';
import { kfxStatus } from '../src/export/kfx';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-calibre-config-isolation-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

const ROOT = join(import.meta.dir, '..');

/** Where Calibre keeps its settings on this machine when nothing points it
 *  elsewhere: Calibre's own per-platform rule, written out here rather than
 *  imported, so a wrong answer in the preload cannot agree with itself. */
function realFolder(): string {
  if (platform === 'darwin') return join(homedir(), 'Library', 'Preferences', 'calibre');
  if (platform === 'win32') {
    return join(process.env.APPDATA ?? join(homedir(), 'AppData', 'Roaming'), 'calibre');
  }
  return join(process.env.XDG_CONFIG_HOME || join(homedir(), '.config'), 'calibre');
}

/** The folder with symlinks resolved when it exists (macOS keeps the temp
 *  folder behind /var -> /private/var), or as written when it does not. */
function canonical(path: string): string {
  return existsSync(path) ? realpathSync(path) : resolve(path);
}

/** Whether `path` is `folder` itself or anywhere inside it. */
function within(path: string, folder: string): boolean {
  const rel = relative(canonical(folder), canonical(path));
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel));
}

/** The one value .env.test gives CALIBRE_CONFIG_DIRECTORY. */
function envTestGuard(): string {
  const match = readFileSync(join(ROOT, '.env.test'), 'utf8').match(/^CALIBRE_CONFIG_DIRECTORY=(.*)$/m);
  expect(match).not.toBeNull();
  return match![1]!;
}

const REAL = realFolder();

describe('the test-run guard: no test can reach the real Calibre settings folder', () => {
  test('CALIBRE_CONFIG_DIRECTORY is set, to a scratch folder in the temp folder, not the real one', () => {
    const live = (process.env.CALIBRE_CONFIG_DIRECTORY ?? '').trim();
    expect(live).not.toBe('');
    // Still the .env.test value means the preload never ran: every in-process
    // Calibre call would then fail on the guard instead of finding its copy.
    expect(live).not.toBe(envTestGuard());
    expect(existsSync(live)).toBe(true);
    // This file's own scratch folder sits in the system temp folder, so its
    // parent IS that folder, without a second tmpdir() call.
    expect(within(live, dirname(SCRATCH))).toBe(true);
    expect(within(live, REAL)).toBe(false);
    expect(within(REAL, live)).toBe(false);
  });

  test('a process spawned with no env gets the guard from .env.test, never the real folder', async () => {
    // Bun.spawn with no env hands the child the environment bun started with,
    // which the preload cannot change; .env.test is what reaches it.
    const proc = Bun.spawn(['bun', '-e', "console.log(process.env.CALIBRE_CONFIG_DIRECTORY ?? '')"], {
      cwd: ROOT,
      stdout: 'pipe',
      stderr: 'pipe',
    });
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]);
    expect(stderr).toBe('');
    expect(exitCode).toBe(0);
    expect(stdout.trim()).toBe(envTestGuard());
    expect(within(stdout.trim(), REAL)).toBe(false);
  });

  test('.env.test and the preload agree on the guard path', () => {
    // The preload treats this exact value as "nothing set", so a drift would
    // have it try to copy a folder under /dev/null and hand every test an
    // empty Calibre instead of a copy of the real one.
    const preload = readFileSync(join(ROOT, 'tools', 'calibre-config.ts'), 'utf8');
    const match = preload.match(/^export const CALIBRE_CONFIG_GUARD = '([^']+)';$/m);
    expect(match).not.toBeNull();
    expect(match![1]).toBe(envTestGuard());
  });

  test('the guard path is one Calibre cannot use, so a bare spawn fails loudly', async () => {
    // Handed the guard explicitly, never through this run's inherited
    // environment: if .env.test had lost its line, a bare spawn here would
    // run Calibre against the real folder, the very thing this file guards.
    const customize = calibreTool('calibre-customize');
    if (customize === null) return;
    const proc = Bun.spawn([customize, '--list-plugins'], {
      stdout: 'pipe',
      stderr: 'pipe',
      env: { ...process.env, CALIBRE_CONFIG_DIRECTORY: envTestGuard() },
    });
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]);
    // Measured 2026-09-24, calibre 9.11: NotADirectoryError on
    // .../global.py.json, exit 1, nothing on stdout.
    expect(exitCode).not.toBe(0);
    expect(stderr).toContain('Not a directory');
    expect(stdout).not.toContain('KFX Output');
  }, 30_000);

  test('the engine hands Calibre the scratch copy, not the environment bun started with', async () => {
    // runCalibre is how toKepub and toKfx start ebook-convert. A
    // stand-in tool writes down the folder it was given.
    if (platform === 'win32') return;
    const dir = mkdtempSync(join(SCRATCH, 'fake-calibre-'));
    const tool = join(dir, 'ebook-convert');
    const seen = join(dir, 'seen.txt');
    writeFileSync(tool, `#!/bin/sh\nprintf '%s' "$CALIBRE_CONFIG_DIRECTORY" > "$1"\n`);
    chmodSync(tool, 0o755);
    await runCalibre(tool, [seen]);
    expect(readFileSync(seen, 'utf8')).toBe(process.env.CALIBRE_CONFIG_DIRECTORY!);
  });

  test('the copy holds everything the real folder holds, caches aside', () => {
    if (!existsSync(REAL)) return;
    const copy = process.env.CALIBRE_CONFIG_DIRECTORY!;
    const inCopy = readdirSync(copy);
    for (const name of readdirSync(REAL).filter((n) => n !== 'caches')) {
      expect(inCopy).toContain(name);
    }
    if (existsSync(join(REAL, 'plugins'))) {
      const pluginsInCopy = readdirSync(join(copy, 'plugins'));
      for (const name of readdirSync(join(REAL, 'plugins'))) expect(pluginsInCopy).toContain(name);
    }
  });

  test('a KFX Output plugin installed in the real folder is found in the copy', async () => {
    // The reason for a copy and not an empty folder: the suite's KFX answers
    // match this machine's. It also proves kfxStatus's own spawn reaches the
    // copy, because a spawn left on bun's starting environment would get the
    // guard, fail, and read as "not installed".
    const plugins = join(REAL, 'plugins');
    if (calibreTool('calibre-customize') === null || !existsSync(plugins)) return;
    if (!readdirSync(plugins).some((name) => name.startsWith('KFX Output'))) return;
    expect((await kfxStatus()).pluginInstalled).toBe(true);
  }, 30_000);
});

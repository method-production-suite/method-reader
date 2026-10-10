// Finding and running Calibre's command-line tools, for the conversions
// that still need Calibre: KFX (export/kfx.ts, with Kindle Previewer behind
// it) and KEPUB. The AZW3 is Screepub's own now (src/azw3/), so no Kindle
// file but the KFX starts Calibre. Kindles do NOT index sideloaded EPUBs,
// which is why a USB copy is a KFX or an AZW3.
import { accessSync, constants, existsSync, renameSync } from 'node:fs';
import { basename, delimiter, join } from 'node:path';
import { platform } from 'node:process';

/** Calibre would otherwise undo two Screepub decisions during the convert: it
 * inserts page-break-before on every h2 (scene-per-page again), and its
 * remove-fake-margins heuristic sees side margins on most blocks — which is
 * what a screenplay's dialogue column looks like — and deletes them as
 * "publisher page margins", regardless of unit. Device-verified 2026-07-29;
 * change them in one place or not at all. (--extra-css is no rescue: on
 * multi-file EPUBs Calibre attaches it only to its generated inline ToC.) */
export const CALIBRE_FORMAT_GUARDS = [
  '--page-breaks-before=/',
  '--chapter-mark=none',
  '--disable-remove-fake-margins',
] as const;

function isExecutable(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/** One scanner for every Calibre CLI tool, so callers can never disagree
 * about whether Calibre exists. Per-OS: macOS keeps the tools inside the app
 * bundle, Linux installs them on PATH, Windows uses Program Files. */
function candidatePaths(name: string): string[] {
  if (platform === 'darwin') {
    return [
      `/Applications/calibre.app/Contents/MacOS/${name}`,
      `/opt/homebrew/bin/${name}`,
      `/usr/local/bin/${name}`,
    ];
  }
  if (platform === 'win32') {
    const exe = `${name}.exe`;
    return [
      join(process.env.ProgramFiles ?? 'C:\\Program Files', 'Calibre2', exe),
      join(process.env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)', 'Calibre2', exe),
      ...pathEntries(exe),
    ];
  }
  return [`/usr/bin/${name}`, `/usr/local/bin/${name}`, ...pathEntries(name)];
}

function pathEntries(name: string): string[] {
  return (process.env.PATH ?? '').split(delimiter).filter(Boolean).map((dir) => join(dir, name));
}

export function calibreTool(name: string): string | null {
  return candidatePaths(name).find((p) => existsSync(p) && isExecutable(p)) ?? null;
}

export function isCalibreAvailable(): boolean {
  return calibreTool('ebook-convert') !== null;
}

export class CalibreMissingError extends Error {
  constructor() {
    super("Calibre's ebook-convert was not found.");
    this.name = 'CalibreMissingError';
  }
}

export class CalibreFailedError extends Error {
  constructor(detail: string) {
    super(`ebook-convert failed: ${detail}`);
    this.name = 'CalibreFailedError';
  }
}

/** How long a conversion may run before it is stopped. A KFX build takes
 * 20 to 40 seconds and a long script more, so this is generous: it is not a
 * performance budget, only the point past which the run is plainly stuck
 * (Kindle Previewer waiting on a first-run dialog nobody can see, say) and
 * the book's turn in the window has to come back. */
export const CONVERSION_TIMEOUT_MS = 10 * 60 * 1000;

export class CalibreTimedOutError extends CalibreFailedError {
  constructor(tool: string, timeoutMs: number) {
    super('timed out');
    // Its own sentence, not CalibreFailedError's "ebook-convert failed:"
    // prefix, which would name the tool twice. Still a CalibreFailedError,
    // so every caller that reports a failed conversion reports this one the
    // same way (export-failed, send-failed or route-failed, by verb).
    this.message =
      `${basename(tool)} was still running after ${durationInWords(timeoutMs)}, so it was stopped. ` +
      'If Kindle Previewer or Calibre opened a window, close it and try again.';
    this.name = 'CalibreTimedOutError';
  }
}

/** "10 minutes", "60 seconds", "300 ms": whichever reads plainly. */
export function durationInWords(ms: number): string {
  if (ms >= 60_000 && ms % 60_000 === 0) {
    const minutes = ms / 60_000;
    return `${minutes} minute${minutes === 1 ? '' : 's'}`;
  }
  if (ms >= 1000 && ms % 1000 === 0) {
    const seconds = ms / 1000;
    return `${seconds} second${seconds === 1 ? '' : 's'}`;
  }
  return `${ms} ms`;
}

export interface TimedRun {
  code: number;
  stdout: string;
  stderr: string;
  /** True when the run was stopped for outliving its timeout. */
  timedOut: boolean;
}

/** Spawn `argv`, collect its output, and stop it if it is still running
 * after `timeoutMs`. Every Calibre spawn goes through here, so none of them
 * can hold a caller forever.
 *
 * Stopping has to reach the CHILDREN too: what actually hangs is Kindle
 * Previewer, which ebook-convert's plugin starts, and a grandchild left
 * alive also keeps the output pipes open, so the reads below would never
 * finish either. On macOS and Linux the run gets a process group of its own
 * (`detached`) and the whole group is killed; Windows has no process groups
 * to signal, so there only the tool itself is stopped. */
export async function runWithTimeout(
  argv: string[],
  env: Record<string, string | undefined>,
  timeoutMs: number,
): Promise<TimedRun> {
  const ownGroup = process.platform !== 'win32';
  const proc = Bun.spawn(argv, { stdout: 'pipe', stderr: 'pipe', env, detached: ownGroup });
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    try {
      if (ownGroup) process.kill(-proc.pid, 'SIGKILL');
      else proc.kill('SIGKILL');
    } catch {
      // Already gone between the timer firing and the kill.
      try {
        proc.kill('SIGKILL');
      } catch {
        // Nothing left to stop.
      }
    }
  }, timeoutMs);
  try {
    const [code, stdout, stderr] = await Promise.all([
      proc.exited,
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
    ]);
    return { code, stdout, stderr, timedOut };
  } finally {
    clearTimeout(timer);
  }
}

/** Exported because export/kfx.ts runs the same tool with the same guards.
 *  `env` replaces the child's environment when given (kfx.ts uses it to
 *  hand Kindle Previewer a temp folder of its own); omitted, the child gets
 *  this process's environment as it stands NOW.
 *
 *  That is why the default is process.env and not a missing `env`: a
 *  Bun.spawn with no env hands the child the environment bun STARTED with,
 *  and a later change to process.env never reaches it. The test suite
 *  depends on the difference. tests/isolate-calibre-config.ts points
 *  CALIBRE_CONFIG_DIRECTORY at a scratch copy of Calibre's settings folder
 *  at run time, and Calibre has to see it or it rewrites the real one.
 *  kfx.ts's own Calibre spawns pass process.env for the same reason. */
export async function runCalibre(
  tool: string,
  args: string[],
  env: Record<string, string | undefined> = process.env,
  timeoutMs: number = CONVERSION_TIMEOUT_MS,
): Promise<void> {
  const { code, stderr, timedOut } = await runWithTimeout([tool, ...args], env, timeoutMs);
  if (timedOut) throw new CalibreTimedOutError(tool, timeoutMs);
  if (code !== 0) throw new CalibreFailedError(stderr.trim() || `exit ${code}`);
}

/** Convert an EPUB to KEPUB for Kobo's own renderer (Calibre 7.1+ emits
 * KEPUB with sentence-level koboSpan markup when the output extension is
 * `.kepub`). Two-step, matching the Swift app's converter exactly: ebook-convert
 * selects its OUTPUT FORMAT from the output file's extension, so converting
 * straight to `<stem>.kepub.epub` makes it see ".epub" and emit a plain
 * EPUB with no koboSpan markup at all — the one thing that makes this a
 * KEPUB. Convert to `<stem>.kepub` first, then rename to the double
 * extension the Kobo renderer selects on.
 *
 * DELIBERATE DIVERGENCE from the Swift app's converter (2026-09-12): the
 * Swift passed the guards to its AZW3 conversion but not here, and this port
 * passed them to both (the AZW3 is Screepub's own now, and KFX takes them). That difference was carried across faithfully at first, on the
 * rule that device behavior nobody can re-verify gets preserved rather
 * than "fixed" — but the rule does not apply, because the guards are not
 * device behavior. `--disable-remove-fake-margins` governs how Calibre
 * READS the EPUB: the heuristic strips per-block side margins during
 * input processing, before the output format is chosen, and the Swift's
 * own note records it deleting them "regardless of unit". A screenplay's
 * dialogue column is exactly what it mistakes for publisher page margins,
 * so KEPUB is affected for the same reason AZW3 was — device-verified
 * 2026-07-29 on the AZW3 path. The Swift omission reads as a plain bug.
 *
 * Consequence: TypeScript and Swift disagreed here until the Swift app was
 * deleted at piece F3. Nothing consumes this function yet, so no user sees
 * the behavior. Still unconfirmed on real hardware —
 * no Kobo has ever been connected to this project — so this stays on the
 * device-checklist for the first Kobo pass. */
export async function toKepub(epub: string): Promise<string> {
  const tool = calibreTool('ebook-convert');
  if (!tool) throw new CalibreMissingError();
  const stem = epub.replace(/\.epub$/i, '');
  const raw = `${stem}.kepub`;
  const kepub = `${stem}.kepub.epub`;
  await runCalibre(tool, [epub, raw, ...CALIBRE_FORMAT_GUARDS]);
  if (!existsSync(raw)) {
    throw new CalibreFailedError('ebook-convert exited cleanly but produced no .kepub');
  }
  renameSync(raw, kepub); // replaces an older .kepub.epub in one step
  return kepub;
}

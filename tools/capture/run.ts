// One capture run, from claiming the demo library to putting it back.
//
// Everything a run creates, it removes: any engine still running, Chrome,
// the page server, the demo library, the first-pass pictures, and the
// folders above the library that it had to make (and only those, and only
// if they are empty). Cleanup runs whether the run succeeds, fails or is
// interrupted (the command hands it to its signal handlers), each step on
// its own so one failure cannot skip the rest, and a cleanup failure is
// reported after the error that ended the run, never instead of it.
//
// Nothing is written to the repository until every picture has been taken,
// no engine call was refused or failed, and the page server hit no error,
// so a run that dies half-way leaves the committed pictures exactly as they
// were. The library's marker names the run's pid, so two runs at once
// cannot delete each other's library.

import {
  existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, rmdirSync, writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, sep } from 'node:path';
import { type Browser, settles } from './cdp';
import { makeHandler, type EngineResult } from './server';
import { type Shot, framedSize, outputsFor, writeIfChanged } from './shots';

/** A failure the tool expected and can name: its message says it all. */
export class CaptureError extends Error {}

const CLEANUP_ERRORS = new WeakMap<object, string[]>();

/** What went wrong while cleaning up after `err`, to report after it. */
export function cleanupErrorsOf(err: unknown): string[] {
  return (typeof err === 'object' && err !== null && CLEANUP_ERRORS.get(err)) || [];
}

/** The file that says a library is the capture tool's. Without it, a
 *  library found at the demo path belongs to somebody and is never touched. */
export const MARKER = '.screepub-capture';

export interface RunOptions {
  shots: Shot[];
  repoDir: string;
  demoPdf: string;
  /** The demo library. The result screen prints paths inside it. */
  library: string;
  launch: () => Promise<Browser>;
  /** The engine's command line, before the window's own arguments. */
  engine: string[];
  /** Where the pictures go: the repository, outside tests. */
  outDir: string;
  /** Where the first-pass pictures are kept. Defaults to the OS temp folder. */
  tmp?: string;
  log: (line: string) => void;
  /** Handed the run's cleanup before anything is created, so a signal
   *  handler can run it. Safe to call more than once. */
  onCleanup?: (cleanup: () => Promise<string[]>) => void;
}

/** The subset of a Bun subprocess the cleanup needs. */
interface Running {
  kill(signal?: NodeJS.Signals | number): void;
  exited: Promise<number>;
}

const messageOf = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** What a marker says: which capture process made the library, and the
 *  highest folder it made. `made` is trusted only if it names the library
 *  itself or a folder above it, never anywhere else. */
function readMarker(marker: string, library: string): { pid: number | null; made: string | null } {
  try {
    const { pid, made } = JSON.parse(readFileSync(marker, 'utf8')) as { pid?: unknown; made?: unknown };
    return {
      pid: typeof pid === 'number' && Number.isInteger(pid) && pid > 0 ? pid : null,
      made: typeof made === 'string' && (made === library || library.startsWith(made + sep)) ? made : null,
    };
  } catch {
    return { pid: null, made: null };
  }
}

/** Whether a process is running. EPERM means it is, as somebody else. */
function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return (e as NodeJS.ErrnoException).code !== 'ESRCH';
  }
}

/** Make the demo library and mark it as this process's. Returns the
 *  highest folder the tool made on the way (the library itself, or a
 *  Documents folder above it), which is as far up as cleanup may ever go.
 *
 *  A library that is already there is removed only if its marker names a
 *  capture process that no longer exists: an interrupted run left it, and
 *  whatever that run made above it is then this run's to remove too. A
 *  marker naming a live process is a capture still running, and two runs
 *  must not delete each other's library. Anything else is refused. */
export function claimLibrary(library: string, pid: number = process.pid): string {
  let inherited: string | null = null;
  if (existsSync(library)) {
    const marker = join(library, MARKER);
    if (!existsSync(marker)) {
      throw new CaptureError(
        `capture: ${library} already exists, and the capture tool did not make it ` +
          `(it has no ${MARKER}). It is not the tool's to touch; move it and rerun.`,
      );
    }
    const found = readMarker(marker, library);
    if (found.pid === null) {
      throw new CaptureError(
        `capture: ${library} has a ${MARKER} that names no capture pid, so there is no telling ` +
          'whether a capture is still using it. If none is running, remove the folder and rerun.',
      );
    }
    if (alive(found.pid)) {
      throw new CaptureError(`capture pid ${found.pid} is using ${library}. Let it finish, or stop it, and rerun.`);
    }
    inherited = found.made;
    rmSync(library, { recursive: true, force: true });
  }
  const made = mkdirSync(library, { recursive: true }) ?? library;
  // Both are the library or a folder above it, so the shorter is higher.
  const top = inherited !== null && inherited.length < made.length ? inherited : made;
  writeFileSync(join(library, MARKER), `${JSON.stringify({ pid, made: top })}\n`);
  return top;
}

/** Remove each folder above the library that the tool made, bottom up,
 *  stopping at the first that is not empty: something in it is not ours. */
function removeMadeParents(library: string, top: string): void {
  if (top === library) return;
  for (let dir = dirname(library); dir === top || dir.startsWith(top + sep); dir = dirname(dir)) {
    try {
      rmdirSync(dir);
    } catch (e) {
      const code = (e as NodeJS.ErrnoException).code;
      if (code === 'ENOENT') continue;
      if (code === 'ENOTEMPTY' || code === 'EEXIST') return;
      throw e;
    }
  }
}

/** Everything a conversion left in the library, gone; the marker stays. */
function emptyLibrary(library: string): void {
  for (const entry of readdirSync(library)) {
    if (entry !== MARKER) rmSync(join(library, entry), { recursive: true, force: true });
  }
}

function engineRunner(engine: string[], library: string, running: Set<Running>, closing: () => boolean) {
  return async (args: string[]): Promise<EngineResult> => {
    // Once cleanup has begun, no engine starts: cleanup stops the engines it
    // knows about, and one started after that would outlive it and make the
    // library again. Answered like an engine that could not start, which the
    // server turns into a 500.
    if (closing()) return { code: null, stdout: '', stderr: 'capture: cleaning up, so no new engine calls' };
    let proc;
    // Made before every call, not once: emptyLibrary() clears the library
    // between shots. A missing root lists nothing too (volumes.ts skips it),
    // but an empty folder is the arrangement the tests pin.
    const noVolumes = join(library, '.no-volumes');
    try {
      mkdirSync(noVolumes, { recursive: true });
    } catch (e) {
      return { code: null, stdout: '', stderr: `capture: could not make ${noVolumes}: ${messageOf(e)}` };
    }
    try {
      proc = Bun.spawn([...engine, ...args], {
        stdin: 'ignore',
        stdout: 'pipe',
        stderr: 'pipe',
        // Piece C's app defaults (src/settings/app-defaults.ts) would
        // otherwise become the base under every conversion this run makes,
        // which means the pictures would drift with whatever the developer
        // running this tool last chose in their own app settings. A folder
        // under the SCRATCH library, cleared with it before every shot,
        // reads back as no settings at all, the same guarantee the test
        // suite's own SCREEPUB_CONFIG_DIR gives it. The only write the gate
        // lets into it is a card's reader device.
        env: {
          ...process.env, SCREEPUB_LIBRARY: library, SCREEPUB_CONFIG_DIR: join(library, '.capture-app-settings'),
          // No real reader, ever: an empty mount parent, and a reMarkable
          // address nothing listens on.
          SCREEPUB_VOLUME_ROOTS: noVolumes,
          SCREEPUB_REMARKABLE_ENDPOINT: 'http://127.0.0.1:9',
        },
      });
    } catch (e) {
      return { code: null, stdout: '', stderr: `capture: could not start the engine: ${messageOf(e)}` };
    }
    running.add(proc);
    try {
      // Both streams at once. A pipe nobody reads fills up, and an engine
      // blocked writing to it never exits.
      const [stdout, stderr, code] = await Promise.all([
        new Response(proc.stdout).text(),
        new Response(proc.stderr).text(),
        proc.exited,
      ]);
      return { code: proc.signalCode ? null : code, stdout, stderr };
    } finally {
      running.delete(proc);
    }
  };
}

/** Wait for the engine calls in flight to finish. A call can outlive the
 *  page that made it (the version check at boot, say), and the library must
 *  not be emptied under it. */
async function settleEngines(running: Set<Running>, ms = 60_000): Promise<void> {
  const all = [...running];
  if (all.length === 0) return;
  if (!(await settles(Promise.all(all.map((p) => p.exited)), ms))) {
    throw new CaptureError(`capture: an engine call was still running after ${ms / 1000} s`);
  }
}

/** Stop every engine still running and wait for it to exit: an engine
 *  mid-conversion would otherwise make the library again after it is gone. */
async function stopEngines(running: Set<Running>): Promise<void> {
  const all = [...running];
  for (const p of all) p.kill('SIGTERM');
  await Promise.all(all.map(async (p) => {
    if (await settles(p.exited, 5_000)) return;
    p.kill('SIGKILL');
    await p.exited;
  }));
}

export async function runCapture(o: RunOptions): Promise<void> {
  const top = claimLibrary(o.library);
  const running = new Set<Running>();
  let passOne: string | null = null;
  let server: ReturnType<typeof Bun.serve> | null = null;
  let browser: Browser | null = null;

  let closing = false;
  let cleaning: Promise<string[]> | null = null;
  const cleanup = () => (cleaning ??= (async () => {
    // First, before anything can await: no engine may start from here on.
    closing = true;
    const errors: string[] = [];
    const step = async (what: string, fn: () => unknown) => {
      try {
        await fn();
      } catch (e) {
        errors.push(`capture: cleanup could not ${what}: ${messageOf(e)}`);
      }
    };
    // In this order: nothing may write into the library after it is removed.
    await step('stop the engine', () => stopEngines(running));
    await step('close Chrome', () => browser?.close());
    await step('stop the page server', async () => {
      if (server !== null && !(await settles(Promise.resolve(server.stop(true)), 5_000))) {
        throw new Error('it did not stop within 5 s');
      }
    });
    await step(`remove ${o.library}`, () => rmSync(o.library, { recursive: true, force: true }));
    await step('remove the first-pass pictures', () => {
      if (passOne !== null) rmSync(passOne, { recursive: true, force: true });
    });
    await step(`remove the folders it made above ${o.library}`, () => removeMadeParents(o.library, top));
    return errors;
  })());
  o.onCleanup?.(cleanup);

  let failure: { error: unknown } | null = null;
  try {
    passOne = mkdtempSync(join(o.tmp ?? tmpdir(), 'screepub-capture-'));
    const token = crypto.randomUUID();
    const refused: string[] = [];
    const failed: string[] = [];
    const notFound: string[] = [];
    const serverErrors: string[] = [];
    let handle: (req: Request) => Promise<Response> = async () => new Response('', { status: 503 });
    server = Bun.serve({
      hostname: '127.0.0.1',
      port: 0,
      // A handler that throws would otherwise get Bun's own error page, a
      // 67 KB HTML document the window cannot use, and the page would sit
      // there until the capture timed out saying nothing useful. Written
      // down instead, so the capture fails at once and names it.
      fetch: (req) => handle(req).catch((e: unknown) => {
        serverErrors.push(messageOf(e));
        return new Response('', { status: 500 });
      }),
    });
    const host = `127.0.0.1:${server.port}`;
    handle = makeHandler({
      repoDir: o.repoDir, passOne, token, library: o.library, demoPdf: o.demoPdf, host,
      runEngine: engineRunner(o.engine, o.library, running, () => closing), refused, failed, notFound,
    });
    const base = `http://${host}`;
    const serverFailure = () => (serverErrors.length === 0 ? null : `the page server failed: ${serverErrors.join('; ')}`);
    /** One capture's checks: fail at once on a server error, and if it
     *  times out, name the files the server could not find for it. */
    const checks = () => {
      const since = notFound.length;
      return {
        failure: serverFailure,
        explain: () => {
          const mine = notFound.slice(since);
          return mine.length === 0 ? '' : `the page server could not find ${mine.join(', ')}`;
        },
      };
    };

    browser = await o.launch();
    // Once a run, so a release can tell "Chrome changed" from "the window did".
    o.log(`chrome  ${browser.version}`);

    const pictures: [string, Uint8Array][] = [];
    for (const shot of o.shots) {
      for (const theme of shot.themes) {
        let png: Uint8Array;
        if (shot.kind === 'site') {
          png = await browser.capture({
            url: `${base}/tools/capture/hero.html`,
            width: shot.width, height: shot.height, theme, transparent: false, ...checks(),
          });
        } else {
          // Every window shot starts from the same empty library, whatever
          // shot ran before it.
          await settleEngines(running);
          emptyLibrary(o.library);
          const one = await browser.capture({
            url: `${base}/capture/window.html?shot=${shot.name}`,
            width: shot.width, height: shot.height, theme, transparent: false, ...checks(),
          });
          const name = `${shot.name}-${theme}.png`;
          writeFileSync(join(passOne, name), one);
          png = await browser.capture({
            url: `${base}/tools/capture/frame.html?img=/pass-one/${name}&w=${shot.width}&h=${shot.height}`,
            ...framedSize(shot), theme, transparent: true, ...checks(),
          });
        }
        for (const out of outputsFor(shot, theme)) pictures.push([out, png]);
      }
    }

    await settleEngines(running);
    if (serverErrors.length > 0) {
      throw new CaptureError(`capture: the page server failed:\n  ${serverErrors.join('\n  ')}`);
    }
    if (refused.length > 0) {
      throw new CaptureError(`capture: the window made engine calls the gate refused:\n  ${refused.join('\n  ')}`);
    }
    if (failed.length > 0) {
      throw new CaptureError(`capture: engine calls failed:\n  ${failed.join('\n  ')}`);
    }
    // Only now: every picture taken, nothing refused, nothing failed.
    for (const [out, png] of pictures) o.log(`${writeIfChanged(join(o.outDir, out), png)}  ${out}`);
  } catch (error) {
    failure = { error };
    throw error;
  } finally {
    const errors = await cleanup();
    if (errors.length > 0) {
      if (failure === null) throw new CaptureError(errors.join('\n'));
      if (typeof failure.error === 'object' && failure.error !== null) {
        CLEANUP_ERRORS.set(failure.error, errors);
      }
    }
  }
}

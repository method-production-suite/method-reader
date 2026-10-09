// The only KFX writer in existence is inside Amazon's Kindle Previewer, so
// the KFX rung needs Calibre + Kindle Previewer + jhowell's KFX Output
// plugin, which drives Previewer headlessly. Amazon ships Previewer for
// macOS and Windows only: on Linux this rung is simply never ready, and
// formats.ts's ladder degrades to AZW3 without any special-casing.
//
// Plugin INSTALLATION lives at the bottom of this file. It used to be
// deferred because it "needs the vendored 485 KB zip"; there is no zip now,
// and that is the point — see the comment above installKfxPlugin.
import { existsSync, mkdtempSync, renameSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { platform } from 'node:process';
import {
  calibreTool, runCalibre, runWithTimeout, CALIBRE_FORMAT_GUARDS,
  CalibreMissingError, CalibreFailedError,
} from './calibre';

/** How long `calibre-customize --list-plugins` may take (normally about a
 * second of Python startup) before the plugin reads as not installed. */
export const PLUGIN_LIST_TIMEOUT_MS = 60 * 1000;

/** How long the plugin install may take: two downloads and Calibre's own
 * add_plugin, normally seconds, but on a slow connection more. */
export const PLUGIN_INSTALL_TIMEOUT_MS = 5 * 60 * 1000;

export interface KfxStatus {
  calibre: boolean;
  previewer: boolean;
  /** Only meaningful when `calibre` is true — the plugin lives inside it. */
  pluginInstalled: boolean;
  ready: boolean;
}

export function previewerPath(): string | null {
  if (platform === 'darwin') {
    const app = '/Applications/Kindle Previewer 3.app';
    return existsSync(app) ? app : null;
  }
  if (platform === 'win32') {
    const local = process.env.LOCALAPPDATA;
    if (!local) return null;
    const exe = join(local, 'Amazon', 'Kindle Previewer 3', 'Kindle Previewer 3.exe');
    return existsSync(exe) ? exe : null;
  }
  return null;
}

/** Whether this Calibre has the KFX Output plugin. A listing that does not
 * finish in time reads as "not installed": the ladder then degrades to AZW3
 * rather than waiting on a Calibre that may never answer. Exported (marked
 * internal) only so a test can give it a fake tool and a short timeout. */
export async function pluginInstalled(
  customize: string,
  timeoutMs: number = PLUGIN_LIST_TIMEOUT_MS,
): Promise<boolean> {
  // env: process.env, not left out: see runCalibre in calibre.ts.
  const { code, stdout, timedOut } = await runWithTimeout([customize, '--list-plugins'], process.env, timeoutMs);
  return !timedOut && code === 0 && listsKfxOutput(stdout);
}

/** Whether a `calibre-customize --list-plugins` listing holds the KFX Output
 * plugin: a plugin NAMED exactly "KFX Output", or a renamed copy of it named
 * "KFX Output (" and something. Exported (marked internal, like
 * computeReady) only so the parse can be tested against captured listings
 * on every platform.
 *
 * A substring test is not enough: the companion metadata writer that ships in
 * the same zip is named "Set KFX metadata (from KFX Output)", so with only
 * that one present the toolchain read as ready and the ladder tried a KFX
 * conversion that died with "No plugin to handle output format: kfx" instead
 * of falling back to AZW3. (INSTALL_SNIPPET below guards against the same
 * name for the same reason.) The renamed copies count because they convert:
 * the old Swift app installed one, "KFX Output (Fix Traditional Chinese)",
 * and a reader who has it keeps KFX. The Swift app itself still tests for
 * the substring (KFXToolchain.pluginMarker), companion and all.
 *
 * The listing is a table, not a line per name: each plugin row is its type,
 * then its name, then its version, then whether it is disabled, padded into
 * columns, with its description indented beneath it. calibre pads the
 * header row through the same format string, sized to the longest type and
 * name it holds, so the header says where the name column starts and ends on
 * this machine; reading it from there is what keeps a name that merely ENDS
 * in "KFX Output" from counting. The header is a literal in calibre (checked
 * in 9.11: it is not passed through translation), and a listing without one
 * names nothing: "not installed" degrades to AZW3, where a guess could strand
 * a send.
 *
 * The Disabled column (True or False) is not consulted, on purpose. Measured
 * 2026-09-24 on calibre 9.11 with a scratch CALIBRE_CONFIG_DIRECTORY: with
 * KFX Output disabled, `ebook-convert book.epub book.kfx` still wrote a real
 * KFX. calibre's plugin_for_output_format() ignores the disabled list; only
 * the calibre window's own format menu hides a disabled plugin, and
 * Screepub converts through ebook-convert, not that menu. */
export function listsKfxOutput(listing: string): boolean {
  const lines = listing.split(/\r?\n/);
  const header = pluginTableHeader(listing);
  if (header === null) return false;
  const nameAt = header.indexOf('Name');
  const versionAt = header.indexOf('Version');
  return lines.some((line) => {
    if (line === header || !/^\S/.test(line)) return false;
    const name = line.slice(nameAt, versionAt).trim();
    return name === 'KFX Output' || name.startsWith('KFX Output (');
  });
}

/** The header row of a `calibre-customize --list-plugins` table, found
 * wherever it is in the listing rather than assumed to be its first line,
 * or null when there is none. listsKfxOutput() reads the columns off it;
 * exported so the test that checks a real Calibre's table finds it the same
 * way. */
export function pluginTableHeader(listing: string): string | null {
  return listing.split(/\r?\n/).find((line) => /^Type +Name +Version\b/.test(line)) ?? null;
}

/** The conjunction behind `KfxStatus.ready`, pulled out as a pure function
 * and exported (marked internal, not part of the module's real interface)
 * only so the AND itself can be unit-tested with synthetic booleans. `ready`
 * can never actually be `true` on a platform where `previewerPath()` has no
 * branch (Linux) — Amazon ships no build there — so without this function
 * the conjunction's correctness would be provable only on macOS/Windows
 * hardware with Kindle Previewer installed. */
export function computeReady(s: { calibre: boolean; previewer: boolean; pluginInstalled: boolean }): boolean {
  return s.calibre && s.previewer && s.pluginInstalled;
}

/** What's present on this machine. Spawns `calibre-customize` (~1s of Python
 * startup) only when Calibre is actually present. */
export async function kfxStatus(): Promise<KfxStatus> {
  const customize = calibreTool('calibre-customize');
  const calibre = customize !== null;
  const previewer = previewerPath() !== null;
  const installed = calibre ? await pluginInstalled(customize) : false;
  return {
    calibre,
    previewer,
    pluginInstalled: installed,
    ready: computeReady({ calibre, previewer, pluginInstalled: installed }),
  };
}

/** The sibling `.kfx` for a given EPUB — same directory, same stem. Exported
 * so callers (including a later export-ladder task) derive the path here
 * rather than repeating the regex; mirrors Export.swift's mobiSibling(for:)
 * precedent. */
export function kfxSibling(epub: string): string {
  return `${epub.replace(/\.epub$/i, '')}.kfx`;
}

/** Hidden same-directory scratch the conversion writes into, mirroring
 * KFXToolchain.scratchURL exactly. Three constraints meet in this one name:
 * same DIRECTORY means same volume, so promoting the finished file is a
 * rename and not a copy; the `.kfx` EXTENSION has to survive, because
 * ebook-convert picks its output format from the extension and dies on
 * anything it does not recognise ("No plugin to handle output format: tmp");
 * and the LEADING DOT keeps Finder, library scans and the staleness rung
 * blind to a file that is not finished yet. */
export function kfxScratchPath(epub: string): string {
  const stem = basename(epub).replace(/\.epub$/i, '');
  return join(dirname(epub), `.${stem}.partial.kfx`);
}

/** What a KFX build still needs on this machine, named for a person:
 * KfxToolchainNotReadyError's sentence and `export --for kfx`'s refusal
 * both read it, so the two cannot list different things. */
export function kfxMissing(status: { calibre: boolean; previewer: boolean; pluginInstalled: boolean }): string[] {
  const missing: string[] = [];
  if (!status.calibre) missing.push('Calibre');
  if (!status.previewer) missing.push('Kindle Previewer');
  if (status.calibre && !status.pluginInstalled) missing.push('the KFX plugin');
  return missing;
}

export class KfxToolchainNotReadyError extends Error {
  constructor(status: { calibre: boolean; previewer: boolean; pluginInstalled: boolean }) {
    super(`KFX conversion needs ${kfxMissing(status).join(' and ')}.`);
    this.name = 'KfxToolchainNotReadyError';
  }
}

/** Injectable seams, present only so `toKfx` is testable. Both default to the
 * real thing. `tool` exists because the PATH-shadowing trick the other
 * Calibre tests use cannot reach a machine that has a REAL Calibre —
 * calibreTool checks the fixed install paths before PATH — and the argv this
 * function hands ebook-convert (specifically the output EXTENSION) has now
 * been wrong three times in this branch, so it needs a test everywhere, not
 * just on bare runners. */
export interface KfxDeps {
  tool?: () => string | null;
  status?: () => Promise<KfxStatus>;
  /** How long the conversion may run; CONVERSION_TIMEOUT_MS unless a test
   * needs to see a timeout in milliseconds. */
  timeoutMs?: number;
}

/** Convert an EPUB to KFX. Runs the same guard trio as the AZW3 recipe, from
 * the same constant, so a device-validated flag change lands on both rungs or
 * neither. Writes to a scratch path and renames into place, so a partial file
 * never appears where a freshness check would trust it — see kfxScratchPath
 * for what constrains that name. Most of the wall-clock is Kindle Previewer
 * cold-starting, hence onStage. */
export async function toKfx(
  epub: string,
  onStage?: (stage: string) => void,
  deps: KfxDeps = {},
): Promise<string> {
  const tool = (deps.tool ?? (() => calibreTool('ebook-convert')))();
  if (!tool) throw new CalibreMissingError();
  // KFXToolchain.convert's opening guard. Without it a machine that has
  // Calibre but not Previewer or the plugin gets ebook-convert's raw Python
  // failure instead of a sentence naming what to install. DIVERGENCE: Swift
  // checks readiness FIRST and reports a missing Calibre through the same
  // error; the port keeps CalibreMissingError ahead of it because that error
  // is the more precise answer and other call sites already discriminate on
  // it. (Swift's status() is cached, so its guard is ~free; ours re-probes,
  // which costs ~1s of Python startup against a ~20s conversion.)
  const status = await (deps.status ?? kfxStatus)();
  if (!status.ready) throw new KfxToolchainNotReadyError(status);
  const kfx = kfxSibling(epub);
  const scratch = kfxScratchPath(epub);
  // Kindle Previewer, which the plugin runs to do the real conversion,
  // writes a <uuid>/ folder into the temp folder on EVERY run (conv_out/,
  // conversionLog.csv, an intermediate .mobi; about 250 KB) and never
  // removes it. Measured 2026-09-22 with Calibre, KFX Output and Previewer
  // 3 on macOS: one conversion, one new folder in $TMPDIR, and 189 had
  // piled up there. Previewer honours TMPDIR (same measurement, redirected:
  // the folder landed in the redirect instead), and the plugin passes
  // TMPDIR, TMP and TEMP through to it, so each conversion gets a temp
  // folder of its own and loses it, whatever Previewer left inside, when
  // the conversion ends either way. TMP and TEMP are Windows's names for
  // the same thing; Previewer on Windows has not been measured.
  const previewerTmp = mkdtempSync(join(tmpdir(), 'screepub-kfx-'));
  const env = { ...process.env, TMPDIR: previewerTmp, TMP: previewerTmp, TEMP: previewerTmp };
  onStage?.('converting to KFX (Kindle Previewer can take ~20s to start)…');
  try {
    await runCalibre(tool, [epub, scratch, ...CALIBRE_FORMAT_GUARDS], env, deps.timeoutMs);
    if (!existsSync(scratch)) {
      throw new CalibreFailedError('ebook-convert exited cleanly but produced no .kfx');
    }
    // No delete first: the rename replaces an older .kfx in one step (on
    // Windows too), so a rename that fails leaves that older one in place
    // rather than no book at all.
    renameSync(scratch, kfx);
  } catch (error) {
    rmSync(scratch, { force: true });
    throw error;
  } finally {
    rmSync(previewerTmp, { recursive: true, force: true });
  }
  return kfx;
}

// ── installing jhowell's plugin, without shipping a copy of it ───────
//
// The header above says installation was deferred because it "needs the
// vendored 485 KB zip". That premise is gone: there is no zip. Calibre's
// own plugin index is the upstream and Calibre's own add_plugin is the
// installer, so the user gets whatever version is current on the day they
// ask rather than whatever we last vendored. The Swift app's copy was
// pinned at 2.12.0 and was a FORK; the index currently offers 2.20.1.
//
// Not shipping it also drops a GPL-3 redistribution obligation and the
// THIRD-PARTY-NOTICES entries that went with it.
//
// This runs inside Calibre's Python (`calibre-debug -c`) because everything
// it needs lives there: bz2 for the index, Calibre's cert-pinned fetch for
// the index itself, and add_plugin. Our side is one spawn and one JSON line.
//
// Integrity: the index declares the zip's exact byte size and the download
// is refused unless it matches. That is weaker than a signature, which does
// not exist for this plugin, and it is the honest limit of what can be
// checked. The index fetch uses Calibre's pinned CA; the zip download
// cannot, because the mirror's cert does not validate under that pinning.
//
// NEVER call this on its own initiative. It writes to the user's Calibre
// and fetches third-party code over the network, so it belongs behind an
// explicit request. It is also the one thing in this module that needs a
// network at all (registry: everything else works offline).
const INSTALL_SNIPPET = `
import bz2, io, json, os, tempfile, urllib.request, zipfile

# Raised only around the two NETWORK fetches below, so the outer except
# clauses can tell "could not reach the index or the zip" (most likely:
# offline) apart from every other way this can fail, and report the
# first one as one plain sentence instead of raw urllib/http text. Defined
# here, before the try, and not as the first line inside it: if anything
# raised before that line would have run, 'except Unreachable' would be
# evaluated against a name that was never bound, and the failure would be
# a bare NameError with no SCREEPUB_RESULT line at all.
class Unreachable(Exception):
    pass

# Older copies cleared to make room. Bound here, beside Unreachable and for
# the same reason, so BOTH failure lines below can report it: add_plugin
# failing after the clearing loop has already taken a copy out, and the
# caller must be able to say which.
removed = []

try:
    from calibre.utils.https import get_https_resource_securely
    from calibre.gui2.dialogs.plugin_updater import INDEX_URL
    from calibre.customize.ui import (
        add_plugin, initialized_plugins, output_format_plugins, remove_plugin)
    try:
        idx_raw = get_https_resource_securely(INDEX_URL)
    except Exception as e:
        raise Unreachable(str(e))
    idx = json.loads(bz2.decompress(idx_raw).decode('utf-8'))
    meta = idx.get('KFX Output')
    if meta is None:
        raise RuntimeError('KFX Output is not in calibre\\'s plugin index')
    # Built OUTSIDE the try below: an index entry with no 'file' is a broken
    # index, not a lost connection, and must not be reported as one.
    zip_url = 'https://plugins.calibre-ebook.com/' + meta['file']
    try:
        data = urllib.request.urlopen(zip_url, timeout=120).read()
    except Exception as e:
        raise Unreachable(str(e))
    if len(data) != meta['size']:
        raise RuntimeError('downloaded %d bytes, index says %d' % (len(data), meta['size']))
    if '__init__.py' not in zipfile.ZipFile(io.BytesIO(data)).namelist():
        raise RuntimeError('downloaded file is not a calibre plugin')
    # Clear FORKS first. A variant registers the same internal package
    # (calibre_plugins.kfx_output) under a different plugin NAME, so calibre
    # happily holds both and the new plugin's code then imports the fork's
    # kfxlib -- KFX conversion dies with ImportError and the toolchain still
    # reports ready. Adding without clearing is worse than not installing.
    #
    # Only CONVERSION OUTPUT plugins can collide for the .kfx slot, and
    # calibre is asked which those are rather than guessed at by name. A
    # name-only test also matched "Set KFX metadata (from KFX Output)",
    # which is the companion metadata writer shipping in the same zip, and
    # deleted it; it survived only because add_plugin put it back.
    #
    # It runs after the download and its checks, so every failure up to this
    # point, offline included, leaves whatever was there untouched. From
    # here on it no longer holds: if add_plugin fails below, a copy may
    # already be gone, which is why both failure lines report 'removed'.
    outs = set()
    for p in output_format_plugins():
        outs.add(getattr(p, 'name', ''))
    for p in list(initialized_plugins()):
        n = getattr(p, 'name', '')
        if n in outs and 'KFX Output' in n and n != 'KFX Output':
            remove_plugin(p)
            removed.append(n)
    fd, path = tempfile.mkstemp(suffix='.zip')
    try:
        os.write(fd, data); os.close(fd)
        add_plugin(path)
    finally:
        os.unlink(path)
    print('SCREEPUB_RESULT ' + json.dumps(
        {'ok': True, 'version': '.'.join(map(str, meta['version'])), 'removed': removed}))
except Unreachable as e:
    print('SCREEPUB_RESULT ' + json.dumps(
        {'ok': False, 'error': str(e), 'offline': True, 'removed': removed}))
except Exception as e:
    print('SCREEPUB_RESULT ' + json.dumps({'ok': False, 'error': str(e), 'removed': removed}))
`;

export interface KfxInstallResult {
  ok: boolean;
  /** The version Calibre installed, when it succeeded. */
  version?: string;
  /** Why not, in words a user can act on. */
  reason?: string;
  /** Conflicting KFX forks cleared to make room. Usually empty; non-empty
   *  for anyone upgrading from the Swift app's vendored copy, which
   *  installs under a different NAME and would otherwise break conversion
   *  outright. Worth surfacing: we removed something they installed.
   *  Carried on a FAILURE too, whenever Calibre's answer was read: a fork is
   *  cleared before add_plugin runs, so an add_plugin that then fails leaves
   *  the reader with one copy fewer than they had. */
  removed?: string[];
}

type DebugRunner = (
  argv: string[],
) => Promise<{ code: number; stdout: string; stderr: string; timedOut?: boolean }>;

/** The real runner: spawns calibre-debug and stops it, children and all, if
 * it is still running after `timeoutMs`. Exported (marked internal) so a
 * test can hand it a short timeout and a fake tool. */
export function debugRunner(timeoutMs: number = PLUGIN_INSTALL_TIMEOUT_MS): DebugRunner {
  // env: process.env, not left out: see runCalibre in calibre.ts.
  return (argv) => runWithTimeout(argv, process.env, timeoutMs);
}


/**
 * Install (or update to) the current KFX Output plugin, using Calibre's own
 * index and installer. Never throws: the KFX rung is the top of a ladder
 * that degrades to AZW3 then MOBI, so a failure here must not take the
 * caller down with it.
 */
export async function installKfxPlugin(
  run: DebugRunner = debugRunner(),
  debugTool: string | null = calibreTool('calibre-debug'),
): Promise<KfxInstallResult> {
  if (!debugTool) {
    return { ok: false, reason: "Calibre was not found, and the plugin lives inside it." };
  }
  const { code, stdout, stderr, timedOut } = await run([debugTool, '-c', INSTALL_SNIPPET]);
  if (timedOut) {
    // Whatever it printed is not an answer: it was cut off. Whether a fork
    // had already been cleared cannot be known, so nothing is claimed.
    return {
      ok: false,
      reason: 'Calibre was still installing the plugin after a long wait, so it was stopped. Check the internet connection, then try again.',
    };
  }
  // The result LINE decides, not the exit code: calibre-debug exits 0 for a
  // snippet that caught its own exception, so a bare exit code would read a
  // reported failure as success.
  const line = stdout.split('\n').find((l) => l.startsWith('SCREEPUB_RESULT '));
  if (!line) {
    const detail = (stderr.trim() || stdout.trim() || `calibre-debug exited ${code}`).slice(0, 400);
    return { ok: false, reason: detail };
  }
  try {
    const parsed = JSON.parse(line.slice('SCREEPUB_RESULT '.length)) as {
      ok?: boolean; version?: string; error?: string; removed?: string[]; offline?: boolean;
    };
    const removed = parsed.removed ?? [];
    if (parsed.ok && parsed.version) {
      return { ok: true, version: parsed.version, removed };
    }
    if (parsed.offline) {
      // Either fetch: the index, or the plugin's own zip. So the sentence
      // names the site both come from, not just the index.
      return {
        ok: false,
        reason: "Calibre's plugin site could not be reached. Check the internet connection, then try again.",
        removed,
      };
    }
    return { ok: false, reason: parsed.error ?? 'calibre reported a failure with no reason', removed };
  } catch {
    return { ok: false, reason: `could not read calibre's answer: ${line.slice(0, 200)}` };
  }
}

// The `export` verb: the file you would actually put on a reader. `send`
// never converts, and a Kindle never indexes a sideloaded EPUB, so the
// window needs a way to ask for the Kindle rung before it sends. The LADDER
// is src/export/artifact.ts's — this file chooses nothing, it only carries
// the answer out. Returns a value; cli.ts owns stdout.
import { mkdirSync, readFileSync, statSync } from 'node:fs';
import { copyFile, mkdir, rename, unlink } from 'node:fs/promises';
import { basename, dirname, extname, isAbsolute, join, resolve } from 'node:path';
import { tempPathFor as atomicTempPathFor, writeFileAtomicSync } from './atomic-write';
import { CliError, errorMessage } from './cli-errors';
import {
  availableFormats,
  freshArtifactOf,
  freshKindleArtifact as realFreshKindleArtifact,
  kindleArtifactPlan,
  rungPlan,
  type FreshKindleArtifactOptions,
  type KindleArtifactPlan,
  type RungBuilders,
} from './export/artifact';
import { isCalibreAvailable } from './export/calibre';
import {
  EXPORT_FORMATS,
  fileExtension,
  formatLabel,
  type ExportFormat,
  type KindleRung,
} from './export/formats';
import { kfxMissing, kfxStatus as realKfxStatus, type KfxStatus } from './export/kfx';
import { fountainToFdx } from './fdx/fdx';
import { resolveFormatOptions, type FormatOptions } from './options';
import { appDefaultOptions } from './settings/app-defaults';
import { loadScriptSettings } from './settings/sidecar';

export interface ExportResult {
  path: string;
  format: ExportFormat;
  extension: string;
  label: string;
  available: ExportFormat[];
  stages: string[];
  /** --check only: whether the file at `path` is current (the EPUB always
   * is: it is the book itself; an FDX is when it is newer than its
   * .fountain). */
  fresh?: boolean;
  /** --check, --for kindle, kfx, azw3 or mobi only: what builds the file
   * when it is not current. */
  builtBy?: KindleArtifactPlan['builtBy'];
  /** --check, --for kindle, kfx, azw3 or mobi only: when the EPUB was last
   * written (ms), which tells the window one version of the book from the
   * next. */
  bookDate?: number | null;
}

export interface ExportOptions {
  /** An EXISTING .epub — the conversion's primary output. */
  epub: string;
  /** One of EXPORT_FORMATS; 'epub' when absent. */
  for?: string;
  /** The script's .fountain: required by `mobi` (the rebuild), `fountain`
   * (the file itself) and `fdx` (built from nothing else), and used by the
   * ladder's MOBI rung. */
  fountain?: string;
  /** This script's settings, as --options-json carries them elsewhere. */
  optionsJson?: string;
  /** Where the window's own save dialog said to put a copy. The window
   * never writes a file itself (see cli.ts's --out); when given, it MUST
   * be absolute, checked before any toolchain probe or ladder run. */
  out?: string;
  /** Say which file this machine hands over and whether the one on disk is
   * current, and build nothing: the window asks this when the send panel
   * opens, to name the file on its rows and to decide whether to start a
   * slow build before anyone presses anything. */
  check?: boolean;
}

// Injectable seams, same shape as tools/build-cli.ts's `Spawn` and
// tools/sidecar-targets.ts's `HostTriple`: a typed function, defaulted to
// the real thing, so a test can drive a specific rung (KFX ready / Calibre
// only / neither) without depending on which toolchain happens to be
// installed on the machine running the suite. Real Calibre on Linux checks
// FIXED paths before PATH (src/export/calibre.ts's candidatePaths), so the
// PATH-shadowing trick used elsewhere in this codebase cannot reach a
// machine that has a genuine install — this seam is the only way to test
// the wrapper's rung SELECTION independently of what is actually on disk.
// Production call sites pass no `deps` at all and get the real detectors
// untouched.
export type CalibreProbe = () => boolean;
export type KfxProbe = () => Promise<KfxStatus>;
export type KindleLadder = (opts: FreshKindleArtifactOptions) => Promise<string>;

export interface ExportDeps {
  calibreAvailable?: CalibreProbe;
  kfxStatus?: KfxProbe;
  freshKindleArtifact?: KindleLadder;
  /** What builds the KFX and AZW3 rungs (the real toKfx and toAzw3 when
   * absent). The explicit rungs and the default ladder both build through
   * these, so a test can drive every branch with no Calibre reachable. */
  rungBuilders?: RungBuilders;
  /** Where to read the app-wide format defaults from. Defaults to the
   * production app settings file (`appSettingsPath()`); a test points this
   * at a scratch file so it never touches a real one. */
  appSettingsPath?: string;
}

/** The base `--options-json` overlays: the same precedence a conversion
 * honours (flags > sidecar > app defaults > shipped), so a rebuild through
 * the MOBI rung starts from this SCRIPT'S own tuning, not a stranger's
 * defaults, whenever export is told which script it is. `--fountain` is
 * optional (a bare `--for epub` never needs it, and the window leaves it
 * out only when the conversion wrote no .fountain, as with a script
 * converted from Fountain input; a window whose settings read failed
 * drops `--options-json` instead, never `--fountain`), so when it is
 * absent the base is just the app defaults, same as before this
 * script-aware read existed. */
function readFormatBase(fountainPath: string | undefined, appSettingsPath: string | undefined): FormatOptions {
  const appDefaults = appDefaultOptions(appSettingsPath);
  return fountainPath === undefined ? appDefaults : loadScriptSettings(fountainPath, appDefaults);
}

/** `--options-json`, overlaid on `base`. A FULL object (what the window
 * actually sends: `screepub settings`'s answer, read back whole) leaves
 * nothing for `base` to fill, so this changes nothing observable for it. */
function readFormat(optionsJson: string | undefined, base: FormatOptions): FormatOptions {
  if (optionsJson === undefined) return base;
  let parsed: unknown;
  try {
    parsed = JSON.parse(optionsJson);
  } catch {
    throw new CliError('bad-options', '--options-json is not valid JSON');
  }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new CliError('bad-options', '--options-json must be a JSON object');
  }
  return resolveFormatOptions(parsed as Record<string, unknown>, base);
}

/** The extension a chosen --out must carry, checked against the artifact's
 * own (never the other way: a person can type any name in a save dialog).
 * Case-insensitive on both sides; the message names the wrong one in caps,
 * the way formatLabel already names a format, and the right one lowercase
 * with its dot, ready to paste into a filename. */
function checkOutExtension(out: string, wantExt: string): void {
  const outExt = extname(out).slice(1);
  if (outExt.toLowerCase() !== wantExt.toLowerCase()) {
    // Names what the FILE is, not the name the person typed: a book that is
    // an AZW3 saved as `.kfx` would be a broken book, so the sentence says
    // AZW3 and the extension to use.
    const kind = wantExt.toUpperCase();
    const article = /^[AEIOU]/.test(kind) ? 'an' : 'a';
    throw new CliError(
      'usage',
      `that is ${article} ${kind} file: choose a name ending in .${wantExt.toLowerCase()}`,
    );
  }
}

/** Temp file in the destination folder, then rename over: a reader watching
 * `out` never observes a partial write, and an existing file there is
 * replaced in one step. On any failure the temp file is removed too. No
 * trace left beside the destination either way. Mirrors cli.ts's own
 * writeFileAtomic, which this cannot import (that one takes bytes already
 * in memory; this one has a source file on disk to stream from instead). */
async function copyArtifactAtomic(source: string, destination: string): Promise<void> {
  await mkdir(dirname(destination), { recursive: true });
  const tmp = tempPathFor(destination);
  try {
    await copyFile(source, tmp);
    await rename(tmp, destination);
  } catch (err) {
    await unlink(tmp).catch(() => {});
    throw err;
  }
}

/** The hidden temp sibling copyArtifactAtomic writes through. One
 * derivation, so the failure message below can find it and take it out. */
function tempPathFor(destination: string): string {
  return join(dirname(destination), `.${basename(destination)}.${process.pid}.tmp`);
}

/** Resolve --out against the artifact the ladder (or the epub rung)
 * actually produced. Already sitting at `out`: answer without copying,
 * since it is already there, and copying a file onto itself is only ever a
 * footgun. Otherwise copy it there atomically. Either way the returned path
 * IS `out`, per the contract: every other field in the answer is unchanged.
 *
 * A failed copy says where it was going, and the system's reason with the
 * hidden temp file (`.<name>.<pid>.tmp`, which the person never chose and
 * cannot find) replaced by the path they did choose. */
async function resolveOut(artifactPath: string, out: string): Promise<string> {
  if (resolve(out) !== resolve(artifactPath)) {
    try {
      await copyArtifactAtomic(artifactPath, out);
    } catch (err) {
      const reason = errorMessage(err).split(tempPathFor(out)).join(out);
      throw new CliError('export-failed', `could not save the copy to ${out}: ${reason}`);
    }
  }
  return out;
}

/** The script behind `--for mobi`, `fountain` and `fdx` is a file that is
 * there, or the export is 'unreadable'. One check, for all three. */
function checkScriptReadable(script: string): void {
  let isFile = false;
  try {
    isFile = statSync(script).isFile();
  } catch {
    isFile = false;
  }
  if (!isFile) {
    throw new CliError('unreadable', `cannot read the script to export: ${script}`);
  }
}

/** The FDX that sits beside a book when no --out names another place: same
 * folder, same stem, the way the Kindle files sit beside it. */
export function fdxSibling(epub: string): string {
  return `${epub.replace(/\.epub$/i, '')}.fdx`;
}

/** --for fdx: the editable screenplay, rebuilt from the .fountain on every
 * call (it takes milliseconds, so there is no ladder and no reuse), and
 * written atomically: the path only ever holds a whole file. */
function exportFdx(
  epub: string,
  fountainPath: string,
  out: string | undefined,
  check: boolean,
): Omit<ExportResult, 'available'> {
  const state = { calibreAvailable: false, kfxReady: false };
  const extension = fileExtension('fdx', state);
  if (out !== undefined) checkOutExtension(out, extension);
  const path = out ?? fdxSibling(epub);

  let fountainText: string;
  let fountainDate: number;
  try {
    fountainText = readFileSync(fountainPath, 'utf8');
    fountainDate = statSync(fountainPath).mtimeMs;
  } catch {
    throw new CliError('unreadable', `cannot read the script's .fountain: ${fountainPath}`);
  }
  const base = { path, format: 'fdx' as const, extension, label: formatLabel('fdx', state), stages: [] };

  if (check) {
    let fresh = false;
    try {
      fresh = statSync(path).mtimeMs >= fountainDate;
    } catch {
      fresh = false;
    }
    return { ...base, fresh };
  }

  try {
    mkdirSync(dirname(path), { recursive: true });
    writeFileAtomicSync(path, fountainToFdx(fountainText));
  } catch (err) {
    // The hidden temp file is not a name anyone chose: say the one they did.
    const reason = errorMessage(err).split(atomicTempPathFor(path)).join(path);
    throw new CliError('export-failed', `could not save the FDX to ${path}: ${reason}`);
  }
  return base;
}

export async function exportCommand(
  options: ExportOptions,
  deps: ExportDeps = {},
): Promise<ExportResult> {
  const wanted = options.for ?? 'epub';
  if (!(EXPORT_FORMATS as readonly string[]).includes(wanted)) {
    throw new CliError('usage', `--for takes epub, kindle, kfx, azw3, mobi, fountain or fdx, not "${wanted}"`);
  }
  const format = wanted as ExportFormat;

  // The file is checked FIRST, before any toolchain probe: a typo must not
  // pay Calibre's ~1s Python start to be reported.
  let isFile = false;
  try {
    isFile = statSync(options.epub).isFile();
  } catch {
    isFile = false;
  }
  if (!isFile) {
    throw new CliError('unreadable', `cannot read the book to export: ${options.epub}`);
  }

  // --out is validated for shape before ANYTHING else that costs real time:
  // Calibre's probe, Kindle Previewer's status check, and the ladder itself.
  // A save dialog always hands back an absolute path, so a relative one here
  // means a caller built the argv by hand and got it wrong; it must not pay
  // for a KFX build to be told so.
  // --check before the shape of --out: a check never uses the path, so what
  // is wrong with it is that it was given at all.
  if (options.check === true && options.out !== undefined) {
    throw new CliError('usage', '--check builds and writes nothing, so it takes no --out');
  }
  if (options.out !== undefined && !isAbsolute(options.out)) {
    throw new CliError('usage', 'the path given with --out must be absolute');
  }
  // A named file type knows its extension before any work, unlike the
  // ladder (`kindle`), which learns it only from the rung it reaches.
  if (options.out !== undefined && format !== 'epub' && format !== 'kindle') {
    checkOutExtension(options.out, format);
  }
  // The MOBI rebuild and the script itself both ARE the .fountain, so
  // without it there is nothing to make.
  if ((format === 'mobi' || format === 'fountain') && options.fountain === undefined) {
    throw new CliError('usage', `--for ${format} needs the script's .fountain: pass it with --fountain`);
  }
  // `--for fountain` hands the file over as the script, so it must be one.
  if (format === 'fountain' && extname(options.fountain as string).toLowerCase() !== '.fountain') {
    throw new CliError('usage', `--for fountain takes a .fountain file, not ${options.fountain}`);
  }

  // An FDX is made from the .fountain and nothing else, so without one there
  // is nothing to build it from: said before any settings are read.
  if (wanted === 'fdx' && options.fountain === undefined) {
    throw new CliError('usage', "--for fdx is built from the script's .fountain: pass it with --fountain");
  }
  // All three read the script, so a script that is not there is refused
  // here, before --check answers and before any probe or build.
  if (format === 'mobi' || format === 'fountain' || format === 'fdx') {
    checkScriptReadable(options.fountain as string);
  }

  // Parsed here, before the epub branch returns and before any toolchain
  // probe: --options-json is either well-formed argv or it is not, and that
  // cannot depend on what --for happens to say. The epub rung ignores the
  // VALUE (it converts nothing), but a window sending malformed JSON must
  // hear the same 'bad-options' either way, or the same argv is valid and
  // invalid at once.
  const formatBase = readFormatBase(options.fountain, deps.appSettingsPath);
  const formatOptions = readFormat(options.optionsJson, formatBase);

  const calibreAvailable = (deps.calibreAvailable ?? isCalibreAvailable)();
  const available = availableFormats(options.epub, calibreAvailable);

  if (format === 'fountain') {
    // The script as it is: no toolchain, no build, always current. Its
    // extension and readability were checked above.
    const script = options.fountain as string;
    const state = { calibreAvailable, kfxReady: false };
    const path = options.out !== undefined ? await resolveOut(script, options.out) : script;
    return {
      path,
      format,
      extension: fileExtension(format, state),
      label: formatLabel(format, state),
      available,
      stages: [],
      ...(options.check === true ? { fresh: true } : {}),
    };
  }

  if (format === 'kfx' || format === 'azw3' || format === 'mobi') {
    return exportRung(format, options, deps, { calibreAvailable, available, formatOptions });
  }

  if (format === 'fdx') {
    return {
      ...exportFdx(options.epub, options.fountain!, options.out, options.check === true),
      available,
    };
  }

  if (format === 'epub') {
    const state = { calibreAvailable, kfxReady: false };
    const extension = fileExtension(format, state);
    // The epub rung does no ladder work at all, so "before any work" means
    // right here: checked before resolveOut so much as looks at the disk.
    if (options.out !== undefined) checkOutExtension(options.out, extension);
    const path = options.out !== undefined
      ? await resolveOut(options.epub, options.out)
      : options.epub;
    return {
      path,
      format,
      extension,
      label: formatLabel(format, state),
      available,
      stages: [],
      ...(options.check === true ? { fresh: true } : {}),
    };
  }

  const kfx = await (deps.kfxStatus ?? realKfxStatus)();
  const state = { calibreAvailable, kfxReady: kfx.ready };

  if (options.check === true) {
    // The ladder's own plan, asked without walking it.
    const plan = kindleArtifactPlan(options.epub, state);
    return {
      path: plan.path,
      format,
      extension: fileExtension(format, state),
      label: formatLabel(format, state),
      available,
      stages: [],
      fresh: plan.fresh,
      builtBy: plan.builtBy,
      bookDate: plan.bookDate,
    };
  }
  const stages: string[] = [];
  let path: string;
  try {
    const ladder: KindleLadder = deps.freshKindleArtifact
      ?? ((opts) => realFreshKindleArtifact(opts, deps.rungBuilders));
    path = await ladder({
      epub: options.epub,
      fountainPath: options.fountain ?? null,
      format: formatOptions,
      calibreAvailable,
      kfxReady: kfx.ready,
      onStage: (stage) => stages.push(stage),
    });
  } catch (err) {
    // Only the LADDER's own errors belong here (CannotRegenerateError,
    // RegenerationFailedError, CalibreMissingError, ...) — none of them are
    // CliError, so this rewrap can't accidentally swallow a typed code. Any
    // CliError this command itself needs to throw (bad-options above,
    // unreadable earlier, usage at the top) is raised OUTSIDE this try, on
    // purpose, so its code survives verbatim.
    throw new CliError('export-failed', errorMessage(err));
  }

  const extension = fileExtension(format, state);
  // Only now does the wrapper know which rung was actually taken. KFX,
  // AZW3 and MOBI are three different right answers to the same --out, and
  // asking any earlier would mean guessing at a toolchain probe the ladder
  // itself already ran a moment ago.
  if (options.out !== undefined) {
    checkOutExtension(options.out, extension);
    path = await resolveOut(path, options.out);
  }

  return {
    path,
    format,
    extension,
    label: formatLabel(format, state),
    available,
    stages,
  };
}

/** `--for kfx`, `azw3` or `mobi`: one rung by name, never the ladder, so a
 * missing toolchain is a refusal and never a quieter rung. `--out`'s
 * extension and `--fountain`'s presence were already checked by the
 * caller, before any probe. The refusals come before --check's answer as
 * well as before a build: a rung this computer cannot make has no plan to
 * report. */
async function exportRung(
  rung: KindleRung,
  options: ExportOptions,
  deps: ExportDeps,
  known: { calibreAvailable: boolean; available: ExportFormat[]; formatOptions: FormatOptions },
): Promise<ExportResult> {
  const { calibreAvailable, available, formatOptions } = known;
  // Only KFX needs Kindle Previewer's status (the ~1s probe); AZW3 needs
  // Calibre alone and MOBI is the engine's own work.
  let kfxReady = false;
  if (rung === 'kfx') {
    const kfx = await (deps.kfxStatus ?? realKfxStatus)();
    if (!kfx.ready) {
      const missing = kfxMissing(kfx);
      const why = missing.length > 0 ? `it needs ${missing.join(' and ')}` : 'its tools are not ready';
      throw new CliError('export-failed', `this computer cannot make a KFX file yet: ${why}`);
    }
    kfxReady = true;
  }
  if (rung === 'azw3' && !calibreAvailable) {
    throw new CliError('export-failed', 'this computer cannot make an AZW3 file without Calibre');
  }
  const state = { calibreAvailable, kfxReady };
  const extension = fileExtension(rung, state);
  const label = formatLabel(rung, state);

  if (options.check === true) {
    const plan = rungPlan(rung, options.epub);
    return {
      path: plan.path,
      format: rung,
      extension,
      label,
      available,
      stages: [],
      fresh: plan.fresh,
      builtBy: plan.builtBy,
      bookDate: plan.bookDate,
    };
  }

  const stages: string[] = [];
  let path: string;
  try {
    path = await freshArtifactOf(
      rung,
      {
        epub: options.epub,
        fountainPath: options.fountain ?? null,
        format: formatOptions,
        calibreAvailable,
        kfxReady,
        onStage: (stage) => stages.push(stage),
      },
      deps.rungBuilders,
    );
  } catch (err) {
    // The rung's own errors only, as in the ladder's branch above: every
    // CliError this verb raises is thrown outside this try.
    throw new CliError('export-failed', errorMessage(err));
  }
  if (options.out !== undefined) path = await resolveOut(path, options.out);

  return { path, format: rung, extension, label, available, stages };
}

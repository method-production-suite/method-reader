import { existsSync } from 'node:fs';
import { writeFileAtomicSync } from '../atomic-write';
import { convertFountain } from '../convert';
import type { FormatOptions } from '../options';
import { azw3Sibling, toAzw3 } from './calibre';
import { modifiedDate, needsRegeneration } from './freshness';
import type { ExportFormat, ToolchainState } from './formats';
import { kfxSibling, toKfx } from './kfx';

export class RegenerationFailedError extends Error {
  constructor(why: string) {
    super(`Couldn't rebuild the Kindle file: ${why}`);
    this.name = 'RegenerationFailedError';
  }
}

export class CannotRegenerateError extends Error {
  constructor() {
    super("Can't rebuild the Kindle file: the script's .fountain is missing.");
    this.name = 'CannotRegenerateError';
  }
}

/** The sibling `.mobi` for a given EPUB — same directory, same stem. Shared
 * by availableFormats and freshKindleArtifact so the derivation lives in
 * exactly one place; call sites go through those two. */
export function mobiSibling(epub: string): string {
  return `${epub.replace(/\.epub$/i, '')}.mobi`;
}

/** EPUB is always available (it is the conversion's primary output). The
 * Kindle format needs either Calibre (converts from the current EPUB) or an
 * already-built .mobi to refresh. */
export function availableFormats(epub: string, calibreAvailable: boolean): ExportFormat[] {
  const formats: ExportFormat[] = ['epub'];
  if (calibreAvailable || existsSync(mobiSibling(epub))) formats.push('kindle');
  return formats;
}

/** The Kindle file this machine's ladder hands over for `epub`, without
 * building anything: where it lives beside the EPUB, whether the one there
 * now is current, and what builds it when it is not. */
export interface KindleArtifactPlan {
  path: string;
  /** The staleness rule's answer (freshness.ts): present and newer than the
   * EPUB, so the ladder would hand it back as it is. */
  fresh: boolean;
  /** 'calibre' for KFX and AZW3 (ebook-convert, with Kindle Previewer behind
   * it for KFX), 'screepub' for the engine's own MOBI. */
  builtBy: 'calibre' | 'screepub';
  /** When the EPUB was last written (ms), or null when it cannot be read:
   * which version of the book this plan is about. A Settings save moves it. */
  bookDate: number | null;
}

/** The rung freshKindleArtifact takes, and whether it would reuse or build.
 * The ladder asks this same question before it builds, so `export --check`
 * (which answers from here) and the build itself cannot disagree. */
export function kindleArtifactPlan(epub: string, state: ToolchainState): KindleArtifactPlan {
  const path = state.kfxReady ? kfxSibling(epub) : state.calibreAvailable ? azw3Sibling(epub) : mobiSibling(epub);
  const builtBy = state.kfxReady || state.calibreAvailable ? 'calibre' : 'screepub';
  return { path, fresh: !needsRegeneration(path, epub), builtBy, bookDate: modifiedDate(epub) };
}

export interface FreshKindleArtifactOptions {
  epub: string;
  fountainPath: string | null;
  /** Must be the script's real settings, not the defaults, or the export
   * silently loses the user's tuned formatting. */
  format: FormatOptions;
  calibreAvailable: boolean;
  kfxReady: boolean;
  onStage?: (stage: string) => void;
}

/** A Kindle-format file guaranteed current with `epub`. Every rung reuses
 * the file already beside the EPUB when the staleness rule (freshness.ts)
 * says it is current, and builds it again only when it is not: KFX and AZW3
 * convert straight from the present EPUB, and the MOBI branch re-runs the
 * engine.
 *
 * NOTE: the MOBI branch REWRITES `epub` in place before it writes the .mobi
 * beside it — this function can mutate the EPUB it was handed, not just
 * produce the Kindle file. */
export async function freshKindleArtifact(opts: FreshKindleArtifactOptions): Promise<string> {
  const { epub, fountainPath, format, calibreAvailable, kfxReady, onStage } = opts;

  // One staleness rule for every rung (kindleArtifactPlan): the EPUB is the
  // sole input and the flags are constant, so a sibling no older than its
  // EPUB is the previous run's answer. For KFX that turns a ~20s Kindle
  // Previewer cold start into a file stat; for AZW3 it is why "Save a Kindle
  // file" does not convert twice on a Calibre-only machine (the window's
  // `export --for kindle` to learn the extension, then `route save-kindle`).
  // toKfx and toAzw3 write scratch-then-rename, so a partial file never
  // appears at the path to be trusted.
  const plan = kindleArtifactPlan(epub, { calibreAvailable, kfxReady });
  if (plan.fresh) return plan.path;

  if (kfxReady) return toKfx(epub, onStage);

  if (calibreAvailable) {
    onStage?.('converting to AZW3 for Kindle…');
    return toAzw3(epub);
  }

  const mobi = plan.path;
  if (!fountainPath) throw new CannotRegenerateError();

  onStage?.('rebuilding the Kindle file…');
  const fountainText = await Bun.file(fountainPath).text();
  const result = await convertFountain(fountainText, { format, mobi: true });
  if (!result.mobi) {
    throw new RegenerationFailedError('the engine reported success but produced no .mobi');
  }
  // Swift's branch shelled out to cli.ts, and its export code leaned on what that
  // does: each output goes to a temp file that is renamed into place, so a
  // partial `.mobi` never appears at the final path in the FIRST PLACE. That
  // is load-bearing for the rung right above — a kill or disk-full mid-write
  // would otherwise leave a truncated `.mobi` whose mtime is NEWER than the
  // EPUB's, which needsRegeneration would then trust as fresh forever. Same
  // discipline as cli.ts's writeFileAtomic, settings/sidecar.ts and
  // export/kfx.ts.
  writeFileAtomicSync(epub, result.epub);
  writeFileAtomicSync(mobi, result.mobi);
  return mobi;
}


import { existsSync } from 'node:fs';
import { writeFileAtomicSync } from '../atomic-write';
import { convertFountain } from '../convert';
import { imageNoteFiles } from '../fountain/notes';
import { readImages } from '../library-images';
import type { FormatOptions } from '../options';
import { azw3Sibling, toAzw3 } from './calibre';
import { modifiedDate, needsRegeneration } from './freshness';
import { ladderRung, type ExportFormat, type KindleRung, type ToolchainState } from './formats';
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

/** One rung's file beside `epub`, without building anything: where it
 * lives, whether the one there now is current (a sibling no older than its
 * EPUB is reused), and what builds it when it is not. Every build asks this
 * same question first, so `export --check` (which answers from here) and
 * the build itself cannot disagree. */
export function rungPlan(rung: KindleRung, epub: string): KindleArtifactPlan {
  const path = rung === 'kfx' ? kfxSibling(epub) : rung === 'azw3' ? azw3Sibling(epub) : mobiSibling(epub);
  const builtBy = rung === 'mobi' ? 'screepub' : 'calibre';
  return { path, fresh: !needsRegeneration(path, epub), builtBy, bookDate: modifiedDate(epub) };
}

/** The rung freshKindleArtifact takes on this machine, and whether it would
 * reuse or build. */
export function kindleArtifactPlan(epub: string, state: ToolchainState): KindleArtifactPlan {
  return rungPlan(ladderRung(state), epub);
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

/** What builds the two Calibre rungs: the real ones by default. A test
 * passes its own, so a wrong branch can never reach a real Calibre. */
export interface RungBuilders {
  kfx: (epub: string, onStage?: (stage: string) => void) => Promise<string>;
  azw3: (epub: string) => Promise<string>;
}

const REAL_BUILDERS: RungBuilders = {
  kfx: (epub, onStage) => toKfx(epub, onStage),
  azw3: (epub) => toAzw3(epub),
};

/** A Kindle-format file guaranteed current with `epub`: the ladder's rung
 * for this machine (formats.ts's ladderRung), made by freshArtifactOf.
 *
 * NOTE: the MOBI rung REWRITES `epub` in place before it writes the .mobi
 * beside it, so this function can change the EPUB it was handed, not just
 * produce the Kindle file. */
export async function freshKindleArtifact(
  opts: FreshKindleArtifactOptions,
  builders: RungBuilders = REAL_BUILDERS,
): Promise<string> {
  const rung = ladderRung({ calibreAvailable: opts.calibreAvailable, kfxReady: opts.kfxReady });
  return buildRung(rung, opts, builders, true);
}

/** One named rung, never the ladder: the caller asked for exactly this
 * file. Every rung reuses the file already beside the EPUB when the
 * staleness rule (freshness.ts, through rungPlan) says it is current, and
 * builds it again only when it is not: KFX and AZW3 convert straight from
 * the present EPUB, and the MOBI rung re-runs the engine. The ladder is
 * this work with the rung chosen for it, so each rung's work has one
 * copy. `opts.calibreAvailable` and `opts.kfxReady` are not consulted:
 * toKfx and toAzw3 each refuse with their own error when their tools are
 * missing.
 *
 * Unlike the ladder's MOBI rung, the named one writes ONLY the .mobi: the
 * EPUB is left exactly as it was, so a .kfx or .azw3 already built from it
 * stays current. */
export async function freshArtifactOf(
  rung: KindleRung,
  opts: FreshKindleArtifactOptions,
  builders: RungBuilders = REAL_BUILDERS,
): Promise<string> {
  return buildRung(rung, opts, builders, false);
}

/** The one copy of each rung's work. `fromLadder` matters to the MOBI rung
 * alone: the ladder lands there only when there is no Calibre, and then the
 * EPUB is regenerated alongside the .mobi; a rung asked for by name leaves
 * the book alone. */
async function buildRung(
  rung: KindleRung,
  opts: FreshKindleArtifactOptions,
  builders: RungBuilders,
  fromLadder: boolean,
): Promise<string> {
  const { epub, fountainPath, format, onStage } = opts;

  // One staleness rule for every rung: the EPUB is the sole input and the
  // flags are constant, so a sibling no older than its EPUB is the previous
  // run's answer. For KFX that turns a ~20s Kindle Previewer cold start into
  // a file stat; for AZW3 it is why "Save a Kindle file" does not convert
  // twice on a Calibre-only machine (the window's `export --for kindle` to
  // learn the extension, then `route save-kindle`). toKfx and toAzw3 write
  // scratch-then-rename, so a partial file never appears at the path to be
  // trusted.
  const plan = rungPlan(rung, epub);
  if (plan.fresh) return plan.path;

  if (rung === 'kfx') return builders.kfx(epub, onStage);

  if (rung === 'azw3') {
    onStage?.('converting to AZW3 for Kindle…');
    return builders.azw3(epub);
  }

  const mobi = plan.path;
  if (!fountainPath) throw new CannotRegenerateError();

  onStage?.('rebuilding the Kindle file…');
  const fountainText = await Bun.file(fountainPath).text();
  // This rung renders the EPUB and the MOBI together from the .fountain, and
  // the EPUB it renders replaces the library book. So both are built with
  // the script's pictures, read from the folder beside the .fountain under
  // the same rules as every other render from the text, and the MOBI shows
  // the same ones the EPUB does (convert.ts hands both the one read). With
  // the option off the folder is not read and both are rebuilt without
  // pictures, which is what the setting asks for.
  //
  // With the option on, the library book is replaced only when the rebuild
  // carries every picture the script's notes ask for. A folder damaged since
  // the book was built (an iCloud placeholder, a deleted file, the folder
  // moved or refused, the cap reached) would otherwise turn the book into a
  // weaker copy of itself, silently, and every route that sends the EPUB
  // (Kobo, Apple Books, Save EPUB) would send the weaker one. One rule covers
  // all of those, because each of them leaves a name the notes ask for out of
  // the folder's read. Kept, the book stays the one its last conversion
  // built.
  //
  // The .mobi is written either way, and it is the file being handed over,
  // so it cannot wait for a picture the folder will not give: it carries the
  // pictures that were read, and a note whose picture was not read renders
  // nothing, as it would with no folder at all. (While the safety gate is
  // shut, MOBI_CARRIES_PICTURES in src/mobi/html.ts, it carries none at all:
  // the folder is still read, for the EPUB this rung may rewrite.) Written after the EPUB, it is
  // newer than the book, so the staleness rule above reads it as current and
  // the next export reuses it rather than rebuild against the same loss. The
  // flip side is that mending the folder does not refresh it: the .mobi stays
  // without that picture until the book itself is rebuilt.
  //
  // Nothing says a picture was left out, or passes on the folder's own
  // warnings (imageFolderWarnings). This function answers with a path, and
  // the export answer it feeds has stages but no warnings, so there is
  // nowhere yet to say them. The loss is in the file being sent, so that
  // channel is owed: it is an open item, not a choice.
  let images: ReadonlyMap<string, Uint8Array> | undefined;
  let keepLibraryBook = false;
  if (format.includeImages) {
    const read = (await readImages(fountainPath)).images;
    images = read;
    keepLibraryBook = [...imageNoteFiles(fountainText)].some((name) => !read.has(name));
  }
  const result = await convertFountain(fountainText, { format, mobi: true, images });
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
  // A rung asked for by name leaves the book alone (freshArtifactOf), and
  // the ladder's leaves it too when the rebuild would drop a picture.
  if (fromLadder && !keepLibraryBook) writeFileAtomicSync(epub, result.epub);
  writeFileAtomicSync(mobi, result.mobi);
  return mobi;
}


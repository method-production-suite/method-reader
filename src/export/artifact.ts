import { readFileSync } from 'node:fs';
import { writeFileAtomicSync } from '../atomic-write';
import { AZW3_WRITER_REVISION, azw3RevisionOfFile } from '../azw3/revision';
import { buildAzw3 } from '../azw3/writer';
import { convertFountain } from '../convert';
import { readImages } from '../library-images';
import type { FormatOptions } from '../options';
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

/** The sibling `.mobi` for a given EPUB — same directory, same stem. One
 * derivation, which rungPlan and the MOBI rung both go through. */
export function mobiSibling(epub: string): string {
  return `${epub.replace(/\.epub$/i, '')}.mobi`;
}

/** The sibling `.azw3` for a given EPUB: same directory, same stem, whatever
 * the case of `.epub`. */
export function azw3Sibling(epub: string): string {
  return `${epub.replace(/\.epub$/i, '')}.azw3`;
}

/** EPUB is always available (it is the conversion's primary output), and so
 * is the Kindle file: with no KFX toolchain the ladder writes Screepub's own
 * AZW3, which needs nothing installed. */
export function availableFormats(): ExportFormat[] {
  return ['epub', 'kindle'];
}

/** The Kindle file this machine's ladder hands over for `epub`, without
 * building anything: where it lives beside the EPUB, whether the one there
 * now is current, and what builds it when it is not. */
export interface KindleArtifactPlan {
  path: string;
  /** The staleness rule's answer (freshness.ts): present and newer than the
   * EPUB, so the ladder would hand it back as it is. */
  fresh: boolean;
  /** 'calibre' for KFX (ebook-convert, with Kindle Previewer behind it),
   * 'screepub' for the AZW3 and the MOBI, which the engine writes itself. */
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
  const builtBy = rung === 'kfx' ? 'calibre' : 'screepub';
  // The AZW3 must also come from this writer revision or a later one
  // (azw3/revision.ts): an .azw3 with none (Calibre's, or ours from before
  // the stamp) or an older one is rebuilt, so a fixed writer reaches it.
  const current = !needsRegeneration(path, epub)
    && (rung !== 'azw3' || (azw3RevisionOfFile(path) ?? 0) >= AZW3_WRITER_REVISION);
  return { path, fresh: current, builtBy, bookDate: modifiedDate(epub) };
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

/** What builds the KFX and AZW3 rungs from the EPUB: the real ones by
 * default. A test passes its own, so a wrong branch can never reach a real
 * Calibre. */
export interface RungBuilders {
  kfx: (epub: string, onStage?: (stage: string) => void) => Promise<string>;
  azw3: (epub: string) => Promise<string>;
}

const REAL_BUILDERS: RungBuilders = {
  kfx: (epub, onStage) => toKfx(epub, onStage),
  azw3: (epub) => writeAzw3(epub),
};

/** Screepub's own AZW3 (src/azw3/, beta) from the EPUB as it stands, written
 * beside it through a temp file and a rename, so a partial file never sits
 * at the path the staleness rule would trust. No Calibre: it takes
 * milliseconds. */
export async function writeAzw3(epub: string): Promise<string> {
  let bytes: Uint8Array;
  try {
    bytes = await buildAzw3(new Uint8Array(readFileSync(epub)));
  } catch (err) {
    throw new RegenerationFailedError(`the AZW3 could not be made from the book (${err instanceof Error ? err.message : String(err)})`);
  }
  const out = azw3Sibling(epub);
  writeFileAtomicSync(out, bytes);
  return out;
}

/** A Kindle-format file guaranteed current with `epub`: the ladder's rung
 * for this machine (formats.ts's ladderRung: KFX, else our AZW3), made by
 * the same work as freshArtifactOf. Neither rung changes the EPUB. */
export async function freshKindleArtifact(
  opts: FreshKindleArtifactOptions,
  builders: RungBuilders = REAL_BUILDERS,
): Promise<string> {
  const rung = ladderRung({ calibreAvailable: opts.calibreAvailable, kfxReady: opts.kfxReady });
  return buildRung(rung, opts, builders);
}

/** One named rung, never the ladder: the caller asked for exactly this
 * file. Every rung reuses the file already beside the EPUB when the
 * staleness rule (freshness.ts, through rungPlan) says it is current, and
 * builds it again only when it is not: KFX and AZW3 from the present EPUB,
 * the MOBI by re-running the engine on the .fountain. The ladder is this
 * work with the rung chosen for it, so each rung's work has one copy.
 * `opts.calibreAvailable` and `opts.kfxReady` are not consulted: toKfx
 * refuses with its own error when its tools are missing, and the AZW3 and
 * MOBI need none.
 *
 * Every rung writes ONLY its own file: the EPUB is left exactly as it was,
 * so a .kfx or .azw3 already built from it stays current. */
export async function freshArtifactOf(
  rung: KindleRung,
  opts: FreshKindleArtifactOptions,
  builders: RungBuilders = REAL_BUILDERS,
): Promise<string> {
  return buildRung(rung, opts, builders);
}

/** The one copy of each rung's work. */
async function buildRung(
  rung: KindleRung,
  opts: FreshKindleArtifactOptions,
  builders: RungBuilders,
): Promise<string> {
  const { epub, fountainPath, format, onStage } = opts;

  // One staleness rule for every rung: the EPUB is the sole input and the
  // build is constant, so a sibling no older than its EPUB is the previous
  // run's answer. For KFX that turns a ~20s Kindle Previewer cold start into
  // a file stat; for every rung it is why "Save a Kindle file" does not
  // build twice (the window's `export --for kindle` to learn the extension,
  // then `route save-kindle`). Every rung writes temp-then-rename, so a
  // partial file never appears at the path to be trusted.
  const plan = rungPlan(rung, epub);
  if (plan.fresh) return plan.path;

  if (rung === 'kfx') return builders.kfx(epub, onStage);

  if (rung === 'azw3') {
    onStage?.('building the AZW3 for Kindle…');
    return builders.azw3(epub);
  }

  const mobi = plan.path;
  if (!fountainPath) throw new CannotRegenerateError();

  onStage?.('rebuilding the Kindle file…');
  const fountainText = await Bun.file(fountainPath).text();
  // The MOBI is rendered from the .fountain, with the script's pictures read
  // from the folder beside it under the same rules as every other render
  // from the text (src/library-images.ts); with includeImages off the folder
  // is not read. A note whose picture was not read renders nothing, as it
  // would with no folder at all. While the safety gate is shut
  // (MOBI_CARRIES_PICTURES in src/mobi/html.ts) the .mobi carries none of
  // them anyway; the read is what lets it carry them once the gate opens.
  //
  // Nothing says a picture was left out, or passes on the folder's own
  // warnings (imageFolderWarnings). This function answers with a path, and
  // the export answer it feeds has stages but no warnings, so there is
  // nowhere yet to say them. That channel is owed: an open item, not a
  // choice.
  //
  // Only the .mobi is written. The EPUB this render also produces is
  // dropped: the library book stays the one its last conversion built, so a
  // .kfx or .azw3 already made from it stays current. (Before Screepub wrote
  // its own AZW3 the ladder landed here on a machine without Calibre and
  // rewrote the book too; it no longer lands here at all.)
  const images = format.includeImages ? (await readImages(fountainPath)).images : undefined;
  const result = await convertFountain(fountainText, { format, mobi: true, images });
  if (!result.mobi) {
    throw new RegenerationFailedError('the engine reported success but produced no .mobi');
  }
  // Temp file then rename (writeFileAtomicSync), so a kill or a full disk
  // mid-write cannot leave a truncated .mobi whose mtime is NEWER than the
  // EPUB's, which needsRegeneration would then trust as fresh forever. Same
  // discipline as cli.ts's writeFileAtomic, settings/sidecar.ts and
  // export/kfx.ts.
  writeFileAtomicSync(mobi, result.mobi);
  return mobi;
}

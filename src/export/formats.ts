// What a converted script can be saved as, labeled by PURPOSE: the file you
// email is not the file you sideload. Amazon stopped accepting MOBI for Send
// to Kindle in 2022, while Kindles never index a sideloaded EPUB. And the
// file you keep writing in is neither: 'fdx' is the script itself, for Final
// Draft and the screenwriting apps that open its files.
//
// `kindle` is the ladder (the best rung this computer reaches). `kfx`,
// `azw3` and `mobi` are its rungs asked for by name, each built only as
// itself, `fountain` is the script as plain text (spec decision 11), and
// `fdx` is the same script as a Final Draft document.
export type ExportFormat = 'epub' | 'kindle' | 'kfx' | 'azw3' | 'mobi' | 'fountain' | 'fdx';

/** Every value `export --for` takes, in the order its help lists them. */
export const EXPORT_FORMATS: readonly ExportFormat[] = ['epub', 'kindle', 'kfx', 'azw3', 'mobi', 'fountain', 'fdx'];

/** One rung of the Kindle ladder, named. */
export type KindleRung = 'kfx' | 'azw3' | 'mobi';

export interface ToolchainState {
  calibreAvailable: boolean;
  kfxReady: boolean;
}

/** The rung the ladder takes on this machine: KFX (Enhanced Typesetting:
 * keeps hold, device-verified 2026-07-29) when the full toolchain is
 * present, and otherwise the AZW3 Screepub writes itself (src/azw3/, beta),
 * on every machine, Calibre or not. Registry §8b has the verdict behind the
 * order. The engine's MOBI is never the ladder's answer: it is the file for
 * the oldest Kindles, which a person asks for by name. `calibreAvailable`
 * is no longer consulted: Calibre matters only to KFX now. */
export function ladderRung(state: ToolchainState): KindleRung {
  return state.kfxReady ? 'kfx' : 'azw3';
}

export function fileExtension(format: ExportFormat, state: ToolchainState): string {
  if (format === 'kindle') return ladderRung(state);
  return format;
}

export function formatLabel(format: ExportFormat, state: ToolchainState): string {
  if (format === 'epub') return 'EPUB: for emailing to Kindle, and most e-readers';
  if (format === 'fountain') return 'Fountain: the script as plain text, for screenwriting apps';
  if (format === 'kfx') return 'KFX: for modern Kindles over USB, the best layout';
  if (format === 'azw3') return 'AZW3 (beta): for Kindles over USB, made by Screepub with nothing else installed';
  if (format === 'mobi') return 'MOBI: for the oldest Kindles';
  if (format === 'fdx') return 'FDX: the script to keep editing, in Final Draft and most screenwriting apps';
  const ext = fileExtension(format, state).toUpperCase();
  const hint = state.kfxReady ? ' (best quality)' : '';
  const beta = ext === 'AZW3' ? ' (beta)' : '';
  return `${ext}${beta}: for USB sideload to Kindle${hint}`;
}

/** Whether a file Screepub makes is still beta: it works in our tests and
 * in Calibre's reader, but has not been tried widely on real devices or
 * apps. The AZW3 Screepub writes itself, and the FDX. The window marks a
 * file the export says this of with its Beta badge. */
export function isBetaFile(extension: string): boolean {
  return extension === 'azw3' || extension === 'fdx';
}

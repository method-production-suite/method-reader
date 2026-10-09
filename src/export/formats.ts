// What a converted script can be saved as, labeled by PURPOSE: the file you
// email is not the file you sideload. Amazon stopped accepting MOBI for Send
// to Kindle in 2022, while Kindles never index a sideloaded EPUB.
//
// `kindle` is the ladder (the best rung this computer reaches). `kfx`,
// `azw3` and `mobi` are its rungs asked for by name, each built only as
// itself, and `fountain` is the script as plain text (spec decision 11).
export type ExportFormat = 'epub' | 'kindle' | 'kfx' | 'azw3' | 'mobi' | 'fountain';

/** Every value `export --for` takes, in the order its help lists them. */
export const EXPORT_FORMATS: readonly ExportFormat[] = ['epub', 'kindle', 'kfx', 'azw3', 'mobi', 'fountain'];

/** One rung of the Kindle ladder, named. */
export type KindleRung = 'kfx' | 'azw3' | 'mobi';

export interface ToolchainState {
  calibreAvailable: boolean;
  kfxReady: boolean;
}

/** The rung the ladder takes on this machine, best first: KFX (Enhanced
 * Typesetting: keeps hold, device-verified 2026-07-29) when the full
 * toolchain is present; AZW3 with Calibre alone; the engine's own MOBI with
 * nothing. Registry §8b has the verdict behind the order. */
export function ladderRung(state: ToolchainState): KindleRung {
  if (state.kfxReady) return 'kfx';
  return state.calibreAvailable ? 'azw3' : 'mobi';
}

export function fileExtension(format: ExportFormat, state: ToolchainState): string {
  if (format === 'kindle') return ladderRung(state);
  return format;
}

export function formatLabel(format: ExportFormat, state: ToolchainState): string {
  if (format === 'epub') return 'EPUB: for emailing to Kindle, and most e-readers';
  if (format === 'fountain') return 'Fountain: the script as plain text, for screenwriting apps';
  if (format === 'kfx') return 'KFX: for modern Kindles over USB, the best layout';
  if (format === 'azw3') return 'AZW3: for older Kindles over USB';
  if (format === 'mobi') return 'MOBI: for the oldest Kindles';
  const ext = fileExtension(format, state).toUpperCase();
  const hint = state.kfxReady ? ' (best quality)' : '';
  return `${ext}: for USB sideload to Kindle${hint}`;
}

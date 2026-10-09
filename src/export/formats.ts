// What a converted script can be saved as, labeled by PURPOSE: the file you
// email is not the file you sideload. Amazon stopped accepting MOBI for Send
// to Kindle in 2022, while Kindles never index a sideloaded EPUB. And the
// file you keep writing in is neither: 'fdx' is the script itself, for Final
// Draft and the screenwriting apps that open its files.
export type ExportFormat = 'epub' | 'kindle' | 'fdx';

export interface ToolchainState {
  calibreAvailable: boolean;
  kfxReady: boolean;
}

/** The Kindle sideload ladder, best rung first: KFX (Enhanced Typesetting —
 * keeps hold, device-verified 2026-07-29) when the full toolchain is present;
 * AZW3 with Calibre alone; the engine's own MOBI with nothing. Registry §8b
 * has the verdict behind the order. */
export function fileExtension(format: ExportFormat, state: ToolchainState): string {
  if (format === 'epub') return 'epub';
  if (format === 'fdx') return 'fdx';
  if (state.kfxReady) return 'kfx';
  return state.calibreAvailable ? 'azw3' : 'mobi';
}

export function formatLabel(format: ExportFormat, state: ToolchainState): string {
  if (format === 'epub') return 'EPUB: for emailing to Kindle, and most e-readers';
  if (format === 'fdx') return 'FDX: the script to keep editing, in Final Draft and most screenwriting apps';
  const ext = fileExtension(format, state).toUpperCase();
  const hint = state.kfxReady ? ' (best quality)' : '';
  return `${ext}: for USB sideload to Kindle${hint}`;
}

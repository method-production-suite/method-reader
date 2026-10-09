// Write-then-rename, synchronously: the one copy of the rule for the files
// the engine keeps beside a book (settings, sidecars, a library folder's
// source.json, a rebuilt Kindle file, the pictures in a script's picture
// folder, via library-images.ts). cli.ts's writeFileAtomic is
// the async twin for conversion outputs, and replace-file.ts the variant for
// a reader's USB copy, which also flushes to the device.
import { copyFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';

/** The hidden temp sibling a write to `path` goes through: same folder, so
 * it shares a volume and the promote is a rename, not a copy; a leading dot
 * so nothing lists an unfinished file; the pid so two engines writing at
 * once do not share one. */
export function tempPathFor(path: string): string {
  return join(dirname(path), `.${basename(path)}.${process.pid}.tmp`);
}

/** Put `fill(tmp)`'s result at `path` in one step, so `path` only ever holds
 * a complete file: a write that dies part way (disk full, the process
 * killed) leaves whatever was at `path` before, never half of the new one.
 * A failure takes the temp file with it, and the original error is what the
 * caller sees. */
function promoteAtomic(path: string, fill: (tmp: string) => void): void {
  const tmp = tempPathFor(path);
  try {
    fill(tmp);
    renameSync(tmp, path);
  } catch (error) {
    try {
      rmSync(tmp, { force: true });
    } catch {
      // Nothing more to do: the write's own error is reported below.
    }
    throw error;
  }
}

export function writeFileAtomicSync(path: string, data: string | Uint8Array): void {
  promoteAtomic(path, (tmp) => writeFileSync(tmp, data));
}

export function copyFileAtomicSync(source: string, destination: string): void {
  promoteAtomic(destination, (tmp) => copyFileSync(source, tmp));
}

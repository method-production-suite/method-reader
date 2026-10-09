// The ONLY part of the device layer that cannot be unit-tested against
// reality: what is actually mounted depends on what is physically plugged in.
// Kept deliberately dumb, with injectable roots, so that all the real device
// knowledge lives in classify.ts where it can be tested everywhere.
import { readdirSync } from 'node:fs';
import { userInfo } from 'node:os';
import { join } from 'node:path';
import { platform } from 'node:process';
import { classify, volumeName } from './classify';
import { isDirectory, isWindowsDriveRoot } from './paths';
import type { ConnectedDevice } from './types';

/** Where this OS mounts removable media. On Windows these are drive roots
 * rather than a parent directory, so enumerateVolumes treats a root that is
 * itself a volume correctly by listing its PARENT's children — see below. */
export function volumeRoots(): string[] {
  if (platform === 'darwin') return ['/Volumes'];
  if (platform === 'win32') {
    // Drive letters are themselves volumes, so each is returned as a root
    // whose single child is itself. C: is skipped: it is the system disk.
    return 'DEFGHIJKLMNOPQRSTUVWXYZ'.split('').map((letter) => `${letter}:\\`);
  }
  const user = userInfo().username;
  return [`/run/media/${user}`, `/media/${user}`, '/media'];
}

/** True when `root` is itself a mountable volume rather than a directory
 * that CONTAINS volumes — the Windows drive-letter case. Pure and
 * OS-injectable (default: the real host) so the branch it drives in
 * enumerateVolumes is directly testable on every platform, not just
 * exercised as dead code gated on `platform === 'win32'`. */
export function rootIsItselfAVolume(root: string, os: NodeJS.Platform = platform): boolean {
  return os === 'win32' && isWindowsDriveRoot(root);
}

/** Every mounted volume path under the given roots (default: this platform's).
 * On Windows a root IS a volume; elsewhere a root CONTAINS volumes.
 *
 * DIVERGENCE from the Swift app's device scan, deliberately: it asked
 * Foundation for mounted volumes with `.skipHiddenVolumes` and there is no
 * equivalent here. A plain
 * directory listing of a mount parent has no notion of a "hidden volume" to
 * skip, and the classifier is signature-based — a hidden volume that has
 * neither `documents/` nor `.kobo` is dropped by classify() anyway. */
export function enumerateVolumes(roots: string[] = volumeRoots()): string[] {
  const found: string[] = [];
  for (const root of roots) {
    if (!isDirectory(root)) continue;
    if (rootIsItselfAVolume(root)) {
      found.push(root);
      continue;
    }
    let entries: string[];
    try {
      entries = readdirSync(root);
    } catch {
      continue;
    }
    for (const entry of entries) {
      const path = join(root, entry);
      if (isDirectory(path)) found.push(path);
    }
  }
  return found;
}

/** All recognized devices among currently mounted volumes. */
export function mountedDevices(roots?: string[]): ConnectedDevice[] {
  const devices: ConnectedDevice[] = [];
  for (const volume of enumerateVolumes(roots)) {
    const kind = classify(volume);
    if (!kind) continue;
    devices.push({ kind, name: volumeName(volume), volume });
  }
  return devices;
}

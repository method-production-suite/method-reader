import { afterAll, test, expect } from 'bun:test';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { isKindleVolume, copyToKindleVolume } from '../src/device/kindle';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-device-kindle-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

/** Mirrors the Swift app's tempDir() check helper: a uniquely-rooted
 * directory whose own name is the "volume name" under test. */
function volume(name: string, subdirs: string[] = []): string {
  const dir = join(mkdtempSync(join(SCRATCH, 'test-')), name);
  mkdirSync(dir, { recursive: true });
  for (const sub of subdirs) mkdirSync(join(dir, sub), { recursive: true });
  return dir;
}

test('volume named Kindle with documents/ is detected', () => {
  expect(isKindleVolume(volume('Kindle', ['documents']))).toBe(true);
});

test('unnamed volume with documents/ + system/ is detected', () => {
  expect(isKindleVolume(volume('NO NAME', ['documents', 'system']))).toBe(true);
});

test('a system FILE, not a directory, still detects a Kindle', () => {
  // KindleDevice.isKindleVolume checks `system` with fileExists and NO
  // directory flag — it asks "did the firmware leave its marker here?", not
  // "is it a folder". Without this test, narrowing the check to isDirectory
  // (the way `documents` is checked, two lines up) passes silently.
  const dir = volume('NO NAME', ['documents']);
  writeFileSync(join(dir, 'system'), 'a marker file, not a folder');
  expect(isKindleVolume(dir)).toBe(true);
});

test('the Kindle name match is case-insensitive', () => {
  // Swift used localizedCaseInsensitiveContains; the port lowercases first.
  expect(isKindleVolume(volume('KINDLE', ['documents']))).toBe(true);
  expect(isKindleVolume(volume('kindle', ['documents']))).toBe(true);
  expect(isKindleVolume(volume('Sams Kindle', ['documents']))).toBe(true);
});

test('volume without documents/ is rejected', () => {
  expect(isKindleVolume(volume('Kindle-empty'))).toBe(false);
});

test('generic thumb drive is rejected', () => {
  expect(isKindleVolume(volume('USB STICK', ['documents']))).toBe(false);
});

test('a documents/ FILE does not make it a Kindle', () => {
  const dir = volume('Kindle-file');
  writeFileSync(join(dir, 'documents'), 'not a directory');
  expect(isKindleVolume(dir)).toBe(false);
});

test('copy lands in documents/, preserves content, and overwrites on re-copy', () => {
  const vol = volume('Kindle', ['documents']);
  const src = join(vol, '..', 'Test.epub');
  writeFileSync(src, 'v1');

  const dest = copyToKindleVolume(src, vol);
  expect(dest.endsWith(join('Kindle', 'documents', 'Test.epub'))).toBe(true);
  expect(readFileSync(dest, 'utf8')).toBe('v1');

  writeFileSync(src, 'v2');
  copyToKindleVolume(src, vol);
  expect(readFileSync(dest, 'utf8')).toBe('v2');
});

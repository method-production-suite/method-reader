import { afterAll, test, expect } from 'bun:test';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { copyToDevice, NoVolumeError } from '../src/device/transfer';
import type { ConnectedDevice } from '../src/device/types';

const SCRATCH = mkdtempSync(join(tmpdir(), 'screepub-device-transfer-'));
afterAll(() => rmSync(SCRATCH, { recursive: true, force: true }));

/** Mirrors the Swift app's tempDir() check helper: a uniquely-rooted
 * directory whose own name is the "volume name" under test. */
function volume(name: string, subdirs: string[] = []): string {
  const dir = join(mkdtempSync(join(SCRATCH, 'test-')), name);
  mkdirSync(dir, { recursive: true });
  for (const sub of subdirs) mkdirSync(join(dir, sub), { recursive: true });
  return dir;
}

test('copy to Kindle lands in documents/', () => {
  const vol = volume('Kindle', ['documents']);
  const src = join(vol, '..', 'Test.epub');
  writeFileSync(src, 'v1');

  const device: ConnectedDevice = {
    kind: 'kindle',
    name: 'Kindle',
    volume: vol,
  };

  const dest = copyToDevice(src, device);
  expect(dest.endsWith(join('documents', 'Test.epub'))).toBe(true);
  expect(readFileSync(dest, 'utf8')).toBe('v1');
});

test('copy to Kobo lands in volume root', () => {
  const vol = volume('KOBOeReader', ['.kobo']);
  const src = join(vol, '..', 'Book.epub');
  writeFileSync(src, 'content');

  const device: ConnectedDevice = {
    kind: 'kobo',
    name: 'KOBOeReader',
    volume: vol,
  };

  const dest = copyToDevice(src, device);
  expect(dest).toBe(join(vol, 'Book.epub'));
  expect(readFileSync(dest, 'utf8')).toBe('content');
});

test('copy to tolino lands in Books/ subfolder', () => {
  const vol = volume('tolino');
  const src = join(vol, '..', 'Story.epub');
  writeFileSync(src, 'text');

  const device: ConnectedDevice = {
    kind: 'tolino',
    name: 'tolino',
    volume: vol,
  };

  const dest = copyToDevice(src, device);
  expect(dest).toBe(join(vol, 'Books', 'Story.epub'));
  expect(readFileSync(dest, 'utf8')).toBe('text');
});

test('tolino Books/ folder is created if missing', () => {
  const vol = volume('tolino');
  const src = join(vol, '..', 'Book.epub');
  writeFileSync(src, 'data');

  const device: ConnectedDevice = {
    kind: 'tolino',
    name: 'tolino',
    volume: vol,
  };

  copyToDevice(src, device);
  const booksDir = join(vol, 'Books');
  expect(Bun.file(booksDir).exists()).toBeTruthy();
});

test('copy overwrites existing file', () => {
  const vol = volume('Kindle', ['documents']);
  const src = join(vol, '..', 'Test.epub');
  writeFileSync(src, 'v1');

  const device: ConnectedDevice = {
    kind: 'kindle',
    name: 'Kindle',
    volume: vol,
  };

  let dest = copyToDevice(src, device);
  expect(readFileSync(dest, 'utf8')).toBe('v1');

  writeFileSync(src, 'v2');
  copyToDevice(src, device);
  expect(readFileSync(dest, 'utf8')).toBe('v2');
});

test('reMarkable throws even with volume present', () => {
  const vol = volume('ReMarkable');
  const src = join(vol, '..', 'Book.epub');
  writeFileSync(src, 'content');

  const device: ConnectedDevice = {
    kind: 'remarkable',
    name: 'reMarkable',
    volume: vol,
  };

  expect(() => copyToDevice(src, device)).toThrow(NoVolumeError);
});

test('throws if device has no volume', () => {
  const src = '/tmp/Book.epub';
  writeFileSync(src, 'content');

  const device: ConnectedDevice = {
    kind: 'kindle',
    name: 'Kindle',
    volume: null,
  };

  expect(() => copyToDevice(src, device)).toThrow(NoVolumeError);
});

test('Kobo copy overwrites existing file at volume root', () => {
  const vol = volume('KOBOeReader', ['.kobo']);
  const src = join(vol, '..', 'Book.epub');
  writeFileSync(src, 'v1');

  const device: ConnectedDevice = {
    kind: 'kobo',
    name: 'KOBOeReader',
    volume: vol,
  };

  let dest = copyToDevice(src, device);
  expect(readFileSync(dest, 'utf8')).toBe('v1');

  writeFileSync(src, 'v2');
  copyToDevice(src, device);
  expect(readFileSync(dest, 'utf8')).toBe('v2');
});

test('tolino copy overwrites existing file in Books/', () => {
  const vol = volume('tolino');
  const src = join(vol, '..', 'Book.epub');
  writeFileSync(src, 'v1');

  const device: ConnectedDevice = {
    kind: 'tolino',
    name: 'tolino',
    volume: vol,
  };

  let dest = copyToDevice(src, device);
  expect(readFileSync(dest, 'utf8')).toBe('v1');

  writeFileSync(src, 'v2');
  copyToDevice(src, device);
  expect(readFileSync(dest, 'utf8')).toBe('v2');
});

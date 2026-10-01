import { test, expect } from 'bun:test';
import { fileExtension, formatLabel } from '../src/export/formats';

test('EPUB is always epub regardless of toolchain', () => {
  expect(fileExtension('epub', { calibreAvailable: false, kfxReady: false })).toBe('epub');
  expect(fileExtension('epub', { calibreAvailable: true, kfxReady: true })).toBe('epub');
});

test('the Kindle ladder prefers KFX, then AZW3, then MOBI', () => {
  expect(fileExtension('kindle', { calibreAvailable: true, kfxReady: true })).toBe('kfx');
  expect(fileExtension('kindle', { calibreAvailable: true, kfxReady: false })).toBe('azw3');
  expect(fileExtension('kindle', { calibreAvailable: false, kfxReady: false })).toBe('mobi');
});

test('the Kindle label names the format it will actually produce', () => {
  expect(formatLabel('kindle', { calibreAvailable: true, kfxReady: false })).toContain('AZW3');
  expect(formatLabel('kindle', { calibreAvailable: false, kfxReady: false })).toContain('MOBI');
});

test('only the KFX rung claims best quality', () => {
  expect(formatLabel('kindle', { calibreAvailable: true, kfxReady: true })).toContain('best quality');
  expect(formatLabel('kindle', { calibreAvailable: true, kfxReady: false })).not.toContain('best quality');
});

test('KFX wins even when Calibre is not detected separately', () => {
  // Swift's ladder is `kfxReady ? "kfx" : calibreAvailable ? "azw3" : "mobi"`,
  // so kfxReady short-circuits and calibreAvailable is never consulted. This
  // pins that: an implementation gating KFX behind Calibre would return 'mobi'.
  expect(fileExtension('kindle', { calibreAvailable: false, kfxReady: true })).toBe('kfx');
  expect(formatLabel('kindle', { calibreAvailable: false, kfxReady: true })).toContain('KFX');
});

test('a named file type is its own extension and label, whatever the toolchain', () => {
  const names = { kfx: 'KFX', azw3: 'AZW3', mobi: 'MOBI', fountain: 'Fountain' } as const;
  for (const format of ['kfx', 'azw3', 'mobi', 'fountain'] as const) {
    for (const state of [{ calibreAvailable: false, kfxReady: false }, { calibreAvailable: true, kfxReady: true }]) {
      expect(fileExtension(format, state)).toBe(format);
      expect(formatLabel(format, state).startsWith(`${names[format]}:`)).toBe(true);
    }
  }
});

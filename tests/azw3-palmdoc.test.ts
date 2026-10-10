import { describe, expect, test } from 'bun:test';
import { palmdocCompress } from '../src/azw3/palmdoc';
import { palmdocDecompress } from './helpers/palmdoc-decode';

const enc = new TextEncoder();

function roundTrips(bytes: Uint8Array) {
  const packed = palmdocCompress(bytes);
  expect(Array.from(palmdocDecompress(packed))).toEqual(Array.from(bytes));
  return packed;
}

describe('PalmDOC compression', () => {
  test('repetitive markup shrinks and comes back byte for byte', () => {
    const html = '<div class="dialogue-block"><p class="character">JO</p><p class="dialogue">Again.</p></div>\n'.repeat(40);
    const bytes = enc.encode(html).slice(0, 4096);
    const packed = roundTrips(bytes);
    expect(packed.length).toBeLessThan(bytes.length / 2);
  });

  test('the space-plus-letter pair is one byte', () => {
    // No repeats long enough to copy, so each " x" pair is the 0xC0 form.
    const packed = roundTrips(enc.encode('a b c d e f g'));
    expect(packed.length).toBeLessThan('a b c d e f g'.length);
  });

  test('bytes outside 0x09-0x7F travel as literal runs, UTF-8 included', () => {
    roundTrips(enc.encode('Mara’s café — “quoted” ünïcödé '.repeat(20)));
    roundTrips(new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 0x80, 0xff, 0x20, 0x41, 0x20, 0x7f, 0x20]));
  });

  test('every byte value, and the edges of a 4096-byte record', () => {
    const all = new Uint8Array(4096);
    for (let i = 0; i < all.length; i++) all[i] = (i * 7919) & 0xff;
    roundTrips(all);
    roundTrips(new Uint8Array(0));
    roundTrips(enc.encode('x'));
    roundTrips(enc.encode('abcabcabcabc'));
  });

  test('a match never reaches further back than 2047 bytes', () => {
    const block = enc.encode('<p class="unique-run">0123456789</p>');
    const filler = new Uint8Array(2100);
    for (let i = 0; i < filler.length; i++) filler[i] = 0x41 + ((i * 31) % 26);
    const bytes = new Uint8Array([...block, ...filler, ...block]);
    roundTrips(bytes);
  });
});

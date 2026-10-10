// PalmDOC compression for the AZW3's text records.
//
// Ported from cpalmdoc_do_compress in Calibre's
// src/calibre/ebooks/compression/palmdoc.c (calibre 9.11.0, GPL-3,
// Copyright Kovid Goyal; this port is part of Screepub, AGPL-3.0-or-later,
// see THIRD-PARTY-NOTICES.md). The output format is the classic one every
// MOBI and KF8 reader decodes: a byte 0x01-0x08 is a run of that many
// literal bytes; 0x00 and 0x09-0x7F stand for themselves; 0xC0-0xFF is a
// space followed by the byte XOR 0x80; and 0x80-0xBF starts a two-byte
// back-reference of 3 to 10 bytes from at most 2047 bytes back.
//
// What differs from the C is only HOW the back-reference is found. Calibre
// tries each length from 10 down to 3 and, for each, scans backward through
// the whole record for the nearest copy, keeping it if it lies within 2047
// bytes. That is the longest length with any copy in the window, taken from
// the nearest copy of that length, and it is what this finds too, through a
// hash chain over the window rather than a scan of everything before it. A
// compressor in C can afford the scan; a script's worth of records in
// TypeScript cannot.

const WINDOW = 2047;
const MAX_LEN = 10;
const MIN_LEN = 3;
const HASH_BITS = 12;

function hash3(d: Uint8Array, p: number): number {
  return ((d[p] << 7) ^ (d[p + 1] << 4) ^ d[p + 2]) & ((1 << HASH_BITS) - 1);
}

/** Compress one text record (Calibre compresses each record on its own). */
export function palmdocCompress(data: Uint8Array): Uint8Array {
  const n = data.length;
  const out = new Uint8Array(Math.ceil(n * 1.25) + 16);
  let o = 0;
  const head = new Int32Array(1 << HASH_BITS).fill(-1);
  const prev = new Int32Array(Math.max(n, 1)).fill(-1);
  let inserted = 0;

  let i = 0;
  while (i < n) {
    const c = data[i];
    // Back-references, under the C's own guard: not in the first 11 bytes,
    // and not within 10 of the end.
    if (i > 10 && n - i > 10) {
      // Every position that could start a copy for this one: a copy must
      // end before i begins, so it starts at i - 3 or earlier.
      while (inserted <= i - MIN_LEN) {
        const h = hash3(data, inserted);
        prev[inserted] = head[h];
        head[h] = inserted;
        inserted++;
      }
      let bestLen = 0;
      let bestDist = 0;
      for (let j = head[hash3(data, i)]; j >= 0 && i - j <= WINDOW; j = prev[j]) {
        const room = Math.min(MAX_LEN, i - j);
        let m = 0;
        while (m < room && data[j + m] === data[i + m]) m++;
        // Strictly longer only, so a tie keeps the nearer copy, as the C's
        // backward scan does.
        if (m >= MIN_LEN && m > bestLen) {
          bestLen = m;
          bestDist = i - j;
          if (m === MAX_LEN) break;
        }
      }
      if (bestLen > 0) {
        const compound = (bestDist << 3) + bestLen - MIN_LEN;
        out[o++] = 0x80 + (compound >> 8);
        out[o++] = compound & 0xff;
        i += bestLen;
        continue;
      }
    }

    // A single character.
    i++;
    if (c === 0x20 && i < n) {
      const next = data[i];
      if (next >= 0x40 && next <= 0x7f) {
        out[o++] = next ^ 0x80;
        i++;
        continue;
      }
    }
    if (c === 0 || (c > 8 && c < 0x80)) {
      out[o++] = c;
    } else {
      // A run of up to 8 bytes that cannot stand for themselves.
      let j = i;
      let len = 1;
      const at = o + 1;
      out[at] = c;
      while (j < n && len < 8) {
        const d = data[j];
        if (d === 0 || (d > 8 && d < 0x80)) break;
        out[at + len] = d;
        len++;
        j++;
      }
      i += len - 1;
      out[o] = len;
      o = at + len;
    }
  }
  return out.slice(0, o);
}

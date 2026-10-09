import { test, expect, describe } from 'bun:test';
import { encodePng, downscaleRgb, isPng } from '../src/parser/png';
import { inflateSync } from 'node:zlib';

const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/** Solid-colour RGB buffer, the simplest thing with a knowable encoding. */
function solid(w: number, h: number, r: number, g: number, b: number): Uint8Array {
  const px = new Uint8Array(w * h * 3);
  for (let i = 0; i < w * h; i++) [px[i * 3], px[i * 3 + 1], px[i * 3 + 2]] = [r, g, b];
  return px;
}

describe('encodePng', () => {
  test('writes a PNG signature and an IHDR describing 8-bit truecolour', () => {
    const png = encodePng(solid(4, 3, 10, 20, 30), 4, 3);
    expect([...png.subarray(0, 8)]).toEqual(PNG_MAGIC);

    const dv = new DataView(png.buffer, png.byteOffset);
    expect(dv.getUint32(8)).toBe(13);                       // IHDR length
    expect(String.fromCharCode(...png.subarray(12, 16))).toBe('IHDR');
    expect(dv.getUint32(16)).toBe(4);                       // width
    expect(dv.getUint32(20)).toBe(3);                       // height
    expect(png[24]).toBe(8);                                // bit depth
    expect(png[25]).toBe(2);                                // colour type: truecolour
    expect(png[26]).toBe(0);                                // compression: deflate
    expect(png[27]).toBe(0);                                // filter method: adaptive
    expect(png[28]).toBe(0);                                // interlace: none
  });

  test('ends with IEND', () => {
    const png = encodePng(solid(2, 2, 0, 0, 0), 2, 2);
    expect(String.fromCharCode(...png.subarray(png.length - 8, png.length - 4))).toBe('IEND');
  });

  test('round-trips the exact pixels through the filters', () => {
    // A gradient exercises every filter branch; a solid colour would not.
    const w = 9, h = 7;
    const px = new Uint8Array(w * h * 3);
    for (let i = 0; i < w * h; i++) {
      px[i * 3] = (i * 7) % 256;
      px[i * 3 + 1] = (i * 13) % 256;
      px[i * 3 + 2] = (i * 29) % 256;
    }
    expect(decodeRgb(encodePng(px, w, h), w, h)).toEqual(px);
  });

  test('every chunk carries a valid CRC', () => {
    // A wrong CRC still opens in tolerant viewers and fails epubcheck later,
    // so it is worth asserting rather than trusting.
    const png = encodePng(solid(5, 5, 200, 100, 50), 5, 5);
    let off = 8;
    let seen = 0;
    while (off < png.length) {
      const len = new DataView(png.buffer, png.byteOffset).getUint32(off);
      const body = png.subarray(off + 4, off + 8 + len);
      const stored = new DataView(png.buffer, png.byteOffset).getUint32(off + 8 + len);
      expect(crc32(body)).toBe(stored);
      off += 12 + len;
      seen++;
    }
    expect(seen).toBe(3); // IHDR, IDAT, IEND
  });

  test('the row filters are live, not a hardcoded None', () => {
    // The round-trip test passes just as well against an encoder that writes
    // every row raw, so nothing above proves a predictor ever runs. On an
    // image whose rows are identical, Up scores zero and must win outright.
    const w = 16, h = 5;
    const px = new Uint8Array(w * h * 3);
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const i = (y * w + x) * 3;
        px[i] = px[i + 1] = px[i + 2] = x * 8;
      }
    }
    const types = filterTypes(encodePng(px, w, h), w, h);
    expect(types.slice(1)).toEqual([2, 2, 2, 2]);
    // Row 0 has no row above it, so only that it predicted at all is stable.
    expect(types[0]).not.toBe(0);
  });

  test('chooses Paeth where Paeth is the best predictor', () => {
    // A predictor that quietly stops being SELECTED leaves every other test
    // green: the bytes stay correct, they are just fatter. Nothing but the
    // filter byte shows that one of the five has dropped out of the running.
    const { px, w, h } = paethImage();
    expect(filterTypes(encodePng(px, w, h), w, h)).toContain(4);
  });

  test('breaks a Paeth tie the way the spec breaks it', () => {
    // The reconstruction below is the spec's rule written out, so the two
    // agree only while the encoder also uses `<=`. A `<` in either comparison
    // reaches for c instead of a or b and lands 80 levels off at the two
    // pixels in the four-pixel tail that actually tie (x=13 and x=15; the
    // other two resolve by strict inequality either way). That is exactly the
    // classic Paeth defect.
    const { px, w, h } = paethImage();
    expect(decodeRgb(encodePng(px, w, h), w, h)).toEqual(px);
  });

  test('round-trips a 1x1, a single row, and a single column', () => {
    // Degenerate shapes are where an off-by-one in the stride or in the
    // `i >= BPP` guard hides: a storyboard rule or a divider is one of these.
    for (const [w, h] of [[1, 1], [7, 1], [1, 6]] as const) {
      const px = new Uint8Array(w * h * 3);
      for (let i = 0; i < px.length; i++) px[i] = (i * 37 + 11) % 256;
      expect(decodeRgb(encodePng(px, w, h), w, h), `${w}x${h}`).toEqual(px);
    }
  });

  test('refuses pixels that do not match the stated dimensions', () => {
    // Silently tolerating this ships an image whose bottom rows are solid
    // black under a valid CRC, and nothing downstream can tell that from art.
    expect(() => encodePng(new Uint8Array(11), 2, 2)).toThrow();
    expect(() => encodePng(new Uint8Array(13), 2, 2)).toThrow();
    expect(() => encodePng(new Uint8Array(0), 0, 0)).toThrow();
    expect(() => encodePng(new Uint8Array(0), 2, 0)).toThrow();
  });
});

describe('isPng', () => {
  /** A real one to cut up, rather than a hand-built header. */
  const real = encodePng(solid(4, 3, 10, 20, 30), 4, 3);

  test('accepts what this module encodes', () => {
    // The pairing that matters: the predicate and the encoder read the same
    // signature and the same IHDR, so nothing we write can be refused by it.
    expect(isPng(real)).toBe(true);
  });

  test('refuses the formats a picture folder can actually be filled with', () => {
    for (const head of [
      [0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46],  // JPEG
      [0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00],  // GIF89a
      [0x52, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00],  // RIFF/WebP
      [0x3c, 0x3f, 0x78, 0x6d, 0x6c, 0x20, 0x3f, 0x3e],  // <?xml ?> (SVG)
      [0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x37],  // %PDF-1.7
    ]) {
      // Padded to a full header's length, or they would be refused for being
      // short and this would prove nothing about the signature.
      const bytes = new Uint8Array(64);
      bytes.set(head);
      expect(isPng(bytes), head.join(' ')).toBe(false);
    }
  });

  test('an empty file is not one — the iCloud placeholder case', () => {
    // `bytes[i]` on a short array is undefined, not a throw, so this is a
    // statement about the answer rather than about crashing.
    expect(isPng(new Uint8Array(0))).toBe(false);
    expect(isPng(new Uint8Array(PNG_MAGIC.slice(0, 7)))).toBe(false);
  });

  test('refuses every truncation epubcheck calls corrupt, and nothing more', () => {
    // The boundary is measured, not chosen: a plate cut to 28 bytes is
    // ERROR(PKG-021) in epubcheck and 29 is clean, so 29 is where this
    // flips too. A test that only checked "short is false" would pass with
    // the bound anywhere.
    for (let n = 0; n < 29; n++) {
      expect(isPng(real.slice(0, n)), `${n} bytes`).toBe(false);
    }
    expect(isPng(real.slice(0, 29))).toBe(true);
  });

  test('refuses a header that is the right length and the wrong thing', () => {
    // Both epubcheck refusals a 29-byte file can still be: the first chunk
    // is not IHDR, or the frame it declares is empty.
    const badType = real.slice(0, 29);
    badType[15] = 0x58;                       // IHDX
    expect(isPng(badType)).toBe(false);

    const zeroed = new Uint8Array(29);
    zeroed.set(real.slice(0, 16));            // signature + length + IHDR
    expect(isPng(zeroed)).toBe(false);        // ...and a 0x0 frame
  });

  test('it reads the header only, and says so by passing a ruined body', () => {
    // Not an oversight: a 45-byte prefix of a real plate is clean in
    // epubcheck too. Deciding a picture is beyond repair is a decoder's job,
    // and this exists to keep an invalid BOOK from being written.
    expect(isPng(real.slice(0, 45))).toBe(true);
  });

  test('reads the header at an offset, not from the start of a pooled buffer', () => {
    // `readFile` hands back Buffers that are views into a shared allocation,
    // so a DataView built on `.buffer` without `.byteOffset` reads somebody
    // else's bytes. The picture sits PAST the width and height fields' own
    // offsets, so a view-blind read finds the pool's zeros and refuses a
    // perfectly good plate: a nearer offset passes either way and proves
    // nothing, which is how the first version of this test let that mutant
    // live.
    const pool = new Uint8Array(64 + real.length);
    pool.set(real, 64);
    expect(isPng(pool.subarray(64))).toBe(true);
    // and the signature is read view-relative too
    expect(isPng(pool.subarray(63))).toBe(false);
  });
});

describe('downscaleRgb', () => {
  test('leaves an image already within the cap untouched', () => {
    const px = solid(10, 8, 1, 2, 3);
    const out = downscaleRgb(px, 10, 8, 100);
    expect(out).toEqual({ rgb: px, width: 10, height: 8 });
  });

  test('an image exactly at the cap is still untouched', () => {
    const px = solid(100, 40, 4, 5, 6);
    const out = downscaleRgb(px, 100, 40, 100);
    expect(out).toEqual({ rgb: px, width: 100, height: 40 });
    // toBe, not toEqual: the contract is that a fitting image comes back as
    // the SAME array. Turning that into a copy would be a silent regression
    // under a structural assertion, and megabytes of needless allocation on
    // every lookbook page.
    expect(out.rgb).toBe(px);
  });

  test('fits the long edge to the cap and keeps the aspect ratio', () => {
    const out = downscaleRgb(solid(400, 200, 9, 9, 9), 400, 200, 100);
    expect(out.width).toBe(100);
    expect(out.height).toBe(50);
    expect(out.rgb.length).toBe(100 * 50 * 3);
  });

  test('fits the HEIGHT when the image is portrait', () => {
    // A full-page storyboard is taller than it is wide; capping the width
    // instead would leave it over the cap in the direction that matters.
    const out = downscaleRgb(solid(200, 400, 9, 9, 9), 200, 400, 100);
    expect(out.width).toBe(50);
    expect(out.height).toBe(100);
    expect(out.rgb.length).toBe(50 * 100 * 3);
  });

  test('averages rather than samples, so a checkerboard greys out', () => {
    // Nearest-neighbour would return pure black or pure white here. A box
    // filter returns the mean, which is the whole point: fine pen lines in a
    // storyboard must not alias into moire on an e-ink panel.
    const w = 4, h = 4;
    const px = new Uint8Array(w * h * 3);
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const v = (x + y) % 2 ? 255 : 0;
        const i = (y * w + x) * 3;
        px[i] = px[i + 1] = px[i + 2] = v;
      }
    }
    const out = downscaleRgb(px, w, h, 2);
    expect(out.width).toBe(2);
    for (const v of out.rgb) expect(v).toBeGreaterThan(100);
    for (const v of out.rgb) expect(v).toBeLessThan(155);
  });

  test('rounds the box mean rather than truncating it', () => {
    // Truncation costs half a level on every pixel of every image we carry,
    // which is a uniform darkening, not a rounding detail.
    const px = new Uint8Array([127, 127, 127, 128, 128, 128]);
    expect([...downscaleRgb(px, 2, 1, 1).rgb]).toEqual([128, 128, 128]);
  });

  test('tiles the source exactly when the ratio is not a whole number', () => {
    // 7 into 3 leaves boxes of 2, 2 and 3. Both edges of a box have to come off
    // the same floored mapping: round either one and the sampling interval
    // shifts, so the means are taken over the wrong pixels — this ramp reads
    // 20, 120, 220 instead of 20, 100, 200. A flat source proves none of this,
    // because a window off by a column still averages to the same value.
    const ramp = new Uint8Array(7 * 3);
    for (let i = 0; i < 7; i++) ramp[i * 3] = ramp[i * 3 + 1] = ramp[i * 3 + 2] = i * 40;
    const expected = [20, 20, 20, 100, 100, 100, 200, 200, 200];

    const across = downscaleRgb(ramp, 7, 1, 3);
    expect([across.width, across.height]).toEqual([3, 1]);
    expect([...across.rgb]).toEqual(expected);

    // The same ramp down the other axis, so a floor lost from the ROW window
    // is caught as well as one lost from the column window.
    const down = downscaleRgb(ramp, 1, 7, 3);
    expect([down.width, down.height]).toEqual([1, 3]);
    expect([...down.rgb]).toEqual(expected);
  });

  test('refuses pixels that do not match the stated dimensions', () => {
    expect(() => downscaleRgb(new Uint8Array(11), 2, 2, 1)).toThrow();
    expect(() => downscaleRgb(new Uint8Array(0), 0, 0, 1)).toThrow();
  });
});

// --- helpers -------------------------------------------------------------

/**
 * Two rows built to make Paeth both CHOSEN and OBSERVABLE, neither of which a
 * gradient can be relied on to do.
 *
 * First four pixels: the row above is flat and this row is shifted, so `b == c`
 * and Paeth predicts from the left while Up is badly wrong. Next eight: this
 * row repeats the jumpy row above it, so `a == c` and Paeth predicts from
 * above while Sub is badly wrong. Between them they make Paeth the cheapest
 * filter for the row by a wide margin, which is the only way its predictor
 * runs at all.
 *
 * Last four pixels are the two triples where the spec's `<=` comparisons are
 * what decide the answer. At a=20 b=140 c=100 the distances are pa=40 pb=80
 * pc=40, so `pa <= pc` picks a=20 where a strict `<` falls through to c=100.
 * At a=140 b=20 c=100 they are pa=80 pb=40 pc=40, so `pb <= pc` picks b=20
 * where a strict `<` again reaches c=100. Both are off by 80 levels.
 */
function paethImage(): { px: Uint8Array; w: number; h: number } {
  const above = [200, 200, 200, 200, 200, 60, 210, 55, 200, 60, 210, 55, 100, 140, 100, 20];
  const below = [50, 50, 50, 50, 200, 60, 210, 55, 200, 60, 210, 55, 20, 20, 140, 20];
  const w = above.length;
  const px = new Uint8Array(w * 2 * 3);
  [above, below].forEach((row, y) => {
    row.forEach((v, x) => {
      const i = (y * w + x) * 3;
      px[i] = px[i + 1] = px[i + 2] = v;
    });
  });
  return { px, w, h: 2 };
}

function crc32(buf: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    c ^= buf[i];
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return (c ^ 0xffffffff) >>> 0;
}

/** The concatenated IDAT payload, inflated: one filtered row per line. */
function rawRows(png: Uint8Array): Uint8Array {
  const dv = new DataView(png.buffer, png.byteOffset);
  let off = 8;
  const parts: Uint8Array[] = [];
  while (off < png.length) {
    const len = dv.getUint32(off);
    const type = String.fromCharCode(...png.subarray(off + 4, off + 8));
    if (type === 'IDAT') parts.push(png.subarray(off + 8, off + 8 + len));
    off += 12 + len;
  }
  const idat = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    idat.set(p, at);
    at += p.length;
  }
  return new Uint8Array(inflateSync(idat));
}

/** The filter byte that leads each row. */
function filterTypes(png: Uint8Array, w: number, h: number): number[] {
  const raw = rawRows(png);
  const stride = w * 3;
  return Array.from({ length: h }, (_, y) => raw[y * (stride + 1)]);
}

/** Minimal PNG reader: enough to prove the encoder round-trips. Test-only. */
function decodeRgb(png: Uint8Array, w: number, h: number): Uint8Array {
  const raw = rawRows(png);
  const stride = w * 3;
  const out = new Uint8Array(stride * h);
  const prev = new Uint8Array(stride);
  for (let y = 0; y < h; y++) {
    const type = raw[y * (stride + 1)];
    const row = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let i = 0; i < stride; i++) {
      const a = i >= 3 ? out[y * stride + i - 3] : 0;
      const b = prev[i];
      const c = i >= 3 ? prev[i - 3] : 0;
      let p = 0;
      if (type === 1) p = a;
      else if (type === 2) p = b;
      else if (type === 3) p = (a + b) >> 1;
      else if (type === 4) {
        const pa = Math.abs(b - c), pb = Math.abs(a - c), pc = Math.abs(a + b - 2 * c);
        p = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      out[y * stride + i] = (row[i] + p) & 0xff;
    }
    prev.set(out.subarray(y * stride, (y + 1) * stride));
  }
  return out;
}

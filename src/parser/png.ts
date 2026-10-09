// Minimal PNG encoder and box downscaler.
//
// pdf.js hands back DECODED pixels and never the original JPEG (there is no
// public route to the raw stream, as of pdfjs-dist 6.2.108, and still none on
// 6.3.289's PDFDocumentProxy or PDFPageProxy, re-checked), so anything
// Screepub carries into a book is a re-encode. That makes the bytes ours to
// choose, which is the whole reason this file exists rather than a
// dependency: the same judgement that produced the hand-built PalmDB
// container in mobi/writer.ts.
//
// Encoder and downscaler share a file because they are one unit: both are
// pure pixel maths with no pdf.js in them, both are only ever called back to
// back by images.ts, and both encode the same RGB-triple contract in BPP.
// Splitting a 200-line pair so a constant can cross a module boundary is the
// drift risk CLAUDE.md warns about for slug.ts and notes.ts, pointed the
// wrong way.
//
// 2026-08-06: output verified OUT OF BAND, not just against this file's own
// test decoder — macOS ImageIO (`sips`) re-emits it as 24-bit BMP pixel-exact
// at 1x1, 9x1, 1x9, 1400x1, 3x1400, 71x53, and on an image using all five
// row predictors. A suite that only checks an encoder against its own reader
// can be self-consistently wrong and stay green.
import { deflateSync } from 'node:zlib';

/** Bytes per pixel. Truecolour 8-bit only: the one thing pdf.js output ever
 * needs to become, once the kind conversions in images.ts have run. */
const BPP = 3;

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(buf: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/**
 * Reject pixels that disagree with the dimensions they are labelled with.
 *
 * Tolerating a short buffer would still produce a well-formed PNG: the filter
 * loop reads past the end, the arithmetic lands on zero, and the book ships an
 * image whose bottom rows are solid black under a correct CRC — a defect no
 * later stage can detect and no reader can explain. A zero or fractional
 * dimension is refused too, because the spec forbids it and the file would
 * instead be rejected by a device, far from the code that made it. The kind
 * conversions upstream produce exactly `width * height * 3` bytes or give up
 * on the image, so a mismatch here is a bug, never bad input.
 */
function assertRgb(rgb: Uint8Array, width: number, height: number, fn: string): void {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1) {
    throw new Error(`${fn}: bad dimensions ${width}x${height}`);
  }
  if (rgb.length !== width * height * BPP) {
    throw new Error(
      `${fn}: ${rgb.length} bytes for ${width}x${height} (expected ${width * height * BPP})`,
    );
  }
}

function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  // Bounded to the view rather than the whole ArrayBuffer: `out` is its own
  // buffer today, and this keeps that from being load-bearing.
  const dv = new DataView(out.buffer, out.byteOffset, out.byteLength);
  dv.setUint32(0, data.length);
  for (let i = 0; i < 4; i++) out[4 + i] = type.charCodeAt(i);
  out.set(data, 8);
  dv.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

/**
 * Filter one row under all five PNG predictors and keep the cheapest, by the
 * spec's minimum-sum-of-absolute-differences heuristic.
 *
 * Worth the five passes: against the same pixels written raw the saving is
 * substantial, and entirely content-dependent — largest where the picture is
 * smooth and a gradient can be predicted away, smallest where it is already
 * noisy. A script padded with storyboards is exactly the script that is
 * already too large to email.
 *
 * Returns the chosen TYPE; the filtered bytes it selected are left in
 * `cand[type]` for the caller to write out. `cand` is allocated once and
 * every one of its five buffers is fully rewritten on every row, which is
 * what makes that reuse safe. Anyone adding the natural optimisation here —
 * abandoning a type once its running score passes `bestScore` — breaks that
 * invariant and leaves a partial buffer behind for a type that can then
 * never be selected. Cheap to get wrong, silent when wrong.
 */
function filterRow(
  row: Uint8Array,
  prev: Uint8Array,
  cand: Uint8Array[],
  stride: number,
): number {
  let bestType = 0;
  let bestScore = Infinity;
  for (let type = 0; type < 5; type++) {
    const buf = cand[type];
    let score = 0;
    for (let i = 0; i < stride; i++) {
      // Left and up-left come from the row as the DECODER will have it, which
      // for the encoder is simply the unfiltered bytes; off the edge is zero.
      const a = i >= BPP ? row[i - BPP] : 0;
      const b = prev[i];
      const c = i >= BPP ? prev[i - BPP] : 0;
      let p: number;
      if (type === 0) p = 0;
      else if (type === 1) p = a;
      else if (type === 2) p = b;
      else if (type === 3) p = (a + b) >> 1;
      else {
        const pa = Math.abs(b - c);
        const pb = Math.abs(a - c);
        const pc = Math.abs(a + b - 2 * c);
        p = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      const v = (row[i] - p) & 0xff;
      buf[i] = v;
      // Signed-magnitude sum: the spec's own heuristic.
      score += v < 128 ? v : 256 - v;
    }
    if (score < bestScore) {
      bestScore = score;
      bestType = type;
    }
  }
  return bestType;
}

/** The eight bytes every PNG starts with (PNG spec §5.2). */
const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/** `IHDR` — the type of the chunk the spec requires to come first. */
const IHDR_TYPE = [0x49, 0x48, 0x44, 0x52];

/**
 * Signature (8) + the IHDR chunk's length (4) and type (4) + its 13 data
 * bytes. Its CRC is deliberately NOT counted: 29 is the exact byte at which
 * epubcheck stops calling a truncated plate corrupt, measured by truncating a
 * real one — 28 bytes is `ERROR(PKG-021)`, 29 is clean.
 */
const PNG_HEADER_BYTES = 29;

/**
 * Does this file carry a PNG header?
 *
 * A HEADER check, not a decode, and the bar is what epubcheck reads rather
 * than what a decoder would. A picture read back off disk is declared
 * `image/png` in an OPF manifest on the strength of its NAME, and epubcheck
 * opens the file: everything below is measured against a real book rather
 * than remembered.
 *
 *   a JPEG named `shot.png`        ERROR(OPF-029) + WARNING(PKG-022)
 *   0 bytes (an iCloud placeholder) ERROR(MED-004) + ERROR(PKG-021)
 *   8..28 bytes of a real plate    ERROR(PKG-021) corrupted image file
 *   29 bytes with `IHDR` mangled   ERROR(PKG-021)
 *   29 bytes of zeroed header      ERROR(PKG-021)
 *   29 bytes of a real plate       clean
 *
 * The signature alone answers only the first two rows, which is why this also
 * demands the IHDR chunk and a non-zero frame: every row above that epubcheck
 * refuses, this refuses too. What it does NOT do is look past the header —
 * a 45-byte prefix of a real plate is clean here and clean in epubcheck, so a
 * file with a good header and ruined pixels is a corrupt picture that both
 * tools pass. That is a decoder's job, and a decoder is not what stands
 * between a picture folder and a book that will not validate.
 *
 * Nothing this engine encodes can fail it — `encodePng` writes this same
 * signature and IHDR below — so the callers that need it are the ones handling
 * a folder somebody else filled.
 */
export function isPng(bytes: Uint8Array): boolean {
  if (bytes.length < PNG_HEADER_BYTES) return false;
  if (!PNG_SIGNATURE.every((b, i) => bytes[i] === b)) return false;
  if (!IHDR_TYPE.every((b, i) => bytes[12 + i] === b)) return false;
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return dv.getUint32(16) > 0 && dv.getUint32(20) > 0;
}

/** Encode 8-bit RGB pixels as a PNG. */
export function encodePng(rgb: Uint8Array, width: number, height: number): Uint8Array {
  assertRgb(rgb, width, height, 'encodePng');

  const stride = width * BPP;
  const raw = new Uint8Array((stride + 1) * height);
  const prev = new Uint8Array(stride);
  const cand = Array.from({ length: 5 }, () => new Uint8Array(stride));

  for (let y = 0; y < height; y++) {
    const row = rgb.subarray(y * stride, (y + 1) * stride);
    const type = filterRow(row, prev, cand, stride);
    raw[y * (stride + 1)] = type;
    raw.set(cand[type], y * (stride + 1) + 1);
    prev.set(row);
  }

  const ihdr = new Uint8Array(13);
  const dv = new DataView(ihdr.buffer, ihdr.byteOffset, ihdr.byteLength);
  dv.setUint32(0, width);
  dv.setUint32(4, height);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // colour type: truecolour
  // 10..12 stay zero: deflate, adaptive filtering, no interlace.

  const parts = [
    new Uint8Array(PNG_SIGNATURE),
    chunk('IHDR', ihdr),
    // deflateSync's Buffer goes in as-is. A Buffer IS a Uint8Array, and it is
    // usually a view into a larger pooled allocation, which `chunk` handles:
    // it copies with `out.set(data, 8)` and CRCs its own output, so it never
    // touches `data.buffer`. Re-wrapping would copy the whole payload twice.
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', new Uint8Array(0)),
  ];
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}

/**
 * Box-filter downscale so the long edge fits `maxEdge`. An image that already
 * fits comes back as THE SAME ARRAY, not a copy, so a caller that wants to
 * mutate the result has to copy it first.
 *
 * A box average rather than nearest-neighbour because storyboard art is full
 * of fine pen lines, and point-sampling those onto an e-ink panel produces
 * moire that reads as a printing fault.
 *
 * `maxEdge` is deliberately unchecked, unlike the dimensions. It comes from a
 * module constant rather than from the file being converted, so a bad value
 * is a bug in our own caller and not something a PDF can cause — which is the
 * opposite of the mismatch `assertRgb` guards, where the input really does
 * come from outside.
 */
export function downscaleRgb(
  rgb: Uint8Array,
  width: number,
  height: number,
  maxEdge: number,
): { rgb: Uint8Array; width: number; height: number } {
  assertRgb(rgb, width, height, 'downscaleRgb');

  const longest = Math.max(width, height);
  if (longest <= maxEdge) return { rgb, width, height };

  const scale = maxEdge / longest;
  const w = Math.max(1, Math.round(width * scale));
  const h = Math.max(1, Math.round(height * scale));
  const out = new Uint8Array(w * h * BPP);

  for (let y = 0; y < h; y++) {
    // Both edges floor off the same mapping, so consecutive boxes meet exactly
    // and every source pixel lands in one of them. Rounding either edge would
    // leave a column read twice or not at all, which on flat art is a stripe.
    const sy0 = Math.floor((y * height) / h);
    const sy1 = Math.max(sy0 + 1, Math.floor(((y + 1) * height) / h));
    for (let x = 0; x < w; x++) {
      const sx0 = Math.floor((x * width) / w);
      const sx1 = Math.max(sx0 + 1, Math.floor(((x + 1) * width) / w));
      let r = 0, g = 0, b = 0, n = 0;
      for (let sy = sy0; sy < sy1; sy++) {
        for (let sx = sx0; sx < sx1; sx++) {
          const i = (sy * width + sx) * BPP;
          r += rgb[i];
          g += rgb[i + 1];
          b += rgb[i + 2];
          n++;
        }
      }
      const o = (y * w + x) * BPP;
      // Rounded, not truncated: a Uint8Array store truncates, and half a level
      // shaved off every pixel of every image is a uniform darkening.
      out[o] = Math.round(r / n);
      out[o + 1] = Math.round(g / n);
      out[o + 2] = Math.round(b / n);
    }
  }
  return { rgb: out, width: w, height: h };
}

//! Port of `src/parser/png.ts`: a minimal PNG encoder and box downscaler.
//!
//! The PDF layer hands back DECODED pixels and never the original JPEG, so
//! anything Method Reader carries into a book is a re-encode. That makes the
//! bytes ours to choose, which is the whole reason this file exists rather
//! than a dependency: the same judgement that produced the hand-built
//! PalmDB container in the MOBI writer.
//!
//! Encoder and downscaler share a file because they are one unit: both are
//! pure pixel maths with nothing of the PDF in them, both are only ever
//! called back to back by images.rs, and both encode the same RGB-triple
//! contract in BPP. Splitting them so a constant can cross a module
//! boundary is the drift risk CLAUDE.md warns about for slug.ts and
//! notes.ts, pointed the wrong way.
//!
//! The IDAT stream is ours too: the TypeScript deflated with node:zlib at
//! level 9, this deflates with zlib-rs at level 6 (see [`PNG_DEFLATE_LEVEL`]),
//! and the two streams differ byte for byte while inflating to the same rows. rust/PARITY.md
//! records the decision that pictures compare by decoded pixels for exactly
//! this reason. Everything else here (signature, IHDR, the per-row filter
//! choice, IEND) is the TypeScript's, byte for byte.

use std::fmt;

/// Bytes per pixel. Truecolour 8-bit only: the one thing a picture ever
/// needs to become, once the kind conversions in images.rs have run.
const BPP: usize = 3;

const fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        #[allow(clippy::cast_possible_truncation)]
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[n] = c;
        n += 1;
    }
    t
}

static CRC_TABLE: [u32; 256] = crc_table();

/// The PNG chunk CRC (CRC-32, as the spec defines it).
pub fn crc32(buf: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in buf {
        c = CRC_TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xffff_ffff
}

/// Pixels that disagree with the dimensions they are labelled with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PngError(pub String);

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PngError {}

/// Reject pixels that disagree with the dimensions they are labelled with.
///
/// Tolerating a short buffer would still produce a well-formed PNG: the
/// filter loop reads past the end, the arithmetic lands on zero, and the
/// book ships an image whose bottom rows are solid black under a correct CRC
/// — a defect no later stage can detect and no reader can explain. A zero
/// dimension is refused too, because the spec forbids it and the file would
/// instead be rejected by a device, far from the code that made it. The kind
/// conversions upstream produce exactly `width * height * 3` bytes or give
/// up on the image, so a mismatch here is a bug, never bad input.
fn assert_rgb(rgb: &[u8], width: usize, height: usize, f: &str) -> Result<(), PngError> {
    if width < 1 || height < 1 || u32::try_from(width).is_err() || u32::try_from(height).is_err() {
        return Err(PngError(format!("{f}: bad dimensions {width}x{height}")));
    }
    let expected = width.checked_mul(height).and_then(|p| p.checked_mul(BPP));
    if expected != Some(rgb.len()) {
        return Err(PngError(format!(
            "{f}: {} bytes for {width}x{height} (expected {})",
            rgb.len(),
            expected.map_or_else(|| "too many".to_owned(), |e| e.to_string())
        )));
    }
    Ok(())
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[4..]);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

/// Filter one row under all five PNG predictors and keep the cheapest, by
/// the spec's minimum-sum-of-absolute-differences heuristic.
///
/// Worth the five passes: against the same pixels written raw the saving is
/// substantial, and entirely content-dependent — largest where the picture
/// is smooth and a gradient can be predicted away, smallest where it is
/// already noisy. A script padded with storyboards is exactly the script
/// that is already too large to email.
///
/// Returns the chosen TYPE; the filtered bytes it selected are left in
/// `cand[type]` for the caller to write out. `cand` is allocated once and
/// every one of its five buffers is fully rewritten on every row, which is
/// what makes that reuse safe. Anyone adding the natural optimisation here —
/// abandoning a type once its running score passes `best_score` — breaks
/// that invariant and leaves a partial buffer behind for a type that can
/// then never be selected. Cheap to get wrong, silent when wrong.
fn filter_row(row: &[u8], prev: &[u8], cand: &mut [Vec<u8>; 5]) -> usize {
    let mut best_type = 0;
    let mut best_score = u64::MAX;
    for (kind, buf) in cand.iter_mut().enumerate() {
        let mut score: u64 = 0;
        for i in 0..row.len() {
            // Left and up-left come from the row as the DECODER will have it,
            // which for the encoder is simply the unfiltered bytes; off the
            // edge is zero.
            let a = if i >= BPP { i32::from(row[i - BPP]) } else { 0 };
            let b = i32::from(prev[i]);
            let c = if i >= BPP {
                i32::from(prev[i - BPP])
            } else {
                0
            };
            let p = match kind {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) >> 1,
                _ => {
                    let pa = (b - c).abs();
                    let pb = (a - c).abs();
                    let pc = (a + b - 2 * c).abs();
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let v = ((i32::from(row[i]) - p) & 0xff) as u8;
            buf[i] = v;
            // Signed-magnitude sum: the spec's own heuristic.
            score += if v < 128 {
                u64::from(v)
            } else {
                256 - u64::from(v)
            };
        }
        if score < best_score {
            best_score = score;
            best_type = kind;
        }
    }
    best_type
}

/// The eight bytes every PNG starts with (PNG spec §5.2).
const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/// `IHDR` — the type of the chunk the spec requires to come first.
const IHDR_TYPE: [u8; 4] = *b"IHDR";

/// Signature (8) + the IHDR chunk's length (4) and type (4) + its 13 data
/// bytes. Its CRC is deliberately NOT counted: 29 is the exact byte at which
/// epubcheck stops calling a truncated plate corrupt, measured by truncating
/// a real one — 28 bytes is `ERROR(PKG-021)`, 29 is clean.
const PNG_HEADER_BYTES: usize = 29;

/// Does this file carry a PNG header?
///
/// A HEADER check, not a decode, and the bar is what epubcheck reads rather
/// than what a decoder would. A picture read back off disk is declared
/// `image/png` in an OPF manifest on the strength of its NAME, and
/// epubcheck opens the file: everything below is measured against a real
/// book rather than remembered.
///
/// ```text
///   a JPEG named `shot.png`         ERROR(OPF-029) + WARNING(PKG-022)
///   0 bytes (an iCloud placeholder) ERROR(MED-004) + ERROR(PKG-021)
///   8..28 bytes of a real plate     ERROR(PKG-021) corrupted image file
///   29 bytes with `IHDR` mangled    ERROR(PKG-021)
///   29 bytes of zeroed header       ERROR(PKG-021)
///   29 bytes of a real plate        clean
/// ```
///
/// The signature alone answers only the first two rows, which is why this
/// also demands the IHDR chunk and a non-zero frame: every row above that
/// epubcheck refuses, this refuses too. What it does NOT do is look past the
/// header — a 45-byte prefix of a real plate is clean here and clean in
/// epubcheck, so a file with a good header and ruined pixels is a corrupt
/// picture that both tools pass. That is a decoder's job, and a decoder is
/// not what stands between a picture folder and a book that will not
/// validate.
///
/// Nothing this engine encodes can fail it — `encode_png` writes this same
/// signature and IHDR below — so the callers that need it are the ones
/// handling a folder somebody else filled.
pub fn is_png(bytes: &[u8]) -> bool {
    if bytes.len() < PNG_HEADER_BYTES {
        return false;
    }
    if bytes[..8] != PNG_SIGNATURE {
        return false;
    }
    if bytes[12..16] != IHDR_TYPE {
        return false;
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    w > 0 && h > 0
}

/// Encode 8-bit RGB pixels as a PNG.
///
/// # Errors
/// When `rgb` is not exactly `width * height * 3` bytes, or a dimension is
/// zero or past what a PNG can state: by contract a bug in the caller.
pub fn encode_png(rgb: &[u8], width: usize, height: usize) -> Result<Vec<u8>, PngError> {
    assert_rgb(rgb, width, height, "encodePng")?;

    let stride = width * BPP;
    let mut raw = vec![0u8; (stride + 1) * height];
    let mut prev = vec![0u8; stride];
    let mut cand: [Vec<u8>; 5] = std::array::from_fn(|_| vec![0u8; stride]);

    for y in 0..height {
        let row = &rgb[y * stride..(y + 1) * stride];
        let kind = filter_row(row, &prev, &mut cand);
        let at = y * (stride + 1);
        #[allow(clippy::cast_possible_truncation)]
        {
            raw[at] = kind as u8;
        }
        raw[at + 1..at + 1 + stride].copy_from_slice(&cand[kind]);
        prev.copy_from_slice(row);
    }

    let mut ihdr = [0u8; 13];
    // Both fit: assert_rgb refused anything past u32.
    ihdr[0..4].copy_from_slice(&u32::try_from(width).unwrap_or(0).to_be_bytes());
    ihdr[4..8].copy_from_slice(&u32::try_from(height).unwrap_or(0).to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 2; // colour type: truecolour
    // 10..12 stay zero: deflate, adaptive filtering, no interlace.

    let mut out = Vec::new();
    out.extend_from_slice(&PNG_SIGNATURE);
    out.extend(chunk(b"IHDR", &ihdr));
    out.extend(chunk(b"IDAT", &deflate(&raw)));
    out.extend(chunk(b"IEND", &[]));
    Ok(out)
}

/// The deflate level for a picture's IDAT, through zlib-rs (flate2).
///
/// Measured 2026-10-11 on the real scripts, release build, the picture
/// stage alone: the one picture-heavy document took 20.0 s at miniz_oxide's
/// level 9 (the whole IMAGE_BUDGET_MS, so pictures were dropped), 10.7 s at
/// miniz's 6, 17.0 s at zlib-rs's 9 and 6.9 s at zlib-rs's 6, whose files
/// are 2.5% larger than zlib-rs's 9 there and 6% on the script with one
/// big picture. Level 1 saves only another 1.8 s (the rest is decoding and
/// downscaling) for files up to three times larger. Pictures compare by
/// decoded pixels (rust/PARITY.md, Decisions), so the level is free to be
/// the one that keeps the budget comfortably met; the budget itself is
/// never loosened.
pub const PNG_DEFLATE_LEVEL: u32 = 6;

fn deflate(raw: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(
        Vec::with_capacity(raw.len() / 2),
        flate2::Compression::new(PNG_DEFLATE_LEVEL),
    );
    // Writing into a Vec cannot fail.
    e.write_all(raw).expect("deflate into memory");
    e.finish().expect("deflate into memory")
}

/// Pixels after `downscale_rgb`: the same buffer when nothing had to move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgb {
    pub rgb: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

/// Box-filter downscale so the long edge fits `max_edge`. An image that
/// already fits comes back unchanged (the TypeScript handed back THE SAME
/// ARRAY; here it is moved back out).
///
/// A box average rather than nearest-neighbour because storyboard art is
/// full of fine pen lines, and point-sampling those onto an e-ink panel
/// produces moire that reads as a printing fault.
///
/// `max_edge` is deliberately unchecked, unlike the dimensions. It comes
/// from a module constant rather than from the file being converted, so a
/// bad value is a bug in our own caller and not something a PDF can cause —
/// which is the opposite of the mismatch `assert_rgb` guards, where the
/// input really does come from outside.
///
/// # Errors
/// As [`encode_png`], on pixels that disagree with their dimensions.
pub fn downscale_rgb(
    rgb: Vec<u8>,
    width: usize,
    height: usize,
    max_edge: usize,
) -> Result<Rgb, PngError> {
    assert_rgb(&rgb, width, height, "downscaleRgb")?;

    let longest = width.max(height);
    if longest <= max_edge {
        return Ok(Rgb { rgb, width, height });
    }

    #[allow(clippy::cast_precision_loss)]
    let scale = max_edge as f64 / longest as f64;
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let to_px = |n: usize| (crate::js::round(n as f64 * scale) as usize).max(1);
    let w = to_px(width);
    let h = to_px(height);
    let mut out = vec![0u8; w * h * BPP];

    for y in 0..h {
        // Both edges floor off the same mapping, so consecutive boxes meet
        // exactly and every source pixel lands in one of them. Rounding either
        // edge would leave a column read twice or not at all, which on flat
        // art is a stripe.
        let sy0 = (y * height) / h;
        let sy1 = (sy0 + 1).max(((y + 1) * height) / h);
        for x in 0..w {
            let sx0 = (x * width) / w;
            let sx1 = (sx0 + 1).max(((x + 1) * width) / w);
            let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let i = (sy * width + sx) * BPP;
                    r += u64::from(rgb[i]);
                    g += u64::from(rgb[i + 1]);
                    b += u64::from(rgb[i + 2]);
                    n += 1;
                }
            }
            let o = (y * w + x) * BPP;
            // Rounded, not truncated: a byte store truncates, and half a level
            // shaved off every pixel of every image is a uniform darkening.
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            let avg = |s: u64| crate::js::round(s as f64 / n as f64) as u8;
            out[o] = avg(r);
            out[o + 1] = avg(g);
            out[o + 2] = avg(b);
        }
    }
    Ok(Rgb {
        rgb: out,
        width: w,
        height: h,
    })
}

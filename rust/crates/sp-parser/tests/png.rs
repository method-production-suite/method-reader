//! Port of tests/png.test.ts.
//!
//! The helpers at the bottom (`crc32`, `raw_rows`, `filter_types`,
//! `decode_rgb`) are this file's own, ported from the TypeScript's, so the
//! encoder is never checked against itself. The IDAT is inflated with
//! miniz_oxide where the TypeScript used node:zlib; the compressed bytes are
//! not compared (rust/PARITY.md: pictures compare by decoded pixels).
//!
//! Every TypeScript test ports. In TS `toThrow()` became `is_err()`, and the
//! "comes back as the SAME array" check compares the buffer's pointer, since
//! `downscale_rgb` takes the Vec by value and moves it back out.

mod common;

use sp_parser::png::{Rgb, downscale_rgb, encode_png, is_png};

const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/// Solid-colour RGB buffer, the simplest thing with a knowable encoding.
fn solid(w: usize, h: usize, r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut px = vec![0u8; w * h * 3];
    for i in 0..w * h {
        px[i * 3] = r;
        px[i * 3 + 1] = g;
        px[i * 3 + 2] = b;
    }
    px
}

fn u32_at(buf: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn enc(px: &[u8], w: usize, h: usize) -> Vec<u8> {
    encode_png(px, w, h).expect("encodes")
}

#[allow(clippy::cast_possible_truncation)]
fn byte(n: usize) -> u8 {
    (n % 256) as u8
}

mod encode_png_tests {
    use super::*;

    #[test]
    fn writes_a_png_signature_and_an_ihdr_describing_8_bit_truecolour() {
        let png = enc(&solid(4, 3, 10, 20, 30), 4, 3);
        assert_eq!(png[0..8], PNG_MAGIC);

        assert_eq!(u32_at(&png, 8), 13); // IHDR length
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32_at(&png, 16), 4); // width
        assert_eq!(u32_at(&png, 20), 3); // height
        assert_eq!(png[24], 8); // bit depth
        assert_eq!(png[25], 2); // colour type: truecolour
        assert_eq!(png[26], 0); // compression: deflate
        assert_eq!(png[27], 0); // filter method: adaptive
        assert_eq!(png[28], 0); // interlace: none
    }

    #[test]
    fn ends_with_iend() {
        let png = enc(&solid(2, 2, 0, 0, 0), 2, 2);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn round_trips_the_exact_pixels_through_the_filters() {
        // A gradient exercises every filter branch; a solid colour would not.
        let (w, h) = (9, 7);
        let mut px = vec![0u8; w * h * 3];
        for i in 0..w * h {
            px[i * 3] = byte(i * 7);
            px[i * 3 + 1] = byte(i * 13);
            px[i * 3 + 2] = byte(i * 29);
        }
        assert_eq!(decode_rgb(&enc(&px, w, h), w, h), px);
    }

    #[test]
    fn every_chunk_carries_a_valid_crc() {
        // A wrong CRC still opens in tolerant viewers and fails epubcheck later,
        // so it is worth asserting rather than trusting.
        let png = enc(&solid(5, 5, 200, 100, 50), 5, 5);
        let mut off = 8;
        let mut seen = 0;
        while off < png.len() {
            let len = u32_at(&png, off) as usize;
            let body = &png[off + 4..off + 8 + len];
            let stored = u32_at(&png, off + 8 + len);
            assert_eq!(crc32(body), stored);
            off += 12 + len;
            seen += 1;
        }
        assert_eq!(seen, 3); // IHDR, IDAT, IEND
    }

    #[test]
    fn the_row_filters_are_live_not_a_hardcoded_none() {
        // The round-trip test passes just as well against an encoder that writes
        // every row raw, so nothing above proves a predictor ever runs. On an
        // image whose rows are identical, Up scores zero and must win outright.
        let (w, h) = (16, 5);
        let mut px = vec![0u8; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 3;
                let v = byte(x * 8);
                px[i] = v;
                px[i + 1] = v;
                px[i + 2] = v;
            }
        }
        let types = filter_types(&enc(&px, w, h), w, h);
        assert_eq!(types[1..], [2, 2, 2, 2]);
        // Row 0 has no row above it, so only that it predicted at all is stable.
        assert_ne!(types[0], 0);
    }

    #[test]
    fn chooses_paeth_where_paeth_is_the_best_predictor() {
        // A predictor that quietly stops being SELECTED leaves every other test
        // green: the bytes stay correct, they are just fatter. Nothing but the
        // filter byte shows that one of the five has dropped out of the running.
        let (px, w, h) = paeth_image();
        assert!(filter_types(&enc(&px, w, h), w, h).contains(&4));
    }

    #[test]
    fn breaks_a_paeth_tie_the_way_the_spec_breaks_it() {
        // The reconstruction below is the spec's rule written out, so the two
        // agree only while the encoder also uses `<=`. A `<` in either
        // comparison reaches for c instead of a or b and lands 80 levels off at
        // the two pixels in the four-pixel tail that actually tie (x=13 and
        // x=15; the other two resolve by strict inequality either way). That is
        // exactly the classic Paeth defect.
        let (px, w, h) = paeth_image();
        assert_eq!(decode_rgb(&enc(&px, w, h), w, h), px);
    }

    #[test]
    fn round_trips_a_1x1_a_single_row_and_a_single_column() {
        // Degenerate shapes are where an off-by-one in the stride or in the
        // `i >= BPP` guard hides: a storyboard rule or a divider is one of these.
        for (w, h) in [(1, 1), (7, 1), (1, 6)] {
            let px: Vec<u8> = (0..w * h * 3).map(|i| byte(i * 37 + 11)).collect();
            assert_eq!(decode_rgb(&enc(&px, w, h), w, h), px, "{w}x{h}");
        }
    }

    #[test]
    fn refuses_pixels_that_do_not_match_the_stated_dimensions() {
        // Silently tolerating this ships an image whose bottom rows are solid
        // black under a valid CRC, and nothing downstream can tell that from art.
        assert!(encode_png(&[0u8; 11], 2, 2).is_err());
        assert!(encode_png(&[0u8; 13], 2, 2).is_err());
        assert!(encode_png(&[], 0, 0).is_err());
        assert!(encode_png(&[], 2, 0).is_err());
    }
}

mod is_png_tests {
    use super::*;

    /// A real one to cut up, rather than a hand-built header.
    fn real() -> Vec<u8> {
        enc(&solid(4, 3, 10, 20, 30), 4, 3)
    }

    #[test]
    fn accepts_what_this_module_encodes() {
        // The pairing that matters: the predicate and the encoder read the same
        // signature and the same IHDR, so nothing we write can be refused by it.
        assert!(is_png(&real()));
    }

    #[test]
    fn refuses_the_formats_a_picture_folder_can_actually_be_filled_with() {
        for head in [
            [0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46], // JPEG
            [0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00], // GIF89a
            [0x52, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00], // RIFF/WebP
            [0x3c, 0x3f, 0x78, 0x6d, 0x6c, 0x20, 0x3f, 0x3e], // <?xml ?> (SVG)
            [0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x37], // %PDF-1.7
        ] {
            // Padded to a full header's length, or they would be refused for
            // being short and this would prove nothing about the signature.
            let mut bytes = [0u8; 64];
            bytes[..8].copy_from_slice(&head);
            assert!(!is_png(&bytes), "{head:?}");
        }
    }

    #[test]
    fn an_empty_file_is_not_one_the_icloud_placeholder_case() {
        // `bytes[i]` on a short array is undefined, not a throw, so this is a
        // statement about the answer rather than about crashing.
        assert!(!is_png(&[]));
        assert!(!is_png(&PNG_MAGIC[..7]));
    }

    #[test]
    fn refuses_every_truncation_epubcheck_calls_corrupt_and_nothing_more() {
        // The boundary is measured, not chosen: a plate cut to 28 bytes is
        // ERROR(PKG-021) in epubcheck and 29 is clean, so 29 is where this
        // flips too. A test that only checked "short is false" would pass with
        // the bound anywhere.
        let real = real();
        for n in 0..29 {
            assert!(!is_png(&real[..n]), "{n} bytes");
        }
        assert!(is_png(&real[..29]));
    }

    #[test]
    fn refuses_a_header_that_is_the_right_length_and_the_wrong_thing() {
        // Both epubcheck refusals a 29-byte file can still be: the first chunk
        // is not IHDR, or the frame it declares is empty.
        let real = real();
        let mut bad_type = real[..29].to_vec();
        bad_type[15] = 0x58; // IHDX
        assert!(!is_png(&bad_type));

        let mut zeroed = [0u8; 29];
        zeroed[..16].copy_from_slice(&real[..16]); // signature + length + IHDR
        assert!(!is_png(&zeroed)); // ...and a 0x0 frame
    }

    #[test]
    fn it_reads_the_header_only_and_says_so_by_passing_a_ruined_body() {
        // Not an oversight: a 45-byte prefix of a real plate is clean in
        // epubcheck too. Deciding a picture is beyond repair is a decoder's job,
        // and this exists to keep an invalid BOOK from being written.
        assert!(is_png(&real()[..45]));
    }

    #[test]
    fn reads_the_header_at_an_offset_not_from_the_start_of_a_pooled_buffer() {
        // In TS `readFile` hands back Buffers that are views into a shared
        // allocation, so a DataView built on `.buffer` without `.byteOffset`
        // reads somebody else's bytes. The picture sits PAST the width and
        // height fields' own offsets, so a view-blind read finds the pool's
        // zeros and refuses a perfectly good plate: a nearer offset passes
        // either way and proves nothing. In Rust a slice cannot be read
        // view-blind, but the check stays.
        let real = real();
        let mut pool = vec![0u8; 64 + real.len()];
        pool[64..].copy_from_slice(&real);
        assert!(is_png(&pool[64..]));
        // and the signature is read view-relative too
        assert!(!is_png(&pool[63..]));
    }
}

mod downscale_rgb_tests {
    use super::*;

    #[test]
    fn leaves_an_image_already_within_the_cap_untouched() {
        let px = solid(10, 8, 1, 2, 3);
        let out = downscale_rgb(px.clone(), 10, 8, 100).unwrap();
        assert_eq!(
            out,
            Rgb {
                rgb: px,
                width: 10,
                height: 8
            }
        );
    }

    #[test]
    fn an_image_exactly_at_the_cap_is_still_untouched() {
        let px = solid(100, 40, 4, 5, 6);
        let want = px.clone();
        let ptr = px.as_ptr();
        let out = downscale_rgb(px, 100, 40, 100).unwrap();
        assert_eq!(
            out,
            Rgb {
                rgb: want,
                width: 100,
                height: 40
            }
        );
        // The SAME buffer, not an equal one: the contract is that a fitting
        // image comes back as the same array. Turning that into a copy would be
        // a silent regression under a structural assertion, and megabytes of
        // needless allocation on every lookbook page.
        assert_eq!(out.rgb.as_ptr(), ptr);
    }

    #[test]
    fn fits_the_long_edge_to_the_cap_and_keeps_the_aspect_ratio() {
        let out = downscale_rgb(solid(400, 200, 9, 9, 9), 400, 200, 100).unwrap();
        assert_eq!(out.width, 100);
        assert_eq!(out.height, 50);
        assert_eq!(out.rgb.len(), 100 * 50 * 3);
    }

    #[test]
    fn fits_the_height_when_the_image_is_portrait() {
        // A full-page storyboard is taller than it is wide; capping the width
        // instead would leave it over the cap in the direction that matters.
        let out = downscale_rgb(solid(200, 400, 9, 9, 9), 200, 400, 100).unwrap();
        assert_eq!(out.width, 50);
        assert_eq!(out.height, 100);
        assert_eq!(out.rgb.len(), 50 * 100 * 3);
    }

    #[test]
    fn averages_rather_than_samples_so_a_checkerboard_greys_out() {
        // Nearest-neighbour would return pure black or pure white here. A box
        // filter returns the mean, which is the whole point: fine pen lines in a
        // storyboard must not alias into moire on an e-ink panel.
        let (w, h) = (4, 4);
        let mut px = vec![0u8; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 1 { 255 } else { 0 };
                let i = (y * w + x) * 3;
                px[i] = v;
                px[i + 1] = v;
                px[i + 2] = v;
            }
        }
        let out = downscale_rgb(px, w, h, 2).unwrap();
        assert_eq!(out.width, 2);
        for &v in &out.rgb {
            assert!(v > 100, "{v} > 100");
        }
        for &v in &out.rgb {
            assert!(v < 155, "{v} < 155");
        }
    }

    #[test]
    fn rounds_the_box_mean_rather_than_truncating_it() {
        // Truncation costs half a level on every pixel of every image we carry,
        // which is a uniform darkening, not a rounding detail.
        let px = vec![127, 127, 127, 128, 128, 128];
        assert_eq!(downscale_rgb(px, 2, 1, 1).unwrap().rgb, vec![128, 128, 128]);
    }

    #[test]
    fn tiles_the_source_exactly_when_the_ratio_is_not_a_whole_number() {
        // 7 into 3 leaves boxes of 2, 2 and 3. Both edges of a box have to come
        // off the same floored mapping: round either one and the sampling
        // interval shifts, so the means are taken over the wrong pixels — this
        // ramp reads 20, 120, 220 instead of 20, 100, 200. A flat source proves
        // none of this, because a window off by a column still averages to the
        // same value.
        let mut ramp = vec![0u8; 7 * 3];
        for i in 0..7 {
            let v = byte(i * 40);
            ramp[i * 3] = v;
            ramp[i * 3 + 1] = v;
            ramp[i * 3 + 2] = v;
        }
        let expected = vec![20, 20, 20, 100, 100, 100, 200, 200, 200];

        let across = downscale_rgb(ramp.clone(), 7, 1, 3).unwrap();
        assert_eq!((across.width, across.height), (3, 1));
        assert_eq!(across.rgb, expected);

        // The same ramp down the other axis, so a floor lost from the ROW window
        // is caught as well as one lost from the column window.
        let down = downscale_rgb(ramp, 1, 7, 3).unwrap();
        assert_eq!((down.width, down.height), (1, 3));
        assert_eq!(down.rgb, expected);
    }

    #[test]
    fn refuses_pixels_that_do_not_match_the_stated_dimensions() {
        assert!(downscale_rgb(vec![0u8; 11], 2, 2, 1).is_err());
        assert!(downscale_rgb(vec![], 0, 0, 1).is_err());
    }
}

// --- helpers -------------------------------------------------------------

/// Two rows built to make Paeth both CHOSEN and OBSERVABLE, neither of which
/// a gradient can be relied on to do.
///
/// First four pixels: the row above is flat and this row is shifted, so
/// `b == c` and Paeth predicts from the left while Up is badly wrong. Next
/// eight: this row repeats the jumpy row above it, so `a == c` and Paeth
/// predicts from above while Sub is badly wrong. Between them they make Paeth
/// the cheapest filter for the row by a wide margin, which is the only way its
/// predictor runs at all.
///
/// Last four pixels are the two triples where the spec's `<=` comparisons are
/// what decide the answer. At a=20 b=140 c=100 the distances are pa=40 pb=80
/// pc=40, so `pa <= pc` picks a=20 where a strict `<` falls through to c=100.
/// At a=140 b=20 c=100 they are pa=80 pb=40 pc=40, so `pb <= pc` picks b=20
/// where a strict `<` again reaches c=100. Both are off by 80 levels.
fn paeth_image() -> (Vec<u8>, usize, usize) {
    let above: [u8; 16] = [
        200, 200, 200, 200, 200, 60, 210, 55, 200, 60, 210, 55, 100, 140, 100, 20,
    ];
    let below: [u8; 16] = [
        50, 50, 50, 50, 200, 60, 210, 55, 200, 60, 210, 55, 20, 20, 140, 20,
    ];
    let w = above.len();
    let mut px = vec![0u8; w * 2 * 3];
    for (y, row) in [above, below].iter().enumerate() {
        for (x, &v) in row.iter().enumerate() {
            let i = (y * w + x) * 3;
            px[i] = v;
            px[i + 1] = v;
            px[i + 2] = v;
        }
    }
    (px, w, 2)
}

/// Bitwise CRC-32, independent of the encoder's table-driven one.
fn crc32(buf: &[u8]) -> u32 {
    let mut c: u32 = 0xffff_ffff;
    for &b in buf {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb8_8320 & (c & 1).wrapping_neg());
        }
    }
    c ^ 0xffff_ffff
}

/// The concatenated IDAT payload, inflated: one filtered row per line.
fn raw_rows(png: &[u8]) -> Vec<u8> {
    let mut off = 8;
    let mut idat = Vec::new();
    while off < png.len() {
        let len = u32_at(png, off) as usize;
        if &png[off + 4..off + 8] == b"IDAT" {
            idat.extend_from_slice(&png[off + 8..off + 8 + len]);
        }
        off += 12 + len;
    }
    miniz_oxide::inflate::decompress_to_vec_zlib(&idat).expect("IDAT inflates")
}

/// The filter byte that leads each row.
fn filter_types(png: &[u8], w: usize, h: usize) -> Vec<u8> {
    let raw = raw_rows(png);
    let stride = w * 3;
    (0..h).map(|y| raw[y * (stride + 1)]).collect()
}

/// Minimal PNG reader: enough to prove the encoder round-trips. Test-only.
fn decode_rgb(png: &[u8], w: usize, h: usize) -> Vec<u8> {
    let raw = raw_rows(png);
    let stride = w * 3;
    let mut out = vec![0u8; stride * h];
    let mut prev = vec![0u8; stride];
    for y in 0..h {
        let kind = raw[y * (stride + 1)];
        let row = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for i in 0..stride {
            let a = if i >= 3 {
                i32::from(out[y * stride + i - 3])
            } else {
                0
            };
            let b = i32::from(prev[i]);
            let c = if i >= 3 { i32::from(prev[i - 3]) } else { 0 };
            let p = match kind {
                1 => a,
                2 => b,
                3 => (a + b) >> 1,
                4 => {
                    let (pa, pb, pc) = ((b - c).abs(), (a - c).abs(), (a + b - 2 * c).abs());
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                _ => 0,
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                out[y * stride + i] = ((i32::from(row[i]) + p) & 0xff) as u8;
            }
        }
        prev.copy_from_slice(&out[y * stride..(y + 1) * stride]);
    }
    out
}

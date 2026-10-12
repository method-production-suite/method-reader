//! Port of tests/jpx-fixture.test.ts's "a JPEG2000 picture through the whole
//! parser", which is also the keep-worthy half of tests/pdfjs-wasm.test.ts:
//! JPEG2000 decodes, and decodes to the pixels the fixture drew.
//!
//! facts.rs already holds every committed fixture's pixels to pdf.js's, byte
//! for byte. This file checks the same picture against the fixture's AUTHOR
//! instead (tools/make-fixture.py's `rgb_gradient`, transcribed), so a
//! decoder and an oracle that agreed on a wrong answer would still fail.
//!
//! RETIRED, with the reason each: pdf.js keeps its JPX, JBIG2 and CCITT
//! decoders outside its bundle and loads them from a folder written at
//! startup; sp-pdf's decoders are compiled in, so there is no folder. "the
//! decoder directory" (the three filenames, materializing them, the trailing
//! slash, null when no directory can be made), "when the decoders cannot be
//! unpacked" (the CLI run with TMPDIR a file, and its warning), "a decoder
//! folder removed between documents", and all of pdfjs-wasm.test.ts's folder,
//! URL and exit-hook checks guard that folder and nothing else.
//! `imageStats.decodersUnavailable` is never set, which the third test below
//! still checks.

use sp_parser::extract::extract_document;
use sp_parser::images::{collect_image_placements, to_rgb};

// make-fixture.py's JPX_GRADIENT.
const JPX_W: usize = 64;
const JPX_H: usize = 48;
const JPX_SEED: usize = 71;

fn fixture() -> Vec<u8> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/jpx.pdf");
    std::fs::read(path).expect("jpx.pdf")
}

/// tools/make-fixture.py's `rgb_gradient`, transcribed: eight-pixel blocks
/// varying in both axes, so a transposed or reversed row shows.
fn rgb_gradient(w: usize, h: usize, seed: usize) -> Vec<u8> {
    let mut px = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            px.push(u8::try_from(((x / 8) * 37 + seed) % 256).unwrap());
            px.push(u8::try_from(((y / 8) * 53 + seed) % 256).unwrap());
            px.push(u8::try_from((seed * 3) % 256).unwrap());
        }
    }
    px
}

/// The dimensions a PNG declares in its IHDR.
fn png_size(b: &[u8]) -> [u32; 2] {
    [
        u32::from_be_bytes(b[16..20].try_into().unwrap()),
        u32::from_be_bytes(b[20..24].try_into().unwrap()),
    ]
}

#[test]
fn reaches_the_book_instead_of_vanishing() {
    let out = extract_document(&sp_pdf::read(fixture()).unwrap(), None, None);
    assert_eq!(out.images.len(), 1);
    assert_eq!(out.image_stats.failed, 0);
}

#[test]
fn at_the_resolution_the_codestream_declares() {
    let out = extract_document(&sp_pdf::read(fixture()).unwrap(), None, None);
    assert_eq!(png_size(&out.images[0].bytes), [JPX_W as u32, JPX_H as u32]);
}

#[test]
fn and_reports_the_decoders_as_available() {
    let out = extract_document(&sp_pdf::read(fixture()).unwrap(), None, None);
    assert_eq!(out.image_stats.decoders_unavailable, None);
}

#[test]
fn with_the_pixels_the_fixture_drew_byte_for_byte() {
    // The pixels as the PDF layer hands them over, before the PNG encoder:
    // this is about the DECODER. Lossless, so an equality, not a tolerance.
    let facts = sp_pdf::read(fixture()).unwrap();
    let mut decoded = None;
    for page in &facts.pages {
        for place in collect_image_placements(page.ops.as_deref().unwrap_or_default()) {
            if decoded.is_some() {
                break;
            }
            if let Some(Some(img)) = page.images.get(&place.obj_id) {
                decoded = to_rgb(img.width, img.height, img.kind, img.data.as_deref());
            }
        }
    }
    let decoded = decoded.expect("a decoded picture");
    assert!(decoded == rgb_gradient(JPX_W, JPX_H, JPX_SEED));
}

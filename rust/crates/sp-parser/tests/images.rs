//! Port of tests/images.test.ts.
//!
//! images.rs against STUBS. Everything that needs a document the PDF layer
//! actually opened lives in images_fixture.rs, which also owns extract.rs's
//! page loop: a stub answers however it is written, and a regression in the
//! loop should not report against a file named for the parser module.
//!
//! The TypeScript stubbed pdf.js's object stores (`objs.get(k, cb)`); here a
//! page's pixels are `PageFacts::images`, so "a store that answers" is a key
//! holding `Some(ImageFact { data: Some(..) })`, "answers with nothing" is a
//! key holding `None`, and "never answers" is the key absent.
//!
//! RETIRED: the whole `resolveImageData` describe (10 tests): "uses the async
//! form, because objs.has() lies about pending images", "falls back to
//! commonObjs, where pdf.js parks a REPEATED image", "prefers the page store
//! when both have the key", "a second answer never overwrites the first",
//! "settles on a callback that fires synchronously", "tolerates a store that
//! calls back twice", "gives up after the timeout instead of hanging the
//! conversion", "an answer that arrives after the timeout is ignored, not
//! thrown", "survives a store that throws", "still finds the image when only
//! the fallback store throws". pdf.js object-store races and timeouts: the
//! Rust port receives pixels already resolved and has no counterpart.
//!
//! RETIRED: "survives a picture whose pixels disagree with its dimensions":
//! its input is a buffer that lies about its own length to reach png.ts's
//! assertRgb throw. A Rust slice cannot misreport its length, so `to_rgb`
//! always hands the encoder exactly width x height x 3 bytes and the
//! encoder's Err branch is unreachable from this side.
//!
//! RETIRED (one assertion): in "charges the document budget, and caps the
//! wait at what is left of it", `remainingMs <= 0` measured the store WAIT
//! being charged. There is no wait in Rust (a picture that never arrived is
//! absent from the facts), so only the encode is charged; the rest of that
//! test is ported.
//!
//! RETIRED (one assertion): "is best-effort: a malformed list yields no
//! placements, never a throw" also fed `null as never` as the whole list; the
//! type `&[Op]` makes that unrepresentable. The malformed-paint half is ported.

mod common;

use std::collections::BTreeMap;
use std::time::Instant;

use sp_model::RawImage;
use sp_parser::artwork::ImageCandidate;
use sp_parser::facts::{ImageFact, Op, PageFacts};
use sp_parser::images::{
    EncodedCandidate, IMAGE_BUDGET_MS, ImagePlacement, PageSize, collect_image_placements,
    collect_page_images, dominant_page_size, finish_images, is_generated_image_file, new_image_run,
    to_raw_images, to_rgb,
};

/// The dimensions a PNG declares in its IHDR (bytes 16..24). Read off the
/// bytes that actually ship, since `RawImage` no longer carries them beside
/// the file: this asserts the encoder's output rather than a field copied
/// from it, which is the stronger of the two.
fn png_size(b: &[u8]) -> [u32; 2] {
    [
        u32::from_be_bytes(b[16..20].try_into().unwrap()),
        u32::from_be_bytes(b[20..24].try_into().unwrap()),
    ]
}

/// `toBeCloseTo(expected, digits)`.
fn close_to(actual: f64, expected: f64, digits: i32) -> bool {
    (actual - expected).abs() < 10f64.powi(-digits) / 2.0
}

fn q() -> Op {
    Op::Save
}

#[allow(non_snake_case)]
fn Q() -> Op {
    Op::Restore
}

fn cm(m: [f64; 6]) -> Op {
    Op::Transform { matrix: m.to_vec() }
}

fn do_(name: &str, px: f64, py: f64) -> Op {
    Op::PaintImageXObject {
        obj_id: Some(name.into()),
        width: Some(px),
        height: Some(py),
    }
}

/// `Do` with the TypeScript's default 8x8 source pixels.
fn do8(name: &str) -> Op {
    do_(name, 8.0, 8.0)
}

/// `Do` on a form XObject: pdf.js carries the form's /Matrix in the begin op.
fn form_begin(matrix: Option<[f64; 6]>) -> Op {
    Op::PaintFormXObjectBegin {
        matrix: matrix.map(|m| m.to_vec()),
    }
}

fn form_end() -> Op {
    Op::PaintFormXObjectEnd
}

/// The `q W 0 0 H X Y cm /Im Do Q` pdf.js emits for each placed image.
fn op_list(entries: &[(&str, [f64; 6], [f64; 2])]) -> Vec<Op> {
    entries
        .iter()
        .flat_map(|(name, ctm, px)| [q(), cm(*ctm), do_(name, px[0], px[1]), Q()])
        .collect()
}

fn placement(obj_id: &str, x0: f64, y0: f64, x1: f64, y1: f64, w: f64, h: f64) -> ImagePlacement {
    ImagePlacement {
        obj_id: obj_id.into(),
        x0,
        y0,
        x1,
        y1,
        src_px_width: w,
        src_px_height: h,
    }
}

mod collect_image_placements {
    use super::*;

    #[test]
    fn reads_position_and_rendered_size_from_the_ctm() {
        let list = op_list(&[(
            "img_p0_1",
            [144.0, 0.0, 0.0, 108.0, 108.0, 540.0],
            [240.0, 180.0],
        )]);
        assert_eq!(
            collect_image_placements(&list),
            vec![placement(
                "img_p0_1", 108.0, 540.0, 252.0, 648.0, 240.0, 180.0
            )]
        );
    }

    #[test]
    fn handles_a_y_flipped_placement() {
        // Final Draft draws under a flip. Transforming one corner would put the
        // box below the page; all four corners plus min/max is what fixes it.
        let list = op_list(&[("a", [100.0, 0.0, 0.0, -80.0, 50.0, 400.0], [10.0, 10.0])]);
        let p = &collect_image_placements(&list)[0];
        assert_eq!(p.y0, 320.0);
        assert_eq!(p.y1, 400.0);
        assert_eq!(p.x0, 50.0);
        assert_eq!(p.x1, 150.0);
    }

    #[test]
    fn restores_the_matrix_after_q_so_a_later_image_is_not_offset() {
        let list = op_list(&[
            ("a", [10.0, 0.0, 0.0, 10.0, 500.0, 500.0], [4.0, 4.0]),
            ("b", [20.0, 0.0, 0.0, 20.0, 5.0, 5.0], [4.0, 4.0]),
        ]);
        let got = collect_image_placements(&list);
        let b = &got[1];
        assert_eq!(b.x0, 5.0);
        assert_eq!(b.y0, 5.0);
    }

    #[test]
    fn composes_an_inner_cm_inside_the_outer_one_not_beside_it() {
        // The whole concat: a translate under a 2x scale moves 2x as far.
        // Transpose the multiply and this lands at (10, 20) — a plausible
        // box, off the art.
        let list = vec![
            q(),
            cm([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]),
            cm([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]),
            do8("a"),
            Q(),
        ];
        let p = &collect_image_placements(&list)[0];
        assert_eq!([p.x0, p.y0, p.x1, p.y1], [20.0, 40.0, 22.0, 42.0]);
    }

    #[test]
    fn a_nested_q_q_leaves_the_outer_matrix_in_force() {
        let list = vec![
            q(),
            cm([2.0, 0.0, 0.0, 2.0, 100.0, 100.0]),
            q(),
            cm([1.0, 0.0, 0.0, 1.0, 5.0, 5.0]),
            do8("inner"),
            Q(),
            do8("outer"),
            Q(),
        ];
        let got = collect_image_placements(&list);
        let (inner, outer) = (&got[0], &got[1]);
        assert_eq!([inner.x0, inner.y0], [110.0, 110.0]);
        assert_eq!([outer.x0, outer.y0], [100.0, 100.0]);
    }

    #[test]
    fn two_images_in_one_q_the_second_cm_composes_on_the_first() {
        let list = vec![
            q(),
            cm([1.0, 0.0, 0.0, 1.0, 10.0, 10.0]),
            do8("a"),
            cm([1.0, 0.0, 0.0, 1.0, 10.0, 10.0]),
            do8("b"),
            Q(),
        ];
        let got = collect_image_placements(&list);
        let (a, b) = (&got[0], &got[1]);
        assert_eq!([a.x0, a.y0], [10.0, 10.0]);
        assert_eq!([b.x0, b.y0], [20.0, 20.0]);
    }

    #[test]
    fn an_image_drawn_with_no_cm_at_all_sits_on_the_unit_square() {
        let list = vec![q(), do8("a"), Q()];
        assert_eq!(
            collect_image_placements(&list),
            vec![placement("a", 0.0, 0.0, 1.0, 1.0, 8.0, 8.0)]
        );
    }

    #[test]
    fn a_rotated_placement_reports_the_page_region_it_covers() {
        // Quarter turn: the box is axis-aligned and its aspect is the
        // transpose of the source's. Reported rather than skipped, because a
        // turned storyboard panel still occupies that region and still has to
        // go somewhere.
        let list = vec![
            q(),
            cm([0.0, 100.0, -50.0, 0.0, 200.0, 300.0]),
            do8("a"),
            Q(),
        ];
        assert_eq!(
            collect_image_placements(&list),
            vec![placement("a", 150.0, 300.0, 200.0, 400.0, 8.0, 8.0)]
        );
    }

    #[test]
    fn a_slanted_placement_needs_all_four_corners_not_the_diagonal() {
        // A 3-4-5 turn: the (0,0)-(1,1) diagonal spans x 80..100 while the
        // picture spans 20..160. Flips and quarter turns forgive mapping two
        // corners; this is the placement that does not.
        let list = vec![
            q(),
            cm([60.0, 80.0, -80.0, 60.0, 100.0, 100.0]),
            do8("a"),
            Q(),
        ];
        let p = &collect_image_placements(&list)[0];
        assert_eq!([p.x0, p.x1], [20.0, 160.0]);
        assert_eq!([p.y0, p.y1], [100.0, 240.0]);
    }

    #[test]
    fn a_zero_scale_placement_is_reported_degenerate_box_and_all() {
        // Geometry only: a collapsed box is a fact about the page, and
        // dropping it here would hide it from the filters that own that
        // judgement.
        let list = vec![q(), cm([0.0, 0.0, 0.0, 40.0, 72.0, 500.0]), do8("a"), Q()];
        let p = &collect_image_placements(&list)[0];
        assert_eq!(p.x0, 72.0);
        assert_eq!(p.x1, 72.0);
        assert_eq!(p.y1 - p.y0, 40.0);
    }

    #[test]
    fn an_unbalanced_q_drops_the_matrix_rather_than_carrying_it_forward() {
        // The stray Q lands AFTER the cm on purpose. With the matrix still at
        // identity the two candidate behaviours are indistinguishable and this
        // test cannot fail; here `?? IDENTITY` puts the image at the origin
        // and `?? m` leaves it at 3x on (9, 9).
        //
        // Which is a real judgement call, because we DIVERGE from pdf.js here:
        // its renderer makes restore() on an empty stack a no-op that keeps
        // the CTM (pdf.mjs, `CanvasGraphics.restore`). extract's
        // collectUnderlineMarks chose otherwise and so does this walk. A
        // viewer has to show something for a corrupt content stream; a
        // best-effort extractor does not, and would rather park one image at
        // the origin, where every size floor downstream catches it, than carry
        // a matrix it no longer trusts across every image left on the page.
        let list = vec![Q(), cm([3.0, 0.0, 0.0, 3.0, 9.0, 9.0]), Q(), do8("b")];
        let b = &collect_image_placements(&list)[0];
        assert_eq!([b.x0, b.y0, b.x1, b.y1], [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn a_form_xobject_transforms_and_restores_like_q_cm_q() {
        // pdf.js does not expand a form into save/transform ops: the /Matrix
        // rides on paintFormXObjectBegin, and the canvas renderer saves,
        // applies it, and restores on End. Ignoring the pair puts placed art
        // (Illustrator, InDesign) at the wrong offset AND leaks the form's own
        // transforms into the page.
        let list = vec![
            q(),
            cm([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]),
            form_begin(Some([1.0, 0.0, 0.0, 1.0, 10.0, 20.0])),
            do8("inside"),
            form_end(),
            do8("after"),
            Q(),
        ];
        let got = collect_image_placements(&list);
        let (inside, after) = (&got[0], &got[1]);
        assert_eq!([inside.x0, inside.y0], [20.0, 40.0]);
        assert_eq!([after.x0, after.y0], [0.0, 0.0]);
    }

    #[test]
    fn a_form_with_no_matrix_still_saves_and_restores() {
        let list = vec![
            q(),
            form_begin(None),
            cm([1.0, 0.0, 0.0, 1.0, 30.0, 30.0]),
            do8("inside"),
            form_end(),
            do8("after"),
            Q(),
        ];
        let got = collect_image_placements(&list);
        let (inside, after) = (&got[0], &got[1]);
        assert_eq!([inside.x0, inside.y0], [30.0, 30.0]);
        assert_eq!([after.x0, after.y0], [0.0, 0.0]);
    }

    #[test]
    fn ignores_images_inside_an_annotation_which_are_markup_on_the_script() {
        let list = vec![
            Op::BeginAnnotation,
            q(),
            cm([60.0, 0.0, 0.0, 30.0, 0.0, 0.0]),
            do8("stamp"),
            Q(),
            Op::EndAnnotation,
            q(),
            cm([60.0, 0.0, 0.0, 30.0, 72.0, 400.0]),
            do8("art"),
            Q(),
        ];
        let ids: Vec<String> = collect_image_placements(&list)
            .into_iter()
            .map(|p| p.obj_id)
            .collect();
        assert_eq!(ids, vec!["art"]);
    }

    #[test]
    fn skips_a_paint_whose_pixel_dimensions_are_not_numbers() {
        let list = vec![
            Op::PaintImageXObject {
                obj_id: Some("bad".into()),
                width: None,
                height: Some(12.0),
            },
            Op::PaintImageXObject {
                obj_id: Some("nan".into()),
                width: Some(f64::NAN),
                height: Some(12.0),
            },
            do8("good"),
        ];
        let ids: Vec<String> = collect_image_placements(&list)
            .into_iter()
            .map(|p| p.obj_id)
            .collect();
        assert_eq!(ids, vec!["good"]);
    }

    #[test]
    fn is_best_effort_a_malformed_list_yields_no_placements_never_a_throw() {
        // TS `argsArray: [null]`: a paint carrying none of its arguments.
        let list = vec![Op::PaintImageXObject {
            obj_id: None,
            width: None,
            height: None,
        }];
        assert_eq!(collect_image_placements(&list), vec![]);
        // `null as never` for the whole list has no Rust spelling; the empty
        // list is the nearest input the type admits.
        assert_eq!(collect_image_placements(&[]), vec![]);
    }

    #[test]
    fn ignores_image_masks_which_in_screenplays_are_decoration() {
        // In the TypeScript the mask op carried pdf.js's real argument shape
        // so that adding mask support would fail here. The facts layer does
        // not model paintImageMaskXObject at all: it arrives as `Op::Other`,
        // which is the shape this pins.
        let list = vec![q(), cm([9.0, 0.0, 0.0, 9.0, 72.0, 700.0]), Op::Other, Q()];
        assert_eq!(collect_image_placements(&list), vec![]);
    }
}

mod to_rgb {
    use super::*;

    #[test]
    fn passes_rgb_24bpp_through_untouched() {
        let data = [1u8, 2, 3, 4, 5, 6];
        assert_eq!(
            to_rgb(2.0, 1.0, 2, Some(&data)).unwrap(),
            vec![1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn reads_the_uint8array_pdf_js_actually_sends_not_only_a_clamped_one() {
        // Every image in tests/fixtures/images.pdf arrives as a plain
        // Uint8Array: the direct DeviceRGB path in `PDFImage.createImageData`
        // hands back the decode stream's own buffer. In Rust there is one
        // byte-slice type, so this pins the same pass-through on it.
        let data = [9u8, 8, 7];
        assert_eq!(to_rgb(1.0, 1.0, 2, Some(&data)).unwrap(), vec![9, 8, 7]);
    }

    #[test]
    fn hands_back_bytes_we_own_not_a_view_into_pdf_js_store() {
        // downscaleRgb returns THE SAME ARRAY for an image that already fits,
        // so a view handed out here would reach encodePng still pointing at
        // the pixels pdf.js is caching: one buffer, two owners, and one of
        // them is a library.
        let data = [1u8, 2, 3];
        let mut rgb: Vec<u8> = to_rgb(1.0, 1.0, 2, Some(&data)).unwrap();
        rgb[0] = 200;
        assert_eq!(data[0], 1);
        assert_eq!(rgb[0], 200);
    }

    #[test]
    fn flattens_rgba_32bpp_onto_white_the_page_a_screenplay_is_read_on() {
        // The design's "Out of scope": transparent images are flattened onto
        // white. Two pixels, transparent then opaque. Dropping alpha instead
        // ships the first pixel as whatever RGB sat underneath it, so a logo
        // cut out over a dark backing arrives as art in a black rectangle.
        let data = [10u8, 20, 30, 0, 40, 50, 60, 255];
        assert_eq!(
            to_rgb(2.0, 1.0, 3, Some(&data)).unwrap(),
            vec![255, 255, 255, 40, 50, 60]
        );
    }

    #[test]
    fn leaves_a_fully_opaque_rgba_pixel_exactly_as_it_was() {
        // The blend has to be an identity at a=255, not merely close to one:
        // a rounding slip here tints every photograph in the book.
        let data = [1u8, 128, 254, 255];
        assert_eq!(to_rgb(1.0, 1.0, 3, Some(&data)).unwrap(), vec![1, 128, 254]);
    }

    #[test]
    fn blends_a_partly_transparent_pixel_instead_of_stepping_to_one_end() {
        // round(c * a/255 + 255 * (1 - a/255)) at half alpha. A threshold —
        // pick white under 128 and the colour above — passes both tests above
        // and turns every soft edge in a storyboard into a jagged one.
        let data = [10u8, 20, 30, 128];
        assert_eq!(
            to_rgb(1.0, 1.0, 3, Some(&data)).unwrap(),
            vec![132, 137, 142]
        );
    }

    #[test]
    fn rounds_the_blend_rather_than_letting_the_store_truncate_it() {
        // Deliberate values: at half alpha every channel here lands just past
        // .5, where rounding and truncation disagree. The obvious midpoint
        // colours do NOT — 10/20/30 all truncate to what they round to, so a
        // test written on them leaves the rounding unpinned. png's
        // downscale_rgb rounds for the same reason: a byte store truncates,
        // and half a level shaved off every pixel of every image is a uniform
        // darkening.
        let data = [1u8, 3, 5, 128];
        assert_eq!(
            to_rgb(1.0, 1.0, 3, Some(&data)).unwrap(),
            vec![128, 129, 130]
        );
    }

    #[test]
    fn expands_grayscale_1bpp_which_is_packed_bits_and_row_padded() {
        // One row of 9 pixels: 101010101, padded to two bytes. A naive reader
        // that ignores the row padding smears every row after the first.
        let data = [0b1010_1010u8, 0b1000_0000];
        let rgb = to_rgb(9.0, 1.0, 1, Some(&data)).unwrap();
        assert_eq!(rgb.len(), 9 * 3);
        assert_eq!([rgb[0], rgb[1], rgb[2]], [255, 255, 255]);
        assert_eq!([rgb[3], rgb[4], rgb[5]], [0, 0, 0]);
        assert_eq!([rgb[24], rgb[25], rgb[26]], [255, 255, 255]);
    }

    #[test]
    fn restarts_each_1bpp_row_on_a_byte_boundary_so_rows_do_not_shear() {
        // The single-row test above cannot fail on a padding bug. Three rows
        // of 3 pixels, one byte each: white-black-black, black-white-black,
        // black-black-white. Read as a continuous bit stream instead, rows 2
        // and 3 slide left by 5 and 10 bits and the diagonal comes out as a
        // smear.
        let data = [0b1000_0000u8, 0b0100_0000, 0b0010_0000];
        let rgb = to_rgb(3.0, 3.0, 1, Some(&data)).unwrap();
        let white: Vec<usize> = (0..9).filter(|p| rgb[p * 3] == 255).collect();
        assert_eq!(white, vec![0, 4, 8]);
    }

    #[test]
    fn a_1bpp_width_that_is_a_multiple_of_8_has_no_padding_to_skip() {
        // The remainder branch is the one that advances past the pad byte; on
        // a width of 8 it must not run, or every row after the first is off
        // by one.
        let data = [0b1111_1111u8, 0b0000_0000];
        let rgb = to_rgb(8.0, 2.0, 1, Some(&data)).unwrap();
        assert_eq!(rgb.len(), 8 * 2 * 3);
        assert_eq!(rgb[0], 255);
        assert_eq!(rgb[23], 255);
        assert_eq!(rgb[24], 0);
        assert_eq!(rgb[47], 0);
    }

    #[test]
    fn refuses_a_buffer_too_short_for_its_declared_size() {
        assert_eq!(to_rgb(100.0, 100.0, 2, Some(&[0u8; 9])), None);
    }

    #[test]
    fn accepts_a_buffer_of_exactly_the_declared_size_and_refuses_one_byte_less() {
        // Not hypothetical: a truncated image stream reaches us with correct
        // width/height and a short buffer, because pdf.js caps the read at
        // what actually decoded (`if (end > bufEnd) end = bufEnd`,
        // pdf.worker.mjs DecodeStream.getBytes). One byte either side of the
        // boundary is where an off-by-one in the guard lives.
        assert_eq!(to_rgb(2.0, 2.0, 2, Some(&[0u8; 12])).unwrap().len(), 12);
        assert_eq!(to_rgb(2.0, 2.0, 2, Some(&[0u8; 11])), None);
        assert_eq!(to_rgb(2.0, 2.0, 3, Some(&[0u8; 16])).unwrap().len(), 12);
        assert_eq!(to_rgb(2.0, 2.0, 3, Some(&[0u8; 15])), None);
        assert_eq!(to_rgb(9.0, 2.0, 1, Some(&[0u8; 4])).unwrap().len(), 54);
        assert_eq!(to_rgb(9.0, 2.0, 1, Some(&[0u8; 3])), None);
    }

    #[test]
    fn refuses_an_unknown_kind_rather_than_guessing() {
        assert_eq!(to_rgb(1.0, 1.0, 99, Some(&[0u8; 3])), None);
    }

    #[test]
    fn refuses_a_zero_dimension() {
        assert_eq!(to_rgb(0.0, 4.0, 2, Some(&[0u8; 12])), None);
        assert_eq!(to_rgb(4.0, 0.0, 2, Some(&[0u8; 12])), None);
    }

    #[test]
    fn refuses_a_fractional_dimension_which_nothing_downstream_would_catch() {
        // NOT a throw, which is the whole reason the guard has to be here.
        // Loosened to "finite" this returns 15 well-formed bytes labelled
        // 2.5x2, and png's assert_rgb errors on them a stage later, having
        // documented that a mismatch there can only be our own bug. pdf.js
        // supplies the input: outside the branch that prefers the decoded
        // image's own dimensions, the `PDFImage` constructor checks the dict
        // as `typeof width === "number" && width > 0`.
        assert_eq!(to_rgb(2.5, 2.0, 2, Some(&[0u8; 64])), None);
        assert_eq!(to_rgb(2.0, f64::NAN, 2, Some(&[0u8; 64])), None);
    }

    #[test]
    fn refuses_the_bitmap_shape_where_pdf_js_sends_no_pixels_at_all() {
        // `{ data: null, bitmap }` is what `PDFImage.createImageData` returns
        // when it has an ImageBitmap. The field is typed for what the PDF
        // layer actually returns, so the guard is visibly load-bearing rather
        // than visibly dead.
        assert_eq!(to_rgb(4.0, 4.0, 2, None), None);
    }
}

mod collect_page_images {
    use super::*;

    /// Distinct pixels per seed, so two pictures do not hash alike by
    /// accident.
    fn pixels(seed: usize, w: usize, h: usize) -> Option<ImageFact> {
        let data: Vec<u8> = (0..w * h * 3)
            .map(|i| u8::try_from((i * 7 + seed) % 256).unwrap())
            .collect();
        #[allow(clippy::cast_precision_loss)]
        Some(ImageFact {
            width: w as f64,
            height: h as f64,
            kind: 2,
            data_length: Some(data.len()),
            data_sha256: None,
            data: Some(data),
        })
    }

    fn px4(seed: usize) -> Option<ImageFact> {
        pixels(seed, 4, 4)
    }

    /// A page whose pixel store holds `answers`. A key holding `None` is a
    /// store that answered with nothing; an absent key one that never did.
    fn page(answers: Vec<(&str, Option<ImageFact>)>) -> PageFacts {
        PageFacts {
            page_num: 1,
            width: 612.0,
            height: 792.0,
            items: vec![],
            fonts: BTreeMap::new(),
            ops: None,
            images: answers
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

    fn at(name: &str, x: f64, y: f64) -> Vec<Op> {
        vec![
            q(),
            cm([40.0, 0.0, 0.0, 40.0, x, y]),
            do_(name, 4.0, 4.0),
            Q(),
        ]
    }

    #[test]
    fn encodes_a_png_and_carries_the_placement_geometry_onto_the_candidate() {
        let s = page(vec![("a", px4(1))]);
        let mut run = new_image_run();
        let got = collect_page_images(&s, Some(&at("a", 72.0, 500.0)), 3, 41, &mut run);
        let c = &got[0];
        assert_eq!(
            &c.bytes[..8],
            &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
        );
        let k = &c.candidate;
        assert_eq!([k.x0, k.y0, k.x1, k.y1], [72.0, 500.0, 112.0, 540.0]);
        assert_eq!([k.src_px_width, k.src_px_height], [4.0, 4.0]);
        // The ENCODED size, read out of the PNG that ships rather than off a
        // field beside it. Equal to the source here because 4px is far under
        // IMAGE_MAX_EDGE, so the downscaler passed it through.
        assert_eq!(png_size(&c.bytes), [4, 4]);
        // The two numbers a caller can transpose without the compiler
        // noticing. page_text_lines is the background rule's whole
        // discriminator, and page_num is what the furniture rule counts
        // distinct values of.
        assert_eq!(k.page_num, 3);
        assert_eq!(k.page_text_lines, 41);
        assert_eq!(run.failed, 0);
    }

    #[test]
    fn gives_identical_pixels_an_identical_hash_whatever_they_are_named() {
        // The furniture rule's premise: a logo is frequently a different
        // XObject per sheet, and identical PIXELS are the fact worth acting
        // on.
        let s = page(vec![
            ("img_p0_2", px4(9)),
            ("g_d0_img_p1_2", px4(9)),
            ("other", px4(11)),
        ]);
        let list = vec![
            q(),
            cm([40.0, 0.0, 0.0, 40.0, 0.0, 0.0]),
            do_("img_p0_2", 4.0, 4.0),
            Q(),
            q(),
            cm([40.0, 0.0, 0.0, 40.0, 0.0, 60.0]),
            do_("g_d0_img_p1_2", 4.0, 4.0),
            Q(),
            q(),
            cm([40.0, 0.0, 0.0, 40.0, 0.0, 120.0]),
            do_("other", 4.0, 4.0),
            Q(),
        ];
        let got = collect_page_images(&s, Some(&list), 1, 0, &mut new_image_run());
        let (a, b, c) = (&got[0], &got[1], &got[2]);
        assert_eq!(a.candidate.hash, b.candidate.hash);
        assert_ne!(a.candidate.hash, c.candidate.hash);
    }

    #[test]
    fn encodes_a_repeated_picture_once_and_shares_the_bytes() {
        // Page furniture is by definition on every sheet, and encoding is the
        // expensive half: a 600x300 letterhead costs ~10ms a sheet to
        // deflate, which is a second of pure waste on a hundred-page script.
        // A cache hit skips the resolve as well. The TypeScript counted the
        // store's lookups (exactly one); here the second page's store does not
        // hold the picture at all, so it can only be placed from the cache.
        let mut run = new_image_run();
        let one = collect_page_images(
            &page(vec![("logo", px4(3))]),
            Some(&at("logo", 0.0, 0.0)),
            1,
            5,
            &mut run,
        );
        let two = collect_page_images(&page(vec![]), Some(&at("logo", 0.0, 0.0)), 2, 5, &mut run);
        assert_eq!(run.encoded.len(), 1);
        assert_eq!(two[0].bytes, one[0].bytes);
        assert_eq!(two[0].candidate.page_num, 2);
        assert_eq!(run.failed, 0);
    }

    #[test]
    fn counts_an_image_with_no_pixels_behind_it_and_carries_on() {
        // Answered, but with nothing. What this test is about is that one
        // lost picture does not end the page.
        let s = page(vec![("nothing", None), ("good", px4(2))]);
        let mut run = new_image_run();
        let list = vec![
            q(),
            cm([40.0, 0.0, 0.0, 40.0, 0.0, 0.0]),
            do_("nothing", 4.0, 4.0),
            Q(),
            q(),
            cm([40.0, 0.0, 0.0, 40.0, 0.0, 60.0]),
            do_("good", 4.0, 4.0),
            Q(),
        ];
        let got = collect_page_images(&s, Some(&list), 1, 0, &mut run);
        assert_eq!(got.len(), 1);
        assert_eq!(run.failed, 1);
    }

    #[test]
    fn counts_an_image_whose_pixels_cannot_be_converted() {
        // The bitmap shape: pdf.js answers with an ImageBitmap and no bytes.
        // If that path opens EVERY image takes it at once — which is the loss
        // this count makes visible.
        let s = page(vec![(
            "bitmap",
            Some(ImageFact {
                width: 4.0,
                height: 4.0,
                kind: 2,
                data_length: None,
                data_sha256: None,
                data: None,
            }),
        )]);
        let mut run = new_image_run();
        assert_eq!(
            collect_page_images(&s, Some(&at("bitmap", 0.0, 0.0)), 1, 0, &mut run),
            vec![]
        );
        assert_eq!(run.failed, 1);
    }

    #[test]
    fn charges_the_document_budget_and_caps_the_wait_at_what_is_left_of_it() {
        // A store that never answers. See the file header for the RETIRED
        // `remainingMs <= 0` half: Rust has no wait to charge.
        let s = page(vec![]);
        let mut run = new_image_run();
        run.remaining_ms = 30.0;
        let started = Instant::now();
        assert_eq!(
            collect_page_images(&s, Some(&at("never", 0.0, 0.0)), 1, 0, &mut run),
            vec![]
        );
        assert!(started.elapsed().as_millis() < 1000);
        assert_eq!(run.failed, 1);
    }

    #[test]
    fn charges_the_encode_too_not_only_the_wait() {
        // The scanned-script case in one assertion. Every sheet of a 300dpi
        // scan is a distinct full-page image, so there is nothing to wait for
        // and nothing the cache can answer: the cost is all deflate, and a
        // budget that charges only the wait would let 120 sheets spend 29
        // seconds of CPU producing 141 MB of PNG for the scanned guard to
        // throw away.
        //
        // 1200x900 is the smallest picture whose encode is unambiguously
        // above a sub-millisecond wait: it costs tens of ms.
        let s = page(vec![("big", pixels(5, 1200, 900))]);
        let mut run = new_image_run();
        collect_page_images(&s, Some(&at("big", 0.0, 0.0)), 1, 0, &mut run);
        assert!(IMAGE_BUDGET_MS - run.remaining_ms > 10.0);
    }

    #[test]
    fn stops_asking_once_the_budget_is_gone_and_counts_what_it_skipped() {
        // Out of budget means stop asking, not ask with no timeout. The
        // skipped ones are still lost pictures, so they are still counted.
        // The TypeScript proved nothing was asked by counting store lookups;
        // here nothing was encoded into the run's cache, which every resolved
        // picture would have been.
        let s = page(vec![("a", px4(1)), ("b", px4(2))]);
        let mut run = new_image_run();
        run.remaining_ms = 0.0;
        let list = vec![
            q(),
            cm([40.0, 0.0, 0.0, 40.0, 0.0, 0.0]),
            do_("a", 4.0, 4.0),
            Q(),
            q(),
            cm([40.0, 0.0, 0.0, 40.0, 0.0, 60.0]),
            do_("b", 4.0, 4.0),
            Q(),
        ];
        assert_eq!(collect_page_images(&s, Some(&list), 1, 0, &mut run), vec![]);
        assert!(run.encoded.is_empty());
        assert_eq!(run.remaining_ms, 0.0);
        assert_eq!(run.failed, 2);
    }

    #[test]
    fn but_still_places_a_cached_picture_after_the_budget_is_gone() {
        // The defect the cache introduced, and the one thing that
        // distinguishes skipping the placement from abandoning the page: bytes
        // already in hand cost nothing to place, so an exhausted budget must
        // not throw away a logo this document has already paid for. With a
        // `break` here the logo goes too, and is counted as a second failure.
        let s = page(vec![("logo", px4(3)), ("fresh", px4(4))]);
        let mut run = new_image_run();
        collect_page_images(&s, Some(&at("logo", 0.0, 0.0)), 1, 5, &mut run);

        run.remaining_ms = 0.0;
        let list = vec![
            q(),
            cm([40.0, 0.0, 0.0, 40.0, 0.0, 0.0]),
            do_("fresh", 4.0, 4.0),
            Q(),
            q(),
            cm([40.0, 0.0, 0.0, 40.0, 0.0, 60.0]),
            do_("logo", 4.0, 4.0),
            Q(),
        ];
        let got = collect_page_images(&s, Some(&list), 2, 5, &mut run);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].candidate.page_num, 2);
        // Only the uncached one is a failure, and only it was skipped.
        assert_eq!(run.failed, 1);
        assert_eq!(run.encoded.keys().collect::<Vec<_>>(), vec!["logo"]);
    }

    #[test]
    fn a_page_whose_operator_list_never_arrived_has_no_images_and_no_failures() {
        let mut run = new_image_run();
        assert_eq!(
            collect_page_images(&page(vec![]), None, 1, 0, &mut run),
            vec![]
        );
        assert_eq!(run.failed, 0);
    }
}

/// An `EncodedCandidate` with the TypeScript's defaults, for `..` updates.
fn candidate(page_num: u32, x0: f64, y0: f64, x1: f64, y1: f64, hash: &str) -> EncodedCandidate {
    EncodedCandidate {
        candidate: ImageCandidate {
            page_num,
            x0,
            y0,
            x1,
            y1,
            src_px_width: 200.0,
            src_px_height: 200.0,
            hash: hash.into(),
            page_text_lines: 0,
        },
        bytes: vec![1],
    }
}

mod to_raw_images {
    use super::*;

    fn c(page_num: u32, y1: f64) -> EncodedCandidate {
        candidate(page_num, 61.2, 396.0, 306.0, y1, "h")
    }

    #[test]
    fn normalizes_the_box_against_the_page_in_the_axes_raw_line_uses() {
        let imgs = to_raw_images(&[c(1, 792.0)], 612.0, 792.0);
        let img = &imgs[0];
        assert_eq!(img.y, 792.0); // TOP edge, like RawLine.y
        assert!(close_to(img.page_width_pct, 40.0, 6));
        assert!(close_to(img.page_height_pct, 50.0, 6));
        // and nothing else: no left edge and no encoded size, both of which
        // rode here unread until 2026-08-12. The TypeScript listed the
        // object's keys; an exhaustive destructure is the compile-time form.
        let RawImage {
            bytes: _,
            file: _,
            page_height_pct: _,
            page_num: _,
            page_width_pct: _,
            y: _,
        } = img;
    }

    #[test]
    fn names_by_page_and_by_order_down_that_page() {
        let imgs = to_raw_images(&[c(1, 700.0), c(1, 300.0), c(12, 500.0)], 612.0, 792.0);
        let files: Vec<&str> = imgs.iter().map(|i| i.file.as_str()).collect();
        assert_eq!(files, vec!["p001-01.png", "p001-02.png", "p012-01.png"]);
    }

    #[test]
    fn sorts_by_page_ascending_and_y_descending_whatever_order_it_was_painted() {
        // `insert_images` in the parser's lib merges this array against the
        // element stream on exactly that ordering, so it has to be a fact
        // rather than a hope: a generator is free to paint the bottom panel of
        // a strip first, and page order out of extract is only as good as the
        // paint order.
        let imgs = to_raw_images(&[c(2, 100.0), c(1, 200.0), c(1, 700.0)], 612.0, 792.0);
        let order: Vec<(u32, f64)> = imgs.iter().map(|i| (i.page_num, i.y)).collect();
        assert_eq!(order, vec![(1, 700.0), (1, 200.0), (2, 100.0)]);
        let files: Vec<&str> = imgs.iter().map(|i| i.file.as_str()).collect();
        assert_eq!(files, vec!["p001-01.png", "p001-02.png", "p002-01.png"]);
    }
}

// The predicate the picture folder's prune deletes by (src/library-images.ts).
// A drift between the namer and this is silent — the picture folder's prune
// simply stops recognizing its own files and leaves a stale picture in the
// book — so the pairing is pinned on real output, including the sizes the
// zero padding does not truncate.
mod is_generated_image_file {
    use super::*;

    fn c(page_num: u32, y1: f64) -> EncodedCandidate {
        candidate(page_num, 0.0, 0.0, 100.0, y1, "h")
    }

    #[test]
    fn every_name_to_raw_images_produces_is_one_it_recognizes() {
        let imgs = to_raw_images(
            &[c(1, 100.0), c(1, 90.0), c(12, 100.0), c(1200, 100.0)],
            612.0,
            792.0,
        );
        let files: Vec<&str> = imgs.iter().map(|i| i.file.as_str()).collect();
        assert_eq!(
            files,
            vec!["p001-01.png", "p001-02.png", "p012-01.png", "p1200-01.png"]
        );
        for img in &imgs {
            assert!(is_generated_image_file(&img.file), "{}", img.file);
        }
    }

    #[test]
    fn a_name_a_person_could_have_chosen_is_not_one_of_ours() {
        // Every one of these passes notes' allowlist, which is why the two
        // predicates are separate: this is the set that may be DELETED.
        for name in [
            "board-01.png",
            "sunset.png",
            "p1.png",
            "A-01.png",
            "P001-01.png",
            "p001-01.PNG",
            "xp001-01.png",
            "p001-01.png.bak",
            "p00-01.png",
            "p001-1.png",
        ] {
            assert!(!is_generated_image_file(name), "{name}");
        }
    }

    #[test]
    fn no_separator_can_hide_in_one_however_it_is_spelled() {
        for name in [
            "../p001-01.png",
            "a/p001-01.png",
            "p001-01.png/x",
            "p001\\-01.png",
        ] {
            assert!(!is_generated_image_file(name), "{name}");
        }
    }
}

const LETTER: PageSize = PageSize {
    width: 612.0,
    height: 792.0,
};
const LANDSCAPE: PageSize = PageSize {
    width: 792.0,
    height: 612.0,
};

mod finish_images {
    // The document-level decisions, tested on documents no fixture has: a
    // mixed page geometry, a broken image, sixty-one keepers. extract_document
    // holds a real PDF, so inline none of these could fail.
    use super::*;

    fn c() -> EncodedCandidate {
        candidate(1, 61.2, 396.0, 306.0, 792.0, "h1")
    }

    #[test]
    fn measures_against_the_majority_geometry_not_the_last_page_read() {
        // The wiring, which a uniform fixture can never pin: one landscape
        // insert at the end of a portrait script. Against the last page this
        // image is 30.9% wide and 64.7% tall; against the document it is 40%
        // and 50%.
        let (images, _) = finish_images(&[c()], &[LETTER, LETTER, LANDSCAPE], 3, &new_image_run());
        assert!(close_to(images[0].page_width_pct, 40.0, 6));
        assert!(close_to(images[0].page_height_pct, 50.0, 6));
    }

    #[test]
    fn carries_the_failure_count_off_the_run_untouched() {
        let mut run = new_image_run();
        run.failed = 7;
        assert_eq!(finish_images(&[], &[LETTER], 1, &run).1.failed, 7);
    }

    #[test]
    fn reports_what_the_cap_refused_which_is_not_what_the_filters_refused() {
        // Distinct hashes and distinct pages, so nothing is furniture and the
        // only thing that can drop one is MAX_IMAGES. 61 in, 60 out, 1 over.
        let many: Vec<EncodedCandidate> = (0..61u32)
            .map(|i| candidate(i + 1, 61.2, 396.0, 306.0, 792.0, &format!("h{i}")))
            .collect();
        let (images, stats) = finish_images(&many, &[LETTER], 61, &new_image_run());
        assert_eq!(images.len(), 60);
        assert_eq!(stats.over_cap, 1);
        let d = stats.dropped_by;
        assert!(
            [
                d.pixels,
                d.width,
                d.height,
                d.area,
                d.furniture,
                d.background
            ]
            .iter()
            .all(|&n| n == 0)
        );
    }

    #[test]
    fn a_document_with_no_pages_at_all_is_refused_rather_than_divided_by() {
        assert_eq!(finish_images(&[c()], &[], 0, &new_image_run()).0, vec![]);
    }
}

mod dominant_page_size {
    // The image filters measure every picture against ONE page geometry, so
    // extract has to pick which page it comes from. Taking the last page's —
    // which is what a loop variable does if nobody chooses — measures an
    // entire document against whatever sheet happened to be last.
    use super::*;

    #[test]
    fn a_uniform_document_reports_its_one_size() {
        assert_eq!(dominant_page_size(&[LETTER, LETTER, LETTER]), LETTER);
    }

    #[test]
    fn a_landscape_insert_does_not_redefine_the_document_even_when_last() {
        assert_eq!(
            dominant_page_size(&[LETTER, LETTER, LETTER, LANDSCAPE]),
            LETTER
        );
    }

    #[test]
    fn ties_go_to_the_first_seen_so_the_answer_is_deterministic() {
        assert_eq!(dominant_page_size(&[LETTER, LANDSCAPE]), LETTER);
        assert_eq!(dominant_page_size(&[LANDSCAPE, LETTER]), LANDSCAPE);
    }

    #[test]
    fn no_pages_at_all_is_zeros_which_keep_artwork_refuses_to_judge_on() {
        assert_eq!(
            dominant_page_size(&[]),
            PageSize {
                width: 0.0,
                height: 0.0
            }
        );
    }
}

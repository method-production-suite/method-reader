//! Port of tests/images-fixture.test.ts.
//!
//! The image subsystem against a REAL document: tests/fixtures/images.pdf,
//! read here through the oracle's dump of it (`rust/oracle/images/`) until
//! sp-pdf opens the PDF itself.
//!
//! Split from images.rs, which is stubs only. A stub answers however it is
//! written, so the facts that cost the most to learn can only be pinned
//! against a file the PDF layer actually opened. And the second half of this
//! file exercises extract.rs's PAGE LOOP rather than images.rs, so a
//! regression there should not report against a file named for the parser
//! module.
//!
//! Pixels: the dump names each picture's pixels by hash and carries none, so
//! where a TS test checked the PNG that ships, this file injects SYNTHETIC
//! pixels into the real facts (`with_pixels`: deterministic bytes per
//! hash, at the dump's own size and kind) and runs the real page loop on
//! those. Identical source pixels stay identical, so the furniture rule sees
//! what it saw on the PDF.
//!
//! RETIRED: "objs.has() LIES, which is why nothing here uses the synchronous
//! get" and "and a page-store image DOES report has() once resolved": pdf.js
//! object-store behaviour; the facts arrive with pixels already resolved and
//! there is no `has()`. The `hasAfter` half of "a REPEATED picture is renamed
//! and parked in commonObjs" is retired for the same reason; its rename half
//! is ported on the dump's object ids.
//!
//! NEEDS PIXELS: "the same picture through two stores carries the same
//! pixels" compared the decoded RGB; ported here on the dump's pixel hash
//! (`dataSha256`), the bytes themselves come with sp-pdf. "every paint
//! resolves to pixels this parser can read" and "every paint converts to
//! exactly width x height x 3 bytes" are ported on the dump's facts plus a
//! buffer of the dump's own `dataLength`; the real decoded bytes come with
//! sp-pdf.
//!
//! MOVED: the `extractTitleMeta` half of "a picture on the title page is not
//! title-page furniture": needs sp-fountain extractTitleMeta, ports with that
//! crate. Its `isTitlePage` assertion is ported.

mod common;

use sp_model::{ElementType, ParsedScreenplay, RawImage, ScreenplayElement};
use sp_parser::facts::DocumentFacts;
use sp_parser::images::{collect_image_placements, to_rgb};

const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

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

/// The fixture's facts with synthetic pixels in every picture: `dataLength`
/// bytes seeded from `dataSha256`, so the same source pixels on two pages
/// stay the same bytes and distinct ones differ.
fn with_pixels(mut facts: DocumentFacts) -> DocumentFacts {
    for page in &mut facts.pages {
        for img in page.images.values_mut().flatten() {
            let (Some(len), Some(sha)) = (img.data_length, img.data_sha256.as_deref()) else {
                continue;
            };
            let seed: usize = sha.bytes().map(usize::from).sum();
            img.data = Some(
                (0..len)
                    .map(|i| u8::try_from((i * 7 + seed) % 256).unwrap())
                    .collect(),
            );
        }
    }
    facts
}

mod the_pixel_side_against_a_real_pdf {
    // Resolved ONCE per test from the dump, with the assertions split across
    // small tests below so a fixture regeneration that shifts a size cannot
    // hide the claims each test exists for.
    use super::*;

    struct Resolved {
        key: String,
        page: u32,
        /// what the PAINT declared, against what the decoded image reports
        declared: [f64; 2],
        img: Option<sp_parser::facts::ImageFact>,
    }

    fn seen() -> Vec<Resolved> {
        let facts = common::facts("images");
        let mut out = Vec::new();
        for page in &facts.pages {
            let ops = page.ops.as_deref().unwrap_or_default();
            for place in collect_image_placements(ops) {
                out.push(Resolved {
                    img: page.images.get(&place.obj_id).cloned().flatten(),
                    key: place.obj_id,
                    page: page.page_num,
                    declared: [place.src_px_width, place.src_px_height],
                });
            }
        }
        out
    }

    /// `toRgb` on a buffer of the dump's own `dataLength`: whether the
    /// parser can read pixels of that shape. The real bytes: NEEDS PIXELS.
    fn rgb(s: &Resolved) -> Option<Vec<u8>> {
        let img = s.img.as_ref()?;
        let data = vec![0u8; img.data_length?];
        to_rgb(img.width, img.height, img.kind, Some(&data))
    }

    #[test]
    fn every_paint_resolves_to_pixels_this_parser_can_read() {
        let seen = seen();
        let missing: Vec<&str> = seen
            .iter()
            .filter(|s| s.img.is_none())
            .map(|s| s.key.as_str())
            .collect();
        assert_eq!(missing, Vec::<&str>::new());
        let unreadable: Vec<&str> = seen
            .iter()
            .filter(|s| rgb(s).is_none())
            .map(|s| s.key.as_str())
            .collect();
        assert_eq!(unreadable, Vec::<&str>::new());
    }

    #[test]
    fn the_decoded_image_is_the_size_its_paint_operator_declared() {
        // Both halves of the same fact: the size artwork judges on comes from
        // the paint, and the size png encodes comes from the image.
        for s in seen() {
            let img = s.img.as_ref().unwrap();
            assert_eq!([img.width, img.height], s.declared, "{}", s.key);
        }
    }

    #[test]
    fn a_repeated_picture_is_renamed_and_parked_in_common_objs() {
        // The logo is `img_p0_2` on sheet 1 and `g_d*_img_p1_2` from sheet 2
        // on: the document-wide store's name. (`hasAfter`: RETIRED, see the
        // file header.)
        let pages: Vec<u32> = seen()
            .iter()
            .filter(|s| s.key.starts_with("g_"))
            .map(|s| s.page)
            .collect();
        assert_eq!(pages, vec![2, 3, 4, 5]);
    }

    #[test]
    fn the_same_picture_through_two_stores_carries_the_same_pixels() {
        // Or the furniture rule sees five different logos and keeps them
        // all. The LAST renamed one, not a fixed index: if a pdf.js bump
        // changed when a picture gets promoted, an index would panic instead
        // of failing on the claim. Compared by the dump's pixel hash; the
        // decoded bytes NEED PIXELS.
        let seen = seen();
        let last = seen.iter().rfind(|s| s.key.starts_with("g_")).unwrap();
        let first = seen.iter().find(|s| s.key == "img_p0_2").unwrap();
        let sha = |s: &Resolved| s.img.as_ref().unwrap().data_sha256.clone();
        assert!(sha(first).is_some());
        assert_eq!(sha(last), sha(first));
    }

    #[test]
    fn every_paint_converts_to_exactly_width_x_height_x_3_bytes() {
        for s in seen() {
            let img = s.img.as_ref().unwrap();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let want = (img.width * img.height * 3.0) as usize;
            assert_eq!(rgb(&s).unwrap().len(), want, "{}", s.key);
        }
    }

    #[test]
    fn ten_paints_of_six_pictures_at_the_sizes_the_fixture_declares() {
        // make-fixture.py's own table (IMAGES / SHEET_PLACEMENTS).
        let seen = seen();
        assert_eq!(seen.len(), 10);
        let mut sizes: Vec<String> = seen
            .iter()
            .map(|s| {
                let img = s.img.as_ref().unwrap();
                format!("{}x{}", img.width, img.height)
            })
            .collect();
        sizes.sort();
        sizes.dedup();
        assert_eq!(
            sizes,
            vec![
                "100x50", "120x160", "16x16", "200x200", "240x180", "261x351"
            ]
        );
    }
}

mod extract_document_on_the_image_fixture {
    use super::*;
    use sp_parser::extract::extract_document;

    #[test]
    fn keeps_the_title_card_and_the_panel_drops_the_other_four() {
        let out = common::extract("images");
        assert_eq!(out.page_count, 5);
        assert_eq!(out.images.len(), 2);
        let pages: Vec<u32> = out.images.iter().map(|i| i.page_num).collect();
        assert_eq!(pages, vec![1, 2]);
    }

    #[test]
    fn attributes_every_drop_to_the_rule_the_fixture_built_for_it() {
        // The fixture's whole point, end to end: four refusals, each by a
        // different rule. Height and area refuse nothing here (their
        // boundaries are hand-built in the artwork tests). `width` and not
        // `area` for imSmall (20x20pt from 200x200px) because the linear
        // floors are asked first, and `background` and not `furniture` for
        // imMark because it sits on one sheet. Six counts plus kept plus
        // overCap add up to the ten paints collect_image_placements found.
        let out = common::extract("images");
        let stats = &out.image_stats;
        let d = stats.dropped_by;
        assert_eq!(d.pixels, 1); // imTiny, 16x16 source pixels
        assert_eq!(d.width, 1); // imSmall, 20pt wide on a 612pt sheet
        assert_eq!(d.height, 0);
        assert_eq!(d.area, 0);
        assert_eq!(d.furniture, 5); // imLogo, one per sheet
        assert_eq!(d.background, 1); // imMark, full bleed under 29 text lines
        assert_eq!(stats.over_cap, 0);
        let dropped = d.pixels + d.width + d.height + d.area + d.furniture + d.background;
        assert_eq!(
            out.images.len() + (stats.over_cap + dropped + stats.failed) as usize,
            10
        );
    }

    #[test]
    fn loses_no_picture_to_a_failed_resolve_conversion_or_encode() {
        // The count that makes a TOTAL loss visible: if every image converted
        // to nothing at once, the book would have no art and no error,
        // indistinguishable from a script that never had pictures.
        assert_eq!(common::extract("images").image_stats.failed, 0);
    }

    #[test]
    fn the_title_card_covers_most_of_sheet_1() {
        let out = common::extract("images");
        let card = &out.images[0];
        assert!(close_to(card.page_width_pct, (522.0 / 612.0) * 100.0, 0));
        assert!(close_to(card.page_height_pct, (702.0 / 792.0) * 100.0, 0));
    }

    #[test]
    fn and_sits_where_it_was_drawn_in_the_axis_raw_line_uses() {
        // y is the TOP edge in PDF user space, so a descending sort
        // interleaves images with lines. The card is 522x702pt at (45, 45),
        // so its top edge is 747.
        let out = common::extract("images");
        assert!(close_to(out.images[0].y, 747.0, 1));
    }

    #[test]
    fn every_kept_image_carries_a_decodable_png_at_its_source_resolution() {
        // On synthetic pixels injected into the real facts (see the file
        // header): the page loop, downscale and encode are the real ones.
        let out = extract_document(&with_pixels(common::facts("images")), None, None);
        let images: &[RawImage] = &out.images;
        for img in images {
            assert_eq!(&img.bytes[..8], &PNG_MAGIC);
            assert!(png_size(&img.bytes)[0] > 0);
        }
        // Both are well under IMAGE_MAX_EDGE, so the downscaler passed them
        // through: these are the fixture's own declared source dimensions.
        let sizes: Vec<String> = images
            .iter()
            .map(|i| {
                let [w, h] = png_size(&i.bytes);
                format!("{w}x{h}")
            })
            .collect();
        assert_eq!(sizes, vec!["261x351", "240x180"]);
        // and the injected pixels changed nothing the filters decided
        assert_eq!(out.image_stats, common::extract("images").image_stats);
    }

    #[test]
    fn names_each_image_by_page_and_order_on_that_page() {
        let out = common::extract("images");
        let files: Vec<&str> = out.images.iter().map(|i| i.file.as_str()).collect();
        assert_eq!(files, vec!["p001-01.png", "p002-01.png"]);
    }

    #[test]
    fn a_pdf_with_no_images_returns_an_empty_array_not_undefined() {
        let out = common::extract("screenplay");
        assert_eq!(out.images, vec![]);
        assert_eq!(out.image_stats.failed, 0);
        assert_eq!(out.image_stats.over_cap, 0);
        let d = out.image_stats.dropped_by;
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
        // The text half is untouched by any of this, and says so: the extract
        // tests already pin what screenplay.pdf's lines are, so a second
        // assertion on them here would only be a slower copy.
        assert!(!out.lines.is_empty());
    }
}

mod parse_lines_merges_the_fixture_images_into_the_element_stream {
    // The check hand-built streams cannot make. The pipeline tests own the
    // merge's cases, but every y in them was chosen by the test author — so
    // they can only show the rule is self-consistent. These are pdf.js's own
    // coordinates, out of a document laid out by tools/make-fixture.py, and
    // the question they answer is whether the two halves agree about which
    // way up a page is.
    use super::*;

    fn parsed() -> ParsedScreenplay {
        common::parse("images")
    }

    fn image_els(p: &ParsedScreenplay) -> Vec<&ScreenplayElement> {
        p.elements
            .iter()
            .filter(|el| el.kind == ElementType::Image)
            .collect()
    }

    #[test]
    fn both_kept_pictures_reach_the_stream_and_nothing_else_does() {
        let p = parsed();
        let files: Vec<&str> = image_els(&p)
            .iter()
            .map(|el| el.img.as_ref().unwrap().file.as_str())
            .collect();
        assert_eq!(files, vec!["p001-01.png", "p002-01.png"]);
    }

    #[test]
    fn an_image_element_keeps_the_page_and_top_edge_it_was_merged_on() {
        // The merge key, carried onto the element rather than consumed and
        // thrown away: the card is 522x702pt at (45, 45) so its top is 747,
        // and the panel is 180x135pt at (108, 561) so its top is 696. Both in
        // the axis the text elements beside them use — element 1 is the title
        // at y=504, element 5 the sheet-2 heading at y=720 — which is the
        // whole basis of the merge.
        let p = parsed();
        let got: Vec<(u32, Option<f64>)> =
            image_els(&p).iter().map(|el| (el.page_num, el.y)).collect();
        assert_eq!(got, vec![(1, Some(747.0)), (2, Some(696.0))]);
    }

    #[test]
    fn the_title_card_comes_before_the_title_text_it_was_drawn_behind() {
        // The card's top edge is y=747 and the title's first line is y=504,
        // both in PDF user space where up is the high y. Compare those two the
        // wrong way round and the book opens on its title with the card
        // underneath.
        let p = parsed();
        assert_eq!(
            p.elements[0].img.as_ref().map(|i| i.file.as_str()),
            Some("p001-01.png")
        );
        assert_eq!(p.elements[1].text, "THE PAPER LANTERN");
    }

    #[test]
    fn the_card_is_full_page_and_fills_the_column() {
        // 85.3% x 88.6% of the sheet is 75.6% of its area, over the 70% bar;
        // and 85.3% of the sheet is 121% of the text column, clamped back to
        // it.
        let p = parsed();
        let img = image_els(&p)[0].img.clone().unwrap();
        assert_eq!(img.file, "p001-01.png");
        assert_eq!(img.width_pct, 100.0);
        assert!(img.full_page);
    }

    #[test]
    fn the_panel_lands_under_its_scene_heading_and_above_the_action() {
        // make-fixture.py reserves 13 rows on sheet 2 for exactly this:
        // heading at y=720, panel top at y=696, action at y=540.
        let p = parsed();
        let at = p
            .elements
            .iter()
            .position(|el| el.img.as_ref().is_some_and(|i| i.file == "p002-01.png"))
            .unwrap();
        assert_eq!(p.elements[at - 1].text, "INT. ARCHIVE ROOM - DAY");
        assert!(
            p.elements[at + 1].text.contains("holds a storyboard panel"),
            "{}",
            p.elements[at + 1].text
        );
    }

    #[test]
    fn the_panel_is_inline_at_its_own_share_of_the_column() {
        // 180pt of a 612pt sheet is 29.4%, which is 42% of the 6.0" column.
        let p = parsed();
        let img = image_els(&p)[1].img.clone().unwrap();
        assert_eq!(img.file, "p002-01.png");
        assert_eq!(img.width_pct, 42.0);
        assert!(!img.full_page);
    }

    #[test]
    fn a_picture_on_the_title_page_is_not_title_page_furniture() {
        // Asserted, not detected, and this is the consequence that makes it
        // right: the serializer drops title-page elements from the book, and
        // extractTitleMeta joins the readable ones into the TITLE. Flagged
        // true, the designed title card would vanish AND the book would be
        // called "p001-01.png THE PAPER LANTERN". (The extractTitleMeta
        // assertion: MOVED, see the file header.)
        let p = parsed();
        assert!(!image_els(&p)[0].is_title_page);
    }

    #[test]
    fn neither_picture_becomes_a_speaker_or_a_scene() {
        let p = parsed();
        let names: Vec<&str> = p.characters.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["WREN", "HOLT"]);
        assert_eq!(p.scenes.len(), 4);
        // element_index counts the images ahead of each heading: the card sits
        // at 0, so sheet 2's heading is element 5 rather than element 4.
        let idx: Vec<u32> = p.scenes.iter().map(|s| s.element_index).collect();
        assert_eq!(idx, vec![5, 14, 22, 42]);
    }
}

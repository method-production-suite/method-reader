//! Port of tests/torture-pictures.test.ts.
//!
//! The pictures torture.pdf carries, so a device pass reads them in the same
//! book as every other row (registry #19 for which are kept, #19a for how the
//! book shows them, #19b for the one that is its cover). Each one is there for
//! a question a desktop render cannot settle; tools/make-fixture.py states each
//! one's geometry and its margin to the threshold that governs it, and
//! tools/torture-manifest.json says what the device is asked about each.
//!
//! Same split by concern as torture.rs: torture.rs parses the text alone and
//! never sees a picture, so nothing here can move an assertion there.
//!
//! The PDF is the oracle's dump of `tests/fixtures/torture.pdf`
//! (`common::extract("torture")`). The first two describe blocks are parser
//! facts and are ported whole; the last two read the built EPUB and .mobi and
//! moved:
//!
// MOVED: "carries exactly the four pictures, and no letterhead": reads the EPUB zip; needs
//   sp-engine convertPdf + sp-epub, ports with that crate.
// MOVED: "the title card is the cover: on no page, and the book opens on the first scene":
//   reads the EPUB's body, nav and OPF; needs sp-engine convertPdf + sp-epub.
// MOVED: "with promoteCoverImage off, the title card is an Opening section of its own":
//   needs sp-engine convertPdf with format options + sp-epub.
// MOVED: "the panel and the tall plate stay in the flow at their own widths": reads the
//   EPUB body's figure classes; needs sp-engine convertPdf + sp-epub.
// MOVED: "a scene heading sits directly above the full-page plate": reads the EPUB body;
//   needs sp-engine convertPdf + sp-epub.
// MOVED: "names the title card as its cover, the record the EPUB's cover is": reads the
//   .mobi EXTH and picture records; needs sp-engine convertPdf + sp-mobi.
// MOVED: "and still draws the card on a page of its own, straight after the title page":
//   reads the .mobi HTML; needs sp-engine convertPdf + sp-mobi.

mod common;

use std::sync::OnceLock;

use sp_model::{ElementType, ImageRef, ScreenplayElement};
use sp_parser::extract::ExtractResult;

// Sheet numbers here are PDF pages, as the picture filenames count them: the
// title sheet is page 1, so `p015-01.png` is the first picture on page 15.
const CARD: &str = "p001-01.png";
const PANEL: &str = "p015-01.png";
const PLATE: &str = "p016-01.png";
const TALL: &str = "p017-01.png";

fn out() -> &'static ExtractResult {
    static OUT: OnceLock<ExtractResult> = OnceLock::new();
    OUT.get_or_init(|| common::extract("torture"))
}

mod which_pictures_torture_pdf_keeps_19 {
    use super::*;
    use sp_parser::artwork::DroppedBy;

    #[test]
    fn the_title_card_the_panel_the_plate_and_the_tall_plate_one_sheet_each() {
        // One sheet each is the point: repetition cannot reach any of them, so
        // every one has to arrive as art on its own merits.
        assert_eq!(out().page_count, 17);
        assert_eq!(
            out()
                .images
                .iter()
                .map(|i| (i.page_num, i.file.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, CARD), (15, PANEL), (16, PLATE), (17, TALL)]
        );
    }

    #[test]
    fn the_letterhead_on_every_sheet_is_refused_as_furniture_and_nothing_else_is_refused() {
        // The furniture rule at document scale: one logo on all 17 sheets, far
        // past the three-sheet threshold. images.pdf refuses one picture by each
        // of four rules (pixels, width, furniture, background), and height and
        // area are pinned only by hand-built candidates in tests/artwork.test.ts.
        assert_eq!(
            out().image_stats.dropped_by,
            DroppedBy {
                pixels: 0,
                width: 0,
                height: 0,
                area: 0,
                furniture: 17,
                background: 0,
            }
        );
        assert_eq!(out().image_stats.over_cap, 0);
        assert_eq!(out().image_stats.failed, 0);
    }
}

mod what_each_picture_becomes_in_the_element_stream_19 {
    use super::*;

    fn elements() -> &'static [ScreenplayElement] {
        static ELEMENTS: OnceLock<Vec<ScreenplayElement>> = OnceLock::new();
        ELEMENTS.get_or_init(|| sp_parser::parse_lines(&out().lines, &out().images).elements)
    }
    fn at(file: &str) -> usize {
        elements()
            .iter()
            .position(|el| el.img.as_ref().is_some_and(|i| i.file == file))
            .unwrap_or_else(|| panic!("{file} is not in the element stream"))
    }

    #[test]
    fn each_picture_carries_the_width_and_the_full_page_verdict_its_sheet_gives_it() {
        // The card 522x702pt and the plate 558x666pt are both over 70% of their
        // sheet and wider than the text column; the panel is 180pt (42% of the
        // column) and the tall plate 240pt (56%), both under the full-page bar.
        let r = |file: &str, width_pct: f64, full_page: bool| {
            Some(ImageRef {
                file: file.to_string(),
                width_pct,
                full_page,
            })
        };
        assert_eq!(
            elements()
                .iter()
                .filter(|el| el.kind == ElementType::Image)
                .map(|el| el.img.clone())
                .collect::<Vec<_>>(),
            vec![
                r(CARD, 100.0, true),
                r(PANEL, 42.0, false),
                r(PLATE, 100.0, true),
                r(TALL, 56.0, false),
            ]
        );
    }

    #[test]
    fn the_title_card_comes_before_the_title_so_it_stands_ahead_of_every_scene_heading() {
        assert_eq!(at(CARD), 0);
        assert_eq!(elements()[1].text, "THE PROOF SHEET");
    }

    #[test]
    fn the_panel_sits_between_its_heading_and_the_action_below_it() {
        let i = at(PANEL);
        assert_eq!(elements()[i - 1].kind, ElementType::Scene);
        assert_eq!(elements()[i - 1].text, "INT. ART DEPARTMENT - DAY");
        assert!(elements()[i + 1].text.contains("storyboard panel"));
    }

    #[test]
    fn the_full_page_plate_follows_its_scene_heading_with_nothing_between() {
        let i = at(PLATE);
        assert_eq!(elements()[i - 1].kind, ElementType::Scene);
        assert_eq!(elements()[i - 1].text, "INT. MAP ROOM - NIGHT");
    }

    #[test]
    fn the_tall_plate_follows_its_heading_and_its_one_line_of_action() {
        let i = at(TALL);
        assert_eq!(elements()[i - 2].text, "INT. FIRE STAIRS - CONTINUOUS");
        assert_eq!(elements()[i - 1].text, "A production still, printed tall.");
    }
}

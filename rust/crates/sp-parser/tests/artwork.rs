//! Port of tests/artwork.test.ts.
//!
//! The editorial rules, against hand-built candidates. Nothing here reads a
//! PDF or an oracle dump: artwork.rs holds judgements about what a picture
//! IS, so asserting that a 20x20pt image is too small to be art must never
//! depend on the PDF layer. If a change to this file ever seems to need one,
//! the change belongs in images.rs.
//!
//! RETIRED: "nothing in the module graph reaches pdf.js, and nothing may":
//! it bundled artwork.ts with Bun.build and grepped the output for pdf.js.
//! The Rust port has no pdf.js and no bundle; sp-parser's own Cargo.toml has
//! no PDF dependency at all, so the crate graph enforces the split that test
//! guarded.

mod common;

use sp_parser::artwork::{
    ArtworkVerdict, DropRule, DroppedBy, FULL_PAGE_AREA_FRAC, ImageCandidate, column_width_pct,
    is_full_page, keep_artwork,
};

const PAGE_W: f64 = 612.0;
const PAGE_H: f64 = 792.0;

/// A candidate that passes every filter, so each test can spoil one thing.
///
/// Deliberately imPanel's geometry from tools/make-fixture.py (180x135pt at
/// 240x180px, 5.0% of the page) and not a knife-edge box: with only tenths of
/// a point of headroom every "keeps" test below would fail at once on an area
/// retune, and none of the failures would name the area floor. The boundaries
/// belong to the tests that are about boundaries.
///
/// TRAP in the default `hash`. Two calls to `candidate()` are two copies of
/// the same picture, so on different pages they are FURNITURE at any
/// `page_count` of 8 or under, and at any `page_count` from three pages up.
/// Any test that wants two unrelated images has to give at least one its own
/// hash — which is what `lookbook()` does and why it takes the trouble.
fn candidate() -> ImageCandidate {
    ImageCandidate {
        page_num: 2,
        x0: 108.0,
        y0: 561.0,
        x1: 288.0,
        y1: 696.0, // 180 x 135 pt
        src_px_width: 240.0,
        src_px_height: 180.0,
        hash: "panel".into(),
        page_text_lines: 30,
    }
}

fn hashes(c: &[ImageCandidate]) -> Vec<String> {
    c.iter().map(|x| x.hash.clone()).collect()
}

/// Every rule at zero, for the cases where nothing was refused by a rule.
const NO_DROPS: DroppedBy = DroppedBy {
    pixels: 0,
    width: 0,
    height: 0,
    area: 0,
    furniture: 0,
    background: 0,
};

const ALL_RULES: [(DropRule, &str); 6] = [
    (DropRule::Pixels, "pixels"),
    (DropRule::Width, "width"),
    (DropRule::Height, "height"),
    (DropRule::Area, "area"),
    (DropRule::Furniture, "furniture"),
    (DropRule::Background, "background"),
];

/// The rules that fired, as a plain list, so an assertion reads like a
/// sentence instead of a struct literal with five zeroes in it.
fn fired_rules(dropped_by: &DroppedBy) -> Vec<(&'static str, u32)> {
    ALL_RULES
        .iter()
        .map(|&(r, name)| (name, dropped_by.get(r)))
        .filter(|&(_, n)| n > 0)
        .collect()
}

fn total_dropped(dropped_by: &DroppedBy) -> u32 {
    ALL_RULES.iter().map(|&(r, _)| dropped_by.get(r)).sum()
}

/// `n` distinct pictures, one per sheet, each a point WIDER than the last so
/// document order and size order disagree. Every one still clears every
/// floor: widths run 180pt up to 180 + n - 1, all of them well inside the
/// sheet.
fn lookbook(n: u32) -> Vec<ImageCandidate> {
    (0..n)
        .map(|i| ImageCandidate {
            page_num: i + 1,
            hash: format!("p{i}"),
            x1: 288.0 + f64::from(i),
            ..candidate()
        })
        .collect()
}

fn keep(cs: &[ImageCandidate], w: f64, h: f64, page_count: u32) -> ArtworkVerdict<ImageCandidate> {
    keep_artwork(cs, w, h, page_count)
}

mod keep_artwork_tests {
    use super::*;

    #[test]
    fn keeps_an_ordinary_storyboard_panel() {
        let v = keep(&[candidate()], PAGE_W, PAGE_H, 5);
        assert_eq!(hashes(&v.kept), vec!["panel"]);
        assert_eq!(v.over_cap, 0);
    }

    #[test]
    fn an_empty_document_is_empty_not_undefined() {
        assert_eq!(
            keep(&[], PAGE_W, PAGE_H, 5),
            ArtworkVerdict {
                kept: vec![],
                over_cap: 0,
                dropped_by: NO_DROPS
            }
        );
    }

    #[test]
    fn drops_anything_below_the_pixel_floor() {
        // Rendered large but only 16x16 real pixels: upscaled to mush.
        let c = ImageCandidate {
            src_px_width: 16.0,
            src_px_height: 16.0,
            ..candidate()
        };
        assert!(keep(&[c], PAGE_W, PAGE_H, 5).kept.is_empty());
    }

    #[test]
    fn drops_anything_rendered_too_small_to_be_art() {
        let tiny = ImageCandidate {
            x1: 128.0,
            y1: 581.0,
            ..candidate()
        }; // 20 x 20 pt
        assert!(keep(&[tiny], PAGE_W, PAGE_H, 5).kept.is_empty());
    }

    #[test]
    fn drops_an_image_repeated_across_pages_as_furniture() {
        // Same pixels in the same corner of every sheet: a letterhead logo.
        let pages: Vec<_> = [1, 2, 3, 4, 5]
            .into_iter()
            .map(|page_num| ImageCandidate {
                page_num,
                hash: "logo".into(),
                ..candidate()
            })
            .collect();
        assert!(keep(&pages, PAGE_W, PAGE_H, 5).kept.is_empty());
    }

    #[test]
    fn keeps_a_repeated_image_that_repeats_only_twice() {
        // Two storyboard panels of the same shot is not furniture.
        let twice = [
            ImageCandidate {
                page_num: 2,
                hash: "same".into(),
                ..candidate()
            },
            ImageCandidate {
                page_num: 3,
                hash: "same".into(),
                ..candidate()
            },
        ];
        assert_eq!(keep(&twice, PAGE_W, PAGE_H, 20).kept.len(), 2);
    }

    #[test]
    fn drops_a_full_bleed_image_sitting_under_a_full_page_of_text() {
        let watermark = ImageCandidate {
            x0: 0.0,
            y0: 0.0,
            x1: PAGE_W,
            y1: PAGE_H,
            hash: "mark".into(),
            page_text_lines: 25,
            ..candidate()
        };
        assert!(keep(&[watermark], PAGE_W, PAGE_H, 5).kept.is_empty());
    }

    #[test]
    fn keeps_a_full_bleed_image_on_a_page_with_almost_no_text() {
        // The designed title card. The ONLY difference from the watermark above
        // is the text count, which is the whole content of the background rule:
        // the geometry has to be identical or the test proves nothing.
        let card = ImageCandidate {
            page_num: 1,
            x0: 0.0,
            y0: 0.0,
            x1: PAGE_W,
            y1: PAGE_H,
            hash: "card".into(),
            page_text_lines: 0,
            ..candidate()
        };
        assert_eq!(hashes(&keep(&[card], PAGE_W, PAGE_H, 5).kept), vec!["card"]);
    }

    #[test]
    fn caps_the_total_and_reports_what_it_dropped_never_silently() {
        let v = keep(&lookbook(70), PAGE_W, PAGE_H, 70);
        assert_eq!(v.kept.len(), 60);
        assert_eq!(v.over_cap, 10);
    }

    #[test]
    fn the_cap_keeps_a_document_order_prefix_not_a_scattered_subset() {
        // lookbook() grows its pictures towards the back of the document, so a
        // cap that preferred larger images would keep p69..p10 and this would
        // fail. An all-identical set would pass under either rule: a stable sort.
        let v = keep(&lookbook(70), PAGE_W, PAGE_H, 70);
        let want: Vec<String> = (0..60).map(|i| format!("p{i}")).collect();
        assert_eq!(hashes(&v.kept), want);
    }

    #[test]
    fn exactly_the_cap_is_not_over_it() {
        let v = keep(&lookbook(60), PAGE_W, PAGE_H, 60);
        assert_eq!(v.kept.len(), 60);
        assert_eq!(v.over_cap, 0);
    }

    #[test]
    fn over_cap_counts_only_the_cap_never_the_filters() {
        // One good picture and nine pieces of furniture. Nothing here is over
        // the cap, and a caller must not warn the user that ten images were lost.
        let mut cs = vec![ImageCandidate {
            page_num: 1,
            hash: "art".into(),
            ..candidate()
        }];
        cs.extend((0..9).map(|i| ImageCandidate {
            page_num: i + 2,
            hash: format!("junk{i}"),
            src_px_width: 8.0,
            src_px_height: 8.0,
            ..candidate()
        }));
        let v = keep(&cs, PAGE_W, PAGE_H, 20);
        assert_eq!(hashes(&v.kept), vec!["art"]);
        assert_eq!(v.over_cap, 0);
    }
}

mod keep_artwork_boundaries {
    use super::*;

    #[test]
    fn exactly_32_source_pixels_on_an_edge_is_enough_31_is_not_either_edge() {
        let px = |w: f64, h: f64| ImageCandidate {
            src_px_width: w,
            src_px_height: h,
            ..candidate()
        };
        assert_eq!(keep(&[px(32.0, 32.0)], PAGE_W, PAGE_H, 20).kept.len(), 1);
        // Both edges, because one clause guarding both would look identical here.
        assert!(keep(&[px(32.0, 31.0)], PAGE_W, PAGE_H, 20).kept.is_empty());
        assert!(keep(&[px(31.0, 32.0)], PAGE_W, PAGE_H, 20).kept.is_empty());
    }

    #[test]
    fn exactly_10_percent_of_page_width_is_wide_enough_a_hair_under_is_not() {
        // Tall enough that only the width floor is in play.
        let bx = |w: f64| ImageCandidate {
            x0: 0.0,
            x1: w,
            y0: 0.0,
            y1: PAGE_H * 0.5,
            ..candidate()
        };
        assert_eq!(keep(&[bx(PAGE_W * 0.1)], PAGE_W, PAGE_H, 20).kept.len(), 1);
        assert!(
            keep(&[bx(PAGE_W * 0.1 - 0.01)], PAGE_W, PAGE_H, 20)
                .kept
                .is_empty()
        );
    }

    #[test]
    fn exactly_3_percent_of_page_height_is_tall_enough_a_hair_under_is_not() {
        // A banner OVERHANGING the sheet, 712pt wide on a 612pt page, and it has
        // to be: with the area floor also at 3%, a box that fits cannot be under
        // 3% of page height and still over 3% of page area, so a full-width box
        // here would fail both floors at once and prove nothing about this one.
        // Overhanging, the area is 3.49% at both sizes and only the height moves.
        let bx = |h: f64| ImageCandidate {
            x0: -50.0,
            x1: 662.0,
            y0: 0.0,
            y1: h,
            ..candidate()
        };
        assert_eq!(keep(&[bx(PAGE_H * 0.03)], PAGE_W, PAGE_H, 20).kept.len(), 1);
        assert!(
            keep(&[bx(PAGE_H * 0.03 - 0.01)], PAGE_W, PAGE_H, 20)
                .kept
                .is_empty()
        );
    }

    #[test]
    fn exactly_3_percent_of_page_area_is_enough_a_hair_under_is_not() {
        // 30% of the width by 10% of the height: both linear floors are cleared
        // by a wide margin, so the area floor is the only thing this can fail.
        let bx = |h: f64| ImageCandidate {
            x0: 0.0,
            x1: PAGE_W * 0.3,
            y0: 0.0,
            y1: h,
            ..candidate()
        };
        assert_eq!(keep(&[bx(PAGE_H * 0.1)], PAGE_W, PAGE_H, 20).kept.len(), 1);
        assert!(
            keep(&[bx(PAGE_H * 0.1 - 0.01)], PAGE_W, PAGE_H, 20)
                .kept
                .is_empty()
        );
    }

    #[test]
    fn the_background_rule_fires_at_exactly_90_percent_of_the_page_and_20_lines() {
        // A square page, and only here: 90% of 612x792 is not exactly
        // representable as a double, so on a letter sheet there is no box that
        // sits ON this threshold rather than a hair to one side of it.
        const SQ: f64 = 1000.0;
        let bx = |h: f64, page_text_lines: usize| ImageCandidate {
            x0: 0.0,
            x1: SQ,
            y0: 0.0,
            y1: h,
            page_text_lines,
            src_px_width: 400.0,
            src_px_height: 400.0,
            ..candidate()
        };
        assert!(keep(&[bx(SQ * 0.9, 20)], SQ, SQ, 20).kept.is_empty());
        assert_eq!(keep(&[bx(SQ * 0.9, 19)], SQ, SQ, 20).kept.len(), 1);
        assert_eq!(keep(&[bx(SQ * 0.9 - 0.01, 20)], SQ, SQ, 20).kept.len(), 1);
    }
}

mod keep_artwork_furniture {
    use super::*;

    fn logo_on(pages: &[u32]) -> Vec<ImageCandidate> {
        pages
            .iter()
            .map(|&page_num| ImageCandidate {
                page_num,
                hash: "logo".into(),
                ..candidate()
            })
            .collect()
    }

    #[test]
    fn three_distinct_pages_is_furniture_even_in_a_long_script() {
        assert!(
            keep(&logo_on(&[1, 2, 3]), PAGE_W, PAGE_H, 120)
                .kept
                .is_empty()
        );
        assert_eq!(keep(&logo_on(&[1, 2]), PAGE_W, PAGE_H, 120).kept.len(), 2);
    }

    #[test]
    fn the_same_picture_twice_on_one_sheet_is_one_page_not_two() {
        // A storyboard strip repeating a panel down the sheet. Counting
        // placements instead of distinct pages would read this as furniture.
        let at = |y0: f64, y1: f64| ImageCandidate {
            page_num: 4,
            hash: "shot".into(),
            y0,
            y1,
            ..candidate()
        };
        let strip = [at(561.0, 696.0), at(300.0, 435.0), at(60.0, 195.0)];
        assert_eq!(keep(&strip, PAGE_W, PAGE_H, 8).kept.len(), 3);
    }

    #[test]
    fn a_quarter_of_the_sheets_is_furniture_once_it_is_on_two_of_them() {
        let on = |pages: &[u32], page_count: u32| keep(&logo_on(pages), PAGE_W, PAGE_H, page_count);
        assert!(on(&[1, 2], 8).kept.is_empty()); // 25%
        assert_eq!(on(&[1, 2], 9).kept.len(), 2); // 22.2%
    }

    #[test]
    fn one_appearance_is_never_furniture_however_short_the_document() {
        // The share alone would make a lone picture furniture in any document
        // of four sheets or fewer, and certain of it in a one-page document at
        // 100%. Repetition needs something to repeat against.
        for page_count in [1, 2, 3, 4] {
            let c = ImageCandidate {
                page_num: 1,
                hash: "art".into(),
                ..candidate()
            };
            let v = keep(&[c], PAGE_W, PAGE_H, page_count);
            assert_eq!(hashes(&v.kept), vec!["art"], "page_count {page_count}");
        }
    }

    #[test]
    fn the_repetition_tally_counts_every_placement_not_only_the_big_ones() {
        // A letterhead scaled down on two sheets and printed large on a third is
        // still letterhead. Tallying only the placements that clear the size
        // floor would carry the large one.
        let small = |page_num: u32| ImageCandidate {
            page_num,
            hash: "logo".into(),
            x1: 128.0,
            y1: 581.0, // 20 x 20 pt, under every size floor
            ..candidate()
        };
        let mixed = [
            small(1),
            small(2),
            ImageCandidate {
                page_num: 3,
                hash: "logo".into(),
                ..candidate()
            },
        ];
        assert!(keep(&mixed, PAGE_W, PAGE_H, 40).kept.is_empty());
    }

    #[test]
    fn an_image_that_is_both_furniture_and_background_is_simply_gone() {
        let both: Vec<_> = [1, 2, 3]
            .into_iter()
            .map(|page_num| ImageCandidate {
                page_num,
                hash: "mark".into(),
                x0: 0.0,
                y0: 0.0,
                x1: PAGE_W,
                y1: PAGE_H,
                page_text_lines: 40,
                ..candidate()
            })
            .collect();
        assert!(keep(&both, PAGE_W, PAGE_H, 12).kept.is_empty());
    }
}

mod keep_artwork_on_boxes_that_are_not_boxes {
    use super::*;

    #[test]
    fn a_zero_area_box_is_not_art() {
        let a = ImageCandidate {
            x1: 108.0,
            ..candidate()
        };
        let b = ImageCandidate {
            y1: 561.0,
            ..candidate()
        };
        assert!(keep(&[a], PAGE_W, PAGE_H, 20).kept.is_empty());
        assert!(keep(&[b], PAGE_W, PAGE_H, 20).kept.is_empty());
    }

    #[test]
    fn an_inverted_box_is_not_art_in_either_axis_or_both() {
        let x = ImageCandidate {
            x0: 288.0,
            x1: 108.0,
            ..candidate()
        };
        let y = ImageCandidate {
            y0: 696.0,
            y1: 561.0,
            ..candidate()
        };
        assert!(keep(&[x], PAGE_W, PAGE_H, 20).kept.is_empty());
        assert!(keep(&[y], PAGE_W, PAGE_H, 20).kept.is_empty());
        // Both inverted multiplies back to a POSITIVE area, so an area-only
        // floor would wave this one through.
        let both = ImageCandidate {
            x0: 288.0,
            x1: 108.0,
            y0: 696.0,
            y1: 561.0,
            ..candidate()
        };
        assert!(keep(&[both], PAGE_W, PAGE_H, 20).kept.is_empty());
    }

    #[test]
    fn a_box_with_no_number_in_it_is_not_art() {
        // Every `<` in a threshold test is false against NaN, so the naive form
        // of each rule would KEEP this and put `w=NaN` in the .fountain.
        let a = ImageCandidate {
            x1: f64::NAN,
            ..candidate()
        };
        let b = ImageCandidate {
            y0: f64::NAN,
            ..candidate()
        };
        let c = ImageCandidate {
            src_px_width: f64::NAN,
            ..candidate()
        };
        assert!(keep(&[a], PAGE_W, PAGE_H, 20).kept.is_empty());
        assert!(keep(&[b], PAGE_W, PAGE_H, 20).kept.is_empty());
        assert!(keep(&[c], PAGE_W, PAGE_H, 20).kept.is_empty());
    }

    #[test]
    fn a_page_with_no_size_yields_no_art_rather_than_infinities() {
        // page_text_lines 0, and it has to be: divided by a zero page the
        // fractions come out Infinity, and Infinity is over the BACKGROUND area
        // threshold as well as every floor, so a candidate on a text-heavy page
        // would be dropped by the background rule and prove nothing about the
        // guard.
        let c = ImageCandidate {
            page_text_lines: 0,
            ..candidate()
        };
        let none = ArtworkVerdict {
            kept: vec![],
            over_cap: 0,
            dropped_by: NO_DROPS,
        };
        assert_eq!(keep(std::slice::from_ref(&c), 0.0, PAGE_H, 20), none);
        assert_eq!(keep(&[c], PAGE_W, 0.0, 20), none);
    }
}

mod the_thresholds_still_mean_what_tools_make_fixture_py_says_they_mean {
    // tests/fixtures/images.pdf is six pictures on five sheets, and
    // make-fixture.py states each one's margin to the ONE threshold that
    // governs it. Reproducing those boxes here catches a retune the moment it
    // is made, in the file that made it, rather than in an integration test
    // that has to open a PDF — and it catches the direction make-fixture.py
    // warns about, a KEPT image quietly becoming dropped, which the generator
    // itself cannot notice.
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn c(
        page_num: u32,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        src_px_width: f64,
        src_px_height: f64,
        hash: &str,
        page_text_lines: usize,
    ) -> ImageCandidate {
        ImageCandidate {
            page_num,
            x0,
            y0,
            x1,
            y1,
            src_px_width,
            src_px_height,
            hash: hash.into(),
            page_text_lines,
        }
    }

    fn fixture() -> Vec<ImageCandidate> {
        // LOGO = { x0: 376, y0: 36, x1: 576, y1: 136 }: PAGE_W - 200 - 36, 36
        let mut f = vec![
            c(1, 45.0, 45.0, 567.0, 747.0, 261.0, 351.0, "imCard", 5),
            c(2, 108.0, 561.0, 288.0, 696.0, 240.0, 180.0, "imPanel", 12),
            c(3, 108.0, 400.0, 238.0, 530.0, 16.0, 16.0, "imTiny", 12),
            c(3, 300.0, 440.0, 320.0, 460.0, 200.0, 200.0, "imSmall", 12),
            c(4, 0.0, 0.0, 612.0, 792.0, 120.0, 160.0, "imMark", 29),
        ];
        f.extend(
            [1, 2, 3, 4, 5]
                .into_iter()
                .map(|p| c(p, 376.0, 36.0, 576.0, 136.0, 100.0, 50.0, "imLogo", 12)),
        );
        f
    }

    #[test]
    fn two_of_the_six_survive_the_title_card_and_the_panel() {
        let v = keep(&fixture(), PAGE_W, PAGE_H, 5);
        assert_eq!(hashes(&v.kept), vec!["imCard", "imPanel"]);
        assert_eq!(v.over_cap, 0);
    }

    #[test]
    fn each_of_the_four_is_dropped_by_the_rule_make_fixture_py_names() {
        // Asked directly, now that the verdict attributes a drop. This used to
        // isolate one hash and re-run, which only worked because this fixture
        // is separable by construction — no real script is, and a corpus pass
        // needs the histogram rather than a bisection.
        let v = keep(&fixture(), PAGE_W, PAGE_H, 5);
        assert_eq!(
            v.dropped_by,
            DroppedBy {
                pixels: 1, // imTiny, 16x16 against 32
                width: 1,  // imSmall, 3.3% against 10% — and width is asked first
                height: 0,
                area: 0,
                furniture: 5,  // imLogo, one per sheet: the count is placements
                background: 1, // imMark, full bleed under 29 lines
            }
        );
    }

    #[test]
    fn the_four_drops_are_attributed_to_the_rule_and_not_merely_to_some_rule() {
        // Each fixture picture, repaired at exactly the threshold that governs
        // it, becomes art. That is the claim make-fixture.py's table makes, and
        // it is the one an attribution count alone cannot make: a candidate
        // failing two rules would report only the first.
        let only = |hash: &str| -> Vec<ImageCandidate> {
            fixture().into_iter().filter(|c| c.hash == hash).collect()
        };
        let survives = |cs: &[ImageCandidate]| !keep(cs, PAGE_W, PAGE_H, 5).kept.is_empty();

        // imTiny clears all three size floors at 130x130pt: real pixels and it is art.
        let tiny: Vec<_> = only("imTiny")
            .into_iter()
            .map(|c| ImageCandidate {
                src_px_width: 200.0,
                src_px_height: 200.0,
                ..c
            })
            .collect();
        assert!(survives(&tiny));
        // imLogo clears every SIZE floor: on one sheet, only repetition was against it.
        assert!(survives(&only("imLogo")[..1]));
        // imMark clears everything but the text count on its sheet.
        let mark: Vec<_> = only("imMark")
            .into_iter()
            .map(|c| ImageCandidate {
                page_text_lines: 19,
                ..c
            })
            .collect();
        assert!(survives(&mark));
        // imSmall is the exception, and make-fixture.py says so: it fails all
        // three size floors at once, so no single repair rescues it. Widening it
        // alone moves the verdict to the next floor rather than to art, which is
        // exactly what "first rule to fire" means.
        let widened: Vec<_> = only("imSmall")
            .into_iter()
            .map(|c| ImageCandidate {
                x1: c.x0 + 200.0,
                ..c
            })
            .collect();
        assert!(!survives(&widened));
        assert_eq!(
            fired_rules(&keep(&widened, PAGE_W, PAGE_H, 5).dropped_by),
            vec![("height", 1)]
        );
    }
}

mod full_page_area_frac {
    use super::*;

    #[test]
    fn stays_inside_the_band_its_own_comment_describes() {
        // Exported for the ImageRef builder and not consumed in this module, so
        // no other test here constrains it: a mutation sweep found that raising
        // it to 0.75 survives everything. A BAND and not an exact pin, because
        // this is one of the numbers a corpus retune is expected to move, and a
        // test that forbids a legitimate retune gets deleted rather than
        // consulted.
        //
        // Ceiling: the fixture's own title card, 75.6% of its page. Above that
        // the one image this constant exists to promote silently stops being
        // full-page, with every suite green. Floor: a storyboard strip taking up
        // to 60% of a sheet must NOT qualify, which is the "half-page strip does
        // not count" half of the comment.
        let fixture_title_card = (522.0 * 702.0) / (612.0 * 792.0);
        let frac = std::hint::black_box(FULL_PAGE_AREA_FRAC);
        assert!(frac <= fixture_title_card, "{frac} <= {fixture_title_card}");
        assert!(frac >= 0.6, "{frac} >= 0.6");
    }
}

mod is_full_page_tests {
    // The units are the whole risk: two PERCENTAGES compared against a
    // FRACTION. Both arguments 90 must not read as 8100x the bar.
    use super::*;

    #[test]
    fn the_fixture_title_card_qualifies_and_the_fixture_panel_does_not() {
        // tools/make-fixture.py: imCard is 522x702pt and imPanel 180x135pt, both
        // on a 612x792 sheet — 75.6% of the page against 5.0%.
        assert!(is_full_page(
            (522.0 / 612.0) * 100.0,
            (702.0 / 792.0) * 100.0
        ));
        assert!(!is_full_page(
            (180.0 / 612.0) * 100.0,
            (135.0 / 792.0) * 100.0
        ));
    }

    #[test]
    fn the_threshold_is_the_area_and_it_is_inclusive() {
        // 70% x 100% is exactly FULL_PAGE_AREA_FRAC. A `>` instead of `>=` would
        // read a full-bleed strip of exactly the bar as inline.
        assert!(is_full_page(FULL_PAGE_AREA_FRAC * 100.0, 100.0));
        assert!(!is_full_page(FULL_PAGE_AREA_FRAC * 100.0 - 0.1, 100.0));
    }

    #[test]
    fn neither_dimension_decides_alone() {
        // Full width and a third of the height is a banner, not a page; full
        // height and a third of the width is a sidebar.
        assert!(!is_full_page(100.0, 33.0));
        assert!(!is_full_page(33.0, 100.0));
    }
}

mod column_width_pct_remaps_a_page_share_to_a_column_share {
    // The one judgement in this file that DESCRIBES a kept image rather than
    // selecting one, so keep_artwork constrains none of it. lib.rs calls it for
    // every image element; these are the arithmetic's own tests.
    use super::*;

    #[test]
    fn the_reference_is_the_6_0_inch_text_column_not_the_8_5_inch_sheet() {
        // docs/screenplay-format-reference.md: L 1.5", R 1.0", body 6.0". Half a
        // sheet is 71% of that column, and half a COLUMN is 35% of the sheet.
        assert_eq!(column_width_pct(50.0), 71.0);
        assert_eq!(column_width_pct(35.3), 50.0);
    }

    #[test]
    fn an_image_exactly_as_wide_as_the_column_comes_out_at_100() {
        // 6.0 / 8.5 = 70.6%. The number the whole remap is anchored on: get the
        // fraction wrong and this is the assertion that says so.
        assert_eq!(column_width_pct(70.6), 100.0);
    }

    #[test]
    fn anything_wider_than_the_column_is_clamped_to_it() {
        // The fixture's title card is 85.3% of its sheet — 121% of the column.
        assert_eq!(column_width_pct((522.0 / 612.0) * 100.0), 100.0);
        assert_eq!(column_width_pct(100.0), 100.0);
    }

    #[test]
    fn the_floor_holds_and_keep_artwork_cannot_reach_it() {
        // MIN_WIDTH_FRAC already refuses anything under 10% of the page, and
        // 10 / 0.706 is 14 — so the floor only ever answers a caller that
        // skipped the filters. Pinned anyway: nothing else in either suite
        // touches it.
        assert_eq!(column_width_pct(0.0), 5.0);
        assert_eq!(column_width_pct(1.0), 5.0);
        assert_eq!(column_width_pct(10.0), 14.0);
    }

    #[test]
    fn a_width_that_is_not_a_number_lands_on_the_floor_not_on_nan() {
        // The house `!(x > floor)` form. A NaN through the naive clamp reaches
        // the [[img: ...]] note as `NaN`, which is an invalid CSS length a long
        // way from whatever produced it.
        assert_eq!(column_width_pct(f64::NAN), 5.0);
    }
}

mod dropped_by_attributes_a_drop_to_the_rule_that_caused_it {
    use super::*;

    fn fired(cs: &[ImageCandidate]) -> Vec<(&'static str, u32)> {
        fired_rules(&keep(cs, PAGE_W, PAGE_H, 20).dropped_by)
    }

    #[test]
    fn each_rule_reports_under_its_own_name() {
        let px = ImageCandidate {
            src_px_width: 16.0,
            src_px_height: 16.0,
            ..candidate()
        };
        assert_eq!(fired(&[px]), vec![("pixels", 1)]);
        // 5% of the width by 70% of the height: 3.5% of the page, so it CLEARS
        // the area floor and width is genuinely alone. (At half the sheet tall
        // it would be 2.5% and fail area as well, reporting `width` only
        // because width is asked first — which would prove ordering, not
        // attribution.)
        let thin = ImageCandidate {
            x0: 0.0,
            x1: PAGE_W * 0.05,
            y0: 0.0,
            y1: PAGE_H * 0.7,
            ..candidate()
        };
        assert_eq!(fired(&[thin]), vec![("width", 1)]);
        // Overhanging the sheet at 2.8% of its height: 3.26% of the page, so
        // the area floor is cleared and height is alone. Only an overhanging
        // box can be in this state at all — see MIN_HEIGHT_FRAC.
        let low = ImageCandidate {
            x0: -50.0,
            x1: 662.0,
            y0: 0.0,
            y1: PAGE_H * 0.028,
            ..candidate()
        };
        assert_eq!(fired(&[low]), vec![("height", 1)]);
        let small = ImageCandidate {
            x0: 0.0,
            x1: PAGE_W * 0.3,
            y0: 0.0,
            y1: PAGE_H * 0.1 - 0.01,
            ..candidate()
        };
        assert_eq!(fired(&[small]), vec![("area", 1)]);
        let logos: Vec<_> = [1, 2, 3]
            .into_iter()
            .map(|page_num| ImageCandidate {
                page_num,
                hash: "logo".into(),
                ..candidate()
            })
            .collect();
        assert_eq!(fired(&logos), vec![("furniture", 3)]);
        let bg = ImageCandidate {
            x0: 0.0,
            y0: 0.0,
            x1: PAGE_W,
            y1: PAGE_H,
            page_text_lines: 20,
            ..candidate()
        };
        assert_eq!(fired(&[bg]), vec![("background", 1)]);
    }

    #[test]
    fn a_candidate_failing_several_rules_reports_the_first_one() {
        // 20x20pt at 8x8px fails the pixel floor, both linear floors and the
        // area floor. Reported as `pixels` because that is the cheapest
        // question and it is asked first — a histogram is only readable if the
        // attribution rule is stated, and this is it.
        let all = ImageCandidate {
            src_px_width: 8.0,
            src_px_height: 8.0,
            x1: 128.0,
            y1: 581.0,
            ..candidate()
        };
        assert_eq!(fired(&[all]), vec![("pixels", 1)]);
        // Width before height: a box that is under both reports width.
        let both = ImageCandidate {
            x0: 0.0,
            x1: PAGE_W * 0.05,
            y0: 0.0,
            y1: PAGE_H * 0.01,
            ..candidate()
        };
        assert_eq!(fired(&[both]), vec![("width", 1)]);
    }

    #[test]
    fn every_candidate_is_accounted_for_exactly_once() {
        // The property that makes the histogram usable on a corpus: kept + over
        // the cap + one rule each for the rest, adding up to what came in. A
        // double-count or a silent skip would show here and nowhere else.
        let mut mixed = lookbook(65); // 60 kept, 5 over cap
        mixed.push(ImageCandidate {
            page_num: 90,
            hash: "tiny".into(),
            src_px_width: 4.0,
            src_px_height: 4.0,
            ..candidate()
        });
        mixed.push(ImageCandidate {
            page_num: 91,
            hash: "thin".into(),
            x0: 0.0,
            x1: 12.0,
            y0: 0.0,
            y1: PAGE_H * 0.5,
            ..candidate()
        });
        mixed.extend([92, 93, 94].into_iter().map(|page_num| ImageCandidate {
            page_num,
            hash: "logo".into(),
            ..candidate()
        }));
        mixed.push(ImageCandidate {
            page_num: 95,
            hash: "mark".into(),
            x0: 0.0,
            y0: 0.0,
            x1: PAGE_W,
            y1: PAGE_H,
            page_text_lines: 40,
            ..candidate()
        });
        let v = keep(&mixed, PAGE_W, PAGE_H, 96);
        let dropped = total_dropped(&v.dropped_by) as usize;
        assert_eq!(v.kept.len() + v.over_cap + dropped, mixed.len());
        assert_eq!(v.kept.len(), 60);
        assert_eq!(v.over_cap, 5);
        assert_eq!(
            v.dropped_by,
            DroppedBy {
                pixels: 1,
                width: 1,
                height: 0,
                area: 0,
                furniture: 3,
                background: 1,
            }
        );
    }

    #[test]
    fn the_cap_is_not_a_rule_so_its_victims_never_appear_in_dropped_by() {
        // A caller warning on `over_cap` and a caller reading the histogram are
        // asking different questions; conflating them would make every lookbook
        // look like a filter problem.
        let v = keep(&lookbook(70), PAGE_W, PAGE_H, 70);
        assert_eq!(v.over_cap, 10);
        assert_eq!(v.dropped_by, NO_DROPS);
    }
}

//! Port of tests/parser-pipeline.test.ts.
//!
//! The first direct tests of the post-extraction pipeline (`parse_lines`):
//! everything else exercises it only through fixture PDFs.
//!
//! The one test that opens a real PDF (`images.pdf`) reads the oracle's dump
//! of it instead (`common::extract`), since sp-pdf is not ready.
//!
//! Serializer and fountain-js halves moved to the crates that own them; the
//! parser-level halves of the same tests are ported here:
//!
// MOVED: "a cue at the foot of a page keeps the speech under the next page plate": the
//   `toFountain(sp, undefined, markers)` string needs sp-fountain toFountain, ports with
//   that crate. The element-type assertion is ported below.
// MOVED: "a plate BELOW the cue does not re-split the rejoined speech": the toFountain
//   string needs sp-fountain toFountain. The element-type assertion is ported below.
// MOVED: "a plate ABOVE the cue does not switch the rejoin off": the toFountain string
//   needs sp-fountain toFountain. The element-type assertion is ported below.
// MOVED: "and the cost is a misplaced picture, nothing more": every assertion is on
//   toFountain output and fountain-js tokens; needs sp-fountain toFountain and
//   fountain-js tokens, ports with that crate.
// MOVED: "an image between a cue and its speech costs NOTHING in ONE column": every
//   assertion is on toFountain output; needs sp-fountain toFountain, ports with that crate.
// All five moved halves are in sp-fountain/tests/parser_pipeline.rs.

mod common;

use sp_model::{CharacterInfo, ElementType, RawImage, RawLine, ScreenplayElement};
use sp_parser::parse_lines;

fn line(text: &str, indent: f64, y: f64, page_num: u32) -> RawLine {
    RawLine {
        text: text.to_string(),
        indent,
        y,
        page_num,
        styled: None,
        dual_right: None,
        fonts: None,
        fmt: None,
    }
}

/// The distinct values of an iterator, in first-seen order (a JS `Set`).
fn distinct<T: PartialEq>(it: impl Iterator<Item = T>) -> Vec<T> {
    let mut out = Vec::new();
    for v in it {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

/// A miniature script as extracted lines. The title page is the trap: a
/// centered uppercase title lands at ~38% indent, square inside the
/// character-cue band (35-50), and used to walk straight into the roster.
fn miniature() -> Vec<RawLine> {
    vec![
        // page 1 — title page, centered lines
        line("THE MOTH ARCHIVE", 38.0, 500.0, 1),
        line("written by", 44.0, 452.0, 1),
        line("A. N. Placeholder", 39.0, 428.0, 1),
        // page 2 — the script proper
        line("INT. READING ROOM - NIGHT", 17.0, 700.0, 2),
        line(
            "The lamp flickers, thinks better of it, and holds.",
            17.0,
            676.0,
            2,
        ),
        line("NOOR", 43.0, 652.0, 2),
        line(
            "Box forty-one. No label, no date, no excuse.",
            29.0,
            640.0,
            2,
        ),
        line("ELIAS", 43.0, 616.0, 2),
        line("Sign it or say why not.", 29.0, 604.0, 2),
        line("INT. STAIRWELL NINE - LATER", 17.0, 580.0, 2),
        line(
            "Coffee in enamel cups. Nobody drinks; everybody holds.",
            17.0,
            556.0,
            2,
        ),
        line("NOOR", 43.0, 532.0, 2),
        line("Lock the cage. Leave the light.", 29.0, 520.0, 2),
    ]
}

mod parse_lines_metadata {
    use super::*;

    #[test]
    fn a_title_landing_in_the_cue_indent_band_stays_out_of_the_roster() {
        let sp = parse_lines(&miniature(), &[]);
        let names: Vec<&str> = sp.characters.iter().map(|c| c.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec!["ELIAS", "NOOR"]);
        assert!(!names.contains(&"THE MOTH ARCHIVE"));
    }

    #[test]
    fn dialogue_counts_drive_the_roster_order() {
        let sp = parse_lines(&miniature(), &[]);
        assert_eq!(sp.characters[0].name, "NOOR");
        assert_eq!(sp.characters[0].dialogue_count, 2);
    }

    #[test]
    fn scenes_and_pages_come_out_of_the_same_pass() {
        let sp = parse_lines(&miniature(), &[]);
        assert_eq!(sp.scenes.len(), 2);
        assert_eq!(sp.page_count, 2);
    }
}

// Classification (priority 9) runs BEFORE suppressBoilerplate, so anything
// the mini-slug rule claims leaves the recurrence layer's view. A left-flush
// all-caps watermark is mini-slug-shaped, and it recurs by definition — the
// two passes have to be told about each other or every page grows a heading.
mod mini_slugs_and_the_recurrence_suppressor {
    use super::*;

    /// `mark_on_pages` pages carry `mark` at the action margin; every page also
    /// carries a heading and a line of action so the doc has a real page count.
    fn pages(page_count: u32, mark: &str, mark_on_pages: &[u32]) -> Vec<RawLine> {
        let mut lines = Vec::new();
        for p in 1..=page_count {
            if mark_on_pages.contains(&p) {
                lines.push(line(mark, 17.0, 720.0, p));
            }
            lines.push(line(&format!("INT. ROOM {p} - DAY"), 17.0, 660.0, p));
            lines.push(line(
                &format!("Someone crosses the floor on page {p}."),
                17.0,
                600.0,
                p,
            ));
        }
        lines
    }
    fn all(n: u32) -> Vec<u32> {
        (1..=n).collect()
    }
    fn types_of(ls: &[RawLine], text: &str) -> Vec<ElementType> {
        distinct(
            parse_lines(ls, &[])
                .elements
                .into_iter()
                .filter(|e| e.text == text)
                .map(|e| e.kind),
        )
    }

    #[test]
    fn a_per_page_watermark_does_not_escape_into_a_heading_on_every_page() {
        // 10 pages, threshold max(3, 40%) = 4. Before mini-slug was a recurrence
        // candidate this rendered ten bold micro-headings.
        assert!(
            !types_of(&pages(10, "CONFIDENTIAL", &all(10)), "CONFIDENTIAL")
                .contains(&ElementType::MiniSlug)
        );
    }

    #[test]
    fn a_suppressed_mini_slug_is_hidden_on_the_same_terms_as_action() {
        // page-number is the suppressor's sink and every consumer skips it. One
        // rule for both types, so a mark that types action on one page and
        // mini-slug on the next cannot come out hidden here and visible there.
        assert_eq!(
            types_of(
                &pages(10, "PROPERTY OF THE STUDIO", &all(10)),
                "PROPERTY OF THE STUDIO"
            ),
            vec![ElementType::PageNumber]
        );
    }

    #[test]
    fn a_mini_slug_that_legitimately_repeats_under_the_threshold_survives() {
        // 3 of 20 pages against a threshold of 8. Mini-slugs do repeat, so the
        // threshold is what has to hold — and it does: the widest page-spread of
        // any real mini-slug in the corpus is 14%, against a 40% bar.
        assert_eq!(
            types_of(&pages(20, "LATER", &[4, 9, 15]), "LATER"),
            vec![ElementType::MiniSlug]
        );
    }
}

mod scene_numbers_vs_page_numbers {
    // Both arrive as `page-number` elements, because a standalone shooting-script
    // scene number is deliberately typed that way so it stays hidden
    // (classify.ts, "Shooting script scene numbers"). attachSceneNumbers then
    // promotes it onto the following heading.
    //
    // The trap: a BARE page number like "1." is caught by PAGE_NUMBER_BARE at
    // priority 1 and is textually indistinguishable from a scene number, so any
    // page that OPENS with a scene heading used to donate its page number to
    // that scene. Ordinary scripts do that constantly.
    use super::*;

    fn page_opening_on_heading(number_text: &str) -> Vec<RawLine> {
        vec![
            line(number_text, 87.0, 740.0, 2), // top-right gutter, first thing on page 2
            line("INT. ARCHIVE - NIGHT", 18.0, 700.0, 2),
            line("Someone is already here.", 18.0, 676.0, 2),
            line("BUNNY", 44.0, 640.0, 2),
            line("You came back.", 26.0, 628.0, 2),
        ]
    }

    fn scene_of(els: &[ScreenplayElement]) -> Option<&ScreenplayElement> {
        els.iter().find(|e| e.kind == ElementType::Scene)
    }

    #[test]
    fn a_bare_page_number_is_not_absorbed_as_a_scene_number() {
        let els = parse_lines(&page_opening_on_heading("2."), &[]).elements;
        let scene = scene_of(&els);
        assert!(scene.is_some());
        assert_eq!(scene.unwrap().scene_number, None);
    }

    #[test]
    fn a_shooting_script_scene_number_is_still_attached() {
        // The documented path must keep working: this is why the rule exists.
        let els = parse_lines(&page_opening_on_heading("1A."), &[]).elements;
        let scene = scene_of(&els);
        assert!(scene.is_some());
        assert_eq!(scene.unwrap().scene_number.as_deref(), Some("1A."));
    }

    #[test]
    fn a_compound_shooting_script_number_is_still_attached() {
        let els = parse_lines(&page_opening_on_heading("2.2."), &[]).elements;
        let scene = scene_of(&els);
        assert_eq!(scene.unwrap().scene_number.as_deref(), Some("2.2."));
    }

    #[test]
    fn the_page_number_is_still_stripped_from_the_body_either_way() {
        let els = parse_lines(&page_opening_on_heading("2."), &[]).elements;
        assert!(
            els.iter()
                .any(|e| e.kind == ElementType::PageNumber && e.text == "2.")
        );
    }
}

mod page_furniture_between_a_cue_and_its_dialogue {
    // A cue as the LAST line of a page, its speech resuming on the next page,
    // with the printed page number in between. Found on a real Kindle: speech
    // thirty-seven of the proof sheet rendered as "BUNNY Speech thirty-seven."
    // in one run instead of a cue and a speech.
    //
    // Page furniture is stripped from the output, so it must not influence
    // structure. Letting it reset the speaker breaks the speech in half.
    use super::*;

    fn cue_stranded_at_page_end() -> Vec<RawLine> {
        vec![
            line("BUNNY", 44.0, 72.0, 1), // last line of page 1
            line("13.", 87.0, 756.0, 2),  // printed page number, top of page 2
            line("Speech thirty-seven. Short.", 29.0, 720.0, 2),
            line("CASSIUS", 44.0, 696.0, 2),
            line("And the reply.", 29.0, 684.0, 2),
        ]
    }

    #[test]
    fn dialogue_after_the_page_number_still_belongs_to_the_cue() {
        let els = parse_lines(&cue_stranded_at_page_end(), &[]).elements;
        let speech = els
            .iter()
            .find(|e| e.text.starts_with("Speech thirty-seven"));
        assert!(speech.is_some());
        let speech = speech.unwrap();
        assert_eq!(speech.kind, ElementType::Dialogue);
        assert_eq!(speech.character.as_deref(), Some("BUNNY"));
    }

    #[test]
    fn the_cue_itself_still_classifies_as_a_character() {
        let els = parse_lines(&cue_stranded_at_page_end(), &[]).elements;
        assert_eq!(
            els.iter().find(|e| e.text == "BUNNY").map(|e| e.kind),
            Some(ElementType::Character)
        );
    }

    #[test]
    fn the_page_number_is_still_stripped() {
        let els = parse_lines(&cue_stranded_at_page_end(), &[]).elements;
        assert_eq!(
            els.iter().find(|e| e.text == "13.").map(|e| e.kind),
            Some(ElementType::PageNumber)
        );
    }

    #[test]
    fn furniture_does_not_invent_a_speaker_where_there_was_none() {
        // Action after a page number must stay action: transparency must not
        // become "attach anything that follows furniture to the last speaker".
        let els = parse_lines(
            &[
                line("BUNNY", 44.0, 72.0, 1),
                line("Her line.", 29.0, 60.0, 1),
                line("13.", 87.0, 756.0, 2),
                line("She crosses to the window.", 18.0, 720.0, 2),
            ],
            &[],
        )
        .elements;
        let act = els.iter().find(|e| e.text.starts_with("She crosses"));
        assert_eq!(act.unwrap().kind, ElementType::Action);
    }
}

mod a_speech_split_by_a_page_boundary_is_one_speech {
    // Device-confirmed on a Kindle: a speech spanning a page in the SOURCE pdf
    // stayed two elements, so the book rendered "...at the top or the" and
    // "bottom of a page..." as two paragraphs with a hard break mid-sentence.
    //
    // The earlier furniture fix gave both halves the right speaker. It never
    // merged their TEXT. Registry 8's rejoin only fires when the script prints
    // (MORE)/(CONT'D); celtx does not, and shows 9 of these seams.
    use super::*;

    fn speech_across_a_page() -> Vec<RawLine> {
        vec![
            line("ODILE", 44.0, 200.0, 1),
            line(
                "Speech nineteen. If this line sits alone at the top or the",
                29.0,
                188.0,
                1,
            ),
            line("13.", 87.0, 756.0, 2),
            line("bottom of a page, the keep failed.", 29.0, 720.0, 2),
        ]
    }

    fn dialogues(els: &[ScreenplayElement]) -> Vec<&ScreenplayElement> {
        els.iter()
            .filter(|e| e.kind == ElementType::Dialogue)
            .collect()
    }

    #[test]
    fn the_halves_merge_into_a_single_dialogue_element() {
        let els = parse_lines(&speech_across_a_page(), &[]).elements;
        let speeches = dialogues(&els);
        assert_eq!(speeches.len(), 1);
        assert_eq!(
            speeches[0].text,
            "Speech nineteen. If this line sits alone at the top or the bottom of a page, the keep failed."
        );
    }

    #[test]
    fn the_merged_speech_keeps_its_speaker() {
        let els = parse_lines(&speech_across_a_page(), &[]).elements;
        assert_eq!(
            els.iter()
                .find(|e| e.kind == ElementType::Dialogue)
                .and_then(|e| e.character.as_deref()),
            Some("ODILE")
        );
    }

    #[test]
    fn two_different_speakers_across_a_page_are_not_merged() {
        // The merge must key on the character, or a page break between two
        // people's lines would fuse them into one speech.
        let els = parse_lines(
            &[
                line("ODILE", 44.0, 200.0, 1),
                line("Her line.", 29.0, 188.0, 1),
                line("13.", 87.0, 756.0, 2),
                line("WREN", 44.0, 720.0, 2),
                line("His line.", 29.0, 708.0, 2),
            ],
            &[],
        )
        .elements;
        let speeches = dialogues(&els);
        assert_eq!(speeches.len(), 2);
        assert_eq!(
            speeches
                .iter()
                .map(|s| s.character.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("ODILE"), Some("WREN")]
        );
    }

    #[test]
    fn two_paragraphs_of_one_speech_on_the_same_page_stay_separate() {
        // A speech may legitimately hold two paragraphs. They arrive as two
        // dialogue elements on one page, and merging them would destroy a break
        // the writer put there. Only a page turn splits a paragraph in half.
        let els = parse_lines(
            &[
                line("ODILE", 44.0, 300.0, 1),
                line("First paragraph of the speech.", 29.0, 288.0, 1),
                line("Second paragraph, deliberately separate.", 29.0, 250.0, 1),
            ],
            &[],
        )
        .elements;
        assert_eq!(dialogues(&els).len(), 2);
    }

    #[test]
    fn the_merge_does_not_reach_across_an_action_line() {
        // Only page furniture is transparent. Real content between two speeches
        // means they are genuinely separate.
        //
        // Note what the parser already does here, which this test pins rather
        // than changes: action RESETS the speaker, so the line after it is not
        // dialogue at all. The guard that matters is that the first speech does
        // not absorb anything past the action line.
        let els = parse_lines(
            &[
                line("ODILE", 44.0, 200.0, 1),
                line("First half.", 29.0, 188.0, 1),
                line("She crosses to the window.", 18.0, 176.0, 1),
                line("Second half.", 29.0, 164.0, 1),
            ],
            &[],
        )
        .elements;
        let speeches = dialogues(&els);
        assert_eq!(speeches.len(), 1);
        assert_eq!(speeches[0].text, "First half.");
    }
}

mod extraction_returns_pictures_beside_the_text {
    // The pipeline's INPUT, not parseLines: the one place in this file a real
    // PDF is opened, because the claim is about what one page loop hands over.
    // tests/images-fixture.test.ts pins the pictures in detail; this pins that
    // the text-side fields kept their shape when the image fields joined them.
    // (Here the PDF is the oracle's dump of it: see the file header.)
    use super::*;
    use sp_parser::artwork::DroppedBy;

    #[test]
    fn the_images_fixture_keeps_the_art_and_refuses_the_furniture() {
        let result = common::extract("images");
        assert!(!result.lines.is_empty());
        assert_eq!(result.page_count, 5);
        // `typeof result.spacingRepairs === 'number'`: the type says so.
        let _: u32 = result.spacing_repairs;
        assert_eq!(
            result
                .images
                .iter()
                .map(|i| i.file.as_str())
                .collect::<Vec<_>>(),
            vec!["p001-01.png", "p002-01.png"]
        );
        assert_eq!(
            result.images.iter().map(|i| i.page_num).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            result.image_stats.dropped_by,
            DroppedBy {
                pixels: 1,
                width: 1,
                height: 0,
                area: 0,
                furniture: 5,
                background: 1,
            }
        );
    }
}

mod images_merged_into_the_element_stream {
    // parseLines takes two ordered streams and interleaves them (registry #19).
    // Everything here is about POSITION: which element a picture lands next to,
    // and what the metadata passes make of one.
    use super::*;

    /// A `RawImage` as `toRawImages` would hand it over — already named, already
    /// measured against the page. `y` is the TOP edge in PDF user space, the same
    /// axis and direction RawLine uses, so a high y is high on the sheet.
    pub(super) fn image(y: f64, page_num: u32, file: &str) -> RawImage {
        RawImage {
            file: file.to_string(),
            page_num,
            y,
            // 50% of the page is 71% of a 6-inch column, and nowhere near full-page.
            page_width_pct: 50.0,
            page_height_pct: 20.0,
            bytes: vec![1],
        }
    }
    pub(super) fn types_of(ls: &[RawLine], imgs: &[RawImage]) -> Vec<ElementType> {
        parse_lines(ls, imgs)
            .elements
            .into_iter()
            .map(|e| e.kind)
            .collect()
    }
    fn only_image(ls: &[RawLine], imgs: &[RawImage]) -> Vec<ScreenplayElement> {
        parse_lines(ls, imgs)
            .elements
            .into_iter()
            .filter(|e| e.kind == ElementType::Image)
            .collect()
    }
    fn files(els: &[ScreenplayElement]) -> Vec<Option<&str>> {
        els.iter()
            .map(|e| e.img.as_ref().map(|i| i.file.as_str()))
            .collect()
    }

    use ElementType::{Action, Character, Dialogue, Image, PageNumber, Scene};

    #[test]
    fn an_image_lands_between_the_elements_it_sat_between_on_the_page() {
        assert_eq!(
            types_of(
                &[
                    line("INT. WAREHOUSE - DAY", 15.0, 700.0, 1),
                    line("The panel above shows the shot.", 15.0, 500.0, 1),
                ],
                &[image(600.0, 1, "p001-01.png")],
            ),
            vec![Scene, Image, Action]
        );
    }

    #[test]
    fn an_image_above_everything_on_its_page_comes_first() {
        assert_eq!(
            types_of(
                &[
                    line("INT. A - DAY", 15.0, 700.0, 1),
                    line("Someone waits.", 15.0, 600.0, 1)
                ],
                &[image(760.0, 1, "p001-01.png")],
            ),
            vec![Image, Scene, Action]
        );
    }

    #[test]
    fn an_image_below_everything_on_its_page_comes_last() {
        assert_eq!(
            types_of(
                &[
                    line("INT. A - DAY", 15.0, 700.0, 1),
                    line("Someone waits.", 15.0, 600.0, 1)
                ],
                &[image(100.0, 1, "p001-01.png")],
            ),
            vec![Scene, Action, Image]
        );
    }

    #[test]
    fn several_images_and_several_elements_on_one_page_interleave() {
        // The case a single insertion point could not get right: three images,
        // none of them adjacent to each other in the finished stream.
        assert_eq!(
            types_of(
                &[
                    line("INT. A - DAY", 15.0, 700.0, 1),
                    line("Someone waits.", 15.0, 600.0, 1),
                    line("Someone leaves.", 15.0, 500.0, 1),
                ],
                &[
                    image(650.0, 1, "p001-01.png"),
                    image(550.0, 1, "p001-02.png"),
                    image(450.0, 1, "p001-03.png")
                ],
            ),
            vec![Scene, Image, Action, Image, Action, Image]
        );
    }

    #[test]
    fn the_filename_rides_through_untouched_in_the_order_it_was_handed_over() {
        // parseLines does NOT number anything: `toRawImages` assigned these names
        // where it assigned the order, and a second counter here would be a second
        // thing to keep in step. This pins the pass-through, not the naming rule.
        let els = only_image(
            &[line("INT. A - DAY", 15.0, 700.0, 1)],
            &[
                image(600.0, 1, "p001-01.png"),
                image(400.0, 1, "p001-02.png"),
            ],
        );
        assert_eq!(files(&els), vec![Some("p001-01.png"), Some("p001-02.png")]);
        // And it is the element's TEXT as well, which is what keeps the element
        // non-empty: toFountain drops anything that trims to nothing, and an image
        // has no words of its own to put there.
        assert_eq!(
            els.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
            vec!["p001-01.png", "p001-02.png"]
        );
    }

    #[test]
    fn two_images_at_the_same_y_keep_the_order_they_arrived_in() {
        // Equal y is not a tie the merge has to break: it drains the queue in
        // order, so the paint order `toRawImages`'s stable sort preserved survives
        // one more stage. A comparator-based insertion could quietly reverse them.
        assert_eq!(
            files(&only_image(
                &[
                    line("INT. A - DAY", 15.0, 700.0, 1),
                    line("Someone waits.", 15.0, 500.0, 1)
                ],
                &[
                    image(600.0, 1, "p001-01.png"),
                    image(600.0, 1, "p001-02.png")
                ],
            )),
            vec![Some("p001-01.png"), Some("p001-02.png")]
        );
    }

    #[test]
    fn the_rendered_width_is_remapped_from_the_page_to_the_text_column() {
        // 50% of an 8.5" sheet is 71% of the 6.0" column the script is set in.
        assert_eq!(
            only_image(
                &[line("INT. A - DAY", 15.0, 700.0, 1)],
                &[image(600.0, 1, "p001-01.png")]
            )[0]
            .img
            .as_ref()
            .unwrap()
            .width_pct,
            71.0
        );
    }

    #[test]
    fn a_picture_wider_than_the_column_is_asked_to_fill_the_column_no_more() {
        // The title-card case: 85% of the sheet is 121% of the column.
        let wide = RawImage {
            page_width_pct: 85.0,
            ..image(600.0, 1, "p001-01.png")
        };
        let els = only_image(&[line("INT. A - DAY", 15.0, 700.0, 1)], &[wide]);
        assert_eq!(els[0].img.as_ref().unwrap().width_pct, 100.0);
    }

    #[test]
    fn a_full_page_image_is_flagged_an_inline_one_is_not() {
        // Both halves, or the flag could be a constant `true`. 90 x 88 is 79% of
        // the page against the 70% bar; the helper's 50 x 20 is 10%.
        let big_img = RawImage {
            page_width_pct: 90.0,
            page_height_pct: 88.0,
            ..image(750.0, 1, "p001-01.png")
        };
        let big = only_image(&[line("INT. A - DAY", 15.0, 700.0, 1)], &[big_img]);
        assert!(big[0].img.as_ref().unwrap().full_page);
        assert!(
            !only_image(
                &[line("INT. A - DAY", 15.0, 700.0, 1)],
                &[image(600.0, 1, "p001-01.png")]
            )[0]
            .img
            .as_ref()
            .unwrap()
            .full_page
        );
    }

    #[test]
    fn an_images_array_in_the_wrong_order_is_normalized_not_misplaced() {
        // parseLines is exported, so any caller can hand it pictures. Today
        // `toRawImages` hands this half over already sorted, so the sort inside
        // insertImages is a no-op — which is exactly why it needs its own test: a
        // caller that builds RawImage[] some other way must not silently scatter
        // pictures through the book. Handed over backwards on purpose.
        let sp = parse_lines(
            &[
                line("INT. A - DAY", 15.0, 700.0, 1),
                line("Someone waits.", 15.0, 500.0, 1),
                line("INT. B - DAY", 15.0, 700.0, 2),
            ],
            &[
                image(600.0, 2, "p002-01.png"),
                image(600.0, 1, "p001-01.png"),
            ],
        );
        assert_eq!(
            sp.elements.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![Scene, Image, Action, Scene, Image]
        );
        assert_eq!(
            sp.elements
                .iter()
                .filter(|e| e.kind == Image)
                .map(|e| e.img.as_ref().unwrap().file.as_str())
                .collect::<Vec<_>>(),
            vec!["p001-01.png", "p002-01.png"]
        );
    }

    #[test]
    fn an_image_level_with_a_line_sits_below_it_the_two_y_are_different_edges() {
        // A line's y is its BASELINE (extract.ts takes the text item's own origin)
        // and an image's is its TOP edge, so a picture whose y equals a line's y
        // starts where that line's glyphs sit and runs downward. Equality is
        // therefore not a coin toss: the line is above, and goes first.
        assert_eq!(
            types_of(
                &[
                    line("INT. A - DAY", 15.0, 700.0, 1),
                    line("Someone waits.", 15.0, 600.0, 1)
                ],
                &[image(600.0, 1, "p001-01.png")],
            ),
            vec![Scene, Action, Image]
        );
    }

    #[test]
    fn ordering_is_by_page_first_and_only_then_down_the_page() {
        // The image on page 1 sits at the BOTTOM of it (y 100) and the one on
        // page 2 at the top (y 750): compare y alone and they come out reversed.
        assert_eq!(
            types_of(
                &[
                    line("INT. A - DAY", 15.0, 700.0, 1),
                    line("INT. B - DAY", 15.0, 700.0, 2)
                ],
                &[
                    image(100.0, 1, "p001-01.png"),
                    image(750.0, 2, "p002-01.png")
                ],
            ),
            vec![Scene, Image, Image, Scene]
        );
    }

    #[test]
    fn an_image_on_a_page_with_no_text_at_all_still_lands_between_the_pages() {
        // A plate sheet: nothing on page 2 but the picture.
        assert_eq!(
            types_of(
                &[
                    line("INT. A - DAY", 15.0, 700.0, 1),
                    line("INT. C - DAY", 15.0, 700.0, 3)
                ],
                &[image(400.0, 2, "p002-01.png")],
            ),
            vec![Scene, Image, Scene]
        );
    }

    #[test]
    fn an_image_past_the_last_text_element_is_kept_and_counts_toward_page_count() {
        // A closing full-page plate. The tail loop is the only thing that carries
        // it, and the sheet it sits on is a real page of the document.
        let sp = parse_lines(
            &[line("INT. A - DAY", 15.0, 700.0, 1)],
            &[image(700.0, 4, "p004-01.png")],
        );
        assert_eq!(
            sp.elements.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![Scene, Image]
        );
        assert_eq!(sp.page_count, 4);
    }

    #[test]
    fn an_image_is_never_a_speaking_character_and_never_a_scene() {
        let sp = parse_lines(
            &[line("INT. A - DAY", 15.0, 700.0, 1)],
            &[image(600.0, 1, "p001-01.png")],
        );
        assert_eq!(sp.scenes.len(), 1);
        assert_eq!(sp.characters.len(), 0);
    }

    #[test]
    fn scene_element_index_counts_the_images_that_precede_it() {
        // Why insertImages runs BEFORE the metadata passes: elementIndex is an
        // index into the array the caller receives, so it has to be computed over
        // the finished stream.
        let sp = parse_lines(
            &[
                line("INT. A - DAY", 15.0, 700.0, 1),
                line("INT. B - DAY", 15.0, 400.0, 1),
            ],
            &[
                image(760.0, 1, "p001-01.png"),
                image(500.0, 1, "p001-02.png"),
            ],
        );
        assert_eq!(
            sp.elements.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![Image, Scene, Image, Scene]
        );
        assert_eq!(
            sp.scenes
                .iter()
                .map(|s| s.element_index)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[test]
    fn character_first_appearance_counts_the_images_too() {
        // The same index into the same array as `elementIndex`, and it has no
        // consumer today — which is exactly why it is pinned here. A mutation
        // sweep showed that counting characters over the pre-image stream survives
        // everything else in the suite, so the day something reads this field it
        // would have read a stale index with every test green.
        let sp = parse_lines(
            &[
                line("INT. A - DAY", 15.0, 700.0, 1),
                line("NOOR", 43.0, 600.0, 1),
                line("Her line.", 29.0, 588.0, 1),
            ],
            &[image(760.0, 1, "p001-01.png")],
        );
        assert_eq!(
            sp.elements.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![Image, Scene, Character, Dialogue]
        );
        assert_eq!(
            sp.characters,
            vec![CharacterInfo {
                name: "NOOR".into(),
                base_name: "NOOR".into(),
                dialogue_count: 1,
                first_appearance: 2,
            }]
        );
    }

    #[test]
    fn omitting_images_changes_nothing() {
        // TS `parseLines(lines)` and `parseLines(lines, [])` are one call in
        // Rust (`images` is a slice, never absent), so the second half pins that
        // the empty-images call is stable rather than comparing two signatures.
        let lines = [
            line("INT. A - DAY", 15.0, 700.0, 1),
            line("Some action.", 15.0, 600.0, 1),
        ];
        assert_eq!(
            parse_lines(&lines, &[])
                .elements
                .iter()
                .map(|e| e.kind)
                .collect::<Vec<_>>(),
            vec![Scene, Action]
        );
        assert_eq!(
            parse_lines(&lines, &[]).elements,
            parse_lines(&lines, &[]).elements
        );
    }

    #[test]
    fn a_text_element_carries_the_y_of_its_block_which_is_what_the_merge_reads() {
        // Named directly because everything above rests on it: drop `y` in
        // classifyBlock and no image can find an anchor.
        let els = parse_lines(
            &[
                line("INT. A - DAY", 15.0, 700.0, 1),
                line("Someone waits.", 15.0, 600.0, 1),
            ],
            &[],
        )
        .elements;
        assert_eq!(
            els.iter().map(|e| e.y).collect::<Vec<_>>(),
            vec![Some(700.0), Some(600.0)]
        );
    }

    #[test]
    fn a_speech_split_across_a_page_does_not_derail_the_images_after_it() {
        // mergeSplitSpeeches folds the second half into the FIRST element, which
        // keeps page 1's y=188 while sitting between two page-2 elements. The
        // merge must not read that stale anchor as "page 2 is over".
        let els = parse_lines(
            &[
                line("ODILE", 44.0, 200.0, 1),
                line(
                    "Speech nineteen. If this line sits alone at the top or the",
                    29.0,
                    188.0,
                    1,
                ),
                line("13.", 87.0, 756.0, 2),
                line("bottom of a page, the keep failed.", 29.0, 720.0, 2),
                line("INT. NEXT ROOM - DAY", 17.0, 690.0, 2),
            ],
            &[image(700.0, 2, "p002-01.png")],
        )
        .elements;
        assert_eq!(
            els.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![Character, Dialogue, PageNumber, Image, Scene]
        );
        assert_eq!(els.iter().filter(|e| e.kind == Dialogue).count(), 1);
    }

    #[test]
    fn a_plate_between_the_halves_of_a_page_split_speech_does_not_split_it() {
        // Why insertImages runs AFTER mergeSplitSpeeches. That pass joins the
        // halves only when nothing but furniture sits between them, and a plate
        // at the top of page 2, under the page number and above the continuation,
        // sits exactly there. Merged in first, the picture would end the run and
        // one speech would come out as two with the plate between them; merged in
        // after, the continuation is already folded into the first half.
        let els = parse_lines(
            &[
                line("ODILE", 44.0, 200.0, 1),
                line(
                    "Speech nineteen. If this line sits alone at the top or the",
                    29.0,
                    188.0,
                    1,
                ),
                line("13.", 87.0, 756.0, 2),
                line("bottom of a page, the keep failed.", 29.0, 720.0, 2),
                line("INT. NEXT ROOM - DAY", 17.0, 690.0, 2),
            ],
            &[image(740.0, 2, "p002-01.png")],
        )
        .elements;
        assert_eq!(
            els.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![Character, Dialogue, PageNumber, Image, Scene]
        );
        assert_eq!(
            els.iter()
                .filter(|e| e.kind == Dialogue)
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Speech nineteen. If this line sits alone at the top or the bottom of a page, the keep failed."
            ]
        );
    }

    mod a_plate_in_a_page_seam_with_page_markers_on {
        // Each of these pins a serializer rule, and they are asserted HERE rather
        // than only in serialize.test.ts because their reachability is the whole
        // point: hand-built elements can be dismissed as a shape the parser never
        // makes, and these are shapes it makes from ordinary lines. Page 1 opens
        // on a heading so nothing on it is read as a title page.
        //
        // The toFountain halves (with `showPageMarkers: true`) moved: see the
        // file header. The element-stream halves are here.
        use super::super::line;
        use super::{image, types_of};
        use sp_model::ElementType::{Action, Character, Dialogue, Image, Scene};
        use sp_model::RawLine;

        #[test]
        fn a_cue_at_the_foot_of_a_page_keeps_the_speech_under_the_next_page_plate() {
            assert_eq!(
                types_of(
                    &[
                        line("INT. ROOM - DAY", 17.0, 700.0, 1),
                        line("The lamp holds.", 17.0, 400.0, 1),
                        line("BUNNY", 40.0, 72.0, 1), // last line of page 1
                        line("Her line.", 30.0, 700.0, 2), // her speech, under the plate
                        line("INT. NEXT - DAY", 17.0, 660.0, 2),
                    ],
                    &[image(756.0, 2, "p002-01.png")], // the plate, above her speech
                ),
                vec![Scene, Action, Character, Image, Dialogue, Scene]
            );
            // MOVED: toFountain(sp, undefined, markers) ===
            // 'INT. ROOM - DAY\n\nThe lamp holds.\n\n@BUNNY\nHer line.\n\n' +
            // '[[img: p002-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n'
        }

        // A speech split by a page, with the plate somewhere in the seam. The two
        // cases below differ ONLY in the picture's y — below the continuation cue
        // or above it — and they must serialize the same, because where a plate
        // sits on the sheet is not a fact about whether a speech continues.
        fn contd_seam() -> Vec<RawLine> {
            vec![
                line("INT. ROOM - DAY", 17.0, 700.0, 1),
                line("BUNNY", 40.0, 200.0, 1),
                line("First half.", 30.0, 188.0, 1),
                line("BUNNY (CONT'D)", 40.0, 740.0, 2),
                line("second half.", 30.0, 660.0, 2),
                line("INT. NEXT - DAY", 17.0, 600.0, 2),
            ]
        }
        // MOVED: seamOutput, the toFountain string both cases must produce:
        // 'INT. ROOM - DAY\n\n@BUNNY\nFirst half.\nsecond half.\n\n' +
        // '[[img: p002-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n'

        #[test]
        fn a_plate_below_the_cue_does_not_re_split_the_rejoined_speech() {
            // rejoinSplitDialogue drops the repeated cue, so the block is OPEN when
            // the plate arrives — which is the condition that makes this bite.
            assert_eq!(
                types_of(&contd_seam(), &[image(700.0, 2, "p002-01.png")]),
                vec![
                    Scene, Character, Dialogue, Character, Image, Dialogue, Scene
                ]
            );
            // MOVED: toFountain(sp, undefined, markers) === seamOutput
        }

        #[test]
        fn a_plate_above_the_cue_does_not_switch_the_rejoin_off() {
            // A plate at the top of a sheet with the continuation cue under it is
            // ordinary layout, and it sits in the slot that prepare's rejoin loop
            // would read as "the previous element" if the loop did not look past
            // images. Read that way, the repeated cue survives and the reader gets a
            // duplicated (CONT'D) with one speech in two blocks. Same lines, same
            // picture, 60pt higher.
            assert_eq!(
                types_of(&contd_seam(), &[image(760.0, 2, "p002-01.png")]),
                vec![
                    Scene, Character, Dialogue, Image, Character, Dialogue, Scene
                ]
            );
            // MOVED: toFountain(sp, undefined, markers) === seamOutput
        }
    }

    mod the_exception_an_image_inside_a_dual_dialogue_region {
        // The one place the element stream is NOT ordered by descending y.
        // deinterleaveDualDialogue emits the left column and then the right, and
        // the right column's y values restart at the top of the exchange — so the
        // first element below the picture is a left-column one, and that is where
        // it goes, in front of that column's first speech instead of after the
        // whole exchange.
        //
        // Pinned rather than fixed HERE: a picture printed inside a two-column
        // simultaneous speech is not a document the corpus contains, and the cost
        // of the misplacement is bounded — serialize.ts's cue lookahead sees
        // past an image and its emitter holds the note until the speech closes, so
        // a misplaced picture stays a misplaced picture and does not take a speech
        // with it. If these go red because someone gave both streams a second
        // ordering key, delete the tests rather than the fix.
        use super::super::line;
        use super::{image, types_of};
        use sp_model::ElementType::{Character, Dialogue, Image};
        use sp_model::RawLine;

        // As extraction hands it over: left column, then right, with the right
        // column's cue flagged `dualRight` (extract.ts sets it on that line only).
        fn dual_exchange() -> Vec<RawLine> {
            vec![
                line("BUNNY", 40.0, 500.0, 1),
                line("Left one.", 30.0, 488.0, 1),
                RawLine {
                    dual_right: Some(true),
                    ..line("CASSIUS", 40.0, 500.0, 1)
                },
                line("Right one.", 30.0, 488.0, 1),
            ]
        }

        #[test]
        fn lands_among_the_left_column_instead_of_after_the_exchange() {
            assert_eq!(
                types_of(&dual_exchange(), &[image(494.0, 1, "p001-01.png")]),
                vec![Character, Image, Dialogue, Character, Dialogue]
            );
        }

        // "and the cost is a misplaced picture, nothing more" and "an image
        // between a cue and its speech costs NOTHING in ONE column" are MOVED
        // whole: see the file header.
    }
}

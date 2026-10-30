//! Port of tests/extract.test.ts.
//!
//! The pure functions (`group_items_into_lines`, `collect_underline_marks`,
//! `mark_underlines_item`, `family_bucket`, `stamp_line_fmt`,
//! `repair_phantom_spaces`) take hand-built inputs exactly as the TypeScript
//! did. pdf.js operator lists become `Vec<Op>`: an `OPS.x` paint number is
//! the op NAME here, and a glyph array is its joined unicode.
//!
//! The tests that read `tests/fixtures/*.pdf` read the oracle's dump of it
//! instead (`common::facts`), since sp-pdf is not ready.
//!
//! The one check on a real script ("a PDF that needs repairs reports a
//! non-zero count") named a file by title; here it runs over the real-script
//! dumps, by number, wherever the oracle itself counted more than 50 repairs,
//! and skips quietly when there are none (CI, and every dump present today).
//!
//! The `null` operator list half of "a malformed operator list yields no
//! marks instead of throwing" is enforced by the type (`&[Op]` cannot be
//! null) and is not ported; the `[null]` args half is.

mod common;

use sp_model::{FamilyBucket, Fmt, FontRun, RawLine, SizeStep};
use sp_parser::extract::{
    PageProgress, TextItem, UnderlineMark, collect_underline_marks, extract_document, extract_text,
    family_bucket, group_items_into_lines, mark_underlines_item, repair_phantom_spaces,
    stamp_line_fmt,
};
use sp_parser::facts::{Op, PaintOp};

fn item(s: &str, x: f64, y: f64) -> TextItem {
    TextItem {
        str: s.into(),
        transform: vec![1.0, 0.0, 0.0, 1.0, x, y],
        ..Default::default()
    }
}

fn texts(lines: &[RawLine]) -> Vec<&str> {
    lines.iter().map(|l| l.text.as_str()).collect()
}

mod group_items_into_lines {
    use super::*;

    #[test]
    fn a_nan_position_from_a_damaged_pdf_does_not_panic_the_sort() {
        // A mutated torture.pdf gave a text item a NaN coordinate, and the
        // NaN-as-equal comparator (JavaScript's reading of `a - b`) is not a
        // total order, which Rust's sort is entitled to panic on.
        let mut items = Vec::new();
        for i in 0..64 {
            let y = if i % 3 == 0 {
                f64::NAN
            } else {
                (i * 37 % 61) as f64 * 10.0
            };
            let x = if i % 5 == 0 {
                f64::NAN
            } else {
                (i * 13 % 29) as f64 * 5.0
            };
            items.push(item("w", x, y));
        }
        let lines = group_items_into_lines(&items, 612.0, 1);
        assert!(!lines.is_empty());
    }

    #[test]
    fn items_on_one_y_join_into_a_single_line_with_indent_pct() {
        let lines = group_items_into_lines(
            &[item("Hello", 61.0, 700.0), item("world", 120.0, 700.0)],
            612.0,
            1,
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello world");
        assert_eq!(lines[0].indent, 10.0); // 61/612
    }

    #[test]
    fn continuation_suffix_printed_1pt_above_its_cue_joins_the_cue_line() {
        // One script's generator prints "(CONT'D)" as a separate item one
        // point higher than the character name; exact-Y bucketing split it into
        // its own line, which then serialized as a stray "(CONT'D)" paragraph.
        let lines = group_items_into_lines(
            &[
                item("(CONT'D)", 290.0, 268.0),
                item("MARGO", 250.0, 267.0),
                item("You look totally wiped.", 180.0, 255.0),
            ],
            612.0,
            1,
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "MARGO (CONT'D)");
        assert_eq!(lines[1].text, "You look totally wiped.");
    }

    #[test]
    fn double_printed_text_at_the_same_position_is_deduplicated() {
        // Final Draft double-prints (MORE) and (CONT'D) cue lines at identical
        // coordinates; naive joining yields "(MORE)(MORE)".
        let lines = group_items_into_lines(
            &[item("(MORE)", 300.0, 80.0), item("(MORE)", 300.0, 80.0)],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "(MORE)");
    }

    #[test]
    fn duplicates_separated_by_an_empty_zero_width_item_still_deduplicate() {
        // Final Draft emits "(MORE)", "", "(MORE)": the empty item must not
        // break duplicate adjacency.
        let lines = group_items_into_lines(
            &[
                item("(MORE)", 252.0, 63.0),
                item("", 252.0, 63.0),
                item("(MORE)", 252.0, 63.0),
            ],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "(MORE)");
    }

    #[test]
    fn right_margin_revision_stars_on_their_own_line_are_dropped() {
        // Revised drafts mark changed lines with * in the right margin; a bare
        // star line must not become an element (it would reset dialogue context).
        let lines = group_items_into_lines(
            &[
                item("Hey! Careful with those tapes,", 180.0, 700.0),
                item("*", 575.0, 688.0),
            ],
            612.0,
            1,
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hey! Careful with those tapes,");
    }

    #[test]
    fn right_margin_revision_star_on_the_same_line_as_text_is_dropped() {
        let lines = group_items_into_lines(
            &[item("He walks out.", 110.0, 700.0), item("*", 575.0, 700.0)],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "He walks out.");
    }

    #[test]
    fn asterisk_emphasis_inside_body_text_is_kept() {
        let lines = group_items_into_lines(
            &[item("***THIS SCENE IS FILMED***", 110.0, 700.0)],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "***THIS SCENE IS FILMED***");
    }

    #[test]
    fn dual_dialogue_de_interleaves_into_left_speech_then_right_speech() {
        // p48 of The Last Video Store: MARGO ‖ DEV speaking simultaneously
        // in two columns. Naive Y-joining interleaves them into garbage.
        let lines = group_items_into_lines(
            &[
                item("MARGO", 180.0, 700.0),
                item("DEV", 400.0, 700.0),
                item("rewind it again.", 150.0, 688.0),
                item("maybe if you were faster", 380.0, 688.0),
                item("at closing.", 380.0, 676.0), // right column runs one line longer
                item("They stare at the screen.", 108.0, 640.0), // back to normal action
            ],
            612.0,
            1,
        );
        assert_eq!(
            texts(&lines),
            [
                "MARGO",
                "rewind it again.",
                "DEV",
                "maybe if you were faster",
                "at closing.",
                "They stare at the screen.",
            ]
        );
        assert_eq!(lines[0].indent, 40.0); // cue zone
        assert_eq!(lines[1].indent, 30.0); // dialogue zone
        assert_eq!(lines[2].indent, 40.0);
        assert_eq!(lines[4].indent, 30.0);
    }

    #[test]
    fn a_one_letter_character_name_still_anchors_a_dual_region() {
        // Found in the wild 2026-09-16: a Final Draft script whose lead is named
        // "Q". isCueShaped required two characters AND two letters, so a
        // "ALANI  Q" cue row failed the dual test, the region was never entered,
        // and the two columns were Y-joined into one line each. The visible
        // damage was not a missing cue: it was every following speech fusing
        // with the other speaker's, and the rest of the scene reading as action,
        // because the interleaved lines carry no cue to attach to.
        //
        // The geometry is the real guard here (left of 42%, right of 48%, a
        // clear gap), so admitting a single letter costs no safety. A page
        // number cannot reach this: it has no letters at all.
        let lines = group_items_into_lines(
            &[
                item("ALANI", 194.0, 700.0),
                item("Q", 432.0, 700.0),
                item("Hang on--", 150.0, 688.0),
                item("We talked about this.", 400.0, 688.0),
                item("She turns away.", 108.0, 640.0),
            ],
            612.0,
            1,
        );
        assert_eq!(
            texts(&lines),
            [
                "ALANI",
                "Hang on--",
                "Q",
                "We talked about this.",
                "She turns away."
            ]
        );
    }

    #[test]
    fn a_lone_letter_does_not_make_any_two_cluster_row_a_dual_cue() {
        // The pair of digits a shooting script prints in both margins, and a
        // scene number row, must not be read as two cues now that one letter is
        // allowed. Neither has a letter in it.
        let lines = group_items_into_lines(
            &[item("2", 100.0, 700.0), item("2", 500.0, 700.0)],
            612.0,
            1,
        );
        assert_eq!(texts(&lines), ["2"]);
    }

    #[test]
    fn dual_body_lines_with_narrow_gaps_still_split_at_the_cue_anchored_boundary() {
        // The real p48 failure in The Last Video Store: long dialogue lines
        // close the inter-column gap, so splitting must use the boundary fixed
        // by the dual-cue line, not per-line gap detection.
        let lines = group_items_into_lines(
            &[
                item("MARGO", 180.0, 700.0),
                item("DEV", 400.0, 700.0),
                item("Let's lock up and then we'll", 150.0, 688.0),
                item("What? Sure, in a minute.", 340.0, 688.0),
            ],
            612.0,
            1,
        );
        assert_eq!(
            texts(&lines),
            [
                "MARGO",
                "Let's lock up and then we'll",
                "DEV",
                "What? Sure, in a minute."
            ]
        );
    }

    #[test]
    fn a_second_dual_cue_pair_starts_a_new_pair_of_speeches() {
        let lines = group_items_into_lines(
            &[
                item("MARGO", 180.0, 700.0),
                item("DEV", 400.0, 700.0),
                item("rewind it again.", 150.0, 688.0),
                item("maybe if you were", 380.0, 688.0),
                item("MARGO (CONT'D)", 170.0, 664.0),
                item("DEV", 400.0, 664.0),
                item("No. Wait.", 150.0, 652.0),
                item("You wait!", 380.0, 652.0),
            ],
            612.0,
            1,
        );
        assert_eq!(
            texts(&lines),
            [
                "MARGO",
                "rewind it again.",
                "DEV",
                "maybe if you were",
                "MARGO (CONT'D)",
                "No. Wait.",
                "DEV",
                "You wait!",
            ]
        );
    }

    #[test]
    fn two_column_title_page_furniture_is_not_treated_as_dual_dialogue() {
        let lines = group_items_into_lines(
            &[
                item("212-555-0134", 72.0, 700.0),
                item("October 28, 2024", 400.0, 700.0),
            ],
            612.0,
            1,
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "212-555-0134 October 28, 2024");
    }

    #[test]
    fn legitimately_repeated_words_at_different_positions_are_kept() {
        let lines = group_items_into_lines(
            &[item("No.", 100.0, 700.0), item("No.", 140.0, 700.0)],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "No. No.");
    }
}

mod dual_margin_scene_numbers {
    use super::*;

    #[test]
    fn duplicated_margin_numbers_collapse_to_one_token() {
        // Shooting scripts print the scene number in BOTH margins on the
        // slugline's row: "2        2" must become "2", which then attaches
        // to the following scene heading.
        let lines = group_items_into_lines(
            &[
                item("2", 54.0, 700.0),
                item("2", 576.0, 700.0),
                item("INT. FOYER - DAY", 108.0, 676.0),
            ],
            612.0,
            1,
        );
        assert_eq!(texts(&lines), ["2", "INT. FOYER - DAY"]);
    }

    #[test]
    fn lettered_scene_numbers_collapse_too() {
        let lines = group_items_into_lines(
            &[item("12A.", 54.0, 700.0), item("12A.", 570.0, 700.0)],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "12A.");
    }

    #[test]
    fn distinct_numbers_on_one_line_are_left_alone() {
        let lines =
            group_items_into_lines(&[item("1", 54.0, 700.0), item("2", 576.0, 700.0)], 612.0, 1);
        assert_eq!(lines[0].text, "1 2");
    }
}

mod whitespace_normalization {
    use super::*;

    #[test]
    fn padded_italic_spans_and_authored_doubles_collapse_to_single_spaces() {
        // Italicized words extract as separate items padded with spaces
        // ("need some  paperwork  here"); HTML collapses them anyway, so the
        // fountain should match what the reader renders.
        let lines = group_items_into_lines(
            &[
                item("need some ", 110.0, 700.0),
                item(" paperwork ", 220.0, 700.0),
                item(" here", 340.0, 700.0),
            ],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "need some paperwork here");
    }
}

mod inline_style_detection {
    use super::*;

    #[derive(Default, Clone, Copy)]
    struct Flags {
        italic: bool,
        bold: bool,
        underline: bool,
    }

    const PLAIN: Flags = Flags {
        italic: false,
        bold: false,
        underline: false,
    };

    fn styled(s: &str, x: f64, y: f64, f: Flags) -> TextItem {
        TextItem {
            italic: f.italic,
            bold: f.bold,
            underline: f.underline,
            ..item(s, x, y)
        }
    }

    #[test]
    fn an_italic_run_inside_a_plain_line_gains_fountain_emphasis_markers() {
        let lines = group_items_into_lines(
            &[
                styled("need some ", 110.0, 700.0, PLAIN),
                styled(
                    "paperwork",
                    220.0,
                    700.0,
                    Flags {
                        italic: true,
                        ..PLAIN
                    },
                ),
                styled(" here", 340.0, 700.0, PLAIN),
            ],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "need some paperwork here");
        assert_eq!(
            lines[0].styled.as_deref(),
            Some("need some *paperwork* here")
        );
    }

    #[test]
    fn bold_and_bold_italic_runs_use_double_and_triple_stars() {
        let lines = group_items_into_lines(
            &[
                styled("a ", 110.0, 700.0, PLAIN),
                styled(
                    "big",
                    200.0,
                    700.0,
                    Flags {
                        bold: true,
                        ..PLAIN
                    },
                ),
                styled(
                    " loud",
                    300.0,
                    700.0,
                    Flags {
                        bold: true,
                        italic: true,
                        ..PLAIN
                    },
                ),
            ],
            612.0,
            1,
        );
        assert_eq!(lines[0].styled.as_deref(), Some("a **big** ***loud***"));
    }

    #[test]
    fn a_fully_italic_line_wraps_whole() {
        let lines = group_items_into_lines(
            &[styled(
                "I never wanted that",
                200.0,
                700.0,
                Flags {
                    italic: true,
                    ..PLAIN
                },
            )],
            612.0,
            1,
        );
        assert_eq!(lines[0].styled.as_deref(), Some("*I never wanted that*"));
    }

    #[test]
    fn punctuation_only_styled_items_never_wrap_alone() {
        let lines = group_items_into_lines(
            &[
                styled("by Nora Vance", 110.0, 700.0, PLAIN),
                styled(
                    ",",
                    189.0,
                    700.0,
                    Flags {
                        italic: true,
                        ..PLAIN
                    },
                ),
                styled(" the b-side", 195.0, 700.0, PLAIN),
            ],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "by Nora Vance, the b-side");
        assert_eq!(lines[0].styled, None);
    }

    #[test]
    fn uniform_plain_lines_carry_no_styled_variant() {
        let lines =
            group_items_into_lines(&[styled("Just action.", 110.0, 700.0, PLAIN)], 612.0, 1);
        assert_eq!(lines[0].styled, None);
    }

    #[test]
    fn an_underlined_run_gains_underscores() {
        let lines = group_items_into_lines(
            &[
                styled("This word is ", 110.0, 700.0, PLAIN),
                styled(
                    "underlined",
                    200.0,
                    700.0,
                    Flags {
                        underline: true,
                        ..PLAIN
                    },
                ),
                styled(" with drawn art", 300.0, 700.0, PLAIN),
            ],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "This word is underlined with drawn art");
        assert_eq!(
            lines[0].styled.as_deref(),
            Some("This word is _underlined_ with drawn art")
        );
    }

    #[test]
    fn mixed_marks_nest_with_the_underscore_innermost() {
        // Not a palindrome: the close mirrors the open. Both renderers unwrap
        // stars first and underscores last, which is exactly this nesting.
        // Two-letter words on purpose: a one-character run is punctuation-only by
        // the guard above and never carries style at all.
        let lines = group_items_into_lines(
            &[
                styled(
                    "bold ",
                    110.0,
                    700.0,
                    Flags {
                        underline: true,
                        bold: true,
                        italic: false,
                    },
                ),
                styled(
                    "slant ",
                    160.0,
                    700.0,
                    Flags {
                        underline: true,
                        italic: true,
                        bold: false,
                    },
                ),
                styled(
                    "both",
                    220.0,
                    700.0,
                    Flags {
                        underline: true,
                        bold: true,
                        italic: true,
                    },
                ),
            ],
            612.0,
            1,
        );
        assert_eq!(
            lines[0].styled.as_deref(),
            Some("**_bold_** *_slant_* ***_both_***")
        );
    }

    #[test]
    fn a_punctuation_only_underlined_item_never_wraps_alone() {
        let lines = group_items_into_lines(
            &[
                styled("by Nora Vance", 110.0, 700.0, PLAIN),
                styled(
                    ",",
                    189.0,
                    700.0,
                    Flags {
                        underline: true,
                        ..PLAIN
                    },
                ),
                styled(" the b-side", 195.0, 700.0, PLAIN),
            ],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "by Nora Vance, the b-side");
        assert_eq!(lines[0].styled, None);
    }
}

mod dual_dialogue_marking {
    use super::*;

    #[test]
    fn the_right_column_cue_line_carries_the_dual_right_flag() {
        let lines = group_items_into_lines(
            &[
                item("MARGO", 180.0, 700.0),
                item("DEV", 400.0, 700.0),
                item("rewind it again.", 150.0, 688.0),
                item("maybe if you were faster", 380.0, 688.0),
            ],
            612.0,
            1,
        );
        assert_eq!(lines[0].text, "MARGO");
        assert_eq!(lines[0].dual_right, None);
        assert_eq!(lines[2].text, "DEV");
        assert_eq!(lines[2].dual_right, Some(true));
        // And each right-column line keeps the y of the row it was printed on,
        // the same as the left-column line beside it. That shared y is the
        // premise of the dual-dialogue picture rule in insertImages
        // (src/parser/index.ts): the right column's y restarts at the top of the
        // exchange, so a picture inside it lands among the left column's lines.
        assert_eq!(lines[1].text, "rewind it again.");
        assert_eq!(lines[3].text, "maybe if you were faster");
        assert_eq!(lines[2].y, lines[0].y);
        assert_eq!(lines[3].y, lines[1].y);
        // And those shared values are the rows' real y, since a pair of NaNs (or
        // of any one wrong number) would agree with each other just as well.
        assert_eq!(lines[0].y, 700.0);
        assert_eq!(lines[1].y, 688.0);
    }
}

mod collect_underline_marks {
    use super::*;

    const PAGE: f64 = 612.0;

    /// pdf.js hands the bbox over as a real Float32Array, so the TypeScript's
    /// 266.4 arrived as 266.3999938964844. The bboxes here go through f32 the
    /// same way.
    #[allow(clippy::cast_possible_truncation)]
    fn f32_box(b: [f64; 4]) -> Vec<f64> {
        b.iter().map(|&v| f64::from(v as f32)).collect()
    }

    /// Build an operator list drawing a path with bbox [x0,y0,x1,y1] per
    /// entry, after `prefix`. `paint` defaults to stroke.
    fn paths(entries: &[([f64; 4], Option<&str>)], prefix: Vec<Op>) -> Vec<Op> {
        let mut ops = prefix;
        for (bbox, paint) in entries {
            ops.push(Op::ConstructPath {
                paint: Some(PaintOp::Name(paint.unwrap_or("stroke").into())),
                bbox: Some(f32_box(*bbox)),
            });
        }
        ops
    }

    /// These are geometry assertions, not bit assertions.
    fn rounded(marks: &[UnderlineMark]) -> Vec<(f64, f64, f64)> {
        let r = |v: f64| (v * 100.0).round() / 100.0;
        marks.iter().map(|m| (r(m.x0), r(m.x1), r(m.y))).collect()
    }

    fn mark(x0: f64, x1: f64, y: f64) -> UnderlineMark {
        UnderlineMark { x0, x1, y }
    }

    #[test]
    fn a_flat_short_filled_rule_is_a_mark() {
        // The invented fixture's shape: a 72pt x 0.6pt filled rect.
        let marks = collect_underline_marks(
            &paths(&[([201.6, 562.0, 273.6, 562.6], Some("fill"))], vec![]),
            PAGE,
        );
        assert_eq!(rounded(&marks), [(201.6, 273.6, 562.3)]);
    }

    #[test]
    fn a_zero_height_stroked_rule_is_a_mark_too() {
        // Every real underline in the local set is a stroke with a zero-height
        // bbox, not a filled rect. Requiring a fill would detect nothing real.
        let marks = collect_underline_marks(
            &paths(&[([266.4, 548.5, 346.4, 548.5], Some("stroke"))], vec![]),
            PAGE,
        );
        assert_eq!(rounded(&marks), [(266.4, 346.4, 548.5)]);
    }

    #[test]
    fn a_tall_box_is_not_a_mark() {
        assert_eq!(
            collect_underline_marks(
                &paths(&[([100.0, 500.0, 300.0, 512.0], None)], vec![]),
                PAGE
            ),
            []
        );
    }

    #[test]
    fn a_hairline_shorter_than_4pt_is_not_a_mark() {
        assert_eq!(
            collect_underline_marks(
                &paths(&[([100.0, 500.0, 103.0, 500.0], None)], vec![]),
                PAGE
            ),
            []
        );
    }

    #[test]
    fn a_page_wide_rule_is_furniture_not_a_mark() {
        // Final Draft's header rules span the full measure; an underline never does.
        assert_eq!(
            collect_underline_marks(&paths(&[([0.0, 744.0, 612.0, 744.0], None)], vec![]), PAGE),
            []
        );
    }

    #[test]
    fn end_path_constructs_geometry_but_paints_nothing() {
        // `W n` clip paths. chromium.pdf draws 34 of them, all full-page boxes.
        assert_eq!(
            collect_underline_marks(
                &paths(&[([100.0, 500.0, 200.0, 500.0], Some("endPath"))], vec![]),
                PAGE
            ),
            []
        );
    }

    #[test]
    fn a_y_flip_ctm_is_applied_not_skipped() {
        // final-draft.pdf draws its underlines under [1,0,0,-1,0,792]. Untransformed
        // the mark lands at y=287; the real baseline is at 505.
        let marks = collect_underline_marks(
            &paths(
                &[([222.0, 287.0, 390.0, 287.0], None)],
                vec![
                    Op::Save,
                    Op::Transform {
                        matrix: vec![1.0, 0.0, 0.0, -1.0, 0.0, 792.0],
                    },
                ],
            ),
            PAGE,
        );
        assert_eq!(marks, [mark(222.0, 390.0, 505.0)]);
    }

    #[test]
    fn restore_pops_the_ctm_back() {
        let marks = collect_underline_marks(
            &paths(
                &[([222.0, 287.0, 390.0, 287.0], None)],
                vec![
                    Op::Save,
                    Op::Transform {
                        matrix: vec![1.0, 0.0, 0.0, -1.0, 0.0, 792.0],
                    },
                    Op::Restore,
                ],
            ),
            PAGE,
        );
        assert_eq!(marks, [mark(222.0, 390.0, 287.0)]);
    }

    #[test]
    fn a_rotated_ctm_is_skipped_rather_than_guessed_at() {
        let marks = collect_underline_marks(
            &paths(
                &[([222.0, 287.0, 390.0, 287.0], None)],
                vec![Op::Transform {
                    matrix: vec![0.0, 1.0, -1.0, 0.0, 0.0, 0.0],
                }],
            ),
            PAGE,
        );
        assert_eq!(marks, []);
    }

    #[test]
    fn a_malformed_operator_list_yields_no_marks_instead_of_throwing() {
        // `{ fnArray: [OPS.constructPath], argsArray: [null] }`: a path with
        // neither paint op nor bbox. (The `null` operator list half is the
        // type's job here; see the file header.)
        assert_eq!(
            collect_underline_marks(
                &[Op::ConstructPath {
                    paint: None,
                    bbox: None
                }],
                PAGE
            ),
            []
        );
    }
}

mod mark_underlines_item {
    use super::*;

    fn mark(y: f64) -> UnderlineMark {
        mark_at(y, 100.0, 200.0)
    }

    fn mark_at(y: f64, x0: f64, x1: f64) -> UnderlineMark {
        UnderlineMark { x0, x1, y }
    }

    #[test]
    fn a_mark_2pt_under_the_baseline_fully_covering_the_item_matches() {
        assert!(mark_underlines_item(&mark(498.0), 100.0, 200.0, 500.0));
    }

    #[test]
    fn the_measured_range_of_real_generators_all_matches() {
        // -1.5 (highland p8/p9), -1.7 (torture), -2.0 (final draft), -3.0 (highland p6)
        for d in [-1.5, -1.7, -2.0, -3.0] {
            assert!(
                mark_underlines_item(&mark(500.0 + d), 100.0, 200.0, 500.0),
                "d={d}"
            );
        }
    }

    #[test]
    fn a_strikethrough_sits_above_the_baseline_and_does_not_match() {
        assert!(!mark_underlines_item(&mark(503.5), 100.0, 200.0, 500.0));
    }

    #[test]
    fn a_table_border_6pt_below_the_baseline_does_not_match() {
        assert!(!mark_underlines_item(&mark(494.0), 100.0, 200.0, 500.0));
    }

    #[test]
    fn the_next_lines_mark_does_not_match_this_line() {
        // 12pt line spacing: the row below's underline sits at 488 - 2.
        assert!(!mark_underlines_item(&mark(486.0), 100.0, 200.0, 500.0));
    }

    #[test]
    fn a_mark_covering_less_than_60_pct_of_the_item_does_not_match() {
        // highland p8: a 36pt rule under a 129.5pt item. Marking the whole item
        // underlined would put _ around 13 characters that are not underlined.
        assert!(!mark_underlines_item(
            &mark_at(498.0, 100.0, 136.0),
            100.0,
            229.5,
            500.0
        ));
    }

    #[test]
    fn a_mark_covering_60_pct_or_more_does_match() {
        assert!(mark_underlines_item(
            &mark_at(498.0, 100.0, 160.0),
            100.0,
            200.0,
            500.0
        ));
    }

    #[test]
    fn a_zero_width_item_never_matches() {
        assert!(!mark_underlines_item(&mark(498.0), 100.0, 100.0, 500.0));
    }
}

mod family_bucket {
    use super::*;

    #[test]
    fn screenplay_monospace_faces_bucket_as_mono() {
        for n in [
            "Courier",
            "CourierPrime-Bold",
            "AAAAAB+CourierFinalDraft",
            "WXPDAA+CourierNewPSMT",
            "LetterGothic",
            "Prestige Elite",
            "Andale Mono",
        ] {
            assert_eq!(family_bucket(n), Some(FamilyBucket::Mono), "{n}");
        }
    }

    #[test]
    fn century_gothic_is_a_sans_not_a_mono() {
        // The trap the spec calls out: match the JOINED name "lettergothic",
        // never bare "gothic".
        assert_eq!(family_bucket("CenturyGothic"), Some(FamilyBucket::Sans));
    }

    #[test]
    fn serif_faces_bucket_as_serif() {
        for n in [
            "Times-Roman",
            "TimesNewRomanPSMT",
            "Georgia",
            "Garamond",
            "Palatino-Roman",
        ] {
            assert_eq!(family_bucket(n), Some(FamilyBucket::Serif), "{n}");
        }
    }

    #[test]
    fn a_roman_weight_suffix_does_not_make_a_sans_into_a_serif() {
        // "-Roman" is PostScript for the REGULAR weight. Times matches on "times"
        // anyway, so bare "roman" buys nothing and misbuckets these.
        for n in ["Helvetica-Roman", "AvenirNext-Roman", "Frutiger-Roman"] {
            assert_eq!(family_bucket(n), Some(FamilyBucket::Sans), "{n}");
        }
    }

    #[test]
    fn handwriting_faces_bucket_as_cursive() {
        for n in ["BrushScriptMT", "BradleyHandITC", "Comic Sans MS"] {
            assert_eq!(family_bucket(n), Some(FamilyBucket::Cursive), "{n}");
        }
    }

    #[test]
    fn everything_else_is_sans() {
        for n in ["AvenirNext-Bold", "Helvetica", "Scream", "Arial-BoldMT"] {
            assert_eq!(family_bucket(n), Some(FamilyBucket::Sans), "{n}");
        }
    }

    #[test]
    fn style_tokens_never_decide_the_bucket() {
        assert_eq!(
            family_bucket("Courier-BoldOblique"),
            Some(FamilyBucket::Mono)
        );
        assert_eq!(family_bucket("Georgia-Italic"), Some(FamilyBucket::Serif));
    }

    #[test]
    fn an_empty_or_missing_name_has_no_bucket() {
        assert_eq!(family_bucket(""), None);
        assert_eq!(family_bucket("AAAAAB+"), None);
    }
}

mod stamp_line_fmt {
    use super::*;
    use FamilyBucket::{Mono, Sans};

    fn run(bucket: FamilyBucket, size: f64, chars: u32) -> FontRun {
        FontRun {
            bucket,
            size,
            chars,
        }
    }

    fn line(fonts: Vec<FontRun>) -> RawLine {
        RawLine {
            text: "x".into(),
            indent: 10.0,
            y: 700.0,
            page_num: 1,
            styled: None,
            dual_right: None,
            fonts: Some(fonts),
            fmt: None,
            placed: false,
        }
    }

    fn body(n: u32) -> RawLine {
        line(vec![run(Mono, 12.0, n)])
    }

    fn fmt(family: Option<FamilyBucket>, size: Option<SizeStep>) -> Option<Fmt> {
        Some(Fmt { family, size })
    }

    #[test]
    fn a_uniform_document_produces_no_fmt_anywhere() {
        let mut lines = vec![body(500), body(500), body(500)];
        stamp_line_fmt(&mut lines);
        assert_eq!(
            lines.iter().map(|l| l.fmt).collect::<Vec<_>>(),
            [None, None, None]
        );
    }

    #[test]
    fn a_line_in_another_family_gets_a_family_fmt() {
        let shifted = line(vec![run(Sans, 12.0, 30)]);
        let mut lines = vec![body(500), shifted, body(500)];
        stamp_line_fmt(&mut lines);
        assert_eq!(lines[1].fmt, fmt(Some(Sans), None));
    }

    #[test]
    fn the_three_size_steps_come_off_the_ratio_both_directions() {
        let smaller = line(vec![run(Mono, 10.0, 30)]); // 0.83
        let bigger = line(vec![run(Mono, 15.0, 30)]); // 1.25
        let biggest = line(vec![run(Mono, 18.0, 30)]); // 1.50
        let mut lines = vec![body(500), smaller, bigger, biggest, body(500)];
        stamp_line_fmt(&mut lines);
        assert_eq!(lines[1].fmt, fmt(None, Some(SizeStep::Smaller)));
        assert_eq!(lines[2].fmt, fmt(None, Some(SizeStep::Larger)));
        assert_eq!(lines[3].fmt, fmt(None, Some(SizeStep::MuchLarger)));
    }

    #[test]
    fn a_shift_in_both_arms_reports_both() {
        let both = line(vec![run(Sans, 18.0, 30)]);
        let mut lines = vec![body(500), both];
        stamp_line_fmt(&mut lines);
        assert_eq!(lines[1].fmt, fmt(Some(Sans), Some(SizeStep::MuchLarger)));
    }

    #[test]
    fn float_jitter_under_1_5pt_never_fires() {
        // 13.4/12 = 1.117 is under the ratio floor anyway; the absolute delta is
        // the belt to the ratio's braces.
        let jittery = line(vec![run(Mono, 13.4, 30)]);
        let mut lines = vec![body(500), jittery];
        stamp_line_fmt(&mut lines);
        assert_eq!(lines[1].fmt, None);
    }

    #[test]
    fn a_ratio_between_the_steps_is_not_a_step() {
        // 13.5/12 = 1.125: bigger than jitter, smaller than +1's 1.15 floor.
        let between = line(vec![run(Mono, 13.5, 30)]);
        let mut lines = vec![body(500), between];
        stamp_line_fmt(&mut lines);
        assert_eq!(lines[1].fmt, None);
    }

    #[test]
    fn a_line_only_half_in_the_deviant_font_gets_nothing() {
        let mixed = line(vec![run(Mono, 12.0, 20), run(Sans, 12.0, 20)]);
        let mut lines = vec![body(500), mixed];
        stamp_line_fmt(&mut lines);
        assert_eq!(lines[1].fmt, None);
    }

    #[test]
    fn eighty_pct_agreement_is_enough() {
        let mostly = line(vec![run(Mono, 12.0, 2), run(Sans, 12.0, 8)]);
        let mut lines = vec![body(500), mostly];
        stamp_line_fmt(&mut lines);
        assert_eq!(lines[1].fmt, fmt(Some(Sans), None));
    }

    #[test]
    fn the_dominant_is_by_character_weight_not_by_line_count() {
        // Many short sans lines lose to one long mono block.
        let mut lines: Vec<RawLine> = (0..20).map(|_| line(vec![run(Sans, 12.0, 5)])).collect();
        lines.push(line(vec![run(Mono, 12.0, 900)]));
        stamp_line_fmt(&mut lines);
        assert_eq!(lines[20].fmt, None);
        assert_eq!(lines[0].fmt, fmt(Some(Sans), None));
    }

    #[test]
    fn lines_with_no_resolved_fonts_are_left_alone() {
        let bare = RawLine {
            fonts: None,
            ..line(vec![])
        };
        let mut lines = vec![body(500), bare];
        stamp_line_fmt(&mut lines);
        assert_eq!(lines[1].fmt, None);
    }
}

mod font_runs_on_extracted_lines {
    use super::*;
    use FamilyBucket::{Mono, Sans};

    fn sized(s: &str, x: f64, y: f64, size: f64, bucket: Option<FamilyBucket>) -> TextItem {
        TextItem {
            str: s.into(),
            transform: vec![size, 0.0, 0.0, size, x, y],
            bucket,
            ..Default::default()
        }
    }

    fn run(bucket: FamilyBucket, size: f64, chars: u32) -> FontRun {
        FontRun {
            bucket,
            size,
            chars,
        }
    }

    #[test]
    fn a_line_records_its_per_run_bucket_and_size_by_trimmed_length() {
        let lines = group_items_into_lines(
            &[
                sized("CHYRON: ", 110.0, 700.0, 12.0, Some(Mono)),
                sized("LIVE", 170.0, 700.0, 18.0, Some(Sans)),
            ],
            612.0,
            1,
        );
        assert_eq!(
            lines[0].fonts,
            Some(vec![run(Mono, 12.0, 7), run(Sans, 18.0, 4)])
        );
    }

    #[test]
    fn adjacent_runs_with_the_same_bucket_and_size_merge() {
        let lines = group_items_into_lines(
            &[
                sized("one ", 110.0, 700.0, 12.0, Some(Mono)),
                sized("two", 150.0, 700.0, 12.0, Some(Mono)),
            ],
            612.0,
            1,
        );
        assert_eq!(lines[0].fonts, Some(vec![run(Mono, 12.0, 6)]));
    }

    #[test]
    fn an_item_whose_font_never_resolved_contributes_nothing() {
        let lines = group_items_into_lines(
            &[
                sized("seen", 110.0, 700.0, 12.0, Some(Mono)),
                sized("unseen", 160.0, 700.0, 12.0, None),
            ],
            612.0,
            1,
        );
        assert_eq!(lines[0].fonts, Some(vec![run(Mono, 12.0, 4)]));
    }

    #[test]
    fn a_negative_vertical_scale_still_reports_a_positive_size() {
        let flipped = TextItem {
            str: "flipped".into(),
            transform: vec![12.0, 0.0, 0.0, -12.0, 110.0, 700.0],
            bucket: Some(Mono),
            ..Default::default()
        };
        assert_eq!(
            group_items_into_lines(&[flipped], 612.0, 1)[0].fonts,
            Some(vec![run(Mono, 12.0, 7)])
        );
    }
}

mod extract_document_progress {
    // The app showed an indeterminate spinner during conversion because the
    // engine reported nothing until it was finished. Page extraction is the
    // long stage and already counts pages, so it is where a real fraction
    // comes from.
    use super::*;

    #[test]
    fn reports_one_progress_tick_per_page_with_the_total() {
        let facts = common::facts("screenplay");
        let mut ticks: Vec<(u32, u32)> = Vec::new();
        let cb: &mut PageProgress = &mut |page, pages, _, _| ticks.push((page, pages));
        let out = extract_document(&facts, None, Some(cb));

        assert_eq!(out.page_count, 5);
        assert_eq!(ticks.len(), 5);
        assert_eq!(
            ticks.iter().map(|t| t.0).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        // Every tick carries the same total so a fraction is computable from
        // the first one, without waiting to learn the denominator.
        assert!(ticks.iter().all(|t| t.1 == 5));
    }

    #[test]
    fn is_optional_extraction_works_with_no_callback() {
        let out = extract_document(&common::facts("screenplay"), None, None);
        assert_eq!(out.page_count, 5);
        assert!(!out.lines.is_empty());
    }
}

mod decode_images_progress {
    // On a picture-heavy document decoding the pictures takes longer than
    // reading the text, so it reports its own ticks rather than leaving the bar
    // standing still. It counts only the pages that place a picture: a page
    // with none costs nothing here, and counting it would spread the ticks over
    // work that is not being done.
    use super::*;

    #[test]
    fn ticks_once_before_the_first_picture_page_and_once_after_each_with_the_total() {
        // images.pdf places at least one picture on each of its five pages
        // (make-fixture.py's SHEET_PLACEMENTS).
        let facts = common::facts("images");
        let doc = extract_text(&facts, None, None);
        let mut ticks: Vec<(u32, u32)> = Vec::new();
        let cb: &mut PageProgress = &mut |done, total, _, _| ticks.push((done, total));
        let out = doc.decode_images(Some(cb));
        assert!(!out.images.is_empty());
        assert_eq!(ticks, [(0, 5), (1, 5), (2, 5), (3, 5), (4, 5), (5, 5)]);
    }

    #[test]
    fn a_document_that_places_no_picture_never_ticks_so_no_caller_names_the_stage() {
        let facts = common::facts("screenplay");
        let doc = extract_text(&facts, None, None);
        let mut ticks: Vec<(u32, u32)> = Vec::new();
        let cb: &mut PageProgress = &mut |done, total, _, _| ticks.push((done, total));
        let out = doc.decode_images(Some(cb));
        assert_eq!(out.images, []);
        assert_eq!(ticks, []);
    }

    #[test]
    fn is_optional_decoding_works_with_no_callback() {
        let facts = common::facts("images");
        let doc = extract_text(&facts, None, None);
        assert!(!doc.decode_images(None).images.is_empty());
    }
}

// ── rotated text is never the script ─────────────────────────────────
//
// Scripts sent out to read often carry the reader's name as a watermark,
// printed large and diagonally across every page. A real one (one item a
// page, rotated about 52 degrees) landed between cues and their lines, so
// a parenthetical after it fell to description. A screenplay's own text is
// never rotated; slanted text (a faked italic shears, it does not rotate)
// still is the script, and stays.
mod rotated_text {
    use super::*;

    fn with_item(text: &str, transform: Vec<f64>) -> (Vec<String>, Vec<String>) {
        let base = common::facts("screenplay");
        let mut stamped = base.clone();
        let page = &mut stamped.pages[1];
        let mut item = page.items[0].clone();
        item.str = text.to_string();
        item.transform = transform;
        page.items.push(item);
        let lines = |f| -> Vec<String> {
            extract_text(f, None, None)
                .text
                .lines
                .into_iter()
                .map(|l| l.text)
                .collect()
        };
        (lines(&base), lines(&stamped))
    }

    #[test]
    fn a_diagonal_watermark_is_dropped_before_the_page_is_read() {
        let (base, stamped) = with_item("READER NAME", vec![68.3, -87.5, 87.5, 68.3, 68.9, 400.0]);
        assert_eq!(stamped, base);
    }

    #[test]
    fn sideways_margin_text_is_dropped_too() {
        let (base, stamped) = with_item("DRAFT", vec![0.0, 12.0, -12.0, 0.0, 20.0, 400.0]);
        assert_eq!(stamped, base);
    }

    #[test]
    fn slanted_text_is_kept() {
        let (base, stamped) = with_item("Slanted aside.", vec![12.0, 0.0, 2.5, 12.0, 108.0, 90.0]);
        assert_eq!(stamped.len(), base.len() + 1);
        assert!(stamped.iter().any(|l| l.contains("Slanted aside.")));
    }
}

// ── dual dialogue: the boundary must refine before it judges ─────────
//
// Geometry lifted from a real script (page width 612): the right column's
// CUE sits at x=424 but its BODY starts at x=332, an offset of 15% of the
// page rather than the 13% the opening boundary assumes. That puts the
// right column's own dialogue LEFT of the boundary while it still runs
// past it, so the straddle test read a cleanly split two-column line as a
// full-width one and ended the region on its first body line.
//
// What the reader got was both cues demoted to action and the two
// speeches concatenated into one sentence: "Really great. Just okay."
mod dual_dialogue_body_lines {
    use super::*;

    fn dual() -> Vec<RawLine> {
        group_items_into_lines(
            &[
                item("LOU", 200.0, 219.0),
                item("SYD", 424.0, 219.0),
                item("Really great.", 108.0, 207.0),
                item("Just okay.", 332.0, 207.0),
            ],
            612.0,
            6,
        )
    }

    #[test]
    fn the_two_speeches_are_not_concatenated_into_one_line() {
        assert!(!texts(&dual()).contains(&"Really great. Just okay."));
    }

    #[test]
    fn each_column_keeps_its_own_speech_left_column_first() {
        assert_eq!(
            texts(&dual()),
            ["LOU", "Really great.", "SYD", "Just okay."]
        );
    }

    #[test]
    fn the_right_columns_cue_is_marked_for_fountains_caret_syntax() {
        let lines = dual();
        assert_eq!(
            lines
                .iter()
                .find(|l| l.text == "SYD")
                .and_then(|l| l.dual_right),
            Some(true)
        );
        assert_eq!(
            lines
                .iter()
                .find(|l| l.text == "LOU")
                .and_then(|l| l.dual_right),
            None
        );
    }
}

// ── dual dialogue: a centred cue after the columns ends them ─────────
//
// Geometry lifted from a real script (page width 612). Straight after the
// two columns, the next speaker's cue sits at the ordinary cue indent,
// x=269.5, LEFT of the column boundary, and its extension "(cont'd)" runs
// on past it. Split at the boundary, the line read as a name in the left
// column and body text in the right, so the region never ended: the name
// was appended to the left speech, the extension to the right one, and the
// cue was lost.
mod dual_dialogue_followed_by_a_centred_cue {
    use super::*;

    fn exchange() -> Vec<RawLine> {
        group_items_into_lines(
            &[
                item("LOU", 155.3, 560.0),
                item("SYD", 381.6, 560.0),
                item("Keep it down, both of", 89.5, 548.0),
                item("Not a chance, not", 315.8, 548.0),
                item("you.", 89.5, 536.0),
                item("tonight.", 315.8, 536.0),
                item("SYD", 269.5, 500.0),
                item("(cont'd)", 312.0, 500.0),
                item("Never mind.", 183.1, 488.0),
            ],
            612.0,
            84,
        )
    }

    #[test]
    fn the_cue_after_the_columns_is_a_line_of_its_own() {
        assert_eq!(
            texts(&exchange()),
            [
                "LOU",
                "Keep it down, both of",
                "you.",
                "SYD",
                "Not a chance, not",
                "tonight.",
                "SYD (cont'd)",
                "Never mind.",
            ]
        );
    }
}

// ── phantom spaces invented by pdf.js ────────────────────────────────
//
// Found 2026-09-16 in a Final Draft script that had been re-saved through
// Quartz. The re-encode left one anomalous kerning adjustment roughly once
// a page: every glyph pair on a line carries +17, and one carries about
// -110, which widens that single gap by ~1.5pt. pdf.js reads a gap that
// size as a word space and puts one in the string it hands us, so "these"
// arrives as "thes e". The PDF contains no space there at all.
//
// The repair reads the glyph run, where a REAL space is a glyph of its own
// and an invented one is not present at all. That is what makes this safe
// against a writer who spaced a word out deliberately: their spaces are
// characters and survive untouched.
mod repair_phantom_spaces {
    use super::*;

    /// One showText per run; the glyph array's unicode, joined.
    fn ops_for(runs: &[&str]) -> Vec<Op> {
        runs.iter()
            .map(|r| Op::ShowText {
                text: Some((*r).to_string()),
            })
            .collect()
    }

    // 7.0 per cell, the tracked Courier this bug appears in.
    fn it(s: &str, cells: u32) -> TextItem {
        TextItem {
            str: s.into(),
            transform: vec![1.0, 0.0, 0.0, 1.0, 100.0, 700.0],
            width: Some(f64::from(cells) * 7.0),
            ..Default::default()
        }
    }

    #[test]
    fn a_space_with_no_glyph_behind_it_is_removed() {
        // "these men." is 10 glyphs of ink but 11 characters of string.
        // Three clean lines alongside, because the cell width is a median over
        // the page and a page with almost nothing on it should not produce one.
        let mut items = vec![
            it("sit well with thes e men.", 24),
            it("a normal line of dialogue here", 30),
            it("another perfectly ordinary line", 31),
            it("and a third one for the median", 30),
        ];
        let fixed = repair_phantom_spaces(
            &mut items,
            Some(&ops_for(&[
                "sit well with these men.",
                "a normal line of dialogue here",
                "another perfectly ordinary line",
                "and a third one for the median",
            ])),
        );
        assert_eq!(fixed, 1);
        assert_eq!(items[0].str, "sit well with these men.");
        assert_eq!(items[1].str, "a normal line of dialogue here");
    }

    #[test]
    fn a_glyph_run_with_a_trailing_space_still_repairs_the_item() {
        // The defect in the first cut of this, found on the real script: the
        // glyph run for a line usually ends with a trailing space, so it is the
        // SAME LENGTH as the damaged item even though it has one less space in
        // the middle. A raw length comparison then reads "no improvement" and
        // skips the repair. Three of the four known cases were skipped this way
        // and only the one run without a trailing space was fixed.
        let mut items = vec![
            it("as we sl owly PAN AWAY.", 22),
            it("a normal line of dialogue here", 30),
            it("another perfectly ordinary line", 31),
            it("and a third one for the median", 30),
        ];
        let fixed = repair_phantom_spaces(
            &mut items,
            Some(&ops_for(&[
                "as we slowly PAN AWAY. ",
                "a normal line of dialogue here",
                "another perfectly ordinary line",
                "and a third one for the median",
            ])),
        );
        assert_eq!(fixed, 1);
        assert_eq!(items[0].str, "as we slowly PAN AWAY.");
    }

    #[test]
    fn a_deliberately_spaced_out_word_is_left_alone() {
        // The writer typed "s l o w l y". Every space is a real glyph, so the
        // ink matches the string and nothing is flagged. This is the assurance
        // that the repair cannot second-guess an author.
        let mut items = vec![
            it("we go s l o w l y now", 21),
            it("a normal line of dialogue here", 30),
            it("another perfectly ordinary line", 31),
            it("and a third one for the median", 30),
        ];
        let fixed = repair_phantom_spaces(
            &mut items,
            Some(&ops_for(&[
                "we go s l o w l y now",
                "a normal line of dialogue here",
                "another perfectly ordinary line",
                "and a third one for the median",
            ])),
        );
        assert_eq!(fixed, 0);
        assert_eq!(items[0].str, "we go s l o w l y now");
    }

    #[test]
    fn an_ambiguous_match_is_refused_rather_than_guessed() {
        // Two runs on the page collapse to the same key. Picking one would be
        // a coin flip, so neither is applied.
        let mut items = vec![
            it("thes e men.", 10),
            it("a normal line of dialogue here", 30),
            it("another perfectly ordinary line", 31),
            it("and a third one for the median", 30),
        ];
        let fixed = repair_phantom_spaces(
            &mut items,
            Some(&ops_for(&[
                "these men.",
                "these men.",
                "a normal line of dialogue here",
                "another perfectly ordinary line",
                "and a third one for the median",
            ])),
        );
        assert_eq!(fixed, 0);
        assert_eq!(items[0].str, "thes e men.");
    }

    #[test]
    fn an_item_whose_ink_matches_its_string_is_never_touched() {
        let mut items = vec![
            it("these men.", 10),
            it("a normal line of dialogue here", 30),
            it("another perfectly ordinary line", 31),
            it("and a third one for the median", 30),
        ];
        assert_eq!(
            repair_phantom_spaces(
                &mut items,
                Some(&ops_for(&[
                    "these men.",
                    "a normal line of dialogue here",
                    "another perfectly ordinary line",
                    "and a third one for the median",
                ])),
            ),
            0
        );
    }

    #[test]
    fn no_operator_list_means_no_repair_not_a_crash() {
        let mut items = vec![it("thes e men.", 10)];
        assert_eq!(repair_phantom_spaces(&mut items, None), 0);
        assert_eq!(items[0].str, "thes e men.");
    }
}

// ── the repair is counted, never silent ──────────────────────────────
//
// repairPhantomSpaces changes an author's text. The owner's first instinct
// was to make it opt-in, and the concern behind that is right: we should
// not quietly rewrite someone's script. But a toggle answers it badly,
// because it defaults to something (off, and people see broken words and
// never find the setting; on, and the switch exists for a case nobody can
// name) and because this is a correctness repair, not a preference: the
// PDF says "these", we were writing "thes e".
//
// So it reports instead of asking. The count rides out with the result, the
// app can say "repaired 94 spacing artifacts", and a number that suddenly
// jumps is how anyone would notice it misfiring on a script.
mod extract_document_reports_what_it_repaired {
    use super::*;

    #[test]
    fn the_count_is_part_of_the_extraction_result() {
        let out = extract_document(&common::facts("screenplay"), None, None);
        // The committed fixture is clean, so the honest answer here is zero,
        // and zero has to be REPORTED rather than absent, or a caller cannot
        // tell "nothing to repair" from "this engine does not count". (In Rust
        // the field is a u32 and cannot be absent; the value is still checked.)
        assert_eq!(out.spacing_repairs, 0);
        assert!(out.page_count > 0);
    }

    #[test]
    fn a_pdf_that_needs_repairs_reports_a_non_zero_count() {
        // The committed fixtures are all clean, so the only honest source for
        // this is a real file. Skips rather than lies when the corpus is absent
        // (it is gitignored and does not exist in CI). The TypeScript named one
        // real script; here every real dump the oracle counted more than 50
        // repairs on must come out above 50 too.
        for (n, dir) in common::real_dumps().iter().enumerate() {
            let Some(oracle) = common::read_json(&dir.join("lines.json")) else {
                continue;
            };
            let theirs = oracle["spacingRepairs"].as_u64().unwrap_or(0);
            if theirs <= 50 {
                continue;
            }
            let out = extract_document(&common::facts_at(dir), None, None);
            assert!(
                out.spacing_repairs > 50,
                "real script {n}: {} repairs",
                out.spacing_repairs
            );
        }
    }
}

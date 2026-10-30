//! The serializer's half of tests/parser-pipeline.test.ts: the `toFountain`
//! strings of the three page-seam tests, and the two dual-dialogue tests
//! whose every assertion is on `toFountain` and fountain-js tokens. The
//! element-type halves are sp-parser/tests/parser_pipeline.rs.
//!
//! They are asserted on what the PARSER makes from ordinary lines, not on
//! hand-built elements, because their reachability is the point: a shape
//! the parser never makes could be dismissed, and these are shapes it makes.

use sp_fountain::{to_fountain, tokenize};
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions, RawImage, RawLine};
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
        placed: false,
    }
}

fn image(y: f64, page_num: u32, file: &str) -> RawImage {
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

fn markers() -> FormatOptions {
    let mut o = DEFAULT_FORMAT_OPTIONS;
    o.show_page_markers = true;
    o
}

fn fountain(lines: &[RawLine], images: &[RawImage], format: &FormatOptions) -> String {
    to_fountain(&parse_lines(lines, images), None, format)
}

// ── a plate in a page seam, with page markers on ──

#[test]
fn a_cue_at_the_foot_of_a_page_keeps_the_speech_under_the_next_page_plate() {
    let lines = [
        line("INT. ROOM - DAY", 17.0, 700.0, 1),
        line("The lamp holds.", 17.0, 400.0, 1),
        line("BUNNY", 40.0, 72.0, 1),
        line("Her line.", 30.0, 700.0, 2),
        line("INT. NEXT - DAY", 17.0, 660.0, 2),
    ];
    assert_eq!(
        fountain(&lines, &[image(756.0, 2, "p002-01.png")], &markers()),
        "INT. ROOM - DAY\n\nThe lamp holds.\n\n@BUNNY\nHer line.\n\n\
         [[img: p002-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n"
    );
}

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

const SEAM_OUTPUT: &str = "INT. ROOM - DAY\n\n@BUNNY\nFirst half.\nsecond half.\n\n\
                           [[img: p002-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n";

#[test]
fn a_plate_below_the_cue_does_not_re_split_the_rejoined_speech() {
    assert_eq!(
        fountain(&contd_seam(), &[image(700.0, 2, "p002-01.png")], &markers()),
        SEAM_OUTPUT
    );
}

#[test]
fn a_plate_above_the_cue_does_not_switch_the_rejoin_off() {
    assert_eq!(
        fountain(&contd_seam(), &[image(760.0, 2, "p002-01.png")], &markers()),
        SEAM_OUTPUT
    );
}

// ── THE EXCEPTION: an image inside a dual-dialogue region ──

fn dual_exchange() -> Vec<RawLine> {
    let mut right = line("CASSIUS", 40.0, 500.0, 1);
    right.dual_right = Some(true);
    vec![
        line("BUNNY", 40.0, 500.0, 1),
        line("Left one.", 30.0, 488.0, 1),
        right,
        line("Right one.", 30.0, 488.0, 1),
    ]
}

#[test]
fn and_the_cost_is_a_misplaced_picture_nothing_more() {
    assert_eq!(
        fountain(&dual_exchange(), &[], &DEFAULT_FORMAT_OPTIONS),
        "@BUNNY\nLeft one.\n\n@CASSIUS ^\nRight one.\n"
    );
    let out = fountain(
        &dual_exchange(),
        &[image(494.0, 1, "p001-01.png")],
        &DEFAULT_FORMAT_OPTIONS,
    );
    assert_eq!(
        out,
        "@BUNNY\nLeft one.\n\n[[img: p001-01.png w=71]]\n\n@CASSIUS ^\nRight one.\n"
    );
    // Nor does the note between the columns break the pair: the tokenizer
    // still reads one dual exchange, with the note inside it.
    let kinds: Vec<String> = tokenize(&out)
        .unwrap()
        .into_iter()
        .map(|t| t.kind.as_str().to_owned())
        .collect();
    assert_eq!(
        kinds,
        [
            "dual_dialogue_begin",
            "dialogue_begin",
            "character",
            "dialogue",
            "dialogue_end",
            "note",
            "dialogue_begin",
            "character",
            "dialogue",
            "dialogue_end",
            "dual_dialogue_end",
        ]
    );
}

#[test]
fn an_image_between_a_cue_and_its_speech_costs_nothing_in_one_column() {
    let single = [
        line("BUNNY", 40.0, 500.0, 1),
        line("Her line.", 30.0, 470.0, 1),
    ];
    assert_eq!(
        fountain(&single, &[], &DEFAULT_FORMAT_OPTIONS),
        "@BUNNY\nHer line.\n"
    );
    assert_eq!(
        fountain(
            &single,
            &[image(480.0, 1, "p001-01.png")],
            &DEFAULT_FORMAT_OPTIONS
        ),
        "@BUNNY\nHer line.\n\n[[img: p001-01.png w=71]]\n"
    );
}

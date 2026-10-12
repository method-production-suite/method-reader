//! Port of tests/cover.test.ts: which picture is the book's cover. The
//! renderers' halves are tested beside each renderer.

use std::collections::HashSet;

use serde_json::json;
use sp_fountain::{pick_cover, tokenize};
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions, TokenType, resolve_format_options};

const CARD: &str = "[[img: p001-01.png w=100 full]]\n\n";
const SCRIPT: &str = "INT. LAB - DAY\n\nWork.\n\nEXT. YARD - NIGHT\n\nRest.\n";

fn have() -> HashSet<String> {
    ["p001-01.png", "p002-01.png"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

fn cover_of(src: &str, format: &FormatOptions, have: &HashSet<String>) -> Option<String> {
    pick_cover(&tokenize(src).unwrap(), format, have).map(|c| c.file)
}

fn cover(src: &str) -> Option<String> {
    cover_of(src, &DEFAULT_FORMAT_OPTIONS, &have())
}

#[test]
fn a_full_page_picture_before_the_first_heading_is_the_cover() {
    let tokens = tokenize(&format!("{CARD}{SCRIPT}")).unwrap();
    let c = pick_cover(&tokens, &DEFAULT_FORMAT_OPTIONS, &have()).unwrap();
    assert_eq!(c.file, "p001-01.png");
    // The note itself, so a renderer can leave out THIS note.
    let note = tokens
        .iter()
        .position(|t| t.kind == TokenType::Note)
        .unwrap();
    assert_eq!(c.index, note);
}

#[test]
fn both_options_are_on_by_default() {
    const { assert!(DEFAULT_FORMAT_OPTIONS.include_images) };
    const { assert!(DEFAULT_FORMAT_OPTIONS.promote_cover_image) };
}

#[test]
fn the_title_page_block_is_not_body_content() {
    assert_eq!(
        cover(&format!(
            "Title: T\nCredit: Written by\nAuthor: A\n\n{CARD}{SCRIPT}"
        ))
        .as_deref(),
        Some("p001-01.png")
    );
}

#[test]
fn a_note_that_renders_nothing_does_not_stand_in_front_of_the_card() {
    for before in [
        "[[a note to self]]",
        "[[img: p009-01.png w=50]]",
        "[[img: ../x.png w=100 full]]",
    ] {
        assert_eq!(
            cover(&format!("{before}\n\n{CARD}{SCRIPT}")).as_deref(),
            Some("p001-01.png"),
            "{before}"
        );
    }
}

#[test]
fn a_full_page_plate_after_a_heading_is_not_the_cover() {
    assert_eq!(cover(&format!("INT. LAB - DAY\n\n{CARD}Work.\n")), None);
}

#[test]
fn a_picture_that_is_not_full_page_is_not_the_cover_and_the_first_decides() {
    let panel = "[[img: p002-01.png w=42]]\n\n";
    assert_eq!(cover(&format!("{panel}{SCRIPT}")), None);
    assert_eq!(cover(&format!("{panel}{CARD}{SCRIPT}")), None);
}

#[test]
fn anything_else_before_the_card_means_no_cover() {
    for before in [
        "A cold open.",
        "> FADE IN:",
        "> THE END <",
        ".MONTAGE",
        "= pg 1",
        "= A synopsis.",
        "# ACT ONE",
        "===",
    ] {
        assert_eq!(
            cover(&format!("{before}\n\n{CARD}{SCRIPT}")),
            None,
            "{before}"
        );
    }
}

#[test]
fn a_picture_the_book_cannot_ship_is_not_the_cover() {
    let src = format!("{CARD}{SCRIPT}");
    let off = resolve_format_options(
        Some(&json!({ "includeImages": false })),
        &DEFAULT_FORMAT_OPTIONS,
    );
    assert_eq!(cover_of(&src, &off, &have()), None);
    let only_two: HashSet<String> = ["p002-01.png".to_owned()].into();
    assert_eq!(cover_of(&src, &DEFAULT_FORMAT_OPTIONS, &only_two), None);
    assert_eq!(
        cover_of(&src, &DEFAULT_FORMAT_OPTIONS, &HashSet::new()),
        None
    );
    let hostile = "[[img: ../../etc/p001-01.png w=100 full]]\n\n";
    let poisoned: HashSet<String> = ["../../etc/p001-01.png".to_owned()].into();
    assert_eq!(
        cover_of(
            &format!("{hostile}{SCRIPT}"),
            &DEFAULT_FORMAT_OPTIONS,
            &poisoned
        ),
        None
    );
}

#[test]
fn promote_cover_image_off_means_no_cover() {
    let off = resolve_format_options(
        Some(&json!({ "promoteCoverImage": false })),
        &DEFAULT_FORMAT_OPTIONS,
    );
    assert_eq!(cover_of(&format!("{CARD}{SCRIPT}"), &off, &have()), None);
}

#[test]
fn a_script_with_no_heading_at_all_can_still_have_a_cover() {
    assert_eq!(
        cover(&format!("{CARD}Just action.\n")).as_deref(),
        Some("p001-01.png")
    );
}

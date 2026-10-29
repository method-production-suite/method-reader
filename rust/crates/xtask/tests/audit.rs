//! `cargo xtask audit-corpus`'s judgement: the shapes in a parser's element
//! list that mean a speech was misread. Each case is one the real scripts
//! produced before it was fixed, retold with invented names.

use serde_json::{Value, json};
use xtask::audit::{Shape, findings};

fn el(kind: &str, text: &str) -> Value {
    json!({ "type": kind, "text": text, "pageNum": 3, "isTitlePage": false })
}

fn shapes(els: &[Value]) -> Vec<Shape> {
    findings(els).into_iter().map(|f| f.shape).collect()
}

#[test]
fn a_clean_speech_has_nothing_to_say() {
    let els = [
        el("action", "The radio crackles."),
        el("character", "NOOR (V.O.)"),
        el("parenthetical", "(whispering)"),
        el("dialogue", "Come back."),
    ];
    assert!(findings(&els).is_empty());
}

#[test]
fn a_parenthetical_read_as_description_is_found() {
    // The page-one misread: the bracketed line fell to action, and the line
    // under it lost its speaker.
    let els = [
        el("character", "NOOR (V.O.)"),
        el("action", "(whispering)"),
        el("action", "Come back."),
    ];
    assert_eq!(shapes(&els), vec![Shape::BracketedDescription]);
}

#[test]
fn one_lost_in_the_middle_of_a_speech_is_found_too() {
    let els = [
        el("character", "NOOR"),
        el("dialogue", "A little."),
        el("action", "(pause)"),
        el("action", "Then why stay?"),
    ];
    assert_eq!(shapes(&els), vec![Shape::BracketedDescription]);
}

#[test]
fn more_at_a_page_break_numbered_lists_and_signs_are_not_misreads() {
    // (MORE) is dropped from the book anyway; "3) In a hotel room" opens no
    // bracket; a sign in capitals is a "cue" with no lines, and comes out as
    // description, which is right.
    let els = [
        el("character", "NOOR"),
        el("dialogue", "Wait."),
        el("action", "(MORE)"),
        el("character", "SYD"),
        el("dialogue", "Fine."),
        el("action", "3) In a hotel room. Noor films."),
        el("character", "OPEN ALL NIGHT"),
        el("action", "The sign flickers."),
    ];
    assert!(findings(&els).is_empty(), "{:?}", shapes(&els));
}

#[test]
fn a_wrapped_parenthetical_split_in_two_is_found() {
    let els = [
        el("character", "NOOR"),
        el("action", "(low, to the radio"),
        el("action", "operator)"),
        el("action", "Copy that."),
    ];
    assert_eq!(shapes(&els), vec![Shape::SplitBracket, Shape::SplitBracket]);
}

#[test]
fn a_name_swallowed_into_a_speech_is_found() {
    // The dual-dialogue overrun: the next speaker's name appended to the
    // left column's speech.
    let els = [
        el("character", "LOU"),
        el("dialogue", "Keep it down."),
        el("dialogue", "SYD"),
        el("character", "SYD"),
        el("dialogue", "Not a chance."),
    ];
    assert_eq!(shapes(&els), vec![Shape::NameInSpeech]);
}

#[test]
fn title_page_lines_and_description_in_brackets_are_not_flagged() {
    // A title-page "cue" is title furniture, and a bracketed line inside
    // description (not after a speech) is the writer's own.
    let mut title = el("character", "THE FIELD STATION");
    title["isTitlePage"] = json!(true);
    let els = [
        title,
        el("action", "Written by"),
        el("action", "The hut is dark."),
        el("action", "(Beat.)"),
    ];
    assert!(findings(&els).is_empty(), "{:?}", shapes(&els));
}

#[test]
fn a_picture_or_page_number_between_a_speech_and_its_lost_parenthetical_is_passed_over() {
    let els = [
        el("character", "NOOR"),
        el("dialogue", "There."),
        el("page-number", "12."),
        el("action", "(away from the radio)"),
    ];
    assert_eq!(shapes(&els), vec![Shape::BracketedDescription]);
}

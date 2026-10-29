//! The serializer blocks of tests/options.test.ts: rejoinSplitDialogue,
//! contdMode, showPageMarkers and dualDialogue, the stage-1 options that
//! move the `.fountain`.

mod support;

use serde_json::json;
use sp_fountain::to_fountain;
use sp_model::{
    DEFAULT_FORMAT_OPTIONS, FormatOptions, ParsedScreenplay, ScreenplayElement,
    resolve_format_options,
};
use support::elements::*;

fn opts(partial: serde_json::Value) -> FormatOptions {
    resolve_format_options(Some(&partial), &DEFAULT_FORMAT_OPTIONS)
}

fn out(els: Vec<ScreenplayElement>, format: FormatOptions) -> String {
    to_fountain(&screenplay(els), None, &format)
}

// ── toFountain rejoin option ─────────────────────────────

fn split() -> Vec<ScreenplayElement> {
    vec![
        said(Character, "JACK", "JACK"),
        said(Dialogue, "First half", "JACK"),
        said(Character, "JACK (CONT'D)", "JACK"),
        said(Dialogue, "second half.", "JACK"),
    ]
}

#[test]
fn rejoin_on_by_default_merges_the_split_speech() {
    assert_eq!(
        out(split(), DEFAULT_FORMAT_OPTIONS),
        "@JACK\nFirst half\nsecond half.\n"
    );
}

#[test]
fn rejoin_off_keeps_the_contd_cue() {
    assert_eq!(
        out(split(), opts(json!({ "rejoinSplitDialogue": false }))),
        "@JACK\nFirst half\n\n@JACK (CONT'D)\nsecond half.\n"
    );
}

#[test]
fn rejoin_never_pulls_a_speech_into_a_dual_column() {
    // A (CONT'D) straight after a side-by-side exchange is said after both
    // speeches, not alongside one: rejoined, it became the right column's
    // last line and lost its cue.
    let els = vec![
        said(Character, "LOU", "LOU"),
        said(Dialogue, "Keep it down.", "LOU"),
        said(Character, "SYD", "SYD").dual_right(),
        said(Dialogue, "Not a chance.", "SYD"),
        said(Character, "SYD (CONT'D)", "SYD"),
        said(Dialogue, "Never mind.", "SYD"),
    ];
    let o = out(els, DEFAULT_FORMAT_OPTIONS);
    assert!(o.contains("@SYD ^\nNot a chance.\n\n@SYD"), "{o}");
    assert!(o.ends_with("\nNever mind.\n"), "{o}");
}

// ── contdMode ────────────────────────────────────────────

fn interrupted() -> Vec<ScreenplayElement> {
    vec![
        el(Scene, "INT. LAB - DAY"),
        said(Character, "JACK", "JACK"),
        said(Dialogue, "Look at this.", "JACK"),
        el(Action, "He points."),
        said(Character, "JACK", "JACK"),
        said(Dialogue, "See?", "JACK"),
    ]
}

#[test]
fn contd_auto_adds_contd_when_the_same_speaker_continues_through_action() {
    let o = out(interrupted(), opts(json!({ "contdMode": "auto" })));
    assert!(o.contains("@JACK (CONT'D)\nSee?"));
}

#[test]
fn contd_auto_strips_stale_contd_after_a_different_speaker() {
    let o = out(
        vec![
            said(Character, "DEV", "DEV"),
            said(Dialogue, "Hi.", "DEV"),
            said(Character, "JACK (CONT'D)", "JACK"),
            said(Dialogue, "Hello.", "JACK"),
        ],
        opts(json!({ "contdMode": "auto" })),
    );
    assert!(o.contains("@JACK\nHello."));
    assert!(!o.contains("JACK (CONT'D)"));
}

#[test]
fn contd_auto_resets_at_scene_boundaries() {
    let o = out(
        vec![
            said(Character, "JACK", "JACK"),
            said(Dialogue, "Bye.", "JACK"),
            el(Scene, "INT. HALL - DAY"),
            said(Character, "JACK", "JACK"),
            said(Dialogue, "New scene.", "JACK"),
        ],
        opts(json!({ "contdMode": "auto" })),
    );
    assert!(o.contains("@JACK\nNew scene."));
}

#[test]
fn contd_strip_removes_all_and_adds_none() {
    let mut with = interrupted();
    with[4] = said(Character, "JACK (CONT'D)", "JACK");
    let o = out(with, opts(json!({ "contdMode": "strip" })));
    assert!(!o.contains("CONT'D"));
}

#[test]
fn contd_keep_leaves_source_cues_untouched() {
    let o = out(interrupted(), opts(json!({ "contdMode": "keep" })));
    assert!(o.contains("@JACK\nSee?"));
}

// ── showPageMarkers ──────────────────────────────────────

fn sheets() -> ParsedScreenplay {
    let mut sp = screenplay(vec![
        el(Scene, "INT. LAB - DAY"),
        el(Action, "Work happens."),
        el(PageNumber, "1.").page(2),
        el(Action, "More work.").page(2),
        el(PageNumber, "2.").page(3),
        el(Action, "Even more.").page(3),
    ]);
    sp.page_count = 3;
    sp
}

#[test]
fn page_markers_on_by_default() {
    let o = to_fountain(&sheets(), None, &DEFAULT_FORMAT_OPTIONS);
    assert!(o.contains("= pg 1\n\nMore work."));
    assert!(o.contains("= pg 2\n\nEven more."));
}

#[test]
fn page_markers_can_still_be_turned_off() {
    let o = to_fountain(&sheets(), None, &opts(json!({ "showPageMarkers": false })));
    assert!(!o.contains("= pg"));
}

#[test]
fn page_markers_carry_the_pdf_printed_numbering_not_the_sheet_index() {
    let o = to_fountain(&sheets(), None, &opts(json!({ "showPageMarkers": true })));
    assert!(o.contains("= pg 1\n\nMore work."));
    assert!(o.contains("= pg 2\n\nEven more."));
}

#[test]
fn a_page_starting_mid_speech_defers_its_marker_to_the_next_block() {
    let o = out(
        vec![
            el(PageNumber, "5.").page(5),
            el(Action, "Setup.").page(5),
            said(Character, "JACK", "JACK").page(5),
            said(Dialogue, "Speech starts here", "JACK").page(5),
            said(Dialogue, "and continues overleaf.", "JACK").page(6),
            el(Action, "He exits.").page(6),
        ],
        opts(json!({ "showPageMarkers": true })),
    );
    assert!(o.contains("Speech starts here\nand continues overleaf."));
    assert!(o.contains("= pg 6\n\nHe exits."));
}

// ── dualDialogue ─────────────────────────────────────────

fn dual() -> Vec<ScreenplayElement> {
    vec![
        said(Character, "VERA", "VERA"),
        said(Dialogue, "Read me the last page--", "VERA"),
        said(Character, "INFORMANT", "INFORMANT").dual_right(),
        said(Dialogue, "--the last page burned.", "INFORMANT"),
    ]
}

#[test]
fn side_by_side_by_default_emits_the_caret_on_the_right_cue() {
    assert!(out(dual(), DEFAULT_FORMAT_OPTIONS).contains("@INFORMANT ^"));
}

#[test]
fn sequential_mode_omits_the_caret() {
    let o = out(dual(), opts(json!({ "dualDialogue": "sequential" })));
    assert!(o.contains("@INFORMANT\n"));
    assert!(!o.contains('^'));
}

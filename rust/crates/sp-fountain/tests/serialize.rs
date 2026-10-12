//! Port of tests/serialize.test.ts.

mod support;

use sp_fountain::{
    TitleMeta, extract_title_meta, is_mini_slug, parse_img_note, to_fountain, tokenize,
};
use sp_model::{
    DEFAULT_FORMAT_OPTIONS, FamilyBucket, FormatOptions, ImageRef, ParsedScreenplay, SizeStep,
    Token, TokenType,
};
use support::elements::*;

fn fountain(sp: &ParsedScreenplay) -> String {
    to_fountain(sp, None, &DEFAULT_FORMAT_OPTIONS)
}

fn fountain_with(sp: &ParsedScreenplay, format: FormatOptions) -> String {
    to_fountain(sp, None, &format)
}

fn tokens(text: &str) -> Vec<Token> {
    tokenize(text).unwrap()
}

fn types(text: &str) -> Vec<String> {
    tokens(text)
        .iter()
        .map(|t| t.kind.as_str().to_owned())
        .collect()
}

fn note_tokens(text: &str) -> Vec<Token> {
    tokens(text)
        .into_iter()
        .filter(|t| t.kind == TokenType::Note)
        .collect()
}

fn markers(on: bool) -> FormatOptions {
    FormatOptions {
        show_page_markers: on,
        ..DEFAULT_FORMAT_OPTIONS
    }
}

// ── extractTitleMeta ─────────────────────────────────────

#[test]
fn title_joins_readable_elements_before_by_line_author_follows_it() {
    let meta = extract_title_meta(&[
        el(Action, "MOONQUAKE").title_page(),
        el(Action, "JAMBOREE").title_page(),
        el(Action, "Written by").title_page(),
        el(Action, "Fenwick Sprocket & Tallulah Quibble").title_page(),
        el(Action, "© 2025 Placeholder Pictures")
            .title_page()
            .unreadable(),
        el(Scene, "INT. HOUSE - DAY").page(2),
    ]);
    assert_eq!(meta.title.as_deref(), Some("Moonquake Jamboree"));
    assert_eq!(
        meta.author.as_deref(),
        Some("Fenwick Sprocket & Tallulah Quibble")
    );
}

#[test]
fn inline_by_line_yields_author_from_same_element() {
    let meta = extract_title_meta(&[
        el(Action, "ZEPPELIN PICNIC").title_page(),
        el(Action, "written by Barnaby Fictitious").title_page(),
    ]);
    assert_eq!(meta.title.as_deref(), Some("Zeppelin Picnic"));
    assert_eq!(meta.author.as_deref(), Some("Barnaby Fictitious"));
}

#[test]
fn no_title_page_elements_yields_empty_meta() {
    let meta = extract_title_meta(&[el(Scene, "INT. HOUSE - DAY")]);
    assert_eq!(meta, TitleMeta::default());
}

#[test]
fn non_readable_contact_info_is_never_the_author() {
    let meta = extract_title_meta(&[
        el(Action, "GARGOYLE BRUNCH").title_page(),
        el(Action, "By").title_page(),
        el(Action, "Imaginaria Nobody").title_page(),
        el(Action, "imaginaria@example.com")
            .title_page()
            .unreadable(),
    ]);
    assert_eq!(meta.author.as_deref(), Some("Imaginaria Nobody"));
}

// ── toFountain ───────────────────────────────────────────

#[test]
fn emits_title_block_when_meta_present() {
    let meta = TitleMeta {
        title: Some("My Script".into()),
        author: Some("Jane Doe".into()),
    };
    let out = to_fountain(
        &screenplay(vec![el(Scene, "INT. HOUSE - DAY")]),
        Some(&meta),
        &DEFAULT_FORMAT_OPTIONS,
    );
    assert!(out.starts_with("Title: My Script\nCredit: Written by\nAuthor: Jane Doe\n\n"));
}

#[test]
fn scene_cue_parenthetical_dialogue_action_serialize_with_blank_lines() {
    let out = fountain(&screenplay(vec![
        el(Scene, "INT. KITCHEN - DAY"),
        said(Character, "JACK", "JACK"),
        said(Parenthetical, "(whispering)", "JACK"),
        said(Dialogue, "Hello there.", "JACK"),
        el(Action, "The door slams."),
    ]));
    assert_eq!(
        out,
        "INT. KITCHEN - DAY\n\n@JACK\n(whispering)\nHello there.\n\nThe door slams.\n"
    );
}

#[test]
fn page_number_and_non_readable_elements_are_skipped() {
    let out = fountain(&screenplay(vec![
        el(PageNumber, "42."),
        el(PageNumber, "Blue Rev. (6/12/26)"),
        el(Action, "Some action."),
        el(Action, "skip me").unreadable(),
    ]));
    assert_eq!(out, "Some action.\n");
}

#[test]
fn title_page_elements_are_excluded_from_body() {
    let out = fountain(&screenplay(vec![
        el(Action, "MY TITLE").title_page(),
        el(Scene, "INT. HOUSE - DAY"),
    ]));
    assert_eq!(out, "INT. HOUSE - DAY\n");
}

#[test]
fn scene_numbers_serialize_as_hash_n_hash() {
    let out = fountain(&screenplay(vec![
        el(Scene, "INT. LAB - NIGHT").scene_no("1A."),
    ]));
    assert_eq!(out, "INT. LAB - NIGHT #1A.#\n");
}

#[test]
fn transitions_are_forced_with_gt() {
    let out = fountain(&screenplay(vec![el(Transition, "SMASH CUT TO:")]));
    assert_eq!(out, "> SMASH CUT TO:\n");
}

#[test]
fn action_starting_with_dot_or_gt_is_forced_with_bang() {
    let out = fountain(&screenplay(vec![
        el(Action, ".45 on the table."),
        el(Action, "> ominous graffiti"),
    ]));
    assert_eq!(out, "!.45 on the table.\n\n!> ominous graffiti\n");
}

#[test]
fn mini_slugs_serialize_as_forced_sluglines() {
    let out = fountain(&screenplay(vec![el(MiniSlug, "LATER")]));
    assert_eq!(out, ".LATER\n");
}

#[test]
fn a_mini_slug_fountain_cannot_force_stays_an_action_line() {
    let out = fountain(&screenplay(vec![el(MiniSlug, ".45 ON THE COUNTER")]));
    assert_eq!(out, "!.45 ON THE COUNTER\n");
}

#[test]
fn other_fountain_markers_ride_through_the_dot_force_intact() {
    let out = fountain(&screenplay(vec![el(MiniSlug, "#2 CAMERA")]));
    assert_eq!(out, ".#2 CAMERA\n");
    let got: Vec<(String, String)> = tokens(&out)
        .iter()
        .map(|t| (t.kind.as_str().to_owned(), t.text_or_empty().to_owned()))
        .collect();
    assert_eq!(got, vec![("scene_heading".into(), "#2 CAMERA".into())]);
}

#[test]
fn a_mini_slug_cuts_the_contd_chain() {
    let out = fountain(&screenplay(vec![
        said(Character, "MARGO", "MARGO"),
        said(Dialogue, "Go home.", "MARGO"),
        el(MiniSlug, "LATER"),
        said(Character, "MARGO", "MARGO"),
        said(Dialogue, "Still here.", "MARGO"),
    ]));
    assert!(!out.contains("(CONT'D)"));
}

#[test]
fn a_mini_slug_never_becomes_a_transition_on_reparse() {
    let out = fountain(&screenplay(vec![el(MiniSlug, "BACK TO:")]));
    let got: Vec<String> = types(&out)
        .into_iter()
        .filter(|t| t != "title_page")
        .collect();
    assert_eq!(got, vec!["scene_heading"]);
}

#[test]
fn more_parentheticals_are_dropped() {
    let out = fountain(&screenplay(vec![
        said(Character, "JACK", "JACK"),
        said(Dialogue, "I was saying—", "JACK"),
        said(Parenthetical, "(MORE)", "JACK"),
    ]));
    assert_eq!(out, "@JACK\nI was saying—\n");
}

#[test]
fn contd_cue_after_page_break_merges_into_the_same_speech() {
    let out = fountain(&screenplay(vec![
        said(Character, "JACK", "JACK"),
        said(Dialogue, "First half of speech", "JACK"),
        said(Parenthetical, "(MORE)", "JACK"),
        el(PageNumber, "3.").page(2),
        said(Character, "JACK (CONT'D)", "JACK").page(2),
        said(Dialogue, "and the second half.", "JACK").page(2),
    ]));
    assert_eq!(out, "@JACK\nFirst half of speech\nand the second half.\n");
}

#[test]
fn contd_cue_after_intervening_action_keeps_its_own_cue() {
    let out = fountain(&screenplay(vec![
        said(Character, "JACK", "JACK"),
        said(Dialogue, "Look at this.", "JACK"),
        el(Action, "He points at the wall."),
        said(Character, "JACK (CONT'D)", "JACK"),
        said(Dialogue, "See what I mean?", "JACK"),
    ]));
    assert_eq!(
        out,
        "@JACK\nLook at this.\n\nHe points at the wall.\n\n@JACK (CONT'D)\nSee what I mean?\n"
    );
}

#[test]
fn cue_with_no_following_dialogue_is_demoted_to_action() {
    let out = fountain(&screenplay(vec![
        said(Character, "ORPHAN CUE", "ORPHAN CUE"),
        el(Action, "The lights go out."),
    ]));
    assert_eq!(out, "ORPHAN CUE\n\nThe lights go out.\n");
}

// ── toFountain with images ───────────────────────────────

#[test]
fn image_writes_a_standalone_note_with_a_blank_line_either_side() {
    let out = fountain(&screenplay(vec![
        el(Scene, "INT. A - DAY"),
        img("p001-01.png"),
        el(Action, "Someone waits."),
    ]));
    assert_eq!(
        out,
        "INT. A - DAY\n\n[[img: p001-01.png w=71]]\n\nSomeone waits.\n"
    );
}

#[test]
fn image_marks_a_full_page_image() {
    let out = fountain(&screenplay(vec![img("p001-01.png").width(100.0, true)]));
    assert!(out.contains("[[img: p001-01.png w=100 full]]"));
}

#[test]
fn image_round_trips_through_fountain_js_as_its_own_note_token() {
    let out = fountain(&screenplay(vec![
        el(Scene, "INT. A - DAY"),
        img("p001-01.png"),
        el(Action, "Someone waits."),
    ]));
    let t = tokens(&out);
    assert_eq!(types(&out), vec!["scene_heading", "note", "action"]);
    assert_eq!(t[1].text_or_empty(), "img: p001-01.png w=71");
}

#[test]
fn what_the_serializer_writes_is_what_the_reader_parses() {
    let out = fountain(&screenplay(vec![
        img("p012-03.png").width(43.0, false),
        img("p012-04.png").width(100.0, true),
    ]));
    let parsed: Vec<Option<ImageRef>> = note_tokens(&out)
        .iter()
        .map(|t| parse_img_note(t.text_or_empty()))
        .collect();
    assert_eq!(
        parsed,
        vec![
            Some(ImageRef {
                file: "p012-03.png".into(),
                width_pct: 43.0,
                full_page: false
            }),
            Some(ImageRef {
                file: "p012-04.png".into(),
                width_pct: 100.0,
                full_page: true
            }),
        ]
    );
}

#[test]
fn an_image_on_a_detected_title_page_still_reaches_the_body() {
    let out = fountain(&screenplay(vec![
        el(Action, "MY TITLE").title_page(),
        img("p001-01.png").title_page(),
        el(Scene, "INT. HOUSE - DAY"),
    ]));
    assert_eq!(out, "[[img: p001-01.png w=71]]\n\nINT. HOUSE - DAY\n");
}

#[test]
fn an_unreadable_image_is_still_dropped() {
    let out = fountain(&screenplay(vec![
        img("p001-01.png").unreadable(),
        el(Scene, "INT. HOUSE - DAY"),
    ]));
    assert_eq!(out, "INT. HOUSE - DAY\n");
}

#[test]
fn two_images_in_a_row_are_two_notes_not_one_paragraph() {
    let out = fountain(&screenplay(vec![img("p001-01.png"), img("p001-02.png")]));
    assert_eq!(
        out,
        "[[img: p001-01.png w=71]]\n\n[[img: p001-02.png w=71]]\n"
    );
    let texts: Vec<String> = note_tokens(&out)
        .iter()
        .map(|t| t.text_or_empty().to_owned())
        .collect();
    assert_eq!(
        texts,
        vec!["img: p001-01.png w=71", "img: p001-02.png w=71"]
    );
}

#[test]
fn an_image_as_the_only_element_of_the_document() {
    assert_eq!(
        fountain(&screenplay(vec![img("p001-01.png")])),
        "[[img: p001-01.png w=71]]\n"
    );
}

#[test]
fn an_image_element_with_no_img_is_emitted_as_nothing_at_all() {
    let out = fountain(&screenplay(vec![
        el(Scene, "INT. A - DAY"),
        el(Image, "p001-01.png"),
        el(Action, "Someone waits."),
    ]));
    assert_eq!(out, "INT. A - DAY\n\nSomeone waits.\n");
}

#[test]
fn images_are_byte_stable_under_a_settings_flip() {
    let doc = || {
        screenplay(vec![
            el(Scene, "INT. A - DAY"),
            img("p001-01.png"),
            el(Action, "Someone waits.").fmt(Some(FamilyBucket::Sans), None),
        ])
    };
    let canonical = fountain(&doc());
    assert!(canonical.contains("[[img: p001-01.png w=71]]"));
    let no_shifts = FormatOptions {
        preserve_font_shifts: false,
        ..DEFAULT_FORMAT_OPTIONS
    };
    assert_eq!(fountain_with(&doc(), no_shifts), canonical);
    const { assert!(DEFAULT_FORMAT_OPTIONS.include_images) };
    let no_images = FormatOptions {
        include_images: false,
        ..DEFAULT_FORMAT_OPTIONS
    };
    assert_eq!(fountain_with(&doc(), no_images), canonical);
}

#[test]
fn an_image_between_a_cue_and_its_speech_does_not_destroy_the_speech() {
    let out = fountain(&screenplay(vec![
        said(Character, "BUNNY", "BUNNY"),
        img("p001-01.png"),
        said(Dialogue, "Her line.", "BUNNY"),
    ]));
    assert_eq!(out, "@BUNNY\nHer line.\n\n[[img: p001-01.png w=71]]\n");
    assert_eq!(
        types(&out),
        vec![
            "dialogue_begin",
            "character",
            "dialogue",
            "dialogue_end",
            "note"
        ]
    );
}

#[test]
fn an_image_inside_a_speech_is_deferred_past_it_in_order() {
    let out = fountain(&screenplay(vec![
        said(Character, "BUNNY", "BUNNY"),
        said(Dialogue, "First half.", "BUNNY"),
        img("p001-01.png"),
        said(Dialogue, "Second half.", "BUNNY"),
        img("p001-02.png"),
        el(Scene, "INT. NEXT - DAY"),
    ]));
    assert_eq!(
        out,
        "@BUNNY\nFirst half.\nSecond half.\n\n\
         [[img: p001-01.png w=71]]\n\n[[img: p001-02.png w=71]]\n\nINT. NEXT - DAY\n"
    );
}

#[test]
fn a_deferred_image_is_never_dropped_when_the_speech_ends_the_document() {
    let out = fountain(&screenplay(vec![
        said(Character, "BUNNY", "BUNNY"),
        said(Dialogue, "Her line.", "BUNNY"),
        img("p009-01.png"),
    ]));
    assert_eq!(out, "@BUNNY\nHer line.\n\n[[img: p009-01.png w=71]]\n");
}

#[test]
fn with_no_speech_open_the_picture_is_written_where_it_stands() {
    let out = fountain(&screenplay(vec![
        img("p001-01.png"),
        el(Dialogue, "An orphan line with no cue."),
    ]));
    assert_eq!(
        out,
        "[[img: p001-01.png w=71]]\n\nAn orphan line with no cue.\n"
    );
}

#[test]
fn a_cue_with_genuinely_nothing_under_it_is_still_demoted() {
    let out = fountain(&screenplay(vec![
        said(Character, "ORPHAN CUE", "ORPHAN CUE"),
        img("p001-01.png"),
        el(Action, "The lights go out."),
    ]));
    assert_eq!(
        out,
        "ORPHAN CUE\n\n[[img: p001-01.png w=71]]\n\nThe lights go out.\n"
    );
}

#[test]
fn the_lookahead_skips_images_and_only_images() {
    let out = fountain(&screenplay(vec![
        said(Character, "ORPHAN CUE", "ORPHAN CUE"),
        el(Action, "The lights go out."),
        said(Character, "NOOR", "NOOR"),
        said(Dialogue, "Her line.", "NOOR"),
    ]));
    assert_eq!(
        out,
        "ORPHAN CUE\n\nThe lights go out.\n\n@NOOR\nHer line.\n"
    );
    assert!(!out.contains("@ORPHAN CUE"));
}

#[test]
fn an_image_between_a_cue_and_a_parenthetical_keeps_the_cue() {
    let out = fountain(&screenplay(vec![
        said(Character, "BUNNY", "BUNNY"),
        img("p001-01.png"),
        said(Parenthetical, "(quietly)", "BUNNY"),
        said(Dialogue, "Her line.", "BUNNY"),
    ]));
    assert_eq!(
        out,
        "@BUNNY\n(quietly)\nHer line.\n\n[[img: p001-01.png w=71]]\n"
    );
}

#[test]
fn a_page_marker_never_separates_a_deferred_image_from_its_own_page() {
    let out = fountain_with(
        &screenplay(vec![
            el(PageNumber, "1."),
            said(Character, "BUNNY", "BUNNY"),
            said(Dialogue, "Her line.", "BUNNY"),
            img("p001-01.png"),
            el(Scene, "INT. NEXT - DAY").page(2),
        ]),
        markers(true),
    );
    assert_eq!(
        out,
        "@BUNNY\nHer line.\n\n[[img: p001-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n"
    );
}

#[test]
fn an_image_never_anchors_a_page_marker() {
    let out = fountain_with(
        &screenplay(vec![
            el(PageNumber, "1."),
            el(Action, "Someone waits."),
            img("p002-01.png").page(2),
            el(Scene, "INT. NEXT - DAY").page(2),
        ]),
        markers(true),
    );
    assert_eq!(
        out,
        "Someone waits.\n\n[[img: p002-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n"
    );
}

#[test]
fn a_sheet_holding_nothing_but_a_plate_gets_no_marker_of_its_own() {
    let out = fountain_with(
        &screenplay(vec![
            el(PageNumber, "1."),
            el(Action, "Someone waits."),
            img("p002-01.png").width(100.0, true).page(2),
            el(Scene, "INT. NEXT - DAY").page(3),
        ]),
        markers(true),
    );
    assert_eq!(
        out,
        "Someone waits.\n\n[[img: p002-01.png w=100 full]]\n\n= pg 3\n\nINT. NEXT - DAY\n"
    );
    assert!(!out.contains("= pg 2"));
}

#[test]
fn markers_on_a_plate_at_the_top_of_a_page_does_not_break_the_speech_under_it() {
    let els = || {
        screenplay(vec![
            said(Character, "BUNNY", "BUNNY"),
            img("p002-01.png").page(2),
            said(Dialogue, "Her line.", "BUNNY").page(2),
            el(Scene, "INT. NEXT - DAY").page(2),
        ])
    };
    let out = fountain_with(&els(), markers(true));
    assert_eq!(
        out,
        "@BUNNY\nHer line.\n\n[[img: p002-01.png w=71]]\n\n= pg 2\n\nINT. NEXT - DAY\n"
    );
    assert_eq!(
        types(&out),
        vec![
            "dialogue_begin",
            "character",
            "dialogue",
            "dialogue_end",
            "note",
            "synopsis",
            "scene_heading"
        ]
    );
    let off = fountain_with(&els(), markers(false));
    assert_eq!(off, out.replace("\n\n= pg 2", ""));
}

#[test]
fn a_plate_above_a_contd_cue_does_not_switch_the_rejoin_off() {
    let out = fountain(&screenplay(vec![
        said(Character, "BUNNY", "BUNNY"),
        said(Dialogue, "First half.", "BUNNY"),
        img("p002-01.png").page(2),
        said(Character, "BUNNY (CONT'D)", "BUNNY").page(2),
        said(Dialogue, "second half.", "BUNNY").page(2),
    ]));
    assert_eq!(
        out,
        "@BUNNY\nFirst half.\nsecond half.\n\n[[img: p002-01.png w=71]]\n"
    );
    assert!(!out.contains("(CONT'D)"));
}

#[test]
fn and_the_rejoin_is_still_a_toggle_not_a_thing_images_turned_on() {
    let out = fountain_with(
        &screenplay(vec![
            said(Character, "BUNNY", "BUNNY"),
            said(Dialogue, "First half.", "BUNNY"),
            img("p002-01.png").page(2),
            said(Character, "BUNNY (CONT'D)", "BUNNY").page(2),
            said(Dialogue, "second half.", "BUNNY").page(2),
        ]),
        FormatOptions {
            rejoin_split_dialogue: false,
            show_page_markers: false,
            ..DEFAULT_FORMAT_OPTIONS
        },
    );
    assert_eq!(
        out,
        "@BUNNY\nFirst half.\n\n[[img: p002-01.png w=71]]\n\n@BUNNY (CONT'D)\nsecond half.\n"
    );
}

#[test]
fn the_emitted_width_is_an_integer_even_if_the_element_carries_a_fraction() {
    let out = fountain(&screenplay(vec![img("p001-01.png").width(71.4, false)]));
    assert_eq!(out, "[[img: p001-01.png w=71]]\n");
    let note = &note_tokens(&out)[0];
    assert_eq!(
        parse_img_note(note.text_or_empty()).map(|i| i.width_pct),
        Some(71.0)
    );
}

#[test]
fn markers_on_a_parenthetical_at_the_top_of_a_page_keeps_its_speech_too() {
    let out = fountain_with(
        &screenplay(vec![
            said(Character, "BUNNY", "BUNNY"),
            said(Dialogue, "First half.", "BUNNY"),
            said(Parenthetical, "(quietly)", "BUNNY").page(2),
            said(Dialogue, "second half.", "BUNNY").page(2),
            el(Scene, "INT. NEXT - DAY").page(2),
        ]),
        markers(true),
    );
    assert_eq!(
        out,
        "@BUNNY\nFirst half.\n(quietly)\nsecond half.\n\n= pg 2\n\nINT. NEXT - DAY\n"
    );
}

#[test]
fn markers_on_a_plate_in_a_page_break_seam_does_not_resplit_the_speech() {
    let out = fountain_with(
        &screenplay(vec![
            said(Character, "BUNNY", "BUNNY"),
            said(Dialogue, "First half.", "BUNNY"),
            img("p002-01.png").page(2),
            said(Dialogue, "second half.", "BUNNY").page(2),
            el(Scene, "INT. NEXT - DAY").page(2),
        ]),
        markers(true),
    );
    assert_eq!(
        out,
        "@BUNNY\nFirst half.\nsecond half.\n\n[[img: p002-01.png w=71]]\n\n\
         = pg 2\n\nINT. NEXT - DAY\n"
    );
    assert!(!tokens(&out).iter().any(|t| t.kind == TokenType::Action));
}

// ── round-trip through fountain-js ───────────────────────

#[test]
fn serialized_output_tokenizes_back_to_the_same_structure() {
    let meta = TitleMeta {
        title: Some("Round Trip".into()),
        author: Some("Tester".into()),
    };
    let out = to_fountain(
        &screenplay(vec![
            el(Scene, "INT. KITCHEN - DAY").scene_no("7"),
            el(Action, "Jack enters."),
            said(Character, "JACK (V.O.)", "JACK"),
            said(Parenthetical, "(tired)", "JACK"),
            said(Dialogue, "Long day.", "JACK"),
            el(Transition, "FADE OUT."),
        ]),
        Some(&meta),
        &DEFAULT_FORMAT_OPTIONS,
    );
    assert_eq!(
        types(&out),
        vec![
            "title",
            "credit",
            "author",
            "scene_heading",
            "action",
            "dialogue_begin",
            "character",
            "parenthetical",
            "dialogue",
            "dialogue_end",
            "transition",
        ]
    );
    let t = tokens(&out);
    let scene = t
        .iter()
        .find(|t| t.kind == TokenType::SceneHeading)
        .unwrap();
    assert_eq!(scene.scene_number.as_deref(), Some("7"));
    let cue = t.iter().find(|t| t.kind == TokenType::Character).unwrap();
    assert_eq!(cue.text_or_empty(), "JACK (V.O.)");
}

/// The TypeScript test goes on to the EPUB's XHTML (`<p class="mini-slug">`);
/// that half is sp-epub's. Here: the token the renderer receives is a
/// mini-slug by the shared discriminator.
#[test]
fn a_parsed_mini_slug_reaches_the_renderers_as_a_mini_slug() {
    let out = fountain(&screenplay(vec![
        el(Scene, "INT. KITCHEN - DAY"),
        el(Action, "Jack waits."),
        el(MiniSlug, "LATER"),
        el(Action, "Jack is still waiting."),
    ]));
    let t = tokens(&out);
    let later = t.iter().find(|t| t.text_or_empty() == "LATER").unwrap();
    assert!(is_mini_slug(later));
    assert!(!is_mini_slug(&t[0]));
}

// ── regression: real-PDF artifacts ───────────────────────

#[test]
fn more_is_dropped_regardless_of_classified_type() {
    let out = fountain(&screenplay(vec![
        said(Character, "JACK", "JACK"),
        said(Dialogue, "Speech before break", "JACK"),
        el(Action, "(MORE)"),
        said(Character, "JACK (CONT'D)", "JACK"),
        said(Dialogue, "speech after break.", "JACK"),
    ]));
    assert_eq!(out, "@JACK\nSpeech before break\nspeech after break.\n");
}

#[test]
fn curly_apostrophe_contd_cue_merges_across_page_break() {
    let out = fountain(&screenplay(vec![
        said(Character, "DEV", "DEV"),
        said(Dialogue, "First part", "DEV"),
        said(Character, "DEV (CONT’D)", "DEV"),
        said(Dialogue, "second part.", "DEV"),
    ]));
    assert_eq!(out, "@DEV\nFirst part\nsecond part.\n");
}

#[test]
fn cue_internal_whitespace_is_normalized() {
    let out = fountain(&screenplay(vec![
        said(
            Character,
            "MIDDLE SEAT  PASSENGER  (V.O.)",
            "MIDDLE SEAT PASSENGER",
        ),
        said(Dialogue, "Hm.", "MIDDLE SEAT PASSENGER"),
    ]));
    assert_eq!(out, "@MIDDLE SEAT PASSENGER (V.O.)\nHm.\n");
}

// ── inline emphasis pass-through ─────────────────────────

#[test]
fn dialogue_and_action_emit_their_styled_variant() {
    let out = fountain(&screenplay(vec![
        el(Action, "The sign reads DANGER.").styled("The sign reads **DANGER**."),
        said(Character, "JACK", "JACK"),
        said(Dialogue, "I want insurance now.", "JACK").styled("I want *insurance* now."),
    ]));
    assert!(out.contains("The sign reads **DANGER**."));
    assert!(out.contains("I want *insurance* now."));
}

#[test]
fn parentheticals_and_cues_stay_plain() {
    let out = fountain(&screenplay(vec![
        said(Character, "JACK", "JACK").styled("**JACK**"),
        said(Parenthetical, "(scream singing)", "JACK").styled("*(scream singing)*"),
        said(Dialogue, "La la la", "JACK"),
    ]));
    assert!(out.contains("@JACK\n(scream singing)\nLa la la"));
}

// ── font-shift notes ─────────────────────────────────────

#[test]
fn an_action_note_is_glued_directly_above_its_block() {
    let out = fountain(&screenplay(vec![
        el(Scene, "INT. NEWSROOM - DAY"),
        el(Action, "CHYRON: BREAKING.").fmt(Some(FamilyBucket::Sans), None),
    ]));
    assert!(out.contains("[[fmt: sans]]\nCHYRON: BREAKING."));
}

#[test]
fn a_dialogue_note_leads_the_line_space_separated() {
    let out = fountain(&screenplay(vec![
        said(Character, "WREN", "WREN"),
        said(Dialogue, "Read it back.", "WREN").fmt(Some(FamilyBucket::Sans), None),
    ]));
    assert!(out.contains("@WREN\n[[fmt: sans]] Read it back."));
}

#[test]
fn family_and_size_ride_one_note_family_first() {
    let out = fountain(&screenplay(vec![
        el(Action, "A SIGN.").fmt(Some(FamilyBucket::Sans), Some(SizeStep::MuchLarger)),
    ]));
    assert!(out.contains("[[fmt: sans +2]]\nA SIGN."));
}

#[test]
fn a_size_only_shift_emits_the_size_alone() {
    let out = fountain(&screenplay(vec![
        el(Action, "INSERT: THE CARD.").fmt(None, Some(SizeStep::Larger)),
    ]));
    assert!(out.contains("[[fmt: +1]]\nINSERT: THE CARD."));
}

#[test]
fn an_element_with_no_fmt_emits_no_note() {
    assert!(!fountain(&screenplay(vec![el(Action, "Plain action.")])).contains("[["));
}

#[test]
fn only_action_and_dialogue_carry_notes() {
    let sans = Some(FamilyBucket::Sans);
    let up = Some(SizeStep::Larger);
    let out = fountain(&screenplay(vec![
        el(Scene, "INT. A - DAY").fmt(sans, None),
        said(Character, "WREN", "WREN").fmt(sans, None),
        el(Parenthetical, "(beat)").fmt(sans, None),
        said(Dialogue, "Hello.", "WREN"),
        el(Transition, "CUT TO:").fmt(None, up),
        el(MiniSlug, "LATER").fmt(None, up),
    ]));
    assert!(!out.contains("[["));
}

#[test]
fn the_note_survives_the_forced_action_prefix() {
    let out = fountain(&screenplay(vec![
        el(Action, ".45 ON THE TABLE").fmt(None, Some(SizeStep::Larger)),
    ]));
    assert!(out.contains("[[fmt: +1]]\n!.45 ON THE TABLE"));
}

// ── boneyard delimiters must never be emitted ────────────

fn both_runs() -> ParsedScreenplay {
    screenplay(vec![
        said(Character, "SINGER", "SINGER"),
        el(
            Dialogue,
            "Roll the tape, we ride tonight / Neon on the overpass",
        )
        .styled("*Roll the tape, we ride tonight /* *Neon on the overpass*"),
        el(Scene, "INT. DINER - NIGHT").page(2),
        el(Action, "A jukebox coughs to life.").page(2),
        said(Character, "SINGER", "SINGER").page(3),
        el(Dialogue, "/ And the morning takes it back")
            .styled("*/ And the morning takes it back*")
            .page(3),
        el(Scene, "INT. GARAGE - DAWN").page(4),
        el(Action, "The last chord rings out.").page(4),
    ])
}

#[test]
fn an_emphasis_run_ending_on_a_slash_does_not_emit_open_boneyard() {
    assert!(!fountain(&both_runs()).contains("/*"));
}

#[test]
fn an_emphasis_run_starting_on_a_slash_does_not_emit_close_boneyard() {
    assert!(!fountain(&both_runs()).contains("*/"));
}

#[test]
fn nothing_between_the_two_lyrics_is_swallowed_by_fountain_js() {
    let text: Vec<String> = tokens(&fountain(&both_runs()))
        .iter()
        .map(|t| t.text_or_empty().to_owned())
        .collect();
    let text = text.join("\n");
    for want in [
        "INT. DINER - NIGHT",
        "A jukebox coughs to life.",
        "INT. GARAGE - DAWN",
        "The last chord rings out.",
    ] {
        assert!(text.contains(want), "{want}");
    }
}

#[test]
fn the_slash_survives_and_the_words_around_it_keep_their_emphasis() {
    let out = fountain(&both_runs());
    assert!(out.contains('/'));
    assert!(out.contains("*Roll the tape, we ride tonight*"));
    assert!(out.contains("*And the morning takes it back*"));
}

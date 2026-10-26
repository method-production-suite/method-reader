//! The Open Screenplay Format (.osf) writer: Fountain in, an editable
//! screenplay out. Every case is a small inline Fountain string, read back
//! through a strict XML reader so a malformed document fails here rather
//! than in someone's screenwriting app. The mapping is
//! docs/superpowers/specs/2026-10-20-osf-out-design.md.

mod common;

use common::{Para, XmlElement, fountain_to_osf, paras_of, parse_xml, try_parse_xml};
use sp_osf::{OSF_DOCUMENT_TYPE, OSF_VERSION};

fn doc(fountain: &str) -> XmlElement {
    parse_xml(&fountain_to_osf(fountain))
}

fn body(fountain: &str) -> Vec<Para> {
    paras_of(&doc(fountain), "paragraphs").expect("a paragraphs element")
}

fn title_page(fountain: &str) -> Option<Vec<Para>> {
    paras_of(&doc(fountain), "titlepage")
}

fn kinds_and_texts(p: &[Para]) -> Vec<(&str, &str)> {
    p.iter().map(|x| (x.kind(), x.text.as_str())).collect()
}

// the document

#[test]
fn an_xml_declaration_then_a_document_root_at_the_one_version_constant() {
    let osf = fountain_to_osf("INT. ROOM - DAY\n");
    assert!(osf.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(osf.ends_with("</document>\n"));
    let root = parse_xml(&osf);
    assert_eq!(root.name, "document");
    assert_eq!(root.attr("type"), Some("Open Screenplay Format document"));
    assert_eq!(root.attr("type"), Some(OSF_DOCUMENT_TYPE));
    assert_eq!(root.attr("version"), Some(OSF_VERSION));
    // The SDK's own 2.0 example declares 30 (the spec's open question 1).
    assert_eq!(OSF_VERSION, "30");
}

#[test]
fn minimal_paragraphs_and_title_page_only_no_settings_styles_or_lists() {
    let root = doc("Title: A Test\n\nINT. ROOM - DAY\n");
    let names: Vec<&str> = root
        .elements(None)
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, ["paragraphs", "titlepage"]);
    assert!(!root.has_stray_text());
}

#[test]
fn no_title_page_tokens_no_title_page_at_all() {
    assert!(title_page("INT. ROOM - DAY\n").is_none());
}

#[test]
fn an_empty_script_is_still_a_well_formed_document() {
    let root = doc("");
    assert_eq!(root.name, "document");
    assert!(
        root.child("paragraphs")
            .expect("paragraphs")
            .elements(None)
            .is_empty()
    );
}

#[test]
fn every_para_is_a_style_then_its_text_runs_and_nothing_else() {
    let root = doc("INT. ROOM - DAY\n\nShe *runs*.\n\nMARGO\nGo.\n");
    for p in paras_of(&root, "paragraphs").unwrap() {
        let names: Vec<&str> =
            p.el.elements(None)
                .iter()
                .map(|e| e.name.as_str())
                .collect();
        assert_eq!(names[0], "style");
        assert!(names[1..].iter().all(|n| *n == "text"), "{names:?}");
        assert!(names.len() >= 2);
        assert!(!p.el.has_stray_text());
    }
}

// the elements

#[test]
fn scene_heading_action_character_parenthetical_dialogue_transition() {
    let p = body(
        [
            "INT. KITCHEN - NIGHT",
            "",
            "The kettle screams.",
            "",
            "MARGO",
            "(to herself)",
            "Not now.",
            "",
            "CUT TO:",
            "",
        ]
        .join("\n")
        .as_str(),
    );
    assert_eq!(
        kinds_and_texts(&p),
        [
            ("Scene Heading", "INT. KITCHEN - NIGHT"),
            ("Action", "The kettle screams."),
            ("Character", "MARGO"),
            ("Parenthetical", "(to herself)"),
            ("Dialogue", "Not now."),
            ("Transition", "CUT TO:"),
        ]
    );
}

#[test]
fn only_the_eight_built_in_style_names_are_ever_used() {
    let p = body(
        "INT. ROOM - DAY\n\n.LATER\n\n> CENTERED <\n\n~a lyric\n\nBOB\n(soft)\nHi.\n~sung\n\nALICE ^\nYo.\n\nCUT TO:\n",
    );
    for x in &p {
        assert!(
            [
                "Scene Heading",
                "Action",
                "Character",
                "Parenthetical",
                "Dialogue",
                "Transition",
                "Shot"
            ]
            .contains(&x.kind()),
            "{}",
            x.kind()
        );
    }
}

#[test]
fn a_scene_number_rides_as_scene_number_and_leaves_the_heading_text() {
    let p = body("INT. HOUSE - DAY #12A#\n");
    assert_eq!(p[0].kind(), "Scene Heading");
    assert_eq!(p[0].el.attr("scene_number"), Some("12A"));
    assert_eq!(p[0].text, "INT. HOUSE - DAY");
}

#[test]
fn an_unnumbered_heading_carries_no_scene_number_attribute() {
    let p = body("EXT. FIELD - DAY\n");
    assert_eq!(p[0].el.attr("scene_number"), None);
}

#[test]
fn a_mini_slug_is_a_shot_without_the_dot_and_without_a_scene_number() {
    let p = body("INT. HOUSE - DAY\n\n.LATER\n\nShe waits.\n\n.THE HALL #7#\n");
    assert_eq!(
        kinds_and_texts(&p),
        [
            ("Scene Heading", "INT. HOUSE - DAY"),
            ("Shot", "LATER"),
            ("Action", "She waits."),
            ("Shot", "THE HALL"),
        ]
    );
    assert_eq!(p[3].el.attr("scene_number"), None);
}

#[test]
fn forced_elements_lose_their_sigils() {
    let p = body(
        [
            ".FORCED HEADING OF A SCENE",
            "",
            "!LOUD ACTION THAT LOOKS LIKE A CUE",
            "",
            "@McCLANE",
            "Yippee.",
            "",
            "> SMASH CUT",
            "",
        ]
        .join("\n")
        .as_str(),
    );
    assert_eq!(
        kinds_and_texts(&p),
        [
            ("Shot", "FORCED HEADING OF A SCENE"),
            ("Action", "LOUD ACTION THAT LOOKS LIKE A CUE"),
            ("Character", "McCLANE"),
            ("Dialogue", "Yippee."),
            ("Transition", "SMASH CUT"),
        ]
    );
}

#[test]
fn character_extensions_stay_in_the_cue() {
    let p = body("MARGO (V.O.) (CONT'D)\nHello.\n");
    assert_eq!(p[0].kind(), "Character");
    assert_eq!(p[0].text, "MARGO (V.O.) (CONT'D)");
}

#[test]
fn centered_text_is_action_aligned_center_on_its_style() {
    let p = body("> THE END <\n");
    assert_eq!(p[0].kind(), "Action");
    assert_eq!(p[0].style.attr("align"), Some("center"));
    assert_eq!(p[0].text, "THE END");
}

#[test]
fn ordinary_paragraphs_carry_only_their_style_name() {
    for x in body("INT. ROOM - DAY\n\nShe runs.\n\nBOB\nHi.\n") {
        let keys: Vec<&str> = x.style.attrs.keys().map(String::as_str).collect();
        assert_eq!(keys, ["basestylename"]);
        assert!(x.el.attrs.is_empty());
    }
}

#[test]
fn lyrics_inside_a_speech_are_dialogue_and_outside_one_are_action() {
    let p = body("~Row, row, row\n~your boat\n\nMARGO\nI sing:\n~la la la\n");
    assert_eq!(
        kinds_and_texts(&p),
        [
            ("Action", "Row, row, row\nyour boat"),
            ("Character", "MARGO"),
            ("Dialogue", "I sing:"),
            ("Dialogue", "la la la"),
        ]
    );
}

#[test]
fn a_multi_line_token_stays_one_paragraph_with_soft_returns_blank_lines_dropped() {
    let p = body("MARGO\nFirst line\nsecond line\n\nAction one\naction two\n");
    assert_eq!(
        kinds_and_texts(&p),
        [
            ("Character", "MARGO"),
            ("Dialogue", "First line\nsecond line"),
            ("Action", "Action one\naction two"),
        ]
    );
}

#[test]
fn a_carriage_return_never_reaches_the_text() {
    let p = body("She runs.\r\nShe stops.\r\n");
    assert_eq!(p.len(), 1);
    assert!(!p[0].text.contains('\r'));
}

// what never reaches the OSF

#[test]
fn notes_including_method_readers_own_fmt_notes_are_stripped() {
    let p = body(
        [
            "[[fmt: sans +1]]",
            "A shout across the room.",
            "",
            "MARGO",
            "[[fmt: mono]] Typed words [[an aside]] here.",
            "",
            "[[a note on its own]]",
            "",
        ]
        .join("\n")
        .as_str(),
    );
    assert_eq!(
        kinds_and_texts(&p),
        [
            ("Action", "A shout across the room."),
            ("Character", "MARGO"),
            ("Dialogue", "Typed words here."),
        ]
    );
    let osf = fountain_to_osf("Words [[note]] more.\n");
    assert!(!osf.contains("[["));
    assert!(!osf.contains("note="), "no note attribute either");
}

#[test]
fn a_picture_note_never_reaches_it() {
    let fountain = [
        "[[img: p001-01.png w=100 full]]",
        "",
        "INT. ARCHIVE - DAY",
        "",
        "She holds up a panel.",
        "",
        "[[img: p002-01.png w=70]]",
        "",
        "MARGO",
        "Look at this. [[img: p002-02.png w=40]]",
        "",
    ]
    .join("\n");
    let osf = fountain_to_osf(&fountain);
    assert!(!osf.contains("img:"));
    assert!(!osf.contains(".png"));
    assert_eq!(
        kinds_and_texts(&body(&fountain)),
        [
            ("Scene Heading", "INT. ARCHIVE - DAY"),
            ("Action", "She holds up a panel."),
            ("Character", "MARGO"),
            ("Dialogue", "Look at this."),
        ]
    );
}

#[test]
fn boneyard_sections_synopses_and_page_markers_are_dropped() {
    let p = body(
        [
            "# Act One",
            "",
            "= The setup",
            "",
            "INT. ROOM - DAY",
            "",
            "/* a cut scene",
            "that is gone */",
            "",
            "= pg 2",
            "",
            "She sits.",
            "",
        ]
        .join("\n")
        .as_str(),
    );
    assert_eq!(
        kinds_and_texts(&p),
        [
            ("Scene Heading", "INT. ROOM - DAY"),
            ("Action", "She sits.")
        ]
    );
}

// page breaks

#[test]
fn a_page_break_sets_pagebreakbefore_on_the_next_paragraphs_style_only() {
    let p = body("One.\n\n===\n\nTwo.\n\nThree.\n");
    let got: Vec<(&str, Option<&str>)> = p
        .iter()
        .map(|x| (x.text.as_str(), x.style.attr("pagebreakbefore")))
        .collect();
    assert_eq!(got, [("One.", None), ("Two.", Some("1")), ("Three.", None)]);
}

#[test]
fn a_trailing_page_break_with_nothing_after_it_leaves_no_trace() {
    let p = body("One.\n\n===\n");
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].style.attr("pagebreakbefore"), None);
}

// dual dialogue

#[test]
fn dual_dialogue_marks_the_first_cue_only_and_keeps_both_speeches_in_order() {
    let p = body("INT. ROOM - DAY\n\nBOB\n(fast)\nLeft.\n\nALICE ^\nRight.\n\nThey stop.\n");
    assert_eq!(
        kinds_and_texts(&p),
        [
            ("Scene Heading", "INT. ROOM - DAY"),
            ("Character", "BOB"),
            ("Parenthetical", "(fast)"),
            ("Dialogue", "Left."),
            ("Character", "ALICE"),
            ("Dialogue", "Right."),
            ("Action", "They stop."),
        ]
    );
    let marks: Vec<Option<&str>> = p.iter().map(|x| x.style.attr("dualdialogue")).collect();
    assert_eq!(
        marks,
        [None, Some("1"), None, None, None, None, None],
        "only BOB, the first cue of the pair"
    );
}

#[test]
fn an_ordinary_exchange_after_a_dual_one_carries_no_mark() {
    let p = body("BOB\nLeft.\n\nALICE ^\nRight.\n\nBOB\nAgain.\n");
    let marked: Vec<&str> = p
        .iter()
        .filter(|x| x.style.attr("dualdialogue").is_some())
        .map(|x| x.text.as_str())
        .collect();
    assert_eq!(marked, ["BOB"]);
    assert_eq!(p.len(), 6);
}

#[test]
fn a_page_break_before_dual_dialogue_lands_on_the_first_cue() {
    let p = body("===\n\nBOB\nLeft.\n\nALICE ^\nRight.\n");
    assert_eq!(p[0].text, "BOB");
    assert_eq!(p[0].style.attr("pagebreakbefore"), Some("1"));
    assert_eq!(p[0].style.attr("dualdialogue"), Some("1"));
    assert!(
        p[1..]
            .iter()
            .all(|x| x.style.attr("pagebreakbefore").is_none())
    );
}

// emphasis

#[test]
fn emphasis_lands_as_bold_italic_and_underline_attributes_on_text_runs() {
    let p = body("plain *it* **bo** _un_ ***both*** end\n");
    let runs: Vec<(bool, bool, bool, &str)> = p[0]
        .runs
        .iter()
        .map(|(b, i, u, t)| (*b, *i, *u, t.as_str()))
        .collect();
    assert_eq!(
        runs,
        [
            (false, false, false, "plain "),
            (false, true, false, "it"),
            (false, false, false, " "),
            (true, false, false, "bo"),
            (false, false, false, " "),
            (false, false, true, "un"),
            (false, false, false, " "),
            (true, true, false, "both"),
            (false, false, false, " end"),
        ]
    );
}

#[test]
fn a_plain_run_carries_no_attribute_at_all() {
    let root = doc("Plain words.\n");
    let para = root.child("paragraphs").unwrap().child("para").unwrap();
    let text = para.child("text").unwrap();
    assert!(text.attrs.is_empty());
}

#[test]
fn escaped_markers_are_literal() {
    let p = body("Two \\*stars\\* and \\_under\\_.\n");
    assert_eq!(p[0].text, "Two *stars* and _under_.");
    assert_eq!(p[0].runs.len(), 1);
}

#[test]
fn ampersand_angle_brackets_and_quotes_survive_a_round_trip_through_the_xml() {
    let p = body("Tom & \"Jerry\" <3 it.\n");
    assert_eq!(p[0].text, "Tom & \"Jerry\" <3 it.");
}

#[test]
fn characters_xml_1_0_forbids_are_dropped_not_written() {
    let osf = fountain_to_osf("Bell\u{7} rings\u{1}.\n");
    let root = try_parse_xml(&osf).expect("well-formed");
    let p = paras_of(&root, "paragraphs").unwrap();
    assert_eq!(p[0].text, "Bell rings.");
}

#[test]
fn a_scene_number_with_markup_characters_is_escaped_in_the_attribute() {
    // fountain-js reads only a plain #n#, so the token is made by hand: the
    // writer must not trust what the tokenizer happens to allow.
    let heading: sp_model::Token = serde_json::from_value(serde_json::json!({
        "type": "scene_heading",
        "text": "INT. HOUSE - DAY",
        "scene_number": "1&\"2<\t3",
    }))
    .unwrap();
    let root = parse_xml(&sp_osf::tokens_to_osf(&[heading]));
    let p = paras_of(&root, "paragraphs").unwrap();
    assert_eq!(p[0].el.attr("scene_number"), Some("1&\"2< 3"));
    assert!(try_parse_xml(&fountain_to_osf("INT. HOUSE - DAY #1-A.#\n")).is_ok());
}

// the title page

const TITLE_PAGE: &str = "Title: **THE FIELD**\nCredit: Written by\nAuthor: A. Writer\nSource: An idea\nDraft date: 1 May\nContact: Box 12\n    Somewhere\n\nINT. ROOM - DAY\n";

#[test]
fn title_credit_author_and_source_are_centered_in_that_order_near_the_top() {
    let tp = title_page(TITLE_PAGE).expect("a title page");
    let texts: Vec<&str> = tp
        .iter()
        .filter(|p| !p.text.is_empty())
        .map(|p| p.text.as_str())
        .collect();
    assert_eq!(
        texts,
        [
            "THE FIELD",
            "Written by",
            "A. Writer",
            "An idea",
            "Box 12",
            "Somewhere",
            "1 May"
        ]
    );
    let first = tp.iter().position(|p| !p.text.is_empty()).unwrap();
    assert_eq!(first, 18, "a third of the way down");
    let title = &tp[first];
    assert_eq!(title.style.attr("align"), Some("center"));
    assert!(title.runs.iter().all(|r| r.0), "the title stays bold");
    for p in &tp {
        assert_eq!(p.kind(), "Normal Text");
    }
}

#[test]
fn contact_and_draft_date_sit_at_the_bottom_left_aligned() {
    let tp = title_page(TITLE_PAGE).unwrap();
    let contact = tp.iter().position(|p| p.text == "Box 12").unwrap();
    let source = tp.iter().position(|p| p.text == "An idea").unwrap();
    assert!(contact > source + 2);
    assert!(
        tp.len() >= 50 && tp.len() <= 56,
        "about a page: {}",
        tp.len()
    );
    for p in &tp[contact..] {
        assert_ne!(p.style.attr("align"), Some("center"), "{}", p.text);
    }
}

#[test]
fn blank_title_page_lines_are_empty_text_runs() {
    let root = doc(TITLE_PAGE);
    let first = root.child("titlepage").unwrap().child("para").unwrap();
    let texts = first.elements(Some("text"));
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0].text(), "");
}

#[test]
fn title_page_tokens_never_leak_into_the_script_body() {
    let p = body(TITLE_PAGE);
    assert_eq!(kinds_and_texts(&p), [("Scene Heading", "INT. ROOM - DAY")]);
}

#[test]
fn a_note_on_the_title_page_is_stripped_there_too() {
    let tp = title_page("Title: THE FIELD [[working title]]\n\nINT. ROOM - DAY\n").unwrap();
    let texts: Vec<&str> = tp
        .iter()
        .filter(|p| !p.text.is_empty())
        .map(|p| p.text.as_str())
        .collect();
    assert_eq!(texts, ["THE FIELD"]);
}

#[test]
fn the_output_is_the_same_on_every_call() {
    let a = fountain_to_osf(TITLE_PAGE);
    let b = fountain_to_osf(TITLE_PAGE);
    assert_eq!(a, b);
}

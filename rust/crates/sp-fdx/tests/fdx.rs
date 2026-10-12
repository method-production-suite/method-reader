//! The Final Draft (.fdx) renderer: Fountain in, an editable screenplay out.
//! Ported from tests/fdx.test.ts. Every case is a small inline Fountain
//! string, read back through a strict XML reader so a malformed document
//! fails here rather than in someone's copy of Final Draft.

mod common;

use common::{XmlElement, fountain_to_fdx, parse_xml, try_parse_xml};
use sp_fdx::{FDX_VERSION, TextRun, text_runs};

#[derive(Debug)]
struct Para {
    kind: Option<String>,
    el: XmlElement,
    runs: Vec<(Option<String>, String)>,
    text: String,
}

impl Para {
    fn attr(&self, name: &str) -> Option<&str> {
        self.el.attr(name)
    }
}

fn to_para(el: &XmlElement) -> Para {
    let runs: Vec<(Option<String>, String)> = el
        .elements(Some("Text"))
        .into_iter()
        .map(|t| (t.attr("Style").map(str::to_owned), t.text()))
        .collect();
    Para {
        kind: el.attr("Type").map(str::to_owned),
        text: runs.iter().map(|(_, t)| t.as_str()).collect(),
        runs,
        el: el.clone(),
    }
}

fn doc(fountain: &str) -> XmlElement {
    parse_xml(&fountain_to_fdx(fountain))
}

fn body(fountain: &str) -> Vec<Para> {
    let root = doc(fountain);
    let content = root.child("Content").expect("a Content element");
    content
        .elements(Some("Paragraph"))
        .into_iter()
        .map(to_para)
        .collect()
}

fn title_page(fountain: &str) -> Option<Vec<Para>> {
    let root = doc(fountain);
    let tp = root.child("TitlePage")?;
    Some(
        tp.child("Content")
            .expect("TitlePage > Content")
            .elements(Some("Paragraph"))
            .into_iter()
            .map(to_para)
            .collect(),
    )
}

fn types_and_texts(p: &[Para]) -> Vec<(&str, &str)> {
    p.iter()
        .map(|x| (x.kind.as_deref().unwrap_or(""), x.text.as_str()))
        .collect()
}

fn run(text: &str, style: Option<&str>) -> TextRun {
    TextRun {
        text: text.to_owned(),
        style: style.map(str::to_owned),
    }
}

fn styled(style: Option<&str>, text: &str) -> (Option<String>, String) {
    (style.map(str::to_owned), text.to_owned())
}

// the document shell

#[test]
fn an_xml_declaration_then_a_finaldraft_root_at_the_one_version_constant() {
    let fdx = fountain_to_fdx("INT. ROOM - DAY\n");
    assert!(fdx.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n"));
    let root = parse_xml(&fdx);
    assert_eq!(root.name, "FinalDraft");
    let attrs: Vec<(&str, &str)> = root
        .attrs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let version = FDX_VERSION.to_string();
    assert_eq!(
        attrs,
        [
            ("DocumentType", "Script"),
            ("Template", "No"),
            ("Version", version.as_str())
        ]
    );
    assert_eq!(FDX_VERSION, 2);
}

#[test]
fn minimal_content_and_title_page_only_no_settings_or_layout_blocks() {
    let root = doc("Title: A Thing\n\nINT. ROOM - DAY\n");
    let names: Vec<&str> = root
        .elements(None)
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["Content", "TitlePage"]);
}

#[test]
fn no_title_page_tokens_no_title_page_at_all() {
    assert!(title_page("INT. ROOM - DAY\n\nSome action.\n").is_none());
}

#[test]
fn an_empty_script_is_still_a_well_formed_document() {
    assert!(body("").is_empty());
}

// paragraph types

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
        types_and_texts(&p),
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
fn a_scene_number_rides_as_number_and_leaves_the_heading_text() {
    let p = body("INT. HOUSE - DAY #12A#\n");
    assert_eq!(p[0].kind.as_deref(), Some("Scene Heading"));
    assert_eq!(p[0].attr("Number"), Some("12A"));
    assert_eq!(p[0].text, "INT. HOUSE - DAY");
}

#[test]
fn an_unnumbered_heading_carries_no_number_attribute() {
    let p = body("EXT. FIELD - DAY\n");
    assert_eq!(p[0].attr("Number"), None);
}

#[test]
fn a_mini_slug_is_a_shot_without_the_dot() {
    let p = body("INT. HOUSE - DAY\n\n.LATER\n\nShe waits.\n");
    assert_eq!(
        types_and_texts(&p),
        [
            ("Scene Heading", "INT. HOUSE - DAY"),
            ("Shot", "LATER"),
            ("Action", "She waits."),
        ]
    );
}

#[test]
fn a_mini_slug_keeps_its_shooting_script_number_too() {
    let p = body("INT. HOUSE - DAY\n\n.THE HALL #7#\n");
    assert_eq!(p[1].kind.as_deref(), Some("Shot"));
    assert_eq!(p[1].attr("Number"), Some("7"));
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
        types_and_texts(&p),
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
    assert_eq!(p[0].kind.as_deref(), Some("Character"));
    assert_eq!(p[0].text, "MARGO (V.O.) (CONT'D)");
}

#[test]
fn centered_text_is_action_aligned_center_without_the_brackets() {
    let p = body("> THE END <\n");
    assert_eq!(p[0].kind.as_deref(), Some("Action"));
    assert_eq!(p[0].attr("Alignment"), Some("Center"));
    assert_eq!(p[0].text, "THE END");
}

#[test]
fn ordinary_action_carries_no_alignment() {
    let p = body("She runs.\n");
    assert_eq!(p[0].attr("Alignment"), None);
}

#[test]
fn lyrics_are_lyrics_one_paragraph_per_line_without_the_tildes() {
    let p = body("~Row, row, row\n~your boat\n");
    assert_eq!(
        types_and_texts(&p),
        [("Lyrics", "Row, row, row"), ("Lyrics", "your boat")]
    );
}

#[test]
fn a_lyric_inside_a_speech_stays_inside_it() {
    let p = body("MARGO\nI sing:\n~la la la\n");
    let kinds: Vec<&str> = p.iter().map(|x| x.kind.as_deref().unwrap()).collect();
    assert_eq!(kinds, ["Character", "Dialogue", "Lyrics"]);
}

#[test]
fn a_multi_line_token_becomes_one_paragraph_per_line_blank_lines_dropped() {
    let p = body("MARGO\nFirst line\nsecond line\n\nAction one\naction two\n");
    assert_eq!(
        types_and_texts(&p),
        [
            ("Character", "MARGO"),
            ("Dialogue", "First line"),
            ("Dialogue", "second line"),
            ("Action", "Action one"),
            ("Action", "action two"),
        ]
    );
}

// what never reaches the FDX

#[test]
fn notes_including_screepubs_own_fmt_notes_are_stripped() {
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
        types_and_texts(&p),
        [
            ("Action", "A shout across the room."),
            ("Character", "MARGO"),
            ("Dialogue", "Typed words here."),
        ]
    );
    assert!(!fountain_to_fdx("Words [[note]] more.\n").contains("[["));
}

#[test]
fn a_picture_note_never_reaches_it_standalone_full_page_or_glued_to_a_line() {
    // Pictures carried out of a PDF are recorded as [[img: ...]] notes
    // (registry #19). An FDX is a script to edit, and a picture has no
    // paragraph type there, so the note goes the way every note goes: the
    // FDX shows neither the note nor the file it names.
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
    let fdx = fountain_to_fdx(&fountain);
    assert!(!fdx.contains("img:"));
    assert!(!fdx.contains(".png"));
    assert_eq!(
        types_and_texts(&body(&fountain)),
        [
            ("Scene Heading", "INT. ARCHIVE - DAY"),
            ("Action", "She holds up a panel."),
            ("Character", "MARGO"),
            ("Dialogue", "Look at this."),
        ]
    );
}

#[test]
fn boneyard_sections_and_synopses_and_page_markers_are_dropped() {
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
        types_and_texts(&p),
        [
            ("Scene Heading", "INT. ROOM - DAY"),
            ("Action", "She sits.")
        ]
    );
}

// page breaks

#[test]
fn a_page_break_sets_starts_new_page_on_the_next_paragraph_only() {
    let p = body("One.\n\n===\n\nTwo.\n\nThree.\n");
    let got: Vec<(&str, Option<&str>)> = p
        .iter()
        .map(|x| (x.text.as_str(), x.attr("StartsNewPage")))
        .collect();
    assert_eq!(
        got,
        [("One.", None), ("Two.", Some("Yes")), ("Three.", None)]
    );
}

#[test]
fn a_trailing_page_break_with_nothing_after_it_leaves_no_trace() {
    let p = body("One.\n\n===\n");
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].attr("StartsNewPage"), None);
}

#[test]
fn a_page_break_before_dual_dialogue_lands_on_the_wrapper() {
    let p = body("===\n\nBOB\nLeft.\n\nALICE ^\nRight.\n");
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].attr("StartsNewPage"), Some("Yes"));
}

// dual dialogue

#[test]
fn both_speeches_nest_inside_one_paragraph_dual_dialogue() {
    let p = body("INT. ROOM - DAY\n\nBOB\n(fast)\nLeft.\n\nALICE ^\nRight.\n\nThey stop.\n");
    let kinds: Vec<&str> = p.iter().map(|x| x.kind.as_deref().unwrap()).collect();
    assert_eq!(kinds, ["Scene Heading", "General", "Action"]);
    let wrapper = &p[1].el;
    // Nothing but the DualDialogue element inside the wrapper: no stray text.
    assert_eq!(wrapper.children.len(), 1);
    let dual = wrapper.child("DualDialogue").expect("a DualDialogue");
    assert!(!dual.has_text_children());
    let inner: Vec<Para> = dual
        .elements(Some("Paragraph"))
        .into_iter()
        .map(to_para)
        .collect();
    assert_eq!(
        types_and_texts(&inner),
        [
            ("Character", "BOB"),
            ("Parenthetical", "(fast)"),
            ("Dialogue", "Left."),
            ("Character", "ALICE"),
            ("Dialogue", "Right."),
        ]
    );
}

// emphasis

#[test]
fn text_runs_splits_fountain_emphasis_into_styled_runs() {
    assert_eq!(
        text_runs("plain *it* **bo** _un_ end"),
        [
            run("plain ", None),
            run("it", Some("Italic")),
            run(" ", None),
            run("bo", Some("Bold")),
            run(" ", None),
            run("un", Some("Underline")),
            run(" end", None),
        ]
    );
}

#[test]
fn styles_combine_as_bold_italic_underline_in_that_order() {
    assert_eq!(text_runs("***both***"), [run("both", Some("Bold+Italic"))]);
    assert_eq!(
        text_runs("_***all three***_"),
        [run("all three", Some("Bold+Italic+Underline"))]
    );
    assert_eq!(
        text_runs("*_it under_*"),
        [run("it under", Some("Italic+Underline"))]
    );
    assert_eq!(
        text_runs("**bold *and it* back**"),
        [
            run("bold ", Some("Bold")),
            run("and it", Some("Bold+Italic")),
            run(" back", Some("Bold")),
        ]
    );
}

#[test]
fn unpaired_or_spaced_markers_stay_literal() {
    assert_eq!(text_runs("2 * 3 = 6"), [run("2 * 3 = 6", None)]);
    assert_eq!(text_runs("snake_case only"), [run("snake_case only", None)]);
}

#[test]
fn backslash_escaped_markers_are_literal_and_lose_the_backslash() {
    assert_eq!(
        text_runs("a \\*not italic\\* b"),
        [run("a *not italic* b", None)]
    );
}

#[test]
fn runs_land_as_text_elements_plain_runs_carry_no_style() {
    let p = body("He is *very* **sure**.\n");
    assert_eq!(
        p[0].runs,
        [
            styled(None, "He is "),
            styled(Some("Italic"), "very"),
            styled(None, " "),
            styled(Some("Bold"), "sure"),
            styled(None, "."),
        ]
    );
}

#[test]
fn no_whitespace_between_or_around_the_text_runs_of_a_paragraph() {
    let fdx = fountain_to_fdx("He is *very* sure.\n");
    assert!(fdx.contains(
        r#"<Paragraph Type="Action"><Text>He is </Text><Text Style="Italic">very</Text><Text> sure.</Text></Paragraph>"#
    ));
}

// escaping

#[test]
fn ampersand_angle_brackets_and_quotes_survive_a_round_trip_through_the_xml() {
    let p = body("Tom & Jerry say \"<hi>\" > loudly.\n");
    assert_eq!(p[0].text, "Tom & Jerry say \"<hi>\" > loudly.");
    let fdx = fountain_to_fdx("Tom & Jerry say \"<hi>\".\n");
    assert!(fdx.contains("Tom &amp; Jerry say &quot;&lt;hi&gt;&quot;."));
}

#[test]
fn characters_xml_1_0_forbids_are_dropped_not_written() {
    let source = "Bell\u{7} and form\u{C} feed and \u{FFFE} end.\n";
    let fdx = fountain_to_fdx(source);
    assert!(try_parse_xml(&fdx).is_ok());
    let p = body(source);
    assert_eq!(p[0].text, "Bell and form feed and  end.");
}

#[test]
fn a_lone_surrogate_cannot_reach_the_writer() {
    // The TypeScript drops a lone UTF-16 surrogate from the text. A Rust
    // string cannot hold one at all, and the tokens' JSON refuses one, so
    // the check here is that the door is shut rather than that the writer
    // sweeps up behind it.
    let wire = r#"{"type":"action","text":"Broken \ud800 pair."}"#;
    assert!(serde_json::from_str::<sp_model::Token>(wire).is_err());
}

#[test]
fn a_scene_number_with_markup_characters_is_escaped_in_the_attribute() {
    let fdx = fountain_to_fdx("INT. HOUSE - DAY #1-A.#\n");
    assert!(try_parse_xml(&fdx).is_ok());
}

// the title page

const SOURCE: &str = "Title: **THE BIG** _Thing_\n    Part Two\nCredit: Written by\nAuthor: A. Writer\nSource: Based on a story\nDraft date: 1 January 2020\nContact:\n    A. Agent\n    555-0100\n\nINT. ROOM - DAY\n";

#[test]
fn title_credit_author_and_source_are_centered_in_that_order_near_the_top() {
    let p = title_page(SOURCE).unwrap();
    let centered: Vec<&Para> = p
        .iter()
        .filter(|x| x.attr("Alignment") == Some("Center") && !x.text.is_empty())
        .collect();
    let texts: Vec<&str> = centered.iter().map(|x| x.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "THE BIG Thing",
            "Part Two",
            "Written by",
            "A. Writer",
            "Based on a story"
        ]
    );
    // The title keeps its own emphasis.
    assert_eq!(
        centered[0].runs,
        [
            styled(Some("Bold"), "THE BIG"),
            styled(None, " "),
            styled(Some("Underline"), "Thing"),
        ]
    );
    // Blank lines push the block down the page.
    let first_text = p.iter().position(|x| !x.text.is_empty()).unwrap();
    assert!(first_text >= 10);
}

#[test]
fn contact_and_draft_date_sit_at_the_bottom_left_aligned_after_the_centered_block() {
    let p = title_page(SOURCE).unwrap();
    let last_centered = p
        .iter()
        .rposition(|x| x.attr("Alignment") == Some("Center") && !x.text.is_empty())
        .unwrap();
    let bottom: Vec<&Para> = p[last_centered + 1..]
        .iter()
        .filter(|x| !x.text.is_empty())
        .collect();
    let texts: Vec<&str> = bottom.iter().map(|x| x.text.as_str()).collect();
    assert_eq!(texts, ["A. Agent", "555-0100", "1 January 2020"]);
    assert!(bottom.iter().all(|x| x.attr("Alignment") == Some("Left")));
    // Below the middle of a ~55-line page.
    let first_bottom = p.iter().position(|x| x.text == "A. Agent").unwrap();
    assert!(first_bottom > 35);
}

#[test]
fn title_page_paragraphs_carry_no_element_type_like_final_drafts_own() {
    for x in title_page(SOURCE).unwrap() {
        assert_eq!(x.kind, None);
    }
}

#[test]
fn title_page_tokens_never_leak_into_the_script_body() {
    let kinds: Vec<Option<String>> = body(SOURCE).into_iter().map(|x| x.kind).collect();
    assert_eq!(kinds, [Some("Scene Heading".to_owned())]);
}

#[test]
fn a_note_on_the_title_page_is_stripped_there_too() {
    let p = title_page("Title: Draft [[check this]] One\n\nINT. ROOM - DAY\n").unwrap();
    let texts: Vec<&str> = p
        .iter()
        .filter(|x| !x.text.is_empty())
        .map(|x| x.text.as_str())
        .collect();
    assert_eq!(texts, ["Draft One"]);
}

// The renderer half of tests/cli-export-fdx.test.ts: the script its export
// writes. Whether `export --for fdx` writes exactly this, where, and how it
// refuses, is the export command's to test (sp-engine, sp-cli).

const EXPORT_FOUNTAIN: &str = "Title: Test Piece\n\nINT. ROOM - DAY\n\nMARGO\nHello *there*.\n";

#[test]
fn the_export_tests_script_is_the_typescripts_fdx_and_a_finaldraft_document() {
    let fdx = fountain_to_fdx(EXPORT_FOUNTAIN);
    assert_eq!(Some(fdx.as_str()), common::typescript_fdx(EXPORT_FOUNTAIN));
    assert_eq!(parse_xml(&fdx).name, "FinalDraft");
}

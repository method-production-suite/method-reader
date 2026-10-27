//! OSF in: the reader, `sp_osf::osf_to_fountain`, the inverse of the
//! writer's mapping (docs/osf-input.md). The round trip runs each committed
//! invented script's .fountain (the oracle's script.fountain) through the
//! writer and back, and holds the tokens to the original's, less only what
//! the writer strips. The rest are hand-written documents in the shapes the
//! format's published examples show, with invented text.

mod common;

use std::path::PathBuf;

use sp_fountain::{strip_notes, tokenize};
use sp_model::{Dual, Token, TokenType};
use sp_osf::{ReadError, osf_to_fountain, tokens_to_osf};

fn oracle_fountain(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle")
        .join(name)
        .join("script.fountain");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// What one token says that the writer carries: its kind, its text with
/// the notes stripped and blank lines dropped, its scene number and its
/// dual column.
#[derive(Debug, PartialEq, Eq)]
struct Kept {
    kind: TokenType,
    text: Option<String>,
    scene_number: Option<String>,
    dual: Option<Dual>,
}

fn kept_text(t: &Token) -> Option<String> {
    let text = t.text.as_deref()?;
    let stripped = strip_notes(text);
    let lines: Vec<&str> = stripped
        .split('\n')
        .filter(|l| !l.trim().is_empty())
        .collect();
    Some(lines.join("\n"))
}

/// The body tokens the writer turns into paragraphs, and the structure
/// around them, with what the writer strips gone: notes, sections,
/// synopses (page markers among them), boneyard, and any content token
/// whose text is only notes. A lyric becomes dialogue inside a speech and
/// action outside one, as the writer maps it.
fn kept_body(tokens: &[Token]) -> Vec<Kept> {
    let mut out = Vec::new();
    let mut in_speech = false;
    for t in tokens.iter().filter(|t| !t.is_title()) {
        let kind = match t.kind {
            TokenType::DialogueBegin => {
                in_speech = true;
                t.kind.clone()
            }
            TokenType::DialogueEnd => {
                in_speech = false;
                t.kind.clone()
            }
            TokenType::Lyrics if in_speech => TokenType::Dialogue,
            TokenType::Lyrics => TokenType::Action,
            TokenType::SceneHeading
            | TokenType::Action
            | TokenType::Centered
            | TokenType::Character
            | TokenType::Parenthetical
            | TokenType::Dialogue
            | TokenType::Transition
            | TokenType::PageBreak
            | TokenType::DualDialogueBegin
            | TokenType::DualDialogueEnd => t.kind.clone(),
            _ => continue,
        };
        let text = kept_text(t);
        if text.as_deref().is_some_and(str::is_empty) {
            continue;
        }
        out.push(Kept {
            kind,
            text,
            scene_number: t.scene_number.clone(),
            dual: t.dual.clone(),
        });
    }
    out
}

fn title_page(tokens: &[Token]) -> Vec<(String, String)> {
    tokens
        .iter()
        .filter(|t| t.is_title())
        .map(|t| (t.kind.as_str().to_owned(), kept_text(t).unwrap_or_default()))
        .collect()
}

fn read(xml: &str) -> String {
    osf_to_fountain(xml).unwrap_or_else(|e| panic!("{e}\n{xml}"))
}

fn tokens_of(fountain: &str) -> Vec<Token> {
    tokenize(fountain).unwrap_or_else(|e| panic!("{e}\n{fountain}"))
}

fn round_trip(name: &str) {
    let original = tokens_of(&oracle_fountain(name));
    let osf = tokens_to_osf(&original);
    let back = read(&osf);
    let again = tokens_of(&back);
    let (want, got) = (kept_body(&original), kept_body(&again));
    for (i, (w, g)) in want.iter().zip(&got).enumerate() {
        assert_eq!(w, g, "{name}: body token {i} differs\n{back}");
    }
    assert_eq!(want.len(), got.len(), "{name}: body token count\n{back}");
    assert_eq!(
        title_page(&original),
        title_page(&again),
        "{name}: title page\n{back}"
    );
    // And the reader's .fountain, written out again, is the same OSF.
    assert_eq!(tokens_to_osf(&again), osf, "{name}: OSF of the read-back");
}

#[test]
fn field_station_round_trips_through_the_writer_and_back() {
    round_trip("field-station");
}

#[test]
fn the_torture_script_round_trips() {
    round_trip("torture");
}

#[test]
fn the_happy_path_script_round_trips() {
    round_trip("screenplay");
}

fn doc(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<document type=\"Open Screenplay Format document\" version=\"30\">\n<paragraphs>\n{body}\n</paragraphs>\n</document>\n"
    )
}

fn para(style: &str, text: &str) -> String {
    format!("<para><style basestylename=\"{style}\"/><text>{text}</text></para>")
}

fn kinds(fountain: &str) -> Vec<(TokenType, String)> {
    tokens_of(fountain)
        .into_iter()
        .filter(|t| t.text.is_some())
        .map(|t| (t.kind.clone(), t.text.clone().unwrap_or_default()))
        .collect()
}

#[test]
fn not_xml_is_refused_plainly() {
    for bad in [
        "",
        "INT. HUT - NIGHT",
        "<document><paragraphs>",
        "<?xml version=\"1.0\"?><a></b>",
    ] {
        let err = osf_to_fountain(bad).expect_err(bad);
        assert!(matches!(err, ReadError::NotXml(_)), "{bad:?}: {err:?}");
        assert!(
            err.to_string().starts_with("this script is not a readable OSF file"),
            "{err}"
        );
    }
}

#[test]
fn xml_without_paragraphs_is_refused_plainly() {
    for xml in [
        "<document type=\"Open Screenplay Format document\" version=\"30\"/>",
        "<document><titlepage><para><style basestylename=\"Normal Text\"/><text>A</text></para></titlepage></document>",
        "<html><body><p>hello</p></body></html>",
    ] {
        let err = osf_to_fountain(xml).expect_err(xml);
        assert_eq!(err, ReadError::NoParagraphs, "{xml}");
        assert_eq!(
            err.to_string(),
            "this file is XML but not an OSF script: it has no <paragraphs>"
        );
    }
}

#[test]
fn an_empty_paragraphs_block_reads_as_an_empty_script() {
    // The engine's own guard then refuses it as empty.
    assert_eq!(read(&doc("")).trim(), "");
}

#[test]
fn each_built_in_style_becomes_its_fountain_element() {
    let xml = doc(&[
        para("Scene Heading", "INT. HUT - NIGHT"),
        para("Action", "The radio hisses."),
        para("Character", "MARA"),
        para("Parenthetical", "(quietly)"),
        para("Dialogue", "Anyone there?"),
        para("Transition", "CUT TO:"),
        para("Shot", "LATER"),
        para("Normal Text", "Rain on the roof."),
    ]
    .join("\n"));
    let fountain = read(&xml);
    assert_eq!(
        kinds(&fountain),
        vec![
            (TokenType::SceneHeading, "INT. HUT - NIGHT".into()),
            (TokenType::Action, "The radio hisses.".into()),
            (TokenType::Character, "MARA".into()),
            (TokenType::Parenthetical, "(quietly)".into()),
            (TokenType::Dialogue, "Anyone there?".into()),
            // Forced as Method Reader's own .fountain forces every
            // transition; fountain-js keeps the `>` in this one's text, and
            // every renderer drops it.
            (TokenType::Transition, "> CUT TO:".into()),
            (TokenType::SceneHeading, "LATER".into()),
            (TokenType::Action, "Rain on the roof.".into()),
        ],
        "{fountain}"
    );
}

#[test]
fn action_that_would_read_as_something_else_is_forced() {
    let xml = doc(&[
        para("Action", "EXT. NOT A HEADING"),
        para("Action", "LOUD NOISE"),
        para("Action", "FADE OUT."),
        para("Action", "Title: not a title page"),
        para("Action", "= not a synopsis"),
    ]
    .join("\n"));
    let fountain = read(&xml);
    let got = kinds(&fountain);
    assert!(
        got.iter().all(|(k, _)| *k == TokenType::Action),
        "{got:?}\n{fountain}"
    );
    assert_eq!(got.len(), 5, "{got:?}\n{fountain}");
}

#[test]
fn a_cue_in_lower_case_and_a_heading_that_does_not_open_like_one_keep_their_kind() {
    let xml = doc(&[
        para("Scene Heading", "The Bridge"),
        para("Character", "Mara"),
        para("Dialogue", "Hello."),
        para("Transition", "Slow fade"),
    ]
    .join("\n"));
    let got = kinds(&read(&xml));
    assert_eq!(
        got,
        vec![
            (TokenType::SceneHeading, "The Bridge".into()),
            (TokenType::Character, "Mara".into()),
            (TokenType::Dialogue, "Hello.".into()),
            (TokenType::Transition, "Slow fade".into()),
        ]
    );
}

#[test]
fn a_parenthetical_without_its_brackets_gets_them() {
    let xml = doc(&[
        para("Character", "TEO"),
        para("Parenthetical", "beat"),
        para("Dialogue", "Fine."),
    ]
    .join("\n"));
    let got = kinds(&read(&xml));
    assert_eq!(got[1], (TokenType::Parenthetical, "(beat)".into()));
}

#[test]
fn speech_with_no_cue_reads_as_action() {
    let xml = doc(&[para("Scene Heading", "INT. HUT - DAY"), para("Dialogue", "Hello?")].join("\n"));
    let got = kinds(&read(&xml));
    assert_eq!(got[1], (TokenType::Action, "Hello?".into()));
}

#[test]
fn scene_numbers_page_breaks_centering_and_soft_returns_carry_over() {
    let xml = doc(
        "<para scene_number=\"12A\"><style basestylename=\"Scene Heading\"/><text>EXT. MAST - DAY</text></para>\n\
         <para><style basestylename=\"Action\" pagebreakbefore=\"1\"/><text>One line\nand a second.</text></para>\n\
         <para><style basestylename=\"Action\" align=\"center\"/><text>THE END</text></para>",
    );
    let fountain = read(&xml);
    let tokens = tokens_of(&fountain);
    let heading = &tokens[0];
    assert_eq!(heading.kind, TokenType::SceneHeading);
    assert_eq!(heading.scene_number.as_deref(), Some("12A"));
    assert_eq!(tokens[1].kind, TokenType::PageBreak, "{fountain}");
    assert_eq!(tokens[2].kind, TokenType::Action);
    assert_eq!(tokens[2].text.as_deref(), Some("One line\nand a second."));
    assert_eq!(tokens[3].kind, TokenType::Centered);
    assert_eq!(tokens[3].text.as_deref(), Some("THE END"));
}

#[test]
fn dual_dialogue_marked_on_the_first_cue_becomes_a_caret_on_the_second() {
    let xml = doc(
        "<para><style basestylename=\"Character\" dualdialogue=\"1\"/><text>MARA</text></para>\n\
         <para><style basestylename=\"Dialogue\"/><text>Left.</text></para>\n\
         <para><style basestylename=\"Character\"/><text>TEO</text></para>\n\
         <para><style basestylename=\"Dialogue\"/><text>Right.</text></para>\n\
         <para><style basestylename=\"Character\"/><text>MARA</text></para>\n\
         <para><style basestylename=\"Dialogue\"/><text>Not dual.</text></para>",
    );
    let fountain = read(&xml);
    // fountain-js marks the column on each speech's dialogue_begin.
    let tokens = tokens_of(&fountain);
    let cues: Vec<(String, Option<Dual>)> = tokens
        .windows(2)
        .filter(|w| w[0].kind == TokenType::DialogueBegin && w[1].kind == TokenType::Character)
        .map(|w| (w[1].text.clone().unwrap_or_default(), w[0].dual.clone()))
        .collect();
    assert_eq!(
        cues,
        vec![
            ("MARA".into(), Some(Dual::Left)),
            ("TEO".into(), Some(Dual::Right)),
            ("MARA".into(), None),
        ],
        "{fountain}"
    );
}

#[test]
fn runs_become_fountain_emphasis_and_literal_markers_are_escaped() {
    let xml = doc(
        "<para><style basestylename=\"Action\"/><text>She reads </text><text italic=\"1\">the log</text><text>, then </text><text bold=\"1\">stops</text><text>. </text><text underline=\"1\">Twice.</text><text> 2 * 3 = snake_case</text></para>",
    );
    let fountain = read(&xml);
    let action = tokens_of(&fountain)
        .into_iter()
        .find(|t| t.kind == TokenType::Action)
        .expect("an action");
    let runs = sp_fdx::text_runs(action.text.as_deref().unwrap_or(""));
    let got: Vec<(String, Option<String>)> =
        runs.into_iter().map(|r| (r.text, r.style)).collect();
    assert_eq!(
        got,
        vec![
            ("She reads ".into(), None),
            ("the log".into(), Some("Italic".into())),
            (", then ".into(), None),
            ("stops".into(), Some("Bold".into())),
            (". ".into(), None),
            ("Twice.".into(), Some("Underline".into())),
            (" 2 * 3 = snake_case".into(), None),
        ],
        "{fountain}"
    );
}

#[test]
fn the_newer_spelling_and_a_declared_style_both_resolve() {
    let xml = "<document version=\"40\">\
        <styles><style name=\"My Heading\" basestyle=\"Scene Heading\"/></styles>\
        <paragraphs>\
        <para><style basestyle=\"Scene Heading\"/><text>INT. HUT - DAY</text></para>\
        <para><style basestylename=\"My Heading\"/><text>EXT. RIDGE - DAY</text></para>\
        <para><style basestylename=\"something unknown\"/><text>Wind.</text></para>\
        </paragraphs></document>";
    let got = kinds(&read(xml));
    assert_eq!(
        got,
        vec![
            (TokenType::SceneHeading, "INT. HUT - DAY".into()),
            (TokenType::SceneHeading, "EXT. RIDGE - DAY".into()),
            (TokenType::Action, "Wind.".into()),
        ]
    );
}

#[test]
fn a_title_page_with_bookmarks_keeps_its_keys() {
    let xml = "<document><paragraphs>\
        <para><style basestylename=\"Scene Heading\"/><text>INT. HUT - DAY</text></para>\
        </paragraphs><titlepage>\
        <para bookmark=\"Title\"><style basestylename=\"Normal Text\" align=\"center\"/><text>THE WEATHER HUT</text></para>\
        <para><style basestylename=\"Normal Text\"/><text></text></para>\
        <para bookmark=\"Author\"><style basestylename=\"Normal Text\" align=\"center\"/><text>An Invented Writer</text></para>\
        <para bookmark=\"Draft\"><style basestylename=\"Normal Text\"/><text>First draft</text></para>\
        <para bookmark=\"Contact\"><style basestylename=\"Normal Text\"/><text>The hut</text></para>\
        <para><style basestylename=\"Normal Text\"/><text>On the ridge</text></para>\
        </titlepage></document>";
    let tokens = tokens_of(&read(xml));
    // Written in the writer's order: title, author, then contact and date.
    let keys: Vec<(String, String)> = title_page(&tokens);
    assert_eq!(keys.len(), 4, "{keys:?}");
    assert_eq!(keys[0], ("title".into(), "THE WEATHER HUT".into()));
    assert_eq!(keys[1], ("author".into(), "An Invented Writer".into()));
    assert_eq!(keys[2].0, "contact");
    assert!(keys[2].1.contains("The hut"), "{keys:?}");
    assert!(keys[2].1.contains("On the ridge"), "{keys:?}");
    assert_eq!(keys[3], ("draft_date".into(), "First draft".into()));
}

#[test]
fn a_utf8_byte_order_mark_and_entities_are_read() {
    let xml = format!(
        "\u{feff}{}",
        doc(&para("Action", "Salt &amp; pepper &lt;here&gt;"))
    );
    let got = kinds(&read(&xml));
    assert_eq!(got, vec![(TokenType::Action, "Salt & pepper <here>".into())]);
}

//! The wire shapes: each type reads and writes the JSON the TypeScript does.

use serde_json::{Value, json};
use sp_model::{
    AgeValue, Dual, ElementType, FamilyBucket, ParsedScreenplay, RawLine, ScreenplayElement,
    SizeStep, Token, TokenType,
};

#[test]
fn element_types_spell_as_the_typescript_union() {
    let all = [
        (ElementType::Scene, "scene"),
        (ElementType::Character, "character"),
        (ElementType::Dialogue, "dialogue"),
        (ElementType::Action, "action"),
        (ElementType::Parenthetical, "parenthetical"),
        (ElementType::Transition, "transition"),
        (ElementType::PageNumber, "page-number"),
        (ElementType::MiniSlug, "mini-slug"),
        (ElementType::Image, "image"),
    ];
    for (t, s) in all {
        assert_eq!(serde_json::to_value(t).unwrap(), json!(s));
    }
    assert_eq!(
        serde_json::to_value(SizeStep::Smaller).unwrap(),
        json!("-1")
    );
    assert_eq!(
        serde_json::to_value(SizeStep::MuchLarger).unwrap(),
        json!("+2")
    );
    assert_eq!(
        serde_json::to_value(FamilyBucket::Cursive).unwrap(),
        json!("cursive")
    );
}

#[test]
fn an_element_round_trips_and_leaves_out_what_it_lacks() {
    let wire = json!({
        "type": "scene", "text": "INT. LAB - DAY", "id": "e3", "pageNum": 2,
        "isTitlePage": false, "isReadable": true, "sceneNumber": "4A", "y": 700,
        "fmt": { "family": "sans", "size": "+1" }
    });
    let el: ScreenplayElement = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(el.kind, ElementType::Scene);
    assert_eq!(el.y, Some(700.0));
    assert_eq!(serde_json::to_value(&el).unwrap(), wire);
}

#[test]
fn age_value_is_a_string_or_a_number() {
    let n: AgeValue = serde_json::from_value(json!(30)).unwrap();
    assert_eq!(n, AgeValue::Number(30.0));
    assert_eq!(serde_json::to_value(&n).unwrap(), json!(30));
    let s: AgeValue = serde_json::from_value(json!("30s")).unwrap();
    assert_eq!(s, AgeValue::Text("30s".into()));
}

#[test]
fn a_raw_line_writes_whole_numbers_as_javascript_does() {
    let line = RawLine {
        text: "JACK".into(),
        indent: 37.0,
        y: 612.5,
        page_num: 1,
        styled: None,
        dual_right: None,
        fonts: None,
        fmt: None,
    };
    assert_eq!(
        serde_json::to_string(&line).unwrap(),
        r#"{"text":"JACK","indent":37,"y":612.5,"pageNum":1}"#
    );
}

#[test]
fn a_parsed_screenplay_round_trips() {
    let wire = json!({
        "elements": [],
        "characters": [{ "name": "JACK", "baseName": "JACK", "dialogueCount": 3, "firstAppearance": 1 }],
        "scenes": [{ "heading": "INT. LAB - DAY", "sceneNumber": 1, "elementIndex": 0, "pageNum": 1 }],
        "pageCount": 5
    });
    let sp: ParsedScreenplay = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&sp).unwrap(), wire);
}

#[test]
fn tokens_read_every_shape_fountain_js_writes() {
    let wire: Value = json!([
        { "is_title": true, "type": "title", "text": "A Title" },
        { "is_title": true, "type": "action", "text": "a title key that collides" },
        { "type": "scene_heading", "text": "INT. KITCHEN - DAY", "scene_number": "7" },
        { "type": "scene_heading", "text": "EXT. YARD - NIGHT" },
        { "type": "action", "text": "Jack enters." },
        { "type": "dual_dialogue_begin" },
        { "type": "dialogue_begin", "dual": "left" },
        { "type": "character", "text": "JACK" },
        { "type": "parenthetical", "text": "(quietly)" },
        { "type": "dialogue", "text": "Hello." },
        { "type": "dialogue_end" },
        { "type": "dialogue_begin" },
        { "type": "section", "text": "Act One", "depth": 1 },
        { "type": "page_break" },
        { "type": "spaces" }
    ]);
    let tokens: Vec<Token> = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(tokens[0].kind, TokenType::Other("title".into()));
    assert!(tokens[0].is_title());
    assert_eq!(tokens[1].kind, TokenType::Action);
    assert!(tokens[1].is_title());
    assert_eq!(tokens[2].scene_number.as_deref(), Some("7"));
    assert_eq!(tokens[6].dual, Some(Dual::Left));
    assert_eq!(tokens[11].dual, None);
    assert_eq!(tokens[12].depth, Some(1));
    assert_eq!(serde_json::to_value(&tokens).unwrap(), wire);
}

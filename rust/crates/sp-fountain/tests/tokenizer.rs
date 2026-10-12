//! Hand-written tokenizer cases the fixtures miss, each answered by
//! fountain-js 1.2.4 itself.
//!
//! tests/data/tokenizer-cases.json holds the inputs and fountain-js's
//! tokens (or its error). It was written by running fountain-js under bun,
//! reset to a first parse before each case, and `cases_still_match_live_
//! fountain_js` re-asks it whenever bun is present, so the file cannot drift
//! from the library it records.

mod support;

use serde_json::{Value, json};
use sp_fountain::tokenizer::tokenize;

fn cases() -> Vec<Value> {
    let text = include_str!("data/tokenizer-cases.json");
    serde_json::from_str::<Value>(text)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

fn answer(input: &str) -> Value {
    match tokenize(input) {
        Ok(tokens) => json!({ "tokens": tokens }),
        Err(e) => json!({ "error": e.message }),
    }
}

fn recorded(case: &Value) -> Value {
    match case.get("error") {
        Some(e) => json!({ "error": e }),
        None => json!({ "tokens": case["tokens"] }),
    }
}

#[test]
fn hand_written_cases_match_fountain_js() {
    let cases = cases();
    assert!(cases.len() >= 40);
    for case in &cases {
        let input = case["input"].as_str().unwrap();
        assert_eq!(
            answer(input),
            recorded(case),
            "case: {}",
            case["name"].as_str().unwrap()
        );
    }
}

#[test]
#[ignore = "needs bun and node_modules"]
fn cases_still_match_live_fountain_js() {
    let cases = cases();
    let inputs: Vec<String> = cases
        .iter()
        .map(|c| c["input"].as_str().unwrap().to_owned())
        .collect();
    let live = support::fountain_js(&inputs);
    for (case, live) in cases.iter().zip(live) {
        assert_eq!(live, recorded(case), "case: {}", case["name"]);
    }
}

/// fountain-js's static `lastLineWasDualDialogue` survives a parse, so in
/// one process a script that OPENS on a right column makes the next parse
/// wrap its last speech in a dual exchange. The port starts every parse
/// fresh (rust/PARITY.md, Decisions): the same text, the same tokens.
#[test]
fn a_script_opening_on_a_right_column_tokenizes_the_same_twice() {
    let script = "@BOB ^\nYo.\n\nINT. A - DAY\n\n@ANN\nLast word.\n";
    let first = tokenize(script).unwrap();
    let second = tokenize(script).unwrap();
    assert_eq!(first, second);
    // And the last speech is an ordinary one, not a left column.
    let last_begin = second
        .iter()
        .rev()
        .find(|t| t.kind == sp_model::TokenType::DialogueBegin)
        .unwrap();
    assert_eq!(last_begin.dual, None);
    assert!(
        !second
            .iter()
            .any(|t| t.kind == sp_model::TokenType::DualDialogueBegin)
    );
}

/// The same for the other piece of module state, `blank_lines.lastIndex`:
/// a parse's answer never depends on the parse before it.
#[test]
fn a_parse_never_depends_on_the_one_before() {
    let a = "foo\n  \n\nbar\n";
    let b = "Action.\n\n  \n\nMore.\n";
    let b_alone = tokenize(b).unwrap();
    let _ = tokenize(a).unwrap();
    assert_eq!(tokenize(b).unwrap(), b_alone);
}

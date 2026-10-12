//! Differential fuzzing: random Fountain-shaped text through the port,
//! against fountain-js 1.2.4's answers for the same text.
//!
//! fountain-js answered once, under bun, for the first [`CASES`] scripts
//! [`SEED`] draws, each parse reset to a first parse because the port does
//! not carry its state between parses (PARITY.md). Its answers are frozen
//! in tests/data/tokenizer-fuzz.json as one SHA-256 per script, taken over
//! the answer's JSON with its keys sorted, so the file stays small and a
//! script whose tokens differ in any way is named by its number and text.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sp_fountain::tokenizer::tokenize;

/// The run fountain-js answered: its seed and how many scripts.
const SEED: u64 = 0x5EED_F00D;
const CASES: usize = 4000;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Fragments that each mean something to some rule, and glue between them.
const PIECES: &[&str] = &[
    "INT. HOUSE - DAY",
    "EXT. ROAD - NIGHT #12#",
    "int. lower - day",
    "INT./EXT. CAR - MOVING",
    "I/E. VAN",
    "EST. CITY",
    "*INT. STARRED*",
    ".LATER",
    "..ELLIPSIS",
    ". DOT SPACE",
    "@bob",
    "BOB",
    "BOB ^",
    "@BOB ^",
    "BOB (V.O.)",
    "BOB (CONT'D)",
    "123",
    "(beat)",
    "*(beat)*",
    "_(beat)_",
    "(unclosed",
    "Hello there.",
    "~la la la",
    "~ not a lyric",
    "> CUT TO:",
    "CUT TO:",
    "SMASH CUT TO:  ",
    "FADE OUT.",
    "FADE TO BLACK.",
    "CUT TO BLACK.",
    "> THE END <",
    ">center<  >two<",
    "# Act One",
    "## Sequence",
    "= synopsis",
    "= pg 3",
    "==",
    "===",
    "====",
    "[[note]]",
    "[[img: p001-01.png w=71]]",
    "[[[odd]]]",
    "/* boneyard",
    "*/",
    "/**/",
    "Title: The Thing",
    "Credit: Written by",
    "Author: Some One",
    "Authors: A and B",
    "Draft date: 1/1/26",
    "Contact:",
    "Notes: x",
    "Source: y",
    "Key: value",
    "   indented continuation",
    "\tTabbed",
    "!forced action",
    "! bang space",
    "\\@escaped",
    "**bold** and *italic* and _under_",
    "lower case action line.",
    "MIXED Case Line",
    "BLACK.",
    "TO:",
    "^",
    "@",
    "#1#",
    "  ",
    " ",
    "\u{A0}",
    "\u{FEFF}",
    "é",
    "Ω",
];

const GLUE: &[&str] = &[
    "\n", "\n", "\n", "\n\n", "\n\n", "\n\n\n", " ", "", "\r\n", "\r", "\n  \n", "\n   \n", "\t",
];

fn random_script(rng: &mut Rng) -> String {
    let n = 1 + rng.below(14);
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(PIECES[rng.below(PIECES.len())]);
        s.push_str(GLUE[rng.below(GLUE.len())]);
    }
    s
}

fn ours(s: &str) -> Value {
    match tokenize(s) {
        Ok(tokens) => json!({ "tokens": tokens }),
        Err(e) => json!({ "error": e.message }),
    }
}

/// `v` as JSON with every object's keys sorted: the same text for the same
/// answer, whatever order a serializer kept.
fn canonical(v: &Value) -> String {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let fields: Vec<String> = keys
                .iter()
                .map(|k| format!("{}:{}", Value::String((*k).clone()), canonical(&o[*k])))
                .collect();
            format!("{{{}}}", fields.join(","))
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        other => other.to_string(),
    }
}

fn digest(v: &Value) -> String {
    Sha256::digest(canonical(v).as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn inputs() -> Vec<String> {
    let mut rng = Rng(SEED);
    (0..CASES).map(|_| random_script(&mut rng)).collect()
}

/// The shapes the run has to reach, or agreeing proves little.
const SHAPES: &[&str] = &[
    "\"error\"",
    "\"dual\":\"left\"",
    "\"dual\":\"right\"",
    "\"is_title\"",
    "\"spaces\"",
    "\"lyrics\"",
    "\"centered\"",
    "\"scene_number\"",
    "\"page_break\"",
    "\"depth\"",
];

#[test]
fn random_scripts_tokenize_as_fountain_js_did() {
    let frozen: Value =
        serde_json::from_str(include_str!("data/tokenizer-fuzz.json")).expect("JSON");
    assert_eq!(frozen["seed"].as_u64(), Some(SEED));
    let want: Vec<&str> = frozen["digests"]
        .as_array()
        .expect("digests")
        .iter()
        .map(|d| d.as_str().expect("a digest"))
        .collect();
    assert_eq!(want.len(), CASES);
    let inputs = inputs();
    let answers: Vec<Value> = inputs.iter().map(|s| ours(s)).collect();
    let all = canonical(&Value::Array(answers.clone()));
    for shape in SHAPES {
        assert!(all.contains(shape), "no case produced {shape}");
    }
    let bad: Vec<String> = answers
        .iter()
        .enumerate()
        .filter(|(i, a)| digest(a) != want[*i])
        .map(|(i, a)| format!("case {i}: {:?}\n  port: {}", inputs[i], canonical(a)))
        .collect();
    assert!(
        bad.is_empty(),
        "{} of {CASES} differ from fountain-js; first:\n{}",
        bad.len(),
        bad.iter().take(5).cloned().collect::<Vec<_>>().join("\n")
    );
}

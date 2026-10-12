//! Differential fuzzing: random Fountain-shaped text through fountain-js and
//! through the port, one process each, parses in the same order, so the
//! module state fountain-js carries between parses is compared too.
//!
//! `#[ignore]`d: it needs bun and node_modules (tests/support/mod.rs).
//! `FUZZ_CASES` and `FUZZ_SEED` widen or move the run.

mod support;

use serde_json::{Value, json};
use sp_fountain::tokenizer::FountainJs;

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

fn ours(fj: &mut FountainJs, s: &str) -> Value {
    match fj.parse(s) {
        Ok(tokens) => json!({ "tokens": tokens }),
        Err(e) => json!({ "error": e.message }),
    }
}

#[test]
#[ignore = "needs bun and node_modules"]
fn random_scripts_tokenize_as_fountain_js_does() {
    let cases: usize = std::env::var("FUZZ_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4000);
    let seed: u64 = std::env::var("FUZZ_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0x5EED_F00D);
    let mut rng = Rng(seed);
    let inputs: Vec<String> = (0..cases).map(|_| random_script(&mut rng)).collect();
    let want = support::fountain_js(&inputs);
    let mut fj = FountainJs::new();
    let mut bad = Vec::new();
    for (i, s) in inputs.iter().enumerate() {
        let got = ours(&mut fj, s);
        if got != want[i] {
            bad.push(format!(
                "case {i}: {s:?}\n  fountain-js: {}\n  port:        {got}",
                want[i]
            ));
        }
    }
    // The run has to reach the shapes that matter, or agreeing proves little.
    let all = serde_json::to_string(&want).unwrap();
    for shape in [
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
    ] {
        assert!(all.contains(shape), "no case produced {shape}");
    }
    assert!(
        bad.is_empty(),
        "{} of {cases} differ; first:\n{}",
        bad.len(),
        bad.iter().take(5).cloned().collect::<Vec<_>>().join("\n")
    );
}

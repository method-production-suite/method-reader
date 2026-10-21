//! The `shape` verb's engine half: a script's shape as counts, never a word
//! from it (docs/superpowers/specs/2026-10-21-report-without-github-design.md).

mod common;

use common::Scratch;
use sp_engine::commands::shape::{ShapeResult, shape_command};

/// An invented script whose every name, place and line is a word no count
/// could ever spell, so finding one in the answer means text leaked.
const DISTINCTIVE: &str = "\
Title: Zephyrquill Almanac
Credit: Written by
Author: Orrinvale Thistlecombe
Draft date: Marchwind

INT. ZEPHYRQUILL OBSERVATORY - NIGHT

Brambleworth polishes the quintessoscope.

BRAMBLEWORTH
(whispering)
Glimmerhaven never sleeps.

= pg 2

QUELLANDRA
Snorkelwhistle again?

BRAMBLEWORTH ^
Vexillumbra, always.

[[img: quokkaflux.png]]

CUT TO:

EXT. MOSSGROVE PIER - DAWN

= pg 3

QUELLANDRA
Farewell, Ptarmiganette.
";

/// Every distinctive word above, lowercased.
const WORDS: &[&str] = &[
    "zephyrquill",
    "almanac",
    "orrinvale",
    "thistlecombe",
    "marchwind",
    "observatory",
    "brambleworth",
    "quintessoscope",
    "whispering",
    "glimmerhaven",
    "quellandra",
    "snorkelwhistle",
    "vexillumbra",
    "quokkaflux",
    "mossgrove",
    "pier",
    "ptarmiganette",
    "farewell",
    "cut to",
];

fn shape_of_text(text: &str) -> ShapeResult {
    let dir = Scratch::new("shape");
    let path = dir.join("Zephyrquill Almanac.fountain");
    common::write(&path, text);
    shape_command(&path).expect("a shape")
}

#[test]
fn the_shape_is_counts_of_what_the_script_holds() {
    let s = shape_of_text(DISTINCTIVE);
    assert_eq!(
        s,
        ShapeResult {
            pages: Some(3),
            scene_headings: 2,
            cues: 4,
            dialogue_blocks: 4,
            dual_exchanges: 1,
            pictures: 1,
            transitions: 1,
        }
    );
}

#[test]
fn no_word_from_the_script_reaches_the_answer() {
    let s = shape_of_text(DISTINCTIVE);
    let json = serde_json::to_string(&s).expect("JSON").to_lowercase();
    for word in WORDS {
        assert!(!json.contains(word), "{word} leaked into {json}");
    }
    // Every value is an integer: there is nowhere for a word to go.
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    for (key, value) in v.as_object().unwrap() {
        assert!(value.is_u64(), "{key} is {value}");
    }
}

#[test]
fn a_script_without_page_markers_has_no_page_count() {
    let s = shape_of_text("INT. ROOM - DAY\n\nA chair.\n");
    assert_eq!(s.pages, None);
    assert_eq!(s.scene_headings, 1);
    let json = serde_json::to_value(s).unwrap();
    assert!(json.get("pages").is_none(), "left out, not guessed");
}

#[test]
fn the_field_station_shape() {
    let path = common::repo_root().join("rust/oracle/field-station/script.fountain");
    let s = shape_command(&path.to_string_lossy()).expect("a shape");
    assert_eq!(s.pages, Some(18), "the title page is not counted");
    assert_eq!(s.scene_headings, 11);
    assert_eq!(s.transitions, 3);
    assert_eq!(s.cues, s.dialogue_blocks);
}

#[test]
fn a_refusal_names_neither_the_file_nor_its_text() {
    let dir = Scratch::new("shape-refuse");
    let missing = dir.join("Zephyrquill.fountain");
    let err = shape_command(&missing).unwrap_err().to_string().to_lowercase();
    assert!(!err.contains("zephyrquill"), "{err}");
    let bad = dir.join("Quokkaflux.fountain");
    std::fs::write(&bad, [0xff, 0xfe, 0x00, 0x51]).unwrap();
    let err = shape_command(&bad).unwrap_err().to_string().to_lowercase();
    assert!(!err.contains("quokkaflux"), "{err}");
}

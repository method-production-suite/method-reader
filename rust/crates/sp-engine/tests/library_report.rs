//! The `library-report` verb's engine half: one library script's report,
//! read from its `.fountain`
//! (docs/superpowers/specs/2026-11-04-conversion-report-design.md). The
//! command line is sp-cli/tests/cli_library_report.rs. Every script here is
//! invented.

mod common;

use common::{Scratch, write};
use serde_json::json;
use sp_engine::commands::library_report::{
    CharacterReport, count, cue_name, grouped, report_in_library, report_lines, word_count,
};
use sp_engine::errors::EngineError;
use sp_engine::library::{library_output, save_kept_name, save_page_count};
use sp_engine::nodepath::host;
use sp_fountain::tokenize;

/// A cold open before any heading, a mini-slug, a dual exchange, a cue
/// with extensions, a forced cue, one name in two cases, a note and a
/// parenthetical inside speeches, and page markers.
const KEEPER: &str = "\
Title: The Lamp Keeper
Author: Pat Doe

= pg 1

WREN (V.O.)
Every night the same.

INT. LIGHTHOUSE - NIGHT

WREN
(to herself)
Up the stairs, then.

.THE LAMP ROOM

= pg 2

OTTO
Again? [[a note, not spoken]] Fine.

Wren climbs.

WREN (CONT'D)
One hundred and twelve steps.

EXT. HARBOUR - DAY

@McKay
Boat's in.

OTTO
Late.

MCKAY ^
Late as ever.

@Wren
Morning already.
";

fn ch(
    name: &str,
    speeches: u32,
    words: u32,
    scenes: u32,
    first_scene: Option<u32>,
    first_page: Option<&str>,
) -> CharacterReport {
    CharacterReport {
        name: name.to_owned(),
        speeches,
        words,
        scenes,
        first_scene,
        first_page: first_page.map(str::to_owned),
    }
}

#[test]
fn a_script_counts_its_scenes_speeches_words_and_speakers() {
    let c = count(&tokenize(KEEPER).unwrap());
    assert_eq!(c.scenes, 2, "the mini-slug is not a scene");
    assert_eq!(c.speeches, 8);
    assert_eq!(c.words, 4 + 4 + 2 + 5 + 2 + 1 + 3 + 2);
    assert_eq!(
        c.characters,
        vec![
            ch("WREN", 4, 4 + 4 + 5 + 2, 2, Some(1), Some("1")),
            ch("OTTO", 2, 2 + 1, 2, Some(1), Some("2")),
            ch("McKay", 2, 2 + 3, 1, Some(2), Some("2")),
        ]
    );
}

#[test]
fn a_tie_goes_to_whoever_spoke_first() {
    let c = count(
        &tokenize("INT. A - DAY\n\nBEA\nOne.\n\nAL\nTwo.\n\nAL\nThree.\n\nBEA\nFour.\n").unwrap(),
    );
    let names: Vec<&str> = c.characters.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["BEA", "AL"]);
}

#[test]
fn a_speech_before_any_heading_is_in_no_scene_and_no_marker_means_no_page() {
    let c = count(&tokenize("WREN\nHello.\n").unwrap());
    assert_eq!(c.scenes, 0);
    assert_eq!(c.characters, vec![ch("WREN", 1, 1, 0, None, None)]);
}

#[test]
fn a_speech_before_the_first_marker_is_on_the_page_before_it() {
    // The serializer writes no marker for the page a script opens on.
    let c = count(&tokenize("INT. A - DAY\n\nBEA\nOne.\n\n= pg 3\n\nAL\nTwo.\n").unwrap());
    assert_eq!(c.characters[0].first_page.as_deref(), Some("2"));
    assert_eq!(c.characters[1].first_page.as_deref(), Some("3"));
}

#[test]
fn a_cue_names_its_character_without_its_trimmings() {
    assert_eq!(cue_name("WREN (V.O.) (CONT'D)"), "WREN");
    assert_eq!(cue_name("@McKay"), "McKay");
    assert_eq!(cue_name("OTTO ^"), "OTTO");
    assert_eq!(cue_name("*WREN*"), "WREN");
    assert_eq!(cue_name("WREN [[a note]]"), "WREN");
}

#[test]
fn a_word_holds_a_letter_or_a_digit() {
    assert_eq!(word_count("Wait -- what? 112 steps..."), 4);
    assert_eq!(word_count("*Now* [[not this]] _go_"), 2);
    assert_eq!(word_count("..."), 0);
}

#[test]
fn numbers_are_grouped_by_thousands() {
    assert_eq!(grouped(7), "7");
    assert_eq!(grouped(2310), "2,310");
    assert_eq!(grouped(1_234_567), "1,234,567");
}

// ── The library half ─────────────────────────────────────

fn script(root: &str, source: &str, text: &str) -> (String, String) {
    let prefix = library_output(source, root).expect("a library folder");
    let fountain = format!("{prefix}.fountain");
    write(&fountain, text);
    (host::dirname(&prefix), fountain)
}

#[test]
fn the_report_of_a_library_script_carries_its_rows_title_and_pages() {
    let s = Scratch::new("library-report");
    let root = s.dir("lib");
    let (folder, fountain) = script(&root, "/scripts/keeper_v3.pdf", KEEPER);
    save_page_count(&fountain, KEEPER, Some(3)).unwrap();
    let r = report_in_library(&root, &folder).unwrap();
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["folder"], json!(folder));
    assert_eq!(v["title"], json!("The Lamp Keeper"));
    assert_eq!(
        v["pages"],
        json!(3),
        "the conversion's count beats the markers' 2"
    );
    assert_eq!(v["scenes"], json!(2));
    assert_eq!(v["speeches"], json!(8));
    assert_eq!(
        v["characters"][2],
        json!({"name": "McKay", "speeches": 2, "words": 5, "scenes": 1, "firstScene": 2, "firstPage": "2"})
    );
    assert_eq!(
        report_lines(&r),
        "The Lamp Keeper\n\
         3 pages, 2 scenes, 8 speeches, 23 words of dialogue\n\
         WREN: 4 speeches, 15 words, 2 scenes, from scene 1, page 1\n\
         OTTO: 2 speeches, 3 words, 2 scenes, from scene 1, page 2\n\
         McKay: 2 speeches, 5 words, 1 scene, from scene 2, page 2"
    );

    save_kept_name(&fountain, Some("Harbour Light")).unwrap();
    assert_eq!(
        report_in_library(&root, &folder).unwrap().title,
        "Harbour Light"
    );
}

#[test]
fn a_script_without_title_or_pages_says_its_file_name_and_no_pages() {
    let s = Scratch::new("library-report");
    let root = s.dir("lib");
    let (folder, _) = script(&root, "/scripts/untitled.fountain", "WREN\nHi.\n");
    let r = report_in_library(&root, &folder).unwrap();
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["title"], json!("untitled"));
    assert_eq!(v["pages"], json!(null));
    assert_eq!(v["characters"][0]["firstScene"], json!(null));
    assert_eq!(v["characters"][0]["firstPage"], json!(null));
    assert_eq!(
        report_lines(&r),
        "untitled\n0 scenes, 1 speech, 1 word of dialogue\nWREN: 1 speech, 1 word, 0 scenes, before the first scene"
    );
}

#[test]
fn only_a_listed_script_is_reported_and_nothing_is_written() {
    let s = Scratch::new("library-report");
    let root = s.dir("lib");
    let (folder, _) = script(&root, "/scripts/keeper.pdf", KEEPER);
    let before = common::names(&folder);
    report_in_library(&root, &folder).unwrap();
    assert_eq!(common::names(&folder), before);

    let outside = s.dir("elsewhere");
    for bad in [outside.as_str(), root.as_str(), "relative/folder"] {
        match report_in_library(&root, bad) {
            Err(EngineError::Cli(e)) => assert!(
                ["not-a-script", "usage"].contains(&e.code.as_str()),
                "{bad}: {}",
                e.code.as_str()
            ),
            other => panic!("{bad}: {other:?}"),
        }
    }
}

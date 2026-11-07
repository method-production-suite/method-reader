//! The `library-sides` verb's engine half: one character's scenes as a book
//! (docs/superpowers/specs/2026-11-05-sides-design.md). The command line is
//! sp-cli/tests/cli_library_sides.rs. Every script here is invented.

mod common;

use std::io::Read;

use common::{Scratch, write};
use sp_engine::commands::library_sides::{
    LibrarySides, lines_file_name, lines_in_library, lines_line, sides_file_name, sides_in_library,
    sides_line, sides_of,
};
use sp_engine::errors::EngineError;
use sp_engine::library::{library_output, save_kept_name};
use sp_engine::nodepath::host;
use sp_fountain::tokenize;
use sp_model::{FormatOptions, Token, TokenType};

const KEEPER: &str = "\
Title: The Lamp Keeper
Author: Pat Doe

= pg 1

INT. LIGHTHOUSE - NIGHT

WREN
Up the stairs, then.

.THE LAMP ROOM

OTTO
Again.

INT. KITCHEN - DAY

Otto eats alone. A wrench lies on the table.

OTTO
Wren would hate this.

= pg 2

INT. CELLAR - NIGHT

BEA
Nobody down here.

= pg 3

EXT. HARBOUR - DAY

Wren walks the pier.

INT. ATTIC - DAY

An empty room. WREN'S coat hangs by the door.
";

fn headings(tokens: &[Token]) -> Vec<String> {
    tokens
        .iter()
        .filter(|t| t.kind == TokenType::SceneHeading)
        .map(|t| t.text_or_empty().to_owned())
        .collect()
}

fn markers(tokens: &[Token]) -> Vec<String> {
    tokens
        .iter()
        .filter(|t| t.kind == TokenType::Synopsis)
        .map(|t| t.text_or_empty().to_owned())
        .collect()
}

#[test]
fn a_character_is_in_the_scenes_they_speak_in_or_action_names_them() {
    let sides = sides_of(&tokenize(KEEPER).unwrap(), "WREN");
    assert_eq!(
        headings(&sides.tokens),
        [
            "INT. LIGHTHOUSE - NIGHT",
            "THE LAMP ROOM",
            "EXT. HARBOUR - DAY",
            "INT. ATTIC - DAY",
        ],
        "a mini-slug stays with its scene; dialogue that mentions them and \
         a word that only holds the name (wrench) do not count"
    );
    assert_eq!((sides.scenes, sides.of_scenes), (3, 5));
}

/// The two scenes of a script: the first names `who` in its action, as
/// `spelled`, and the second doesn't. How many scenes `who` is in.
fn scenes_naming(who: &str, spelled: &str) -> u32 {
    let script = format!(
        "INT. BOATHOUSE - DAY\n\n{spelled} coils a rope.\n\nEXT. QUAY - DAY\n\nGulls circle.\n"
    );
    sides_of(&tokenize(&script).unwrap(), who).scenes
}

#[test]
fn an_action_line_names_a_character_however_their_name_is_capitalized() {
    for (who, spelled) in [
        ("JEAN-LUC", "Jean-Luc"),
        ("O'BRIEN", "O'Brien"),
        ("O'BRIEN", "O\u{2019}Brien"),
        ("O\u{2019}BRIEN", "O'Brien"),
        ("MCCOY", "McCoy"),
        ("MCCOY", "Mccoy"),
        ("VAN DER BERG", "van der Berg"),
        ("STRASSE", "Straße"),
        ("WREN", "WREN"),
        ("WREN", "Wren"),
    ] {
        assert_eq!(scenes_naming(who, spelled), 1, "{who} as {spelled}");
    }
}

#[test]
fn a_word_with_no_capital_or_more_letters_around_it_is_not_the_name() {
    for (who, spelled) in [
        ("WREN", "wren"),
        ("WREN", "Wrench"),
        ("WREN", "Swren"),
        ("JEAN-LUC", "jean-luc"),
        ("MCCOY", "mccoy"),
    ] {
        assert_eq!(scenes_naming(who, spelled), 0, "{who} as {spelled}");
    }
}

#[test]
fn a_name_with_no_capitals_to_it_is_found_as_it_is_spelled() {
    assert_eq!(scenes_naming("R2", "R2"), 1);
    assert_eq!(scenes_naming("r2", "r2"), 1);
    assert_eq!(scenes_naming("\u{674e}", "\u{674e}"), 1);
}

#[test]
fn the_title_page_stays() {
    let sides = sides_of(&tokenize(KEEPER).unwrap(), "BEA");
    let title: Vec<&Token> = sides.tokens.iter().filter(|t| t.is_title()).collect();
    assert_eq!(title.len(), 2);
    assert_eq!(headings(&sides.tokens), ["INT. CELLAR - NIGHT"]);
    assert_eq!(sides.scenes, 1);
}

#[test]
fn a_kept_scene_gets_the_page_marker_a_dropped_scene_held() {
    let bea = sides_of(&tokenize(KEEPER).unwrap(), "BEA");
    assert_eq!(
        markers(&bea.tokens),
        ["pg 2", "pg 3"],
        "the latest before the scene, then the one that scene runs into"
    );

    let wren = sides_of(&tokenize(KEEPER).unwrap(), "WREN");
    assert_eq!(markers(&wren.tokens), ["pg 1", "pg 3"], "once each");
    let harbour = wren
        .tokens
        .iter()
        .position(|t| t.text_or_empty() == "EXT. HARBOUR - DAY")
        .unwrap();
    assert_eq!(wren.tokens[harbour - 1].text_or_empty(), "pg 3");
}

#[test]
fn what_comes_before_the_first_heading_is_kept_by_the_same_test() {
    let script = "WREN\nEvery night.\n\nINT. A - DAY\n\nOTTO\nHello.\n";
    let wren = sides_of(&tokenize(script).unwrap(), "WREN");
    assert!(headings(&wren.tokens).is_empty());
    assert_eq!((wren.scenes, wren.of_scenes), (0, 1));
    assert!(
        wren.tokens
            .iter()
            .any(|t| t.text_or_empty() == "Every night.")
    );
    let otto = sides_of(&tokenize(script).unwrap(), "OTTO");
    assert!(
        !otto
            .tokens
            .iter()
            .any(|t| t.text_or_empty() == "Every night.")
    );
}

#[test]
fn dual_dialogue_keeps_the_scene_for_either_speaker() {
    let script = "INT. A - DAY\n\nAL\nOne.\n\nBEA ^\nTwo.\n\nINT. B - DAY\n\nCY\nThree.\n";
    assert_eq!(
        headings(&sides_of(&tokenize(script).unwrap(), "BEA").tokens),
        ["INT. A - DAY"]
    );
}

#[test]
fn a_file_name_holds_the_stem_and_a_safe_name() {
    assert_eq!(
        sides_file_name("keeper", "WREN"),
        "keeper sides - WREN.epub"
    );
    assert_eq!(
        sides_file_name("keeper", "DR. O'HARA/2"),
        "keeper sides - DR. O_HARA_2.epub"
    );
    assert_eq!(
        sides_file_name("keeper", "..."),
        "keeper sides - character.epub"
    );
}

/// A library at `<scratch>/lib` holding KEEPER; answers the root and folder.
fn library(s: &Scratch) -> (String, String) {
    let root = s.dir("lib");
    let stem = library_output("/scripts/keeper.pdf", &root).unwrap();
    write(&format!("{stem}.fountain"), KEEPER);
    (root, host::dirname(&stem))
}

fn made(root: &str, folder: &str, who: &str) -> LibrarySides {
    sides_in_library(root, folder, who, &FormatOptions::default()).unwrap()
}

fn entry(epub: &str, wanted: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(epub).unwrap()).unwrap();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        if f.name().unwrap().ends_with(wanted) {
            let mut text = String::new();
            f.read_to_string(&mut text).unwrap();
            return text;
        }
    }
    panic!("no {wanted} in {epub}");
}

#[test]
fn it_makes_a_book_in_the_sides_folder_and_touches_nothing_else() {
    let s = Scratch::new("library-sides");
    let (root, folder) = library(&s);
    let fountain = host::join(&[folder.as_str(), "keeper.fountain"]);
    let sides = made(&root, &folder, "wren (v.o.)");
    assert_eq!(sides.name, "WREN", "the report's spelling");
    assert_eq!(sides.title, "The Lamp Keeper: WREN sides");
    assert_eq!((sides.scenes, sides.of_scenes), (3, 5));
    assert_eq!(
        sides.epub,
        host::join(&[folder.as_str(), "sides", "keeper sides - WREN.epub"])
    );
    assert_eq!(std::fs::read_to_string(&fountain).unwrap(), KEEPER);
    assert!(!std::path::Path::new(&format!("{}/keeper.epub", folder)).exists());

    let opf = entry(&sides.epub, ".opf");
    assert!(opf.contains("The Lamp Keeper: WREN sides"), "{opf}");
    assert!(opf.contains("Pat Doe"), "{opf}");
    assert_eq!(
        sides_line(&sides),
        format!("sides for WREN: 3 of 5 scenes, {}", sides.epub)
    );
    // Made again in place.
    assert_eq!(made(&root, &folder, "WREN").epub, sides.epub);
}

#[test]
fn the_book_title_follows_a_kept_name() {
    let s = Scratch::new("library-sides");
    let (root, folder) = library(&s);
    save_kept_name(
        &host::join(&[folder.as_str(), "keeper.fountain"]),
        Some("Harbour Light"),
    )
    .unwrap();
    assert_eq!(
        made(&root, &folder, "BEA").title,
        "Harbour Light: BEA sides"
    );
}

#[test]
fn someone_with_no_cue_or_no_listed_folder_is_refused() {
    let s = Scratch::new("library-sides");
    let (root, folder) = library(&s);
    let refusal = |folder: &str, who: &str| match sides_in_library(
        &root,
        folder,
        who,
        &FormatOptions::default(),
    ) {
        Err(EngineError::Cli(e)) => e.code.as_str().to_owned(),
        other => panic!("{other:?}"),
    };
    assert_eq!(refusal(&folder, "NOBODY"), "usage");
    assert_eq!(refusal(&folder, "   "), "usage");
    assert_eq!(refusal(&s.dir("elsewhere"), "WREN"), "not-a-script");
}

#[test]
fn lines_in_bold_keep_every_scene_and_mark_only_that_speech() {
    let s = Scratch::new("library-lines");
    let (root, folder) = library(&s);
    let fountain = format!("{}/keeper.fountain", folder);
    let before = std::fs::read(&fountain).unwrap();

    let lines = lines_in_library(&root, &folder, "otto", &FormatOptions::default()).unwrap();
    assert_eq!(lines.name, "OTTO");
    assert_eq!(lines.title, "The Lamp Keeper: OTTO lines");
    assert_eq!(lines.speeches, 2);
    assert_eq!(
        lines.epub,
        format!("{folder}/sides/{}", lines_file_name("keeper", "OTTO"))
    );
    assert_eq!(
        lines_line(&lines),
        format!("lines in bold for OTTO: 2 speeches, {}", lines.epub)
    );

    let opf = entry(&lines.epub, ".opf");
    assert!(opf.contains("The Lamp Keeper: OTTO lines"), "{opf}");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&lines.epub).unwrap()).unwrap();
    let mut book = String::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        if f.name().unwrap().ends_with(".xhtml") {
            f.read_to_string(&mut book).unwrap();
        }
    }
    for heading in [
        "INT. CELLAR - NIGHT",
        "EXT. HARBOUR - DAY",
        "INT. ATTIC - DAY",
    ] {
        assert!(book.contains(heading), "every scene stays: {heading}");
    }
    assert!(book.contains("<strong>Again.</strong>"), "{book}");
    assert!(
        book.contains("<strong>Wren would hate this.</strong>"),
        "{book}"
    );
    assert!(book.contains(">Nobody down here.</p>"), "{book}");
    assert!(!book.contains("<strong>Nobody"), "{book}");
    assert!(!book.contains("<strong>Otto eats"), "{book}");

    assert_eq!(std::fs::read(&fountain).unwrap(), before);
}

#[test]
fn lines_in_bold_refuse_who_sides_refuse() {
    let s = Scratch::new("library-lines-refused");
    let (root, folder) = library(&s);
    assert!(lines_in_library(&root, &folder, "NOBODY", &FormatOptions::default()).is_err());
    assert!(lines_in_library(&root, "/nowhere", "OTTO", &FormatOptions::default()).is_err());
    assert!(!std::path::Path::new(&format!("{folder}/sides")).exists());
}

#[test]
fn a_lines_file_name_holds_the_stem_and_a_safe_name() {
    assert_eq!(
        lines_file_name("keeper", "DR. O'HARA/2"),
        "keeper lines - DR. O_HARA_2.epub"
    );
}

#[test]
fn each_answer_holds_its_book_for_the_preview() {
    let s = Scratch::new("library-preview");
    let (root, folder) = library(&s);
    let sides = made(&root, &folder, "WREN");
    assert!(sides.preview_html.contains("INT. LIGHTHOUSE - NIGHT"));
    assert!(
        !sides.preview_html.contains("INT. CELLAR - NIGHT"),
        "only the scenes kept"
    );
    let lines = lines_in_library(&root, &folder, "OTTO", &FormatOptions::default()).unwrap();
    assert!(lines.preview_html.contains("INT. CELLAR - NIGHT"));
    assert!(lines.preview_html.contains("<strong>Again.</strong>"));
}

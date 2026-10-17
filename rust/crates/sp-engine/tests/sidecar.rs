//! Port of tests/sidecar.test.ts: the per-script settings file.

mod common;

use common::{Scratch, names, write};
use serde_json::{Value, json};
use sp_engine::nodepath::host;
use sp_engine::settings::sidecar::{
    legacy_sidecar_path, load_script_settings, read_script_settings, save_script_settings,
    sidecar_path,
};
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions, resolve_format_options};

fn library(s: &Scratch) -> String {
    s.dir("library")
}

#[test]
fn sidecar_path_derives_from_the_fountain_stem() {
    let s = Scratch::new("sidecar");
    let fountain = host::join(&[library(&s).as_str(), "Test Script.fountain"]);
    assert_eq!(
        host::basename(&sidecar_path(&fountain)),
        "Test Script.method-reader.json"
    );
}

#[test]
fn sidecar_naming_agrees_with_the_library_for_every_stem() {
    let s = Scratch::new("sidecar");
    let dir = library(&s);
    for name in [
        "Draft.pdf",
        "My.Script.pdf",
        "My.Script.fountain",
        "a.b.c",
        ".hidden",
    ] {
        let stem = host::basename_ext(name, &host::extname(name));
        assert_eq!(
            host::basename(&sidecar_path(&host::join(&[dir.as_str(), name]))),
            format!("{stem}.method-reader.json")
        );
    }
}

#[test]
fn sidecar_round_trips_settings() {
    let s = Scratch::new("sidecar");
    let fountain = host::join(&[library(&s).as_str(), "Test Script.fountain"]);
    write(&fountain, "Title: T");
    let settings = FormatOptions {
        dialogue_side_margin_pct: 27.0,
        keep_speeches_whole: true,
        ..DEFAULT_FORMAT_OPTIONS
    };
    save_script_settings(&settings, &fountain).unwrap();
    assert_eq!(
        load_script_settings(&fountain, &DEFAULT_FORMAT_OPTIONS),
        settings
    );
}

#[test]
fn the_written_sidecar_has_sorted_keys_as_json_stringify_writes_them() {
    let s = Scratch::new("sidecar");
    let fountain = host::join(&[library(&s).as_str(), "Sorted.fountain"]);
    save_script_settings(&DEFAULT_FORMAT_OPTIONS, &fountain).unwrap();
    let text = common::read(&sidecar_path(&fountain));
    assert!(text.starts_with("{\n  \"contdMode\": \"auto\",\n  \"cueAlignment\": \"centered\",\n  \"cueIndentPct\": 33,\n"));
    assert!(text.ends_with("  \"showSceneNumbers\": false\n}\n"));
}

#[test]
fn absent_sidecar_falls_back() {
    let s = Scratch::new("sidecar");
    let fountain = host::join(&[library(&s).as_str(), "Other.fountain"]);
    assert_eq!(
        load_script_settings(&fountain, &DEFAULT_FORMAT_OPTIONS),
        DEFAULT_FORMAT_OPTIONS
    );
}

#[test]
fn corrupt_sidecar_falls_back() {
    let s = Scratch::new("sidecar");
    let fountain = host::join(&[library(&s).as_str(), "Garbage.fountain"]);
    write(&sidecar_path(&fountain), "not json");
    assert_eq!(
        load_script_settings(&fountain, &DEFAULT_FORMAT_OPTIONS),
        DEFAULT_FORMAT_OPTIONS
    );
}

#[test]
fn a_json_array_falls_back() {
    let s = Scratch::new("sidecar");
    let fountain = host::join(&[library(&s).as_str(), "Array.fountain"]);
    write(&sidecar_path(&fountain), "[1,2,3]");
    assert_eq!(
        load_script_settings(&fountain, &DEFAULT_FORMAT_OPTIONS),
        DEFAULT_FORMAT_OPTIONS
    );
}

#[test]
fn partial_sidecar_overlays_the_present_field() {
    let s = Scratch::new("sidecar");
    let fountain = host::join(&[library(&s).as_str(), "Partial.fountain"]);
    write(&sidecar_path(&fountain), r#"{"dialogueSideMarginPct": 9}"#);
    assert_eq!(
        load_script_settings(&fountain, &DEFAULT_FORMAT_OPTIONS),
        FormatOptions {
            dialogue_side_margin_pct: 9.0,
            ..DEFAULT_FORMAT_OPTIONS
        }
    );
}

/// Every row writes the OPPOSITE of the default, ported from the Swift app's
/// own checks through the TypeScript's.
const OVERRIDE_CASES: [(&str, &str); 20] = [
    ("scenePageBreaks", r#"{"scenePageBreaks": true}"#),
    ("dialogueSideMarginPct", r#"{"dialogueSideMarginPct": 9}"#),
    ("cueIndentPct", r#"{"cueIndentPct": 11}"#),
    ("parentheticalIndentPct", r#"{"parentheticalIndentPct": 5}"#),
    ("elementSpacingEm", r#"{"elementSpacingEm": 1.6}"#),
    (
        "keepSceneHeadingWithScene",
        r#"{"keepSceneHeadingWithScene": false}"#,
    ),
    ("keepSpeechesWhole", r#"{"keepSpeechesWhole": true}"#),
    ("fontFamily", r#"{"fontFamily": "serif"}"#),
    ("rejoinSplitDialogue", r#"{"rejoinSplitDialogue": false}"#),
    ("contdMode", r#"{"contdMode": "strip"}"#),
    ("cueAlignment", r#"{"cueAlignment": "indented"}"#),
    ("includeTitlePage", r#"{"includeTitlePage": false}"#),
    ("showSceneNumbers", r#"{"showSceneNumbers": true}"#),
    ("showPageMarkers", r#"{"showPageMarkers": true}"#),
    ("dualDialogue", r#"{"dualDialogue": "sequential"}"#),
    ("justifyText", r#"{"justifyText": true}"#),
    ("printSplitMinimums", r#"{"printSplitMinimums": false}"#),
    ("preserveFontShifts", r#"{"preserveFontShifts": false}"#),
    ("includeImages", r#"{"includeImages": false}"#),
    ("promoteCoverImage", r#"{"promoteCoverImage": false}"#),
];

#[test]
fn sidecar_merge_overrides_each_field_against_its_default() {
    let s = Scratch::new("sidecar");
    let dir = library(&s);
    for (field, text) in OVERRIDE_CASES {
        let fountain = host::join(&[dir.as_str(), &format!("Override-{field}.fountain")]);
        write(&sidecar_path(&fountain), text);
        let overlay: Value = serde_json::from_str(text).unwrap();
        // The expected value is the defaults with that one key replaced,
        // built through JSON so it cannot share the merge under test.
        let mut expected = serde_json::to_value(DEFAULT_FORMAT_OPTIONS).unwrap();
        expected[field] = overlay[field].clone();
        let expected: FormatOptions = serde_json::from_value(expected).unwrap();
        assert_eq!(
            load_script_settings(&fountain, &DEFAULT_FORMAT_OPTIONS),
            expected,
            "{field}"
        );
        assert_eq!(
            resolve_format_options(Some(&overlay), &DEFAULT_FORMAT_OPTIONS),
            expected
        );
    }
}

#[test]
fn every_format_options_field_has_an_override_row() {
    let all = serde_json::to_value(DEFAULT_FORMAT_OPTIONS).unwrap();
    let covered: Vec<&str> = OVERRIDE_CASES.iter().map(|(f, _)| *f).collect();
    let missing: Vec<&String> = all
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| !covered.contains(&k.as_str()))
        .collect();
    assert!(missing.is_empty(), "{missing:?}");
    let _ = json!(null);
}

#[test]
fn a_sidecar_write_that_cannot_land_leaves_no_temp_file() {
    let s = Scratch::new("sidecar");
    let dir = library(&s);
    let fountain = host::join(&[dir.as_str(), "Blocked.fountain"]);
    std::fs::create_dir(sidecar_path(&fountain)).unwrap();
    assert!(save_script_settings(&DEFAULT_FORMAT_OPTIONS, &fountain).is_err());
    assert_eq!(names(&dir), vec![host::basename(&sidecar_path(&fountain))]);
}

// The 0.12 name, `<Stem>.screepub.json`: read when the new file is missing,
// and replaced by the new name the first time the settings are saved.

#[test]
fn a_sidecar_under_the_old_name_is_still_read() {
    let s = Scratch::new("sidecar");
    let fountain = host::join(&[library(&s).as_str(), "Draft.fountain"]);
    write(&fountain, "Title: T");
    let old = host::join(&[library(&s).as_str(), "Draft.screepub.json"]);
    write(&old, r#"{"keepSpeechesWhole":true}"#);
    assert_eq!(legacy_sidecar_path(&fountain), old);
    let read = read_script_settings(&fountain, &DEFAULT_FORMAT_OPTIONS).expect("read");
    assert_eq!(read.path, old);
    assert!(read.settings.expect("an object").keep_speeches_whole);
}

#[test]
fn the_new_name_wins_when_both_are_there() {
    let s = Scratch::new("sidecar");
    let fountain = host::join(&[library(&s).as_str(), "Draft.fountain"]);
    write(&fountain, "Title: T");
    write(
        &host::join(&[library(&s).as_str(), "Draft.screepub.json"]),
        r#"{"keepSpeechesWhole":true}"#,
    );
    write(&sidecar_path(&fountain), r#"{"keepSpeechesWhole":false}"#);
    let read = read_script_settings(&fountain, &DEFAULT_FORMAT_OPTIONS).expect("read");
    assert_eq!(read.path, sidecar_path(&fountain));
    assert!(!read.settings.expect("an object").keep_speeches_whole);
}

#[test]
fn saving_writes_the_new_name_and_removes_the_old_one() {
    let s = Scratch::new("sidecar");
    let fountain = host::join(&[library(&s).as_str(), "Draft.fountain"]);
    write(&fountain, "Title: T");
    let old = legacy_sidecar_path(&fountain);
    write(&old, r#"{"keepSpeechesWhole":true}"#);
    let settings = FormatOptions {
        keep_speeches_whole: true,
        ..DEFAULT_FORMAT_OPTIONS
    };
    save_script_settings(&settings, &fountain).unwrap();
    assert!(std::path::Path::new(&sidecar_path(&fountain)).is_file());
    assert!(!std::path::Path::new(&old).exists());
    assert_eq!(
        load_script_settings(&fountain, &DEFAULT_FORMAT_OPTIONS),
        settings
    );
}

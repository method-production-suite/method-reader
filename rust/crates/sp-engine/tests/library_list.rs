//! The `library` verb's engine half: what is in the library folder, read
//! from the folder itself (docs/superpowers/specs/2026-11-02-library-view-design.md).
//! Every script here is invented; Field Station's `.fountain` is the
//! committed oracle's.

mod common;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{Scratch, guarded_env, names, read, repo_root, write};
use serde_json::{Value, json};
use sp_engine::commands::library::{LibraryListing, library_command, list_library};
use sp_engine::library::library_output;
use sp_engine::nodepath::host;

/// 2026-11-02T18:00:00Z.
const EVENING: u64 = 1_793_642_400;

/// A short invented script: a title with emphasis and a note, two real
/// scenes, one mini-slug and two page markers.
const MOTH_HOUR: &str = "\
Title: **The Moth Hour** [[working title]]
Credit: Written by
Author: Nobody In Particular

INT. LIGHTHOUSE - NIGHT

The lamp turns.

.ON THE LENS

= pg 2

EXT. CLIFF PATH - DAWN

WREN
Morning already.

= pg 3

Gulls.
";

fn j(parts: &[&str]) -> String {
    host::join(parts)
}

/// Its modified time set to `epoch` seconds.
fn stamp(path: &str, epoch: u64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(epoch))
        .unwrap();
}

/// A script the way a library conversion leaves it: its folder (named by
/// the engine, source.json and all) and its `.fountain`, modified at
/// `epoch`. Answers the path prefix every file in the folder is built from.
fn script(root: &str, source: &str, text: &str, epoch: u64) -> String {
    let prefix = library_output(source, root).expect("a library folder");
    let fountain = format!("{prefix}.fountain");
    write(&fountain, text);
    stamp(&fountain, epoch);
    prefix
}

fn listing(root: &str) -> Value {
    serde_json::to_value(list_library(root)).unwrap()
}

fn names_listed(v: &Value) -> Vec<String> {
    v["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn two_scripts_come_newest_first_with_every_field() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    let scripts = s.dir("scripts");
    let moth_pdf = j(&[&scripts, "The Moth Hour.pdf"]);
    write(&moth_pdf, "%PDF stand-in");
    let older = script(&root, &moth_pdf, MOTH_HOUR, EVENING - 3600);
    write(&format!("{older}.epub"), "PK stand-in");
    let field =
        std::fs::read_to_string(repo_root().join("rust/oracle/field-station/script.fountain"))
            .unwrap();
    let newer = script(&root, &j(&[&scripts, "Field Station.pdf"]), &field, EVENING);

    let v = listing(&root);
    assert_eq!(v["root"], json!(root));
    assert_eq!(names_listed(&v), ["Field Station", "The Moth Hour"]);
    assert_eq!(
        v["scripts"][0],
        json!({
            "folder": j(&[&root, "Field Station"]),
            "name": "Field Station",
            "title": "Field Station",
            "source": j(&[&scripts, "Field Station.pdf"]),
            "sourceExists": false,
            "fountain": format!("{newer}.fountain"),
            "epub": null,
            "pages": 18,
            "scenes": 11,
            "converted": "2026-11-02T18:00:00Z",
            "others": [],
            "settings": null,
        })
    );
    assert_eq!(
        v["scripts"][1],
        json!({
            "folder": j(&[&root, "The Moth Hour"]),
            "name": "The Moth Hour",
            "title": "The Moth Hour",
            "source": moth_pdf,
            "sourceExists": true,
            "fountain": format!("{older}.fountain"),
            "epub": format!("{older}.epub"),
            "pages": 3,
            "scenes": 2,
            "converted": "2026-11-02T17:00:00Z",
            "others": [],
            "settings": null,
        })
    );
}

#[test]
fn the_counts_agree_with_the_shape_verb_on_field_station() {
    // The same `.fountain`, two readings: the library's pages and scenes
    // are the shape verb's pages and scene headings.
    let path = repo_root().join("rust/oracle/field-station/script.fountain");
    let shape = sp_engine::commands::shape::shape_command(&path.to_string_lossy()).unwrap();
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    let text = std::fs::read_to_string(&path).unwrap();
    script(&root, "/scripts/Field Station.pdf", &text, EVENING);
    let v = listing(&root);
    assert_eq!(v["scripts"][0]["pages"], json!(shape.pages));
    assert_eq!(v["scripts"][0]["scenes"], json!(shape.scene_headings));
}

#[test]
fn scripts_converted_at_the_same_moment_come_by_name() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    for name in ["Weatherglass", "Anchorage", "Mudlark"] {
        script(&root, &format!("/scripts/{name}.pdf"), MOTH_HOUR, EVENING);
    }
    assert_eq!(
        names_listed(&listing(&root)),
        ["Anchorage", "Mudlark", "Weatherglass"]
    );
}

#[test]
fn two_scripts_with_one_name_are_both_listed_one_in_its_hashed_folder() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    let (drafts, last) = (
        s.join("drafts/The Moth Hour.pdf"),
        s.join("final/The Moth Hour.pdf"),
    );
    let first = script(&root, &drafts, MOTH_HOUR, EVENING);
    let second = script(&root, &last, MOTH_HOUR, EVENING - 60);
    assert_eq!(first, j(&[&root, "The Moth Hour", "The Moth Hour"]));
    let hashed = host::dirname(&second);
    assert_ne!(hashed, j(&[&root, "The Moth Hour"]));
    assert!(host::basename(&hashed).starts_with("The Moth Hour-"));

    let v = listing(&root);
    let scripts = v["scripts"].as_array().unwrap();
    assert_eq!(scripts.len(), 2);
    assert_eq!(scripts[0]["folder"], json!(j(&[&root, "The Moth Hour"])));
    assert_eq!(scripts[0]["source"], json!(drafts));
    assert_eq!(scripts[1]["folder"], json!(hashed));
    assert_eq!(scripts[1]["name"], json!("The Moth Hour"));
    assert_eq!(scripts[1]["source"], json!(last));
    assert_eq!(scripts[1]["fountain"], json!(format!("{second}.fountain")));
}

#[test]
fn anything_that_is_not_a_script_folder_is_left_out_silently() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    script(&root, "/scripts/Weatherglass.pdf", MOTH_HOUR, EVENING);
    // A folder with no source.json, though it holds a .fountain.
    write(&j(&[&root, "Loose", "Loose.fountain"]), MOTH_HOUR);
    // A source.json that names no source.
    write(&j(&[&root, "Unnamed", "source.json"]), "{\"scenes\": {}}\n");
    write(&j(&[&root, "Unnamed", "Unnamed.fountain"]), MOTH_HOUR);
    // A source.json that is not JSON.
    write(&j(&[&root, "Garbled", "source.json"]), "not json");
    write(&j(&[&root, "Garbled", "Garbled.fountain"]), MOTH_HOUR);
    // A script folder whose .fountain has gone.
    let prefix = library_output("/scripts/Mudlark.pdf", &root).unwrap();
    write(&format!("{prefix}.epub"), "PK stand-in");
    // A script folder whose .fountain is named for another source.
    let prefix = library_output("/scripts/Anchorage.pdf", &root).unwrap();
    write(
        &format!("{}.fountain", j(&[&host::dirname(&prefix), "Other"])),
        MOTH_HOUR,
    );
    // A stray file.
    write(&j(&[&root, "notes.txt"]), "a stray file");

    assert_eq!(names_listed(&listing(&root)), ["Weatherglass"]);
}

#[test]
fn a_source_that_has_gone_is_flagged_and_the_book_still_listed() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    let pdf = s.join("Weatherglass.pdf");
    write(&pdf, "%PDF stand-in");
    script(&root, &pdf, MOTH_HOUR, EVENING);
    assert_eq!(listing(&root)["scripts"][0]["sourceExists"], json!(true));
    std::fs::remove_file(&pdf).unwrap();
    let v = listing(&root);
    assert_eq!(v["scripts"][0]["sourceExists"], json!(false));
    assert_eq!(v["scripts"][0]["source"], json!(pdf));
}

#[test]
fn a_folder_with_no_book_yet_answers_a_null_epub() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    let prefix = script(&root, "/scripts/Weatherglass.pdf", MOTH_HOUR, EVENING);
    assert_eq!(listing(&root)["scripts"][0]["epub"], Value::Null);
    write(&format!("{prefix}.epub"), "PK stand-in");
    assert_eq!(
        listing(&root)["scripts"][0]["epub"],
        json!(format!("{prefix}.epub"))
    );
}

#[test]
fn a_root_that_does_not_exist_yet_answers_no_scripts_and_is_not_made() {
    let s = Scratch::new("library-list");
    let root = s.join("not-yet");
    assert_eq!(listing(&root), json!({ "root": root, "scripts": [] }));
    assert!(!std::path::Path::new(&root).exists());
    // A file where the root should be is no library either.
    write(&root, "a file");
    assert_eq!(listing(&root)["scripts"], json!([]));
}

#[cfg(unix)]
#[test]
fn a_linked_folder_is_not_followed() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    let elsewhere = s.dir("elsewhere");
    script(&elsewhere, "/scripts/Weatherglass.pdf", MOTH_HOUR, EVENING);
    std::os::unix::fs::symlink(
        j(&[&elsewhere, "Weatherglass"]),
        j(&[&root, "Weatherglass"]),
    )
    .unwrap();
    assert_eq!(listing(&root)["scripts"], json!([]));
    // The same folder for real is listed.
    script(&root, "/scripts/Mudlark.pdf", MOTH_HOUR, EVENING);
    assert_eq!(names_listed(&listing(&root)), ["Mudlark"]);
}

#[test]
fn the_settings_file_is_named_when_there_is_one() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    let prefix = script(&root, "/scripts/Weatherglass.pdf", MOTH_HOUR, EVENING);
    assert_eq!(listing(&root)["scripts"][0]["settings"], Value::Null);
    // The 0.12 name is still the script's settings while the new one is
    // missing, as the settings verb reads it.
    let legacy = format!("{prefix}.screepub.json");
    write(&legacy, "{}\n");
    assert_eq!(listing(&root)["scripts"][0]["settings"], json!(legacy));
    let current = format!("{prefix}.method-reader.json");
    write(&current, "{}\n");
    assert_eq!(listing(&root)["scripts"][0]["settings"], json!(current));
}

#[test]
fn the_other_files_beside_the_book_are_listed_sorted() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    let prefix = script(&root, "/scripts/Weatherglass.pdf", MOTH_HOUR, EVENING);
    for ext in ["mobi", "fdx", "kfx", "azw3", "epub", "txt"] {
        write(&format!("{prefix}.{ext}"), "stand-in");
    }
    // Not the script's own name: not its book's.
    write(&j(&[&host::dirname(&prefix), "Other.azw3"]), "stand-in");
    assert_eq!(
        listing(&root)["scripts"][0]["others"],
        json!([
            format!("{prefix}.azw3"),
            format!("{prefix}.fdx"),
            format!("{prefix}.kfx"),
            format!("{prefix}.mobi"),
        ])
    );
}

#[test]
fn a_title_page_with_no_title_answers_null_and_no_markers_no_pages() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    script(
        &root,
        "/scripts/Weatherglass.pdf",
        "INT. BOATHOUSE - DAY\n\nRain on the roof.\n",
        EVENING,
    );
    let v = listing(&root);
    assert_eq!(v["scripts"][0]["title"], Value::Null);
    assert_eq!(v["scripts"][0]["pages"], Value::Null);
    assert_eq!(v["scripts"][0]["scenes"], json!(1));
}

#[test]
fn the_command_finds_the_root_as_a_conversion_does_and_writes_nothing() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    let prefix = script(&root, "/scripts/Weatherglass.pdf", MOTH_HOUR, EVENING);
    let folder = host::dirname(&prefix);
    let before = names(&folder);
    let marker = read(&j(&[&folder, "source.json"]));
    let mut env = guarded_env();
    env.set("METHOD_READER_LIBRARY", root.as_str());
    let platform = sp_engine::env::host_platform();
    let answer: LibraryListing = library_command(platform, &env, None);
    assert_eq!(answer.root, root);
    assert_eq!(answer.scripts.len(), 1);
    assert_eq!(answer.scripts[0].name, "Weatherglass");
    // Read only: no file added, source.json as it was, the root untouched.
    assert_eq!(names(&folder), before);
    assert_eq!(read(&j(&[&folder, "source.json"])), marker);
    assert_eq!(names(&root), ["Weatherglass"]);
}

#[test]
fn converted_is_the_fountains_modified_time_in_utc() {
    let s = Scratch::new("library-list");
    let root = s.dir("lib");
    let prefix = script(&root, "/scripts/Weatherglass.pdf", MOTH_HOUR, 0);
    assert_eq!(
        listing(&root)["scripts"][0]["converted"],
        json!("1970-01-01T00:00:00Z")
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    stamp(&format!("{prefix}.fountain"), now);
    let converted = listing(&root)["scripts"][0]["converted"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        converted.ends_with('Z') && converted.len() == 20,
        "{converted}"
    );
}

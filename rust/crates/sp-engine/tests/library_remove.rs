//! The `library-remove` verb's engine half: one script's folder off the
//! library, to the Trash
//! (docs/superpowers/specs/2026-11-03-library-remove-design.md). Every
//! test moves into a scratch stand-in Trash (METHOD_READER_TRASH); the
//! guard's own stand-in, under /dev/null, moves nothing. Every script here
//! is invented.

mod common;

use common::{Scratch, TRASH_GUARD, guarded_env, names, write};
use serde_json::json;
use sp_engine::commands::library::list_library;
use sp_engine::commands::library_remove::{library_remove_command, remove_from_library};
use sp_engine::env::Env;
use sp_engine::errors::EngineError;
use sp_engine::library::library_output;
use sp_engine::nodepath::host;

const WREN: &str = "\
Title: The Lamp Keeper

INT. LIGHTHOUSE - NIGHT

WREN
Morning already.
";

fn j(parts: &[&str]) -> String {
    host::join(parts)
}

/// A script folder the way a library conversion leaves it; answers the
/// folder.
fn script(root: &str, source: &str) -> String {
    let prefix = library_output(source, root).expect("a library folder");
    write(&format!("{prefix}.fountain"), WREN);
    write(&format!("{prefix}.epub"), "PK stand-in");
    host::dirname(&prefix)
}

/// The guarded environment with a scratch stand-in Trash, made.
fn with_trash(trash: &str) -> Env {
    if !trash.is_empty() {
        std::fs::create_dir_all(trash).expect("the stand-in Trash");
    }
    guarded_env().with("METHOD_READER_TRASH", trash)
}

fn refusal(err: EngineError) -> (String, String) {
    match err {
        EngineError::Cli(e) => (e.code.as_str().to_owned(), e.message),
        other => panic!("not a refusal: {other:?}"),
    }
}

fn listed(root: &str) -> Vec<String> {
    list_library(root)
        .scripts
        .into_iter()
        .map(|s| s.folder)
        .collect()
}

#[test]
fn a_script_goes_into_the_trash_whole_and_off_the_listing() {
    let s = Scratch::new("library-remove");
    let root = s.dir("lib");
    let trash = s.join("Trash");
    let folder = script(&root, "/scripts/The Lamp Keeper.pdf");
    let other = script(&root, "/scripts/Weatherglass.pdf");
    write(&j(&[&folder, "The Lamp Keeper.method-reader.json"]), "{}");

    let removed = remove_from_library(&root, &folder, &with_trash(&trash)).unwrap();
    assert_eq!(
        serde_json::to_value(&removed).unwrap(),
        json!({"removed": folder, "name": "The Lamp Keeper"})
    );
    assert!(!std::path::Path::new(&folder).exists());
    let moved = j(&[&trash, "The Lamp Keeper"]);
    assert_eq!(
        names(&moved),
        [
            "The Lamp Keeper.epub",
            "The Lamp Keeper.fountain",
            "The Lamp Keeper.method-reader.json",
            "source.json"
        ]
    );
    assert_eq!(listed(&root), [other]);
}

#[test]
fn a_name_already_in_the_trash_is_kept_and_the_next_is_numbered() {
    let s = Scratch::new("library-remove");
    let root = s.dir("lib");
    let trash = s.join("Trash");
    let env = with_trash(&trash);
    let first = script(&root, "/scripts/Weatherglass.pdf");
    remove_from_library(&root, &first, &env).unwrap();
    // Converted again, and removed again: both copies stay.
    let again = script(&root, "/scripts/Weatherglass.pdf");
    assert_eq!(again, first);
    remove_from_library(&root, &again, &env).unwrap();
    let third = script(&root, "/scripts/Weatherglass.pdf");
    remove_from_library(&root, &third, &env).unwrap();
    assert_eq!(
        names(&trash),
        ["Weatherglass", "Weatherglass 2", "Weatherglass 3"]
    );
    assert!(listed(&root).is_empty());
}

#[test]
fn the_guards_stand_in_moves_nothing() {
    let s = Scratch::new("library-remove");
    let root = s.dir("lib");
    let folder = script(&root, "/scripts/Anchorage.pdf");
    let env = guarded_env();
    assert_eq!(env.get("METHOD_READER_TRASH"), Some(TRASH_GUARD));
    let (code, message) = refusal(remove_from_library(&root, &folder, &env).unwrap_err());
    assert_eq!(code, "trash-failed");
    assert!(
        message.starts_with("could not move Anchorage to the Trash: "),
        "{message}"
    );
    assert!(message.contains(TRASH_GUARD), "{message}");
    assert!(!std::path::Path::new(TRASH_GUARD).exists());
    assert_eq!(listed(&root), std::slice::from_ref(&folder));
    // A stand-in that is not there yet is not made.
    let missing = s.join("No Trash");
    let (code, _) = refusal(
        remove_from_library(
            &root,
            &folder,
            &guarded_env().with("METHOD_READER_TRASH", &missing),
        )
        .unwrap_err(),
    );
    assert_eq!(code, "trash-failed");
    assert!(!std::path::Path::new(&missing).exists());
    // An empty stand-in is no Trash either.
    let (code, _) =
        refusal(remove_from_library(&root, &folder_of(&root), &with_trash("")).unwrap_err());
    assert_eq!(code, "trash-failed");
    assert_eq!(listed(&root).len(), 1);
}

fn folder_of(root: &str) -> String {
    listed(root).remove(0)
}

/// Each refusal: its code and reason, and nothing moved anywhere.
fn refused(root: &str, folder: &str, trash: &str) -> String {
    let before = names(root);
    let (code, message) =
        refusal(remove_from_library(root, folder, &with_trash(trash)).unwrap_err());
    assert_eq!(names(root), before, "{folder}");
    assert!(
        names(trash).is_empty(),
        "nothing reached the Trash for {folder}"
    );
    format!("{code}: {message}")
}

#[test]
fn anything_the_listing_would_not_list_is_refused_and_nothing_moves() {
    let s = Scratch::new("library-remove");
    let root = s.dir("lib");
    let trash = s.join("Trash");
    let kept = script(&root, "/scripts/Mudlark.pdf");
    // A stray folder in the root: a .fountain but no source.json.
    let loose = j(&[&root, "Loose"]);
    write(&j(&[&loose, "Loose.fountain"]), WREN);
    // A source.json whose .fountain is not there.
    let hollow = script(&root, "/scripts/Hollow.pdf");
    std::fs::remove_file(j(&[&hollow, "Hollow.fountain"])).unwrap();
    // A script folder one level too deep, and a file.
    let deep = script(&j(&[&root, "Shelf"]), "/scripts/Deep.pdf");
    let file = j(&[&root, "notes.txt"]);
    write(&file, "a note");
    let outside = script(&s.dir("elsewhere"), "/scripts/Outside.pdf");

    let not =
        |why: &str, f: &str| format!("not-a-script: not a script in the library ({why}): {f}");
    let unlisted = "it has no source.json naming its source, or no .fountain of that name";
    assert_eq!(refused(&root, &loose, &trash), not(unlisted, &loose));
    assert_eq!(refused(&root, &hollow, &trash), not(unlisted, &hollow));
    let inside = format!("it is not directly inside {root}");
    assert_eq!(refused(&root, &deep, &trash), not(&inside, &deep));
    assert_eq!(refused(&root, &outside, &trash), not(&inside, &outside));
    assert_eq!(
        refused(&root, &file, &trash),
        not("it is not a folder", &file)
    );
    assert_eq!(
        refused(&root, &root, &trash),
        not("it is the library folder itself", &root)
    );
    let missing = j(&[&root, "Nobody"]);
    assert_eq!(
        refused(&root, &missing, &trash),
        not("it is not there", &missing)
    );
    // `..` cannot climb out of the root.
    let climb = format!("{root}/Mudlark/../../elsewhere/Outside");
    assert!(refused(&root, &climb, &trash).starts_with("not-a-script: "));
    let up = format!("{root}/Mudlark/..");
    assert_eq!(
        refused(&root, &up, &trash),
        not("it is the library folder itself", &up)
    );
    assert_eq!(
        refused(&root, "Mudlark", &trash),
        "usage: library-remove needs a script folder's full path, not \"Mudlark\""
    );
    assert_eq!(listed(&root), [kept]);
}

#[cfg(unix)]
#[test]
fn a_link_is_refused_but_a_linked_root_still_reaches_its_own_scripts() {
    let s = Scratch::new("library-remove");
    let real_root = s.dir("lib");
    let trash = s.join("Trash");
    let elsewhere = script(&s.dir("elsewhere"), "/scripts/Outside.pdf");
    // A link in the root to a script folder outside it.
    let link = j(&[&real_root, "Outside"]);
    std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
    assert_eq!(
        refused(&real_root, &link, &trash),
        format!("not-a-script: not a script in the library (it is a link): {link}")
    );
    assert!(std::path::Path::new(&elsewhere).exists());

    // The root reached through a link: its scripts are its own.
    let linked_root = s.join("linked-lib");
    std::os::unix::fs::symlink(&real_root, &linked_root).unwrap();
    let folder = script(&real_root, "/scripts/Anchorage.pdf");
    let through = j(&[&linked_root, "Anchorage"]);
    let removed = remove_from_library(&linked_root, &through, &with_trash(&trash)).unwrap();
    assert_eq!(removed.name, "Anchorage");
    assert!(!std::path::Path::new(&folder).exists());
    assert_eq!(names(&trash), ["Anchorage"]);
}

#[test]
fn the_command_removes_from_the_library_a_conversion_lands_in() {
    let s = Scratch::new("library-remove");
    let root = s.dir("lib");
    let trash = s.join("Trash");
    let folder = script(&root, "/scripts/Weatherglass.pdf");
    let env = with_trash(&trash).with("METHOD_READER_LIBRARY", &root);
    let removed = library_remove_command("darwin", &env, None, &folder).unwrap();
    assert_eq!(removed.removed, folder);
    assert_eq!(names(&trash), ["Weatherglass"]);
}

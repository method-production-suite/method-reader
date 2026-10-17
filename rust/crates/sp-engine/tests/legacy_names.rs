//! The 0.12 names, Screepub's, still known: the two variables a person
//! might have set, the settings and library folders moved once to Method
//! Reader's, and the old-named sidecar beside a PDF. Every folder here is a
//! scratch HOME handed in as a value, never this machine's.

#![cfg(unix)]

mod common;

use common::{SETTINGS_GUARD, Scratch, env_of, read_json, write};
use serde_json::json;
use sp_engine::legacy::{LegacyMove, move_legacy_folders};
use sp_engine::library::{
    adopt_sidecar, env_library_override, library_output, library_root, platform_library_default,
};
use sp_engine::nodepath::posix;
use sp_engine::settings::app::{app_settings_path, read_app_settings, write_app_settings};

fn j(parts: &[&str]) -> String {
    posix::join(parts)
}

fn exists(p: &str) -> bool {
    std::path::Path::new(p).exists()
}

// ---- the two variables a person might have set ----

#[test]
fn the_old_config_dir_variable_is_read_when_the_new_one_is_unset() {
    let s = Scratch::new("legacy");
    let dir = s.dir("old-config");
    let env = env_of(&[("HOME", "/home/ada"), ("SCREEPUB_CONFIG_DIR", &dir)]);
    assert_eq!(
        app_settings_path("linux", &env),
        j(&[&dir, "settings.json"])
    );
}

#[test]
fn the_new_config_dir_variable_wins_over_the_old_one() {
    let s = Scratch::new("legacy");
    let new = s.dir("new-config");
    let old = s.dir("old-config");
    let env = env_of(&[
        ("HOME", "/home/ada"),
        ("METHOD_READER_CONFIG_DIR", &new),
        ("SCREEPUB_CONFIG_DIR", &old),
    ]);
    assert_eq!(
        app_settings_path("linux", &env),
        j(&[&new, "settings.json"])
    );
}

#[test]
fn the_old_library_variable_is_read_when_the_new_one_is_unset() {
    let env = env_of(&[
        ("HOME", "/home/ada"),
        ("METHOD_READER_CONFIG_DIR", SETTINGS_GUARD),
        ("SCREEPUB_LIBRARY", "/tmp/old-lib"),
    ]);
    assert_eq!(env_library_override(&env).as_deref(), Some("/tmp/old-lib"));
    assert_eq!(library_root("linux", &env, None), "/tmp/old-lib");
    let env = env_of(&[
        ("HOME", "/home/ada"),
        ("METHOD_READER_CONFIG_DIR", SETTINGS_GUARD),
        ("METHOD_READER_LIBRARY", "/tmp/new-lib"),
        ("SCREEPUB_LIBRARY", "/tmp/old-lib"),
    ]);
    assert_eq!(library_root("linux", &env, None), "/tmp/new-lib");
}

// ---- an old folder that has not moved is still used ----

#[test]
fn an_old_settings_folder_is_used_until_it_has_moved() {
    let s = Scratch::new("legacy");
    let home = s.dir("home");
    let env = env_of(&[("HOME", &home)]);
    let old = j(&[&home, "Library", "Application Support", "Screepub"]);
    std::fs::create_dir_all(&old).unwrap();
    assert_eq!(
        app_settings_path("darwin", &env),
        j(&[&old, "settings.json"])
    );
    let new = j(&[&home, "Library", "Application Support", "Method Reader"]);
    std::fs::create_dir_all(&new).unwrap();
    assert_eq!(
        app_settings_path("darwin", &env),
        j(&[&new, "settings.json"])
    );
}

#[test]
fn an_old_library_folder_is_used_until_it_has_moved() {
    let s = Scratch::new("legacy");
    let home = s.dir("home");
    let env = env_of(&[("HOME", &home)]);
    let old = j(&[&home, "Documents", "Screepub"]);
    std::fs::create_dir_all(&old).unwrap();
    assert_eq!(platform_library_default("darwin", &env), old);
    let new = j(&[&home, "Documents", "Method Reader"]);
    std::fs::create_dir_all(&new).unwrap();
    assert_eq!(platform_library_default("darwin", &env), new);
}

// ---- moving them once ----

#[test]
fn both_folders_move_once_and_paths_inside_follow() {
    let s = Scratch::new("legacy");
    let home = s.dir("home");
    let env = env_of(&[("HOME", &home)]);
    let old_settings = j(&[&home, ".config", "screepub"]);
    let old_library = j(&[&home, "Documents", "Screepub"]);
    write_app_settings(
        &vec![
            ("libraryPath".into(), Some(json!(old_library))),
            ("lastRoute".into(), Some(json!("books"))),
            (
                "window".into(),
                Some(json!({ "lastBook": j(&[&old_library, "Draft", "Draft.epub"]) })),
            ),
        ],
        &j(&[&old_settings, "settings.json"]),
    )
    .unwrap();
    let script = j(&[&old_library, "Draft", "source.json"]);
    write(
        &script,
        &format!(
            "{{\n  \"source\": {}\n}}\n",
            json!(j(&[&old_library, "pdfs", "Draft.pdf"]))
        ),
    );
    let outside = j(&[&old_library, "Other", "source.json"]);
    write(&outside, "{\n  \"source\": \"/elsewhere/Other.pdf\"\n}\n");

    let moved = move_legacy_folders("linux", &env);
    let new_settings = j(&[&home, ".config", "method-reader"]);
    let new_library = j(&[&home, "Documents", "Method Reader"]);
    assert_eq!(
        moved,
        LegacyMove {
            settings: Some((old_settings.clone(), new_settings.clone())),
            library: Some((old_library.clone(), new_library.clone())),
        }
    );
    assert!(!exists(&old_settings) && !exists(&old_library));
    let settings = read_app_settings(&j(&[&new_settings, "settings.json"]));
    assert_eq!(settings["libraryPath"], json!(new_library));
    assert_eq!(settings["lastRoute"], json!("books"));
    assert_eq!(
        settings["window"]["lastBook"],
        json!(j(&[&new_library, "Draft", "Draft.epub"]))
    );
    assert_eq!(
        read_json(&j(&[&new_library, "Draft", "source.json"]))["source"],
        json!(j(&[&new_library, "pdfs", "Draft.pdf"]))
    );
    assert_eq!(
        read_json(&j(&[&new_library, "Other", "source.json"]))["source"],
        json!("/elsewhere/Other.pdf")
    );
    assert_eq!(library_root("linux", &env, None), new_library);

    // Once: nothing is left to move, and nothing moves back.
    assert_eq!(move_legacy_folders("linux", &env), LegacyMove::default());
}

#[test]
fn nothing_moves_over_a_folder_already_there() {
    let s = Scratch::new("legacy");
    let home = s.dir("home");
    let env = env_of(&[("HOME", &home)]);
    let old = j(&[&home, "Library", "Application Support", "Screepub"]);
    let new = j(&[&home, "Library", "Application Support", "Method Reader"]);
    write(&j(&[&old, "settings.json"]), "{\"old\": true}\n");
    write(&j(&[&new, "settings.json"]), "{\"new\": true}\n");
    let old_lib = j(&[&home, "Documents", "Screepub"]);
    let new_lib = j(&[&home, "Documents", "Method Reader"]);
    write(&j(&[&old_lib, "a.txt"]), "old");
    write(&j(&[&new_lib, "b.txt"]), "new");
    assert_eq!(move_legacy_folders("darwin", &env), LegacyMove::default());
    assert!(exists(&j(&[&old, "settings.json"])));
    assert_eq!(
        read_json(&j(&[&new, "settings.json"])),
        json!({ "new": true })
    );
    assert!(exists(&j(&[&old_lib, "a.txt"])) && !exists(&j(&[&new_lib, "a.txt"])));
}

#[test]
fn a_library_folder_the_person_chose_is_left_where_it_is() {
    let s = Scratch::new("legacy");
    let home = s.dir("home");
    let env = env_of(&[("HOME", &home)]);
    let chosen = s.dir("books");
    write_app_settings(
        &vec![("libraryPath".into(), Some(json!(chosen)))],
        &j(&[
            &home,
            "Library",
            "Application Support",
            "Screepub",
            "settings.json",
        ]),
    )
    .unwrap();
    let old_lib = j(&[&home, "Documents", "Screepub"]);
    write(&j(&[&old_lib, "a.txt"]), "old");
    let moved = move_legacy_folders("darwin", &env);
    assert!(moved.settings.is_some());
    assert_eq!(moved.library, None);
    assert!(exists(&j(&[&old_lib, "a.txt"])));
    assert_eq!(library_root("darwin", &env, None), chosen);
}

#[test]
fn a_run_pointed_at_its_own_folders_moves_nothing() {
    let s = Scratch::new("legacy");
    let home = s.dir("home");
    let old = j(&[&home, ".config", "screepub"]);
    let old_lib = j(&[&home, "Documents", "Screepub"]);
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&old_lib).unwrap();
    for var in ["METHOD_READER_CONFIG_DIR", "SCREEPUB_CONFIG_DIR"] {
        let env = env_of(&[("HOME", &home), (var, SETTINGS_GUARD)]);
        assert_eq!(move_legacy_folders("linux", &env), LegacyMove::default());
    }
    for var in ["METHOD_READER_LIBRARY", "SCREEPUB_LIBRARY"] {
        let lib = s.dir("override");
        let env = env_of(&[("HOME", &home), (var, &lib)]);
        let moved = move_legacy_folders("linux", &env);
        assert_eq!(moved.library, None);
    }
    assert!(exists(&old_lib));
}

// ---- the old-named sidecar beside a PDF ----

#[test]
fn an_old_named_sidecar_beside_the_pdf_is_adopted_under_the_new_name() {
    let s = Scratch::new("legacy");
    let root = s.dir("lib");
    let scripts = s.dir("scripts");
    let pdf = j(&[&scripts, "Draft.pdf"]);
    write(&pdf, "not really a pdf");
    write(
        &j(&[&scripts, "Draft.screepub.json"]),
        r#"{"cueIndentPct": 41}"#,
    );
    let output = library_output(&pdf, &root).unwrap();
    assert!(adopt_sidecar(&pdf, &output).unwrap());
    assert_eq!(
        read_json(&format!("{output}.method-reader.json")),
        json!({ "cueIndentPct": 41 })
    );
    // The one beside the PDF is the person's, and stays.
    assert!(exists(&j(&[&scripts, "Draft.screepub.json"])));
}

#[test]
fn an_old_named_sidecar_in_the_library_is_never_overwritten() {
    let s = Scratch::new("legacy");
    let root = s.dir("lib");
    let scripts = s.dir("scripts");
    let pdf = j(&[&scripts, "Draft.pdf"]);
    write(&pdf, "not really a pdf");
    write(
        &j(&[&scripts, "Draft.method-reader.json"]),
        r#"{"cueIndentPct": 41}"#,
    );
    let output = library_output(&pdf, &root).unwrap();
    write(
        &format!("{output}.screepub.json"),
        r#"{"cueIndentPct": 12}"#,
    );
    assert!(!adopt_sidecar(&pdf, &output).unwrap());
    assert!(!exists(&format!("{output}.method-reader.json")));
}

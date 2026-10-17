//! Port of tests/app-settings.test.ts: where the app settings file is, that
//! reading it never fails, how a write merges, and the guard that keeps
//! tests off the real file. (The bun preload checks have their Rust form in
//! tests/guards.rs.)

#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

mod common;

use common::{Scratch, env_of, names, read_json, write};
use serde_json::{Value, json};
use sp_engine::settings::app::{app_settings_path, read_app_settings, write_app_settings};

const HOME: &str = "/home/ada";

fn obj(v: Value) -> serde_json::Map<String, Value> {
    v.as_object().cloned().expect("object")
}

fn set(pairs: &[(&str, Value)]) -> sp_engine::settings::app::Patch {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), Some(v.clone())))
        .collect()
}

// ---- where the app settings file is ----

#[test]
fn darwin_under_application_support() {
    assert_eq!(
        app_settings_path("darwin", &env_of(&[("HOME", HOME)])),
        "/home/ada/Library/Application Support/com.methodreader.desktop/settings.json"
    );
}

#[test]
fn win32_under_appdata_when_set() {
    assert_eq!(
        app_settings_path(
            "win32",
            &env_of(&[("APPDATA", "C:\\Users\\a\\AppData\\Roaming")])
        ),
        "C:\\Users\\a\\AppData\\Roaming\\com.methodreader.desktop\\settings.json"
    );
}

#[test]
fn win32_falls_back_to_userprofile_appdata_roaming() {
    assert_eq!(
        app_settings_path("win32", &env_of(&[("USERPROFILE", "C:\\Users\\a")])),
        "C:\\Users\\a\\AppData\\Roaming\\com.methodreader.desktop\\settings.json"
    );
}

#[test]
fn win32_blank_appdata_is_ignored() {
    assert_eq!(
        app_settings_path(
            "win32",
            &env_of(&[("USERPROFILE", "C:\\Users\\a"), ("APPDATA", "   ")])
        ),
        "C:\\Users\\a\\AppData\\Roaming\\com.methodreader.desktop\\settings.json"
    );
}

#[test]
fn linux_absolute_xdg_config_home_is_honoured() {
    assert_eq!(
        app_settings_path(
            "linux",
            &env_of(&[("HOME", HOME), ("XDG_CONFIG_HOME", "/home/ada/.xdgconfig")])
        ),
        "/home/ada/.xdgconfig/com.methodreader.desktop/settings.json"
    );
}

#[test]
fn linux_relative_xdg_config_home_is_ignored() {
    assert_eq!(
        app_settings_path(
            "linux",
            &env_of(&[("HOME", HOME), ("XDG_CONFIG_HOME", "not/absolute")])
        ),
        "/home/ada/.config/com.methodreader.desktop/settings.json"
    );
}

#[test]
fn linux_no_xdg_config_home_falls_back_to_dot_config() {
    assert_eq!(
        app_settings_path("linux", &env_of(&[("HOME", HOME)])),
        "/home/ada/.config/com.methodreader.desktop/settings.json"
    );
}

#[test]
fn method_reader_config_dir_wins_on_every_platform() {
    let scratch = Scratch::new("app-settings");
    for platform in ["darwin", "win32", "linux"] {
        let dir = scratch.join("config-dir");
        assert_eq!(
            app_settings_path(
                platform,
                &env_of(&[("HOME", HOME), ("METHOD_READER_CONFIG_DIR", &dir)])
            ),
            std::path::Path::new(&dir)
                .join("settings.json")
                .to_string_lossy()
        );
    }
}

#[test]
fn whitespace_only_config_dir_is_ignored() {
    assert_eq!(
        app_settings_path(
            "linux",
            &env_of(&[("HOME", HOME), ("METHOD_READER_CONFIG_DIR", "   ")])
        ),
        "/home/ada/.config/com.methodreader.desktop/settings.json"
    );
}

#[test]
fn relative_config_dir_is_ignored_not_resolved() {
    for relative in ["config", "./config", "../config", "."] {
        assert_eq!(
            app_settings_path(
                "linux",
                &env_of(&[("HOME", HOME), ("METHOD_READER_CONFIG_DIR", relative)])
            ),
            "/home/ada/.config/com.methodreader.desktop/settings.json"
        );
    }
}

#[test]
fn empty_config_dir_is_ignored() {
    assert_eq!(
        app_settings_path(
            "linux",
            &env_of(&[("HOME", HOME), ("METHOD_READER_CONFIG_DIR", "")])
        ),
        "/home/ada/.config/com.methodreader.desktop/settings.json"
    );
}

// ---- reading app settings never fails ----

#[test]
fn missing_file_reads_as_empty() {
    let s = Scratch::new("app-settings");
    assert!(read_app_settings(&s.join("read/settings.json")).is_empty());
}

#[test]
fn text_that_is_not_json_reads_as_empty() {
    let s = Scratch::new("app-settings");
    write(&s.join("read.json"), "not json");
    assert!(read_app_settings(&s.join("read.json")).is_empty());
}

#[test]
fn json_array_reads_as_empty() {
    let s = Scratch::new("app-settings");
    write(&s.join("read.json"), "[1,2]");
    assert!(read_app_settings(&s.join("read.json")).is_empty());
}

#[test]
fn json_null_reads_as_empty() {
    let s = Scratch::new("app-settings");
    write(&s.join("read.json"), "null");
    assert!(read_app_settings(&s.join("read.json")).is_empty());
}

#[test]
fn valid_object_reads_back_as_itself() {
    let s = Scratch::new("app-settings");
    write(
        &s.join("read.json"),
        r#"{"lastRoute":"apple-books","libraryPath":"/x"}"#,
    );
    assert_eq!(
        read_app_settings(&s.join("read.json")),
        obj(json!({ "lastRoute": "apple-books", "libraryPath": "/x" }))
    );
}

// ---- writing app settings ----

#[test]
fn write_creates_missing_parent_folders() {
    let s = Scratch::new("app-settings");
    let path = s.join("write/nested/deeper/settings.json");
    assert!(!std::path::Path::new(&path).exists());
    write_app_settings(&set(&[("lastRoute", json!("kindle"))]), &path).unwrap();
    assert!(std::path::Path::new(&path).exists());
    assert_eq!(
        read_app_settings(&path),
        obj(json!({ "lastRoute": "kindle" }))
    );
}

#[test]
fn write_merges_keeping_unknown_keys() {
    let s = Scratch::new("app-settings");
    let path = s.join("settings.json");
    write_app_settings(&set(&[("libraryPath", json!("x"))]), &path).unwrap();
    let result = write_app_settings(&set(&[("lastRoute", json!("apple-books"))]), &path).unwrap();
    let want = obj(json!({ "libraryPath": "x", "lastRoute": "apple-books" }));
    assert_eq!(result, want);
    assert_eq!(read_app_settings(&path), want);
}

#[test]
fn a_key_set_to_undefined_is_removed() {
    let s = Scratch::new("app-settings");
    let path = s.join("settings.json");
    write_app_settings(
        &set(&[("libraryPath", json!("x")), ("lastRoute", json!("kindle"))]),
        &path,
    )
    .unwrap();
    let result = write_app_settings(&vec![("lastRoute".into(), None)], &path).unwrap();
    assert_eq!(result, obj(json!({ "libraryPath": "x" })));
    assert_eq!(read_app_settings(&path), obj(json!({ "libraryPath": "x" })));
}

#[test]
fn write_leaves_no_temp_file_behind() {
    let s = Scratch::new("app-settings");
    let dir = s.dir("write");
    let path = format!("{dir}/settings.json");
    write_app_settings(&set(&[("lastRoute", json!("kindle"))]), &path).unwrap();
    assert_eq!(names(&dir), vec!["settings.json"]);
}

#[test]
fn a_write_that_cannot_land_leaves_no_temp_file_and_fails() {
    let s = Scratch::new("app-settings");
    let dir = s.dir("write");
    let path = format!("{dir}/settings.json");
    std::fs::create_dir_all(&path).unwrap();
    assert!(write_app_settings(&set(&[("lastRoute", json!("kindle"))]), &path).is_err());
    assert_eq!(names(&dir), vec!["settings.json"]);
}

#[test]
fn saving_over_a_corrupt_file_replaces_it_with_the_patch_alone() {
    let s = Scratch::new("app-settings");
    let path = s.join("settings.json");
    for corrupt in [r#"{"libraryPath": "/x", trunc"#, "[1,2]", "null", ""] {
        write(&path, corrupt);
        assert_eq!(
            write_app_settings(&set(&[("lastRoute", json!("kindle"))]), &path).unwrap(),
            obj(json!({ "lastRoute": "kindle" }))
        );
        assert_eq!(read_json(&path), json!({ "lastRoute": "kindle" }));
    }
}

#[cfg(unix)]
#[test]
fn saving_over_an_unreadable_file_replaces_it_too() {
    if common::is_root() {
        return;
    }
    let s = Scratch::new("app-settings");
    let path = s.join("settings.json");
    write(&path, r#"{"libraryPath":"/x"}"#);
    common::chmod(&path, 0o000);
    assert_eq!(
        write_app_settings(&set(&[("lastRoute", json!("kindle"))]), &path).unwrap(),
        obj(json!({ "lastRoute": "kindle" }))
    );
    assert_eq!(read_json(&path), json!({ "lastRoute": "kindle" }));
}

#[test]
fn write_returns_exactly_what_was_written() {
    let s = Scratch::new("app-settings");
    let path = s.join("settings.json");
    let result = write_app_settings(&set(&[("lastRoute", json!("kindle"))]), &path).unwrap();
    assert_eq!(Value::Object(result), read_json(&path));
}

// ---- byte compatibility with files the TypeScript app wrote ----

#[test]
fn the_written_file_is_json_stringify_two_spaces_and_a_newline() {
    let s = Scratch::new("app-settings");
    let path = s.join("settings.json");
    write_app_settings(
        &set(&[
            ("lastRoute", json!("kindle")),
            (
                "formatDefaults",
                json!({ "elementSpacingEm": 1.0, "cueIndentPct": 33.5 }),
            ),
        ]),
        &path,
    )
    .unwrap();
    assert_eq!(
        common::read(&path),
        "{\n  \"lastRoute\": \"kindle\",\n  \"formatDefaults\": {\n    \"elementSpacingEm\": 1,\n    \"cueIndentPct\": 33.5\n  }\n}\n"
    );
}

#[test]
fn an_untouched_key_is_written_back_byte_for_byte() {
    // A file as the TypeScript wrote it, with a key this build knows nothing
    // about, numbers JavaScript spells its own way, and integer-like keys,
    // which a JavaScript object lists first.
    let s = Scratch::new("app-settings");
    let path = s.join("settings.json");
    write(
        &path,
        "{\n  \"future\": {\n    \"b\": 1e+21,\n    \"a\": 0.000001,\n    \"2\": [],\n    \"1\": {}\n  },\n  \"lastRoute\": \"save-epub\"\n}\n",
    );
    write_app_settings(&set(&[("lastRoute", json!("apple-books"))]), &path).unwrap();
    assert_eq!(
        common::read(&path),
        "{\n  \"future\": {\n    \"1\": {},\n    \"2\": [],\n    \"b\": 1e+21,\n    \"a\": 0.000001\n  },\n  \"lastRoute\": \"apple-books\"\n}\n"
    );
}

// ---- the guard ----

#[cfg(unix)]
#[test]
fn a_write_through_the_guarded_path_fails_loudly() {
    let guarded = app_settings_path("linux", &common::guarded_env());
    assert_eq!(guarded, format!("{}/settings.json", common::SETTINGS_GUARD));
    assert!(write_app_settings(&vec![], &guarded).is_err());
    assert!(read_app_settings(&guarded).is_empty());
}

// ---- one process, many threads ----

#[test]
fn saves_from_many_threads_at_once_all_land_and_none_is_lost() {
    // The window runs every engine call as a thread of one process, so the
    // pid no longer kept two saves' temp files apart (one promoted a file
    // the other was still writing), and two read-merge-writes could each
    // drop the other's key.
    let s = Scratch::new("app-settings-threads");
    let path = s.join("settings.json");
    std::thread::scope(|scope| {
        for t in 0..8 {
            let path = &path;
            scope.spawn(move || {
                for i in 0..25 {
                    let key = format!("k{t}-{i}");
                    write_app_settings(&set(&[(key.as_str(), json!(i))]), path)
                        .expect("every save succeeds");
                }
            });
        }
    });
    let read = read_app_settings(&path);
    assert_eq!(read.len(), 8 * 25, "every key survives");
    assert_eq!(names(&s.str()), vec!["settings.json"]);
}

// ---- module boundaries ----

#[test]
fn the_module_never_imports_anything_from_the_window() {
    let source = common::read(
        &common::repo_root()
            .join("rust/crates/sp-engine/src/settings/app.rs")
            .to_string_lossy(),
    );
    assert!(!source.contains("sp_app"));
    assert!(!source.contains("desktop/"));
}

//! Port of tests/library.test.ts: where the library is, which folder a
//! script owns, tuning following the script in, and writes that die part
//! way. (The `--library` CLI block is the conversion's, wave 3.)

#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

mod common;

use common::{SETTINGS_GUARD, Scratch, env_of, names, read_json, write};
use serde_json::{Value, json};
use sp_engine::env::Env;
use sp_engine::fsx::testing::{Fault, fail_next};
use sp_engine::library::{adopt_sidecar, env_library_override, library_output, library_root};
use sp_engine::nodepath::{host, posix, win32};
use sp_engine::settings::app::write_app_settings;

/// `guardedRoot`: libraryRoot with the guard's METHOD_READER_CONFIG_DIR folded
/// in first, since a fake env REPLACES the real one.
fn guarded_root(platform: &str, pairs: &[(&str, &str)]) -> String {
    let mut env: Env = env_of(&[("METHOD_READER_CONFIG_DIR", SETTINGS_GUARD)]);
    for (k, v) in pairs {
        env.set(*k, *v);
    }
    library_root(platform, &env, None)
}

fn j(parts: &[&str]) -> String {
    host::join(parts)
}

fn store(path: &str, pairs: &[(&str, Value)]) {
    write_app_settings(
        &pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), Some(v.clone())))
            .collect(),
        path,
    )
    .unwrap();
}

#[test]
fn a_fake_home_that_is_real_and_writable_is_exactly_the_hole() {
    let s = Scratch::new("library");
    let home = s.dir("home");
    let settings = j(&[
        &home,
        "Library",
        "Application Support",
        "com.methodreader.desktop",
        "settings.json",
    ]);
    store(&settings, &[("libraryPath", json!("/poisoned"))]);
    assert_eq!(
        library_root("darwin", &env_of(&[("HOME", &home)]), None),
        "/poisoned"
    );
    // Joined by darwin's rules, whatever this host's separator is.
    assert_eq!(
        guarded_root("darwin", &[("HOME", &home)]),
        posix::join(&[home.as_str(), "Documents", "Method Reader"])
    );
}

// ---- where the library is ----

const HOME: &str = "/home/ada";

#[test]
fn the_library_is_under_documents_not_application_state() {
    assert_eq!(
        guarded_root("darwin", &[("HOME", HOME)]),
        "/home/ada/Documents/Method Reader"
    );
    assert_eq!(
        guarded_root("linux", &[("HOME", HOME)]),
        "/home/ada/Documents/Method Reader"
    );
    for platform in ["darwin", "win32", "linux"] {
        let root = guarded_root(platform, &[("HOME", HOME)]);
        assert!(!root.contains(".local"));
        assert!(!root.contains("Application Support"));
        assert!(!root.contains("AppData"));
    }
}

#[test]
fn windows_keeps_documents_under_the_profile() {
    assert_eq!(
        guarded_root("win32", &[("USERPROFILE", "C:\\Users\\Ada")]),
        "C:\\Users\\Ada\\Documents\\Method Reader"
    );
    assert_eq!(
        guarded_root("win32", &[("HOME", "C:\\Users\\Bo")]),
        "C:\\Users\\Bo\\Documents\\Method Reader"
    );
}

#[cfg(unix)]
#[test]
fn method_reader_library_wins_on_every_platform() {
    for platform in ["darwin", "win32", "linux"] {
        assert_eq!(
            guarded_root(
                platform,
                &[("HOME", HOME), ("METHOD_READER_LIBRARY", "/tmp/lib")]
            ),
            "/tmp/lib"
        );
    }
}

#[test]
fn a_relative_override_is_not_a_library() {
    for relative in ["lib", "./lib", "../lib", "."] {
        assert_eq!(
            guarded_root(
                "linux",
                &[("HOME", HOME), ("METHOD_READER_LIBRARY", relative)]
            ),
            "/home/ada/Documents/Method Reader"
        );
        assert_eq!(
            env_library_override(&env_of(&[("METHOD_READER_LIBRARY", relative)])),
            None
        );
    }
}

#[test]
fn a_blank_override_is_not_a_library() {
    assert_eq!(
        guarded_root("linux", &[("HOME", HOME), ("METHOD_READER_LIBRARY", "  ")]),
        "/home/ada/Documents/Method Reader"
    );
}

// ---- the Documents folder a Linux user actually has ----

fn home_with_user_dirs(s: &Scratch, name: &str, body: Option<&str>) -> String {
    let home = s.dir(name);
    if let Some(body) = body {
        write(&j(&[&home, ".config", "user-dirs.dirs"]), body);
    }
    home
}

#[cfg(unix)]
#[test]
fn a_renamed_documents_folder_is_honoured() {
    let s = Scratch::new("library");
    let home = home_with_user_dirs(
        &s,
        "home",
        Some(
            "# generated\nXDG_DESKTOP_DIR=\"$HOME/Bureaublad\"\nXDG_DOCUMENTS_DIR=\"$HOME/Documenten\"\n",
        ),
    );
    assert_eq!(
        guarded_root("linux", &[("HOME", &home)]),
        j(&[&home, "Documenten", "Method Reader"])
    );
}

#[cfg(unix)]
#[test]
fn an_absolute_path_in_the_file_is_taken_as_written() {
    let s = Scratch::new("library");
    let home = home_with_user_dirs(&s, "home", Some("XDG_DOCUMENTS_DIR=\"/mnt/work/docs\"\n"));
    assert_eq!(
        guarded_root("linux", &[("HOME", &home)]),
        "/mnt/work/docs/Method Reader"
    );
}

#[cfg(unix)]
#[test]
fn xdg_config_home_says_where_that_file_is() {
    let s = Scratch::new("library");
    let home = s.dir("home");
    let config = s.dir("config");
    write(
        &j(&[&config, "user-dirs.dirs"]),
        "XDG_DOCUMENTS_DIR=\"$HOME/Papers\"\n",
    );
    assert_eq!(
        guarded_root("linux", &[("HOME", &home), ("XDG_CONFIG_HOME", &config)]),
        j(&[&home, "Papers", "Method Reader"])
    );
}

#[cfg(unix)]
#[test]
fn the_environment_variable_wins_over_the_file() {
    let s = Scratch::new("library");
    let home = home_with_user_dirs(&s, "home", Some("XDG_DOCUMENTS_DIR=\"$HOME/Documenten\"\n"));
    assert_eq!(
        guarded_root(
            "linux",
            &[("HOME", &home), ("XDG_DOCUMENTS_DIR", "/srv/docs")]
        ),
        "/srv/docs/Method Reader"
    );
}

#[cfg(unix)]
#[test]
fn the_environment_branch_gets_every_guard_the_file_branch_gets() {
    let s = Scratch::new("library");
    for (i, value) in ["$HOME", "$HOME/", "/", "\"$HOME/Documenten\""]
        .iter()
        .enumerate()
    {
        let home = home_with_user_dirs(
            &s,
            &format!("home{i}"),
            Some(&format!("XDG_DOCUMENTS_DIR={value}\n")),
        );
        assert_eq!(
            guarded_root("linux", &[("HOME", &home), ("XDG_DOCUMENTS_DIR", value)]),
            guarded_root("linux", &[("HOME", &home)]),
            "{value}"
        );
    }
    let home = s.dir("plain");
    for value in ["$HOME", "$HOME/", "/"] {
        assert_eq!(
            guarded_root("linux", &[("HOME", &home), ("XDG_DOCUMENTS_DIR", value)]),
            j(&[&home, "Documents", "Method Reader"])
        );
    }
    assert_eq!(
        guarded_root(
            "linux",
            &[
                ("HOME", &home),
                ("XDG_DOCUMENTS_DIR", "\"$HOME/Documenten\"")
            ]
        ),
        j(&[&home, "Documenten", "Method Reader"])
    );
}

#[cfg(unix)]
#[test]
fn no_file_no_entry_or_documents_is_home_falls_back_to_documents() {
    let s = Scratch::new("library");
    let bare = home_with_user_dirs(&s, "bare", None);
    assert_eq!(
        guarded_root("linux", &[("HOME", &bare)]),
        j(&[&bare, "Documents", "Method Reader"])
    );
    assert!(!std::path::Path::new(&j(&[&bare, "Documents"])).exists());

    let no_entry = home_with_user_dirs(&s, "noentry", Some("XDG_MUSIC_DIR=\"$HOME/Music\"\n"));
    assert_eq!(
        guarded_root("linux", &[("HOME", &no_entry)]),
        j(&[&no_entry, "Documents", "Method Reader"])
    );

    let disabled = home_with_user_dirs(&s, "disabled", Some("XDG_DOCUMENTS_DIR=\"$HOME/\"\n"));
    assert_eq!(
        guarded_root("linux", &[("HOME", &disabled)]),
        j(&[&disabled, "Documents", "Method Reader"])
    );

    let relative = home_with_user_dirs(&s, "relative", Some("XDG_DOCUMENTS_DIR=\"Documenten\"\n"));
    assert_eq!(
        guarded_root("linux", &[("HOME", &relative)]),
        j(&[&relative, "Documents", "Method Reader"])
    );
}

#[cfg(unix)]
#[test]
fn macos_and_windows_do_not_read_the_file() {
    let s = Scratch::new("library");
    let home = home_with_user_dirs(&s, "home", Some("XDG_DOCUMENTS_DIR=\"$HOME/Documenten\"\n"));
    assert_eq!(
        guarded_root("darwin", &[("HOME", &home)]),
        j(&[&home, "Documents", "Method Reader"])
    );
    assert_eq!(
        guarded_root("win32", &[("HOME", &home)]),
        win32::join(&[home.as_str(), "Documents", "Method Reader"])
    );
}

// ---- the library folder the user chose in the app ----

fn settings_file_with(s: &Scratch, library_path: Value) -> String {
    let path = j(&[&s.dir("settings"), "settings.json"]);
    store(
        &path,
        &[
            ("lastRoute", json!("kindle")),
            ("libraryPath", library_path),
        ],
    );
    path
}

#[cfg(unix)]
#[test]
fn method_reader_library_beats_a_stored_folder() {
    let s = Scratch::new("library");
    let settings = settings_file_with(&s, json!("/chosen/lib"));
    assert_eq!(
        library_root(
            "linux",
            &env_of(&[("HOME", HOME), ("METHOD_READER_LIBRARY", "/env/lib")]),
            Some(&settings)
        ),
        "/env/lib"
    );
}

#[test]
fn a_stored_absolute_folder_beats_the_default_on_darwin() {
    let s = Scratch::new("library");
    let settings = settings_file_with(&s, json!("/Volumes/Scripts"));
    assert_eq!(
        library_root("darwin", &env_of(&[("HOME", HOME)]), Some(&settings)),
        "/Volumes/Scripts"
    );
}

#[test]
fn a_stored_absolute_folder_beats_the_default_on_win32() {
    let s = Scratch::new("library");
    let settings = settings_file_with(&s, json!("C:\\Books"));
    assert_eq!(
        library_root(
            "win32",
            &env_of(&[("USERPROFILE", "C:\\Users\\Ada")]),
            Some(&settings)
        ),
        "C:\\Books"
    );
}

#[test]
fn a_stored_path_is_resolved_trailing_slash() {
    let s = Scratch::new("library");
    let settings = settings_file_with(&s, json!("/Volumes/Scripts/"));
    assert_eq!(
        library_root("darwin", &env_of(&[("HOME", HOME)]), Some(&settings)),
        "/Volumes/Scripts"
    );
}

#[test]
fn a_stored_path_is_resolved_dot_dot() {
    let s = Scratch::new("library");
    let settings = settings_file_with(&s, json!("/a/../b"));
    assert_eq!(
        library_root("linux", &env_of(&[("HOME", HOME)]), Some(&settings)),
        "/b"
    );
}

#[test]
fn a_stored_path_is_resolved_win32() {
    let s = Scratch::new("library");
    let settings = settings_file_with(&s, json!("C:\\Books\\"));
    assert_eq!(
        library_root(
            "win32",
            &env_of(&[("USERPROFILE", "C:\\Users\\Ada")]),
            Some(&settings)
        ),
        "C:\\Books"
    );
}

#[test]
fn unusable_stored_values_fall_through_to_the_default() {
    for stored in [json!("Scripts"), json!(7), json!(""), json!("   ")] {
        let s = Scratch::new("library");
        let settings = settings_file_with(&s, stored.clone());
        assert_eq!(
            library_root("linux", &env_of(&[("HOME", HOME)]), Some(&settings)),
            "/home/ada/Documents/Method Reader",
            "{stored}"
        );
    }
}

#[test]
fn a_missing_settings_file_falls_through() {
    let s = Scratch::new("library");
    let settings = j(&[&s.dir("settings"), "never-written", "settings.json"]);
    assert_eq!(
        library_root("linux", &env_of(&[("HOME", HOME)]), Some(&settings)),
        "/home/ada/Documents/Method Reader"
    );
}

#[test]
fn a_corrupt_settings_file_falls_through() {
    let s = Scratch::new("library");
    let path = j(&[&s.dir("settings"), "settings.json"]);
    write(&path, "this is not json");
    assert_eq!(
        library_root("linux", &env_of(&[("HOME", HOME)]), Some(&path)),
        "/home/ada/Documents/Method Reader"
    );
}

#[test]
fn library_output_lands_under_the_stored_folder() {
    let s = Scratch::new("library");
    let chosen = s.dir("chosen-library");
    let settings = settings_file_with(&s, json!(chosen));
    let scripts = s.dir("scripts");
    let pdf = j(&[&scripts, "Bright Angel.pdf"]);
    write(&pdf, "not really a pdf");
    let platform = sp_engine::env::host_platform();
    let root = library_root(platform, &env_of(&[("HOME", HOME)]), Some(&settings));
    assert_eq!(root, chosen);
    let output = library_output(&pdf, &root).unwrap();
    assert_eq!(output, j(&[&chosen, "Bright Angel", "Bright Angel"]));
    assert!(std::path::Path::new(&j(&[&chosen, "Bright Angel", "source.json"])).exists());
}

#[cfg(unix)]
#[test]
fn production_shape_config_dir_in_the_env_is_honoured() {
    let s = Scratch::new("library");
    let config = s.dir("config");
    let home = s.dir("home");
    store(
        &j(&[&config, "settings.json"]),
        &[("libraryPath", json!("/chosen/via-config-dir"))],
    );
    assert_eq!(
        library_root(
            "linux",
            &env_of(&[("HOME", &home), ("METHOD_READER_CONFIG_DIR", &config)]),
            None
        ),
        "/chosen/via-config-dir"
    );
}

#[test]
fn production_shape_darwin_derives_the_settings_path_from_home() {
    let s = Scratch::new("library");
    let home = s.dir("home");
    store(
        &j(&[
            &home,
            "Library",
            "Application Support",
            "com.methodreader.desktop",
            "settings.json",
        ]),
        &[("libraryPath", json!("/chosen/via-home"))],
    );
    assert_eq!(
        library_root("darwin", &env_of(&[("HOME", &home)]), None),
        "/chosen/via-home"
    );
}

// ---- which folder a script owns ----

fn parent_name(path: &str) -> String {
    host::basename(&host::dirname(path))
}

fn is_hashed(name: &str, stem: &str, width: usize) -> bool {
    name.strip_prefix(&format!("{stem}-")).is_some_and(|h| {
        h.len() == width
            && h.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}

#[cfg(unix)]
#[test]
fn one_folder_per_script_named_for_it() {
    let s = Scratch::new("library");
    let root = s.dir("lib");
    let output = library_output("/scripts/Bright Angel.pdf", &root).unwrap();
    assert_eq!(output, j(&[&root, "Bright Angel", "Bright Angel"]));
    assert!(std::path::Path::new(&j(&[&root, "Bright Angel"])).exists());
}

#[cfg(unix)]
#[test]
fn the_same_script_always_comes_back_to_the_same_folder() {
    let s = Scratch::new("library");
    let root = s.dir("lib");
    let first = library_output("/scripts/Draft.pdf", &root).unwrap();
    assert_eq!(library_output("/scripts/Draft.pdf", &root).unwrap(), first);
    assert_eq!(
        library_output("/scripts/../scripts/Draft.pdf", &root).unwrap(),
        first
    );
}

#[cfg(unix)]
#[test]
fn two_scripts_called_the_same_thing_do_not_share_a_folder() {
    let s = Scratch::new("library");
    let root = s.dir("lib");
    let mine = library_output("/producers/alice/Draft.pdf", &root).unwrap();
    let theirs = library_output("/producers/bob/Draft.pdf", &root).unwrap();
    assert_ne!(theirs, mine);
    assert_eq!(mine, j(&[&root, "Draft", "Draft"]));
    assert!(is_hashed(&parent_name(&theirs), "Draft", 8));
    assert_eq!(
        library_output("/producers/bob/Draft.pdf", &root).unwrap(),
        theirs
    );
    assert_eq!(
        library_output("/producers/alice/Draft.pdf", &root).unwrap(),
        mine
    );
}

#[cfg(unix)]
#[test]
fn the_hashed_name_is_not_a_free_for_all() {
    let s = Scratch::new("library");
    let probe = s.dir("probe");
    library_output("/x/Draft.pdf", &probe).unwrap();
    let hashed = parent_name(&library_output("/producers/bob/Draft.pdf", &probe).unwrap());
    assert!(is_hashed(&hashed, "Draft", 8));

    let root = s.dir("lib");
    library_output("/x/Draft.pdf", &root).unwrap();
    let decoy = library_output(&format!("/a/{hashed}.pdf"), &root).unwrap();
    assert_eq!(host::dirname(&decoy), j(&[&root, &hashed]));

    let intruder = library_output("/producers/bob/Draft.pdf", &root).unwrap();
    assert_ne!(host::dirname(&intruder), j(&[&root, &hashed]));
    assert!(is_hashed(&parent_name(&intruder), "Draft", 12));
    assert_eq!(
        read_json(&j(&[&root, &hashed, "source.json"])),
        json!({ "source": format!("/a/{hashed}.pdf") })
    );
    assert_eq!(
        library_output(&format!("/a/{hashed}.pdf"), &root).unwrap(),
        decoy
    );
    assert_eq!(
        library_output("/producers/bob/Draft.pdf", &root).unwrap(),
        intruder
    );
}

#[cfg(unix)]
#[test]
fn a_file_standing_where_the_folder_would_go_is_not_clobbered() {
    let s = Scratch::new("library");
    let root = s.dir("lib");
    write(&j(&[&root, "Draft.epub"]), "the Mac app’s book");
    let output = library_output("/scripts/Draft.epub.pdf", &root).unwrap();
    assert!(is_hashed(&parent_name(&output), "Draft.epub", 8));
    assert_eq!(
        common::read(&j(&[&root, "Draft.epub"])),
        "the Mac app’s book"
    );
}

#[cfg(unix)]
#[test]
fn a_folder_the_engine_did_not_make_is_left_alone() {
    let s = Scratch::new("library");
    let root = s.dir("lib");
    write(&j(&[&root, "Notes", "mine.txt"]), "not the library’s");
    let output = library_output("/scripts/Notes.pdf", &root).unwrap();
    assert_ne!(host::dirname(&output), j(&[&root, "Notes"]));
    assert_eq!(names(&j(&[&root, "Notes"])), vec!["mine.txt"]);
}

#[test]
fn the_marker_is_json_stringify_two_spaces_and_a_newline() {
    let s = Scratch::new("library");
    let root = s.dir("lib");
    let scripts = s.dir("scripts");
    // Windows forbids `"` in a name; there the path's own backslashes are
    // what the marker must escape.
    let name = if cfg!(windows) {
        "Quote 'Me'.pdf"
    } else {
        "Quote \"Me\".pdf"
    };
    let pdf = j(&[&scripts, name]);
    write(&pdf, "x");
    let output = library_output(&pdf, &root).unwrap();
    let marker = common::read(&j(&[&host::dirname(&output), "source.json"]));
    let mut quoted = String::new();
    sp_engine::jsjson::write_string(&mut quoted, &pdf);
    assert!(quoted.contains('\\'), "an escape to check: {quoted}");
    assert_eq!(marker, format!("{{\n  \"source\": {quoted}\n}}\n"));
}

// ---- tuning follows the script into the library ----

#[test]
fn a_sidecar_beside_the_pdf_is_adopted_on_the_first_conversion() {
    let s = Scratch::new("library");
    let root = s.dir("lib");
    let scripts = s.dir("scripts");
    let pdf = j(&[&scripts, "Draft.pdf"]);
    write(&pdf, "not really a pdf");
    write(
        &j(&[&scripts, "Draft.method-reader.json"]),
        r#"{"cueIndentPct": 41}"#,
    );
    let output = library_output(&pdf, &root).unwrap();
    assert!(adopt_sidecar(&pdf, &output).unwrap());
    assert_eq!(
        read_json(&format!("{output}.method-reader.json")),
        json!({ "cueIndentPct": 41 })
    );
}

#[test]
fn tuning_already_in_the_library_is_never_overwritten() {
    let s = Scratch::new("library");
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
        &format!("{output}.method-reader.json"),
        r#"{"cueIndentPct": 12}"#,
    );
    assert!(!adopt_sidecar(&pdf, &output).unwrap());
    assert_eq!(
        read_json(&format!("{output}.method-reader.json")),
        json!({ "cueIndentPct": 12 })
    );
}

#[test]
fn nothing_to_adopt_is_not_a_failure() {
    let s = Scratch::new("library");
    let root = s.dir("lib");
    let scripts = s.dir("scripts");
    let pdf = j(&[&scripts, "Draft.pdf"]);
    write(&pdf, "not really a pdf");
    let output = library_output(&pdf, &root).unwrap();
    assert!(!adopt_sidecar(&pdf, &output).unwrap());
    assert!(!std::path::Path::new(&format!("{output}.method-reader.json")).exists());
}

// ---- a write that dies part way leaves nothing half written ----

#[test]
fn a_failed_marker_leaves_no_folder_and_the_retry_gets_the_plain_one() {
    let s = Scratch::new("library");
    let root = s.dir("lib");
    let scripts = s.dir("scripts");
    let pdf = j(&[&scripts, "Draft.pdf"]);
    write(&pdf, "not really a pdf");
    fail_next(Fault::Write);
    let err = library_output(&pdf, &root).unwrap_err();
    assert!(err.message().contains("ENOSPC"));
    assert!(names(&root).is_empty());
    assert_eq!(
        library_output(&pdf, &root).unwrap(),
        j(&[&root, "Draft", "Draft"])
    );
}

#[test]
fn a_failed_adoption_leaves_nothing_and_the_next_one_adopts() {
    let s = Scratch::new("library");
    let root = s.dir("lib");
    let scripts = s.dir("scripts");
    let pdf = j(&[&scripts, "Draft.pdf"]);
    write(&pdf, "not really a pdf");
    write(
        &j(&[&scripts, "Draft.method-reader.json"]),
        r#"{"cueIndentPct": 41}"#,
    );
    let output = library_output(&pdf, &root).unwrap();
    fail_next(Fault::Copy);
    let err = adopt_sidecar(&pdf, &output).unwrap_err();
    assert!(err.message().contains("ENOSPC"));
    assert_eq!(names(&host::dirname(&output)), vec!["source.json"]);
    assert!(adopt_sidecar(&pdf, &output).unwrap());
    assert_eq!(
        read_json(&format!("{output}.method-reader.json")),
        json!({ "cueIndentPct": 41 })
    );
}

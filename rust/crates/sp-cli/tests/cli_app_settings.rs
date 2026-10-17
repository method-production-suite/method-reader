//! Port of tests/cli-app-settings.test.ts: the `app-settings` verb. It reads
//! and writes `libraryPath`, `formatDefaults`, `keepScriptSettings` and the
//! window's three remembered choices in the shared settings file. Most of
//! this file drives the handler IN PROCESS with an injected settings path,
//! platform and environment, so it never touches a real settings file; the
//! last block spawns the real CLI in a Sandbox.

#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

mod common;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sp_engine::commands::app_settings::{
    AppSettingsDeps, AppSettingsOptions, AppSettingsResult, app_settings_command,
};
use sp_engine::env::Env;
use sp_engine::errors::EngineError;
use sp_engine::settings::app::{READER_DEVICES, read_app_settings, write_app_settings};
use sp_model::DEFAULT_FORMAT_OPTIONS;

use common::{Sandbox, repo_root};

// This HOME does not exist: nothing here may depend on, or touch, a real
// home directory.
// The handler is driven as THIS host's platform, because the folders it
// checks and makes are real ones here, and a Windows folder is not a full
// path by Linux's rules.
#[cfg(not(windows))]
const PLATFORM: &str = "linux";
#[cfg(not(windows))]
const HOME: &str = "/home/ada";
#[cfg(not(windows))]
const HOME_LIBRARY: &str = "/home/ada/Documents/Method Reader";
#[cfg(not(windows))]
const ENV_LIBRARY: &str = "/env/lib";
#[cfg(windows)]
const PLATFORM: &str = "win32";
#[cfg(windows)]
const HOME: &str = r"C:\home\ada";
#[cfg(windows)]
const HOME_LIBRARY: &str = r"C:\home\ada\Documents\Method Reader";
#[cfg(windows)]
const ENV_LIBRARY: &str = r"C:\env\lib";

/// `name` inside the folder `dir`, as the host writes it.
fn under(dir: &str, name: &str) -> String {
    s(&Path::new(dir).join(name))
}

/// A scratch folder whose subfolders are numbered, so every test gets its
/// own. Locked folders are unlocked again before it is removed.
struct Scratch {
    _dir: tempfile::TempDir,
    root: PathBuf,
    counter: std::cell::Cell<usize>,
}

impl Scratch {
    fn new() -> Scratch {
        let dir = tempfile::Builder::new()
            .prefix("method-reader-cli-app-settings-")
            .tempdir()
            .expect("a scratch folder");
        let root = common::canonical(dir.path());
        Scratch {
            _dir: dir,
            root,
            counter: std::cell::Cell::new(0),
        }
    }

    fn dir(&self, name: &str) -> String {
        let n = self.counter.get();
        self.counter.set(n + 1);
        let d = self.root.join(format!("{name}-{n}"));
        std::fs::create_dir_all(&d).expect("mkdir");
        s(&d)
    }

    /// A settings.json path inside its own fresh folder, unwritten.
    fn settings_file(&self) -> String {
        under(&self.dir("settings"), "settings.json")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            use std::os::unix::fs::PermissionsExt;
            for e in entries.flatten() {
                let _ = std::fs::set_permissions(e.path(), std::fs::Permissions::from_mode(0o700));
            }
        }
    }
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn env_with(extra: &[(&str, &str)]) -> Env {
    let mut env: Env = [("HOME", HOME)].into_iter().collect();
    for (k, v) in extra {
        env.set(*k, *v);
    }
    env
}

fn deps_on(file: &str, platform: &str, env: Env) -> AppSettingsDeps {
    AppSettingsDeps {
        settings_path: Some(file.to_owned()),
        platform: Some(platform.to_owned()),
        env: Some(env),
    }
}

fn deps(file: &str) -> AppSettingsDeps {
    deps_on(file, PLATFORM, env_with(&[]))
}

fn run(set: Option<&str>, deps: &AppSettingsDeps) -> Result<AppSettingsResult, EngineError> {
    app_settings_command(
        &AppSettingsOptions {
            set: set.map(str::to_owned),
        },
        deps,
    )
}

fn read(file: &str) -> AppSettingsResult {
    run(None, &deps(file)).expect("a plain read")
}

fn set(file: &str, patch: &str) -> AppSettingsResult {
    run(Some(patch), &deps(file)).unwrap_or_else(|e| panic!("{patch}: {e}"))
}

/// The refusal a `--set` must produce.
fn refused(file: &str, patch: &str) -> EngineError {
    match run(Some(patch), &deps(file)) {
        Ok(_) => panic!("{patch} was accepted"),
        Err(e) => e,
    }
}

fn code_of(err: &EngineError) -> &'static str {
    match err {
        EngineError::Cli(e) => e.code.as_str(),
        _ => "",
    }
}

fn exists(p: &str) -> bool {
    Path::new(p).exists()
}

fn option_keys() -> Vec<String> {
    let mut k: Vec<String> = serde_json::to_value(DEFAULT_FORMAT_OPTIONS)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    k.sort();
    k
}

fn sorted_keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().expect("object").keys().cloned().collect();
    k.sort();
    k
}

#[cfg(unix)]
fn lock(dir: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o500)).expect("chmod");
}

/// Root bypasses ordinary permission bits.
#[cfg(unix)]
fn is_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
}

// ---- a plain read ----

#[test]
fn no_settings_file_shipped_defaults_no_chosen_folder_not_customized() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let result = read(&file);
    assert_eq!(result.file, file);
    assert_eq!(result.library.chosen, None);
    assert_eq!(result.library.path, HOME_LIBRARY);
    assert_eq!(result.library.platform_default, result.library.path);
    assert!(!result.library.from_env);
    assert_eq!(result.home, HOME);
    assert_eq!(result.format_defaults, DEFAULT_FORMAT_OPTIONS);
    assert_eq!(result.shipped_defaults, DEFAULT_FORMAT_OPTIONS);
    assert!(!result.customized);
    // A read must not create the file it reads.
    assert!(!exists(&file));
}

// ---- setting and resetting libraryPath ----

#[test]
fn a_good_absolute_folder_is_stored_created_and_reported_back_as_chosen() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let chosen = under(&sc.dir("chosen-lib"), "not-yet-made");
    let result = set(&file, &json!({ "libraryPath": chosen }).to_string());
    assert_eq!(result.library.chosen.as_deref(), Some(chosen.as_str()));
    assert_eq!(result.library.path, chosen);
    assert_eq!(read_app_settings(&file)["libraryPath"], json!(chosen));
    // The check IS making the folder, per the spec.
    assert!(exists(&chosen));
}

#[test]
fn null_resets_library_path_the_key_is_removed_not_written_as_null() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let chosen = sc.dir("chosen-lib");
    set(&file, &json!({ "libraryPath": chosen }).to_string());

    let result = set(&file, r#"{"libraryPath":null}"#);
    assert_eq!(result.library.chosen, None);
    assert_eq!(result.library.path, HOME_LIBRARY);
    assert!(!read_app_settings(&file).contains_key("libraryPath"));
}

// ---- setting and resetting formatDefaults ----

#[test]
fn an_object_is_stored_as_the_full_resolved_options_clamped() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    // 999 is well outside dialogueSideMarginPct's 0-30 range, so a clamped
    // 30 in the answer proves the options were resolved, not echoed.
    let result = set(&file, r#"{"formatDefaults":{"dialogueSideMarginPct":999}}"#);
    assert_eq!(result.format_defaults.dialogue_side_margin_pct, 30.0);
    assert_eq!(
        sorted_keys(&serde_json::to_value(result.format_defaults).unwrap()),
        option_keys()
    );
    assert!(result.customized);

    let on_disk = read_app_settings(&file)["formatDefaults"].clone();
    assert_eq!(on_disk["dialogueSideMarginPct"], json!(30));
    // The FULL object landed on disk, not the one key the caller sent.
    assert_eq!(sorted_keys(&on_disk), option_keys());
}

#[test]
fn null_resets_format_defaults_customized_goes_back_to_false() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    set(&file, r#"{"formatDefaults":{"justifyText":true}}"#);
    let result = set(&file, r#"{"formatDefaults":null}"#);
    assert_eq!(result.format_defaults, DEFAULT_FORMAT_OPTIONS);
    assert!(!result.customized);
    assert!(!read_app_settings(&file).contains_key("formatDefaults"));
}

// ---- refusals before anything is written ----

#[test]
fn an_unknown_key_is_usage_naming_the_allowed_keys_and_writes_nothing() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let err = refused(&file, r#"{"theme":"dark"}"#);
    assert_eq!(code_of(&err), "usage");
    assert!(err.message().contains("libraryPath"));
    assert!(err.message().contains("formatDefaults"));
    assert!(!exists(&file));
}

#[test]
fn last_route_is_refused_here_it_is_not_writable_through_this_verb() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let err = refused(&file, r#"{"lastRoute":"kindle"}"#);
    assert_eq!(code_of(&err), "usage");
    assert!(!exists(&file));
}

#[test]
fn set_that_is_not_valid_json_is_bad_settings_nothing_written() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    assert_eq!(code_of(&refused(&file, "{oops")), "bad-settings");
    assert!(!exists(&file));
}

#[test]
fn set_as_a_json_array_is_bad_settings_not_silently_accepted() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    assert_eq!(code_of(&refused(&file, "[1,2,3]")), "bad-settings");
    assert!(!exists(&file));
}

#[test]
fn library_path_that_is_not_a_string_or_null_is_bad_settings() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    assert_eq!(
        code_of(&refused(&file, r#"{"libraryPath":7}"#)),
        "bad-settings"
    );
    assert!(!exists(&file));
}

#[test]
fn format_defaults_that_is_not_an_object_or_null_is_bad_settings() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    assert_eq!(
        code_of(&refused(&file, r#"{"formatDefaults":"kindleEink"}"#)),
        "bad-settings"
    );
    assert!(!exists(&file));
}

#[test]
fn a_relative_library_path_is_refused_and_nothing_is_written() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let err = refused(&file, r#"{"libraryPath":"Scripts"}"#);
    assert_eq!(code_of(&err), "bad-settings");
    assert!(err.message().contains("full path"));
    assert!(!exists(&file));
}

#[test]
fn a_path_through_a_regular_file_is_refused_not_a_stack_trace() {
    let sc = Scratch::new();
    let blocker = under(&sc.dir("blocker"), "not-a-dir");
    std::fs::write(&blocker, "x").unwrap();
    let target = under(&blocker, "library");
    let file = sc.settings_file();
    let err = refused(&file, &json!({ "libraryPath": target }).to_string());
    assert_eq!(code_of(&err), "bad-settings");
    assert!(!exists(&file));
}

#[test]
fn a_regular_file_at_the_exact_library_path_is_refused_in_plain_words_and_a_good_format_defaults_alongside_is_not_stored()
 {
    // formatDefaults here is GOOD: this failure happens in phase 2, the
    // folder probe, after formatDefaults has already been resolved.
    let sc = Scratch::new();
    let already = under(&sc.dir("eexist"), "already-a-file");
    std::fs::write(&already, "x").unwrap();
    let file = sc.settings_file();
    let err = refused(
        &file,
        &json!({ "libraryPath": already, "formatDefaults": { "justifyText": true } }).to_string(),
    );
    assert_eq!(code_of(&err), "bad-settings");
    assert!(
        err.message().contains("that is a file, not a folder"),
        "{}",
        err.message()
    );
    assert!(!err.message().contains("EEXIST"));
    assert!(!exists(&file));
}

#[cfg(unix)]
#[test]
fn an_unwritable_folder_is_refused_and_nothing_is_written() {
    if is_root() {
        return;
    }
    let sc = Scratch::new();
    let locked = sc.dir("locked");
    lock(&locked);
    let target = under(&locked, "child");
    let file = sc.settings_file();
    let err = refused(&file, &json!({ "libraryPath": target }).to_string());
    assert_eq!(code_of(&err), "bad-settings");
    assert!(
        err.message().contains("permission denied"),
        "{}",
        err.message()
    );
    // The mapped phrase, not the raw code.
    assert!(!err.message().contains("EACCES"));
    assert!(!exists(&file));
}

#[cfg(unix)]
#[test]
fn the_write_permission_check_itself_refuses_an_existing_unwritable_folder() {
    if is_root() {
        return;
    }
    let sc = Scratch::new();
    let locked = sc.dir("locked-existing");
    lock(&locked);
    let file = sc.settings_file();
    let err = refused(&file, &json!({ "libraryPath": locked }).to_string());
    assert_eq!(code_of(&err), "bad-settings");
    assert!(
        err.message().contains("permission denied"),
        "{}",
        err.message()
    );
    assert!(!err.message().contains("EACCES"));
    assert!(!exists(&file));
}

#[test]
fn good_format_defaults_alongside_a_bad_library_path_stores_neither() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let err = refused(
        &file,
        r#"{"formatDefaults":{"justifyText":true},"libraryPath":"relative"}"#,
    );
    assert_eq!(code_of(&err), "bad-settings");
    assert!(!exists(&file));
}

#[test]
fn a_good_new_library_path_alongside_a_bad_format_defaults_creates_no_folder_stores_neither() {
    // Every pure check runs for BOTH keys before either one touches disk.
    let sc = Scratch::new();
    let file = sc.settings_file();
    let new_folder = under(&sc.dir("would-be-lib"), "not-yet-made");
    let err = refused(
        &file,
        &json!({ "libraryPath": new_folder, "formatDefaults": "nope" }).to_string(),
    );
    assert_eq!(code_of(&err), "bad-settings");
    assert!(!exists(&file));
    assert!(!exists(&new_folder));
}

#[test]
fn set_with_an_empty_object_changes_nothing_it_answers_like_a_plain_read_and_writes_nothing() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let result = set(&file, "{}");
    assert_eq!(result.library.chosen, None);
    assert!(!result.customized);
    assert!(!exists(&file));
}

#[test]
fn set_with_an_empty_object_does_not_touch_an_existing_file_even_a_corrupt_one() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    std::fs::write(&file, "not json at all").unwrap();
    set(&file, "{}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "not json at all");
}

#[cfg(unix)]
#[test]
fn a_settings_file_that_cannot_be_saved_is_bad_settings_not_a_raw_crash_and_names_the_file() {
    if is_root() {
        return;
    }
    let sc = Scratch::new();
    let locked = sc.dir("locked-config");
    lock(&locked);
    let file = under(&locked, "settings.json");
    // libraryPath: null is a pure reset, so the ONLY thing that can fail is
    // the settings write itself.
    let err = refused(&file, r#"{"libraryPath":null}"#);
    assert_eq!(code_of(&err), "bad-settings");
    assert!(
        err.message().contains("permission denied"),
        "{}",
        err.message()
    );
    assert!(!err.message().contains("EACCES"));
    assert!(err.message().contains(&file));
}

// ---- a pure check can pass on one platform and not another, and never
// touches disk either way ----

/// Where a stray `C:\Books` would land if the folder probe ever ran on this
/// POSIX host: relative to the working folder.
fn bogus_folders() -> Vec<PathBuf> {
    vec![
        repo_root().join("C:\\Books"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("C:\\Books"),
    ]
}

#[test]
fn win32_c_books_passes_the_path_check_so_a_bad_format_defaults_is_what_refuses_and_nothing_is_created()
 {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let err = run(
        Some(r#"{"libraryPath":"C:\\Books","formatDefaults":"nope"}"#),
        &deps_on(&file, "win32", env_with(&[])),
    )
    .expect_err("refused");
    assert_eq!(code_of(&err), "bad-settings");
    assert_eq!(err.message(), "formatDefaults must be an object or null");
    assert!(!exists(&file));
    for b in bogus_folders() {
        assert!(!b.exists(), "{}", b.display());
    }
}

#[test]
fn linux_the_same_input_is_refused_on_the_path_itself_c_books_is_not_absolute_for_posix() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let err = run(
        Some(r#"{"libraryPath":"C:\\Books","formatDefaults":"nope"}"#),
        &deps_on(&file, "linux", env_with(&[])),
    )
    .expect_err("refused");
    assert_eq!(code_of(&err), "bad-settings");
    assert_eq!(err.message(), "the library folder must be a full path");
    assert!(!exists(&file));
    for b in bogus_folders() {
        assert!(!b.exists(), "{}", b.display());
    }
}

// ---- one write covers both keys ----

#[test]
fn both_keys_land_together_in_one_write_keeping_last_route() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    write_app_settings(&vec![("lastRoute".into(), Some(json!("kindle")))], &file).unwrap();
    let chosen = sc.dir("both-chosen");

    let result = set(
        &file,
        &json!({ "libraryPath": chosen, "formatDefaults": { "justifyText": true } }).to_string(),
    );
    assert_eq!(result.library.chosen.as_deref(), Some(chosen.as_str()));
    assert!(result.format_defaults.justify_text);

    let on_disk = read_app_settings(&file);
    assert_eq!(on_disk["libraryPath"], json!(chosen));
    assert_eq!(on_disk["formatDefaults"]["justifyText"], json!(true));
    // lastRoute survived a write this verb made.
    assert_eq!(on_disk["lastRoute"], json!("kindle"));
    // No stray temp file beside settings.json.
    let names: Vec<String> = std::fs::read_dir(Path::new(&file).parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["settings.json"]);
}

// ---- fromEnv and chosen disagree on purpose ----

#[test]
fn method_reader_library_overrides_path_but_chosen_still_names_the_stored_choice() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let chosen = sc.dir("env-chosen");
    set(&file, &json!({ "libraryPath": chosen }).to_string());

    let result = run(
        None,
        &deps_on(
            &file,
            PLATFORM,
            env_with(&[("METHOD_READER_LIBRARY", ENV_LIBRARY)]),
        ),
    )
    .unwrap();
    assert!(result.library.from_env);
    assert_eq!(result.library.path, ENV_LIBRARY);
    assert_eq!(result.library.chosen.as_deref(), Some(chosen.as_str()));
}

#[test]
fn a_blank_method_reader_library_does_not_count_as_from_env() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let result = run(
        None,
        &deps_on(
            &file,
            "linux",
            env_with(&[("METHOD_READER_LIBRARY", "   ")]),
        ),
    )
    .unwrap();
    assert!(!result.library.from_env);
}

// ---- chosen is null for anything libraryRoot would not honour ----

#[test]
fn a_stored_relative_path_is_not_reported_as_chosen() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    write_app_settings(&vec![("libraryPath".into(), Some(json!("Scripts")))], &file).unwrap();
    let result = read(&file);
    assert_eq!(result.library.chosen, None);
    assert_eq!(result.library.path, HOME_LIBRARY);
}

// ---- platformDefault ----

#[test]
fn platform_default_is_correct_for_darwin_with_a_fake_home_independent_of_a_chosen_folder() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    // Only read, never made: a path that is full by darwin's rules on
    // every host.
    let chosen = "/chosen/darwin-library";
    write_app_settings(&vec![("libraryPath".into(), Some(json!(chosen)))], &file).unwrap();

    let env: Env = [("HOME", "/home/ada")].into_iter().collect();
    let result = run(None, &deps_on(&file, "darwin", env)).unwrap();
    assert_eq!(
        result.library.platform_default,
        "/home/ada/Documents/Method Reader"
    );
    // The chosen folder still wins for `path`.
    assert_eq!(result.library.path, chosen);
}

// ---- home ----

#[test]
fn home_is_present_and_is_the_same_home_the_engine_resolves_everything_else_from() {
    let sc = Scratch::new();
    assert_eq!(read(&sc.settings_file()).home, HOME);
}

// ---- keepScriptSettings ----

#[test]
fn a_plain_read_answers_keep_true_when_nothing_is_stored() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    assert!(read(&file).keep_script_settings);
    assert!(!exists(&file));
}

#[test]
fn keep_false_is_stored_and_answered_and_every_other_key_in_the_file_survives_the_write() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    write_app_settings(
        &vec![
            ("lastRoute".into(), Some(json!("kindle"))),
            (
                "formatDefaults".into(),
                Some(json!({ "justifyText": true })),
            ),
        ],
        &file,
    )
    .unwrap();
    let result = set(&file, r#"{"keepScriptSettings":false}"#);
    assert!(!result.keep_script_settings);
    let on_disk = read_app_settings(&file);
    assert_eq!(on_disk["keepScriptSettings"], json!(false));
    assert_eq!(on_disk["lastRoute"], json!("kindle"));
    assert_eq!(on_disk["formatDefaults"]["justifyText"], json!(true));
    // customized still reads the stored formatDefaults.
    assert!(result.customized);
}

#[test]
fn keep_true_is_stored_as_true_and_null_removes_the_key_which_reads_as_the_default_again() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    set(&file, r#"{"keepScriptSettings":false}"#);
    let on = set(&file, r#"{"keepScriptSettings":true}"#);
    assert!(on.keep_script_settings);
    assert_eq!(read_app_settings(&file)["keepScriptSettings"], json!(true));

    set(&file, r#"{"keepScriptSettings":false}"#);
    let reset = set(&file, r#"{"keepScriptSettings":null}"#);
    assert!(reset.keep_script_settings);
    assert!(!read_app_settings(&file).contains_key("keepScriptSettings"));
}

#[test]
fn keep_anything_but_true_false_or_null_is_bad_settings_and_nothing_is_written() {
    let sc = Scratch::new();
    for value in [json!("false"), json!(0), json!(1), json!({}), json!([])] {
        let file = sc.settings_file();
        let err = refused(&file, &json!({ "keepScriptSettings": value }).to_string());
        assert_eq!(
            format!("{value}: {}", code_of(&err)),
            format!("{value}: bad-settings")
        );
        assert!(err.message().contains("keepScriptSettings"));
        assert!(!exists(&file));
    }
}

#[test]
fn a_good_keep_script_settings_alongside_a_bad_format_defaults_stores_neither() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let err = refused(
        &file,
        r#"{"keepScriptSettings":false,"formatDefaults":"kindleEink"}"#,
    );
    assert_eq!(code_of(&err), "bad-settings");
    assert!(!exists(&file));
}

#[test]
fn a_bad_keep_script_settings_alongside_a_good_new_library_path_creates_no_folder_stores_neither() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let chosen = under(&sc.dir("keep-lib"), "not-yet-made");
    let err = refused(
        &file,
        &json!({ "libraryPath": chosen, "keepScriptSettings": "no" }).to_string(),
    );
    assert_eq!(code_of(&err), "bad-settings");
    assert!(!exists(&chosen));
    assert!(!exists(&file));
}

#[test]
fn an_unknown_key_is_refused_naming_every_key_this_verb_may_write() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let err = refused(&file, r#"{"theme":"dark"}"#);
    assert_eq!(code_of(&err), "usage");
    assert_eq!(
        err.message(),
        "app-settings --set takes only libraryPath, formatDefaults, keepScriptSettings, readerDevice, animatePage, typewriterSounds and panelIntent, not \"theme\""
    );
}

// ---- readerDevice, animatePage, typewriterSounds ----

#[test]
fn a_fresh_file_reports_no_device_the_page_animation_on_the_typewriter_sounds_off() {
    let sc = Scratch::new();
    let result = read(&sc.settings_file());
    assert_eq!(result.reader_device, None);
    assert!(result.animate_page);
    assert!(!result.typewriter_sounds);
}

#[test]
fn a_device_is_stored_reported_back_and_removed_by_null() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let stored = set(&file, r#"{"readerDevice":"kobo"}"#);
    assert_eq!(stored.reader_device.as_deref(), Some("kobo"));
    assert_eq!(read_app_settings(&file)["readerDevice"], json!("kobo"));
    assert_eq!(read(&file).reader_device.as_deref(), Some("kobo"));

    let cleared = set(&file, r#"{"readerDevice":null}"#);
    assert_eq!(cleared.reader_device, None);
    assert!(!read_app_settings(&file).contains_key("readerDevice"));
}

// ---- panelIntent (the Rust window's Read it | Edit it; new, not parity) ----

#[test]
fn the_panel_intent_defaults_to_read_is_stored_and_reset_by_null() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    assert_eq!(read(&file).panel_intent, "read");
    let stored = set(&file, r#"{"panelIntent":"edit"}"#);
    assert_eq!(stored.panel_intent, "edit");
    assert_eq!(read_app_settings(&file)["panelIntent"], json!("edit"));
    let cleared = set(&file, r#"{"panelIntent":null}"#);
    assert_eq!(cleared.panel_intent, "read");
    assert!(!read_app_settings(&file).contains_key("panelIntent"));
}

#[test]
fn an_unknown_panel_intent_is_refused_and_the_file_is_untouched() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    set(&file, r#"{"panelIntent":"edit"}"#);
    let before = std::fs::read_to_string(&file).unwrap();
    for bad in ["\"write\"", "1", "true", "\"Edit\""] {
        let err = refused(&file, &format!("{{\"panelIntent\":{bad}}}"));
        assert_eq!(code_of(&err), "bad-settings", "{bad}");
        assert_eq!(err.message(), "panelIntent must be read, edit or null");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    }
    // A stored value the window never wrote reads as the default.
    write_app_settings(
        &vec![("panelIntent".into(), Some(json!("sideways")))],
        &file,
    )
    .unwrap();
    assert_eq!(read(&file).panel_intent, "read");
}

#[test]
fn every_known_device_is_accepted() {
    let sc = Scratch::new();
    for device in READER_DEVICES {
        let result = set(
            &sc.settings_file(),
            &json!({ "readerDevice": device }).to_string(),
        );
        assert_eq!(result.reader_device.as_deref(), Some(device));
    }
}

#[test]
fn an_unknown_device_is_refused_with_bad_settings_and_the_file_is_byte_identical() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    write_app_settings(
        &vec![
            ("lastRoute".into(), Some(json!("usb"))),
            ("readerDevice".into(), Some(json!("kindle"))),
        ],
        &file,
    )
    .unwrap();
    let before = std::fs::read_to_string(&file).unwrap();
    for bad in ["\"nook\"", "3", "true", "\"Kobo\""] {
        let err = refused(&file, &format!("{{\"readerDevice\":{bad}}}"));
        assert_eq!(code_of(&err), "bad-settings", "{bad}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    }
}

#[test]
fn both_switches_are_stored_and_reported() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let result = set(&file, r#"{"animatePage":false,"typewriterSounds":true}"#);
    assert!(!result.animate_page);
    assert!(result.typewriter_sounds);
    let on_disk = read_app_settings(&file);
    assert_eq!(on_disk["animatePage"], json!(false));
    assert_eq!(on_disk["typewriterSounds"], json!(true));
}

#[test]
fn null_puts_a_switch_back_to_its_default() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    set(&file, r#"{"animatePage":false,"typewriterSounds":true}"#);
    let result = set(&file, r#"{"animatePage":null,"typewriterSounds":null}"#);
    assert!(result.animate_page);
    assert!(!result.typewriter_sounds);
    let on_disk = read_app_settings(&file);
    assert!(!on_disk.contains_key("animatePage"));
    assert!(!on_disk.contains_key("typewriterSounds"));
}

#[test]
fn a_non_boolean_for_either_switch_is_bad_settings_and_writes_nothing() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    write_app_settings(&vec![("lastRoute".into(), Some(json!("usb")))], &file).unwrap();
    let before = std::fs::read_to_string(&file).unwrap();
    for patch in [
        r#"{"animatePage":"false"}"#,
        r#"{"typewriterSounds":1}"#,
        r#"{"animatePage":false,"typewriterSounds":"yes"}"#,
    ] {
        assert_eq!(code_of(&refused(&file, patch)), "bad-settings", "{patch}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    }
}

#[test]
fn setting_one_key_keeps_every_other_key_in_the_file() {
    let sc = Scratch::new();
    let file = sc.settings_file();
    let library = sc.dir("kept-lib");
    write_app_settings(
        &vec![
            ("lastRoute".into(), Some(json!("usb"))),
            ("libraryPath".into(), Some(json!(library))),
            ("keepScriptSettings".into(), Some(json!(false))),
        ],
        &file,
    )
    .unwrap();
    set(&file, r#"{"readerDevice":"tolino"}"#);
    set(&file, r#"{"animatePage":false}"#);
    let on_disk = read_app_settings(&file);
    assert_eq!(on_disk["lastRoute"], json!("usb"));
    assert_eq!(on_disk["libraryPath"], json!(library));
    assert_eq!(on_disk["keepScriptSettings"], json!(false));
    assert_eq!(on_disk["readerDevice"], json!("tolino"));
    assert_eq!(on_disk["animatePage"], json!(false));
}

// ---- method-reader app-settings (through the CLI) ----

/// A Sandbox with METHOD_READER_LIBRARY blank, so the DEFAULT library location
/// is what is reported, and HOME in the scratch folder, so that default
/// resolves there and never in a real home.
fn cli_sandbox() -> Sandbox {
    let mut sb = Sandbox::new();
    let home = sb.mkdir("home");
    sb.set("METHOD_READER_LIBRARY", "")
        .set("HOME", home.to_string_lossy().into_owned())
        .set("USERPROFILE", home.to_string_lossy().into_owned());
    sb
}

fn line(stdout: &str, n: usize) -> String {
    stdout.trim().lines().nth(n).unwrap_or_default().to_owned()
}

#[test]
fn cli_json_answer_has_the_documented_shape() {
    let sb = cli_sandbox();
    let ran = sb.run(&["app-settings", "--json"]);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    let answer = ran.json();
    assert_eq!(answer["ok"], json!(true));
    assert_eq!(answer["file"], json!(sb.p("config/settings.json")));
    assert_eq!(answer["library"]["chosen"], Value::Null);
    assert!(answer["library"]["path"].is_string());
    assert!(answer["library"]["platformDefault"].is_string());
    assert_eq!(answer["library"]["fromEnv"], json!(false));
    assert!(answer["home"].is_string());
    assert!(answer["formatDefaults"].is_object());
    assert!(answer["shippedDefaults"].is_object());
    assert_eq!(answer["customized"], json!(false));
    assert_eq!(answer["keepScriptSettings"], json!(true));
}

#[test]
fn cli_set_round_trips() {
    let sb = cli_sandbox();
    let chosen = sb.p("cli-chosen");
    sb.mkdir("cli-chosen");
    let set = sb.run(&[
        "app-settings",
        "--set",
        &json!({ "libraryPath": chosen }).to_string(),
        "--json",
    ]);
    assert_eq!(set.code, 0, "{}", set.stdout);
    assert_eq!(set.json()["library"]["chosen"], json!(chosen));

    let back = sb.run(&["app-settings", "--json"]);
    assert_eq!(back.code, 0);
    assert_eq!(back.json()["library"]["chosen"], json!(chosen));
}

#[test]
fn human_output_names_the_folder_and_by_default_method_readers_own_defaults() {
    let sb = cli_sandbox();
    let ran = sb.run(&["app-settings"]);
    assert_eq!(ran.code, 0);
    let first = line(&ran.stdout, 0);
    assert!(first.contains("books are saved in"), "{first}");
    assert!(first.contains("default folder"), "{first}");
    assert_eq!(
        line(&ran.stdout, 1),
        "new scripts start from: Method Reader's defaults"
    );
}

#[test]
fn human_output_says_your_own_defaults_once_format_defaults_are_customized() {
    let sb = cli_sandbox();
    sb.run(&[
        "app-settings",
        "--set",
        r#"{"formatDefaults":{"justifyText":true}}"#,
        "--json",
    ]);
    let ran = sb.run(&["app-settings"]);
    assert_eq!(
        line(&ran.stdout, 1),
        "new scripts start from: your own defaults"
    );
}

#[test]
fn human_output_says_what_a_converted_pdf_does_keeps_its_settings_by_default_or_follows_the_defaults()
 {
    let sb = cli_sandbox();
    let before = sb.run(&["app-settings"]);
    assert_eq!(
        line(&before.stdout, 2),
        "when a PDF is converted: keep its settings"
    );

    let set = sb.run(&[
        "app-settings",
        "--set",
        r#"{"keepScriptSettings":false}"#,
        "--json",
    ]);
    assert_eq!(set.code, 0);
    assert_eq!(set.json()["keepScriptSettings"], json!(false));

    let after = sb.run(&["app-settings"]);
    assert_eq!(
        line(&after.stdout, 2),
        "when a PDF is converted: follow the defaults"
    );
}

#[test]
fn human_output_says_the_folder_you_chose_once_one_is_stored() {
    let sb = cli_sandbox();
    let chosen = sb.p("chosen-human");
    sb.mkdir("chosen-human");
    sb.run(&[
        "app-settings",
        "--set",
        &json!({ "libraryPath": chosen }).to_string(),
        "--json",
    ]);
    let ran = sb.run(&["app-settings"]);
    assert_eq!(ran.code, 0);
    let first = line(&ran.stdout, 0);
    assert!(first.contains("books are saved in"), "{first}");
    assert!(first.contains("the folder you chose"), "{first}");
}

#[test]
fn human_output_says_set_by_method_reader_library_when_the_env_override_wins() {
    let mut sb = cli_sandbox();
    let env_lib = sb.mkdir("env-lib-human");
    sb.set(
        "METHOD_READER_LIBRARY",
        env_lib.to_string_lossy().into_owned(),
    );
    let ran = sb.run(&["app-settings"]);
    assert_eq!(ran.code, 0);
    let first = line(&ran.stdout, 0);
    assert!(first.contains("books are saved in"), "{first}");
    assert!(first.contains("set by METHOD_READER_LIBRARY"), "{first}");
}

#[test]
fn cli_refuses_a_stray_positional() {
    let sb = cli_sandbox();
    let ran = sb.run(&["app-settings", "extra", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    assert!(ran.error_message().contains("extra"));
}

#[test]
fn help_describes_what_set_accepts_and_the_method_reader_library_override() {
    let ran = cli_sandbox().run(&["app-settings", "--help"]);
    assert_eq!(ran.code, 0);
    for want in [
        "method-reader app-settings",
        "libraryPath",
        "formatDefaults",
        "METHOD_READER_LIBRARY",
    ] {
        assert!(ran.stdout.contains(want), "{want}");
    }
}

#[test]
fn help_describes_keep_script_settings() {
    let ran = cli_sandbox().run(&["app-settings", "--help"]);
    assert!(ran.stdout.contains("keepScriptSettings"));
    assert!(!ran.stdout.contains('\u{2014}'));
}

#[test]
fn help_says_format_defaults_replaces_the_stored_defaults_unlike_settings_set() {
    let ran = cli_sandbox().run(&["app-settings", "--help"]);
    assert!(ran.stdout.contains("REPLACES"));
    assert!(ran.stdout.contains("settings --set"));
}

#[test]
fn the_top_level_help_usage_lists_app_settings() {
    let ran = cli_sandbox().run(&["--help"]);
    assert_eq!(ran.code, 0);
    assert!(
        ran.stdout
            .contains("method-reader app-settings [--set <json>] [--json]")
    );
    assert!(
        ran.stdout
            .contains("where books land, and what new scripts start from")
    );
}

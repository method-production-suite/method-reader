//! Port of tests/cli-settings.test.ts: the `settings` verb, a script's own
//! formatting read and written through its sidecar. The handler runs in
//! process against a scratch script and a scratch app settings path; the
//! last block spawns the real CLI in a Sandbox.

mod common;

use std::path::Path;

use serde_json::{Value, json};
use sp_engine::commands::settings::{SettingsOptions, SettingsResult, settings_command};
use sp_engine::errors::EngineError;
use sp_engine::settings::app::write_app_settings;
use sp_model::{ContdMode, DEFAULT_FORMAT_OPTIONS, DevicePresetId, DualDialogue};

use common::Sandbox;

/// A scratch folder holding `Script.fountain`, and an app settings path
/// inside it that nothing has written.
struct Script {
    _dir: tempfile::TempDir,
    dir: String,
    fountain: String,
}

impl Script {
    fn new() -> Script {
        let dir = tempfile::Builder::new()
            .prefix("method-reader-cli-settings-")
            .tempdir()
            .expect("a scratch folder");
        let path = common::canonical(dir.path());
        let fountain = path.join("Script.fountain");
        std::fs::write(&fountain, "INT. ROOM - DAY\n\nA beat.\n").unwrap();
        Script {
            dir: path.to_string_lossy().into_owned(),
            fountain: fountain.to_string_lossy().into_owned(),
            _dir: dir,
        }
    }

    fn app_settings(&self) -> String {
        join(&self.dir, "app-settings.json")
    }

    fn sidecar(&self) -> String {
        join(&self.dir, "Script.method-reader.json")
    }

    fn run(&self, set: Option<&str>) -> Result<SettingsResult, EngineError> {
        self.run_with(set, &self.app_settings())
    }

    fn run_with(&self, set: Option<&str>, app: &str) -> Result<SettingsResult, EngineError> {
        self.run_on(&self.fountain, set, app)
    }

    fn run_on(
        &self,
        fountain: &str,
        set: Option<&str>,
        app: &str,
    ) -> Result<SettingsResult, EngineError> {
        settings_command(&SettingsOptions {
            fountain: fountain.to_owned(),
            set: set.map(str::to_owned),
            app_settings_path: app.to_owned(),
        })
    }

    fn ok(&self, set: Option<&str>) -> SettingsResult {
        self.run(set).expect("settings")
    }
}

/// `name` inside the folder `dir`, as the host writes it.
fn join(dir: &str, name: &str) -> String {
    Path::new(dir).join(name).to_string_lossy().into_owned()
}

fn code_of(err: &EngineError) -> &'static str {
    match err {
        EngineError::Cli(e) => e.code.as_str(),
        _ => "",
    }
}

fn store(path: &str, key: &str, value: Value) {
    write_app_settings(&vec![(key.to_owned(), Some(value))], path).unwrap();
}

// ---- settingsCommand ----

#[test]
fn an_untouched_script_reports_the_shipped_defaults_and_its_preset() {
    let s = Script::new();
    let result = s.ok(None);
    assert_eq!(result.settings, DEFAULT_FORMAT_OPTIONS);
    // kindleEink IS the defaults.
    assert_eq!(result.preset, Some(DevicePresetId::KindleEink));
    assert_eq!(result.sidecar, s.sidecar());
    assert!(!Path::new(&result.sidecar).exists()); // reading must not create
}

#[test]
fn it_offers_exactly_the_two_presets_with_their_real_settings() {
    let s = Script::new();
    let presets = s.ok(None).presets;
    let mut ids: Vec<String> = presets
        .iter()
        .map(|p| {
            serde_json::to_value(p.id)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    ids.sort();
    assert_eq!(ids, ["kindleEink", "phone"]);
    let phone = presets
        .iter()
        .find(|p| p.id == DevicePresetId::Phone)
        .unwrap();
    assert_eq!(phone.display_name, "Phone / narrow screen");
    assert_eq!(phone.settings.dual_dialogue, DualDialogue::Sequential);
    assert_eq!(phone.settings.dialogue_side_margin_pct, 10.0);
}

#[test]
fn set_writes_the_sidecar_and_reports_the_merged_result() {
    let s = Script::new();
    let result = s.ok(Some(r#"{"justifyText":true}"#));
    assert!(result.settings.justify_text);
    // The other nineteen must survive.
    assert_eq!(result.settings.contd_mode, ContdMode::Auto);
    let as_json = serde_json::to_value(result.settings).unwrap();
    assert_eq!(as_json.as_object().unwrap().len(), 20);
    let on_disk: Value =
        serde_json::from_str(&std::fs::read_to_string(&result.sidecar).unwrap()).unwrap();
    assert_eq!(on_disk["justifyText"], json!(true));
    assert_eq!(on_disk["contdMode"], json!("auto"));
}

#[test]
fn a_second_set_overlays_the_first_rather_than_resetting_it() {
    let s = Script::new();
    s.ok(Some(r#"{"justifyText":true}"#));
    let result = s.ok(Some(r#"{"showSceneNumbers":true}"#));
    assert!(result.settings.justify_text);
    assert!(result.settings.show_scene_numbers);
}

#[test]
fn set_sequential_then_set_side_by_side_ends_side_by_side() {
    let s = Script::new();
    s.ok(Some(r#"{"dualDialogue":"sequential"}"#));
    let result = s.ok(Some(r#"{"dualDialogue":"sideBySide"}"#));
    assert_eq!(result.settings.dual_dialogue, DualDialogue::SideBySide);
}

#[test]
fn a_tuned_script_matches_no_preset() {
    let s = Script::new();
    assert_eq!(s.ok(Some(r#"{"elementSpacingEm":1.9}"#)).preset, None);
}

#[test]
fn set_clamps_rather_than_trusting() {
    let s = Script::new();
    let result = s.ok(Some(r#"{"dialogueSideMarginPct":999}"#));
    assert_eq!(result.settings.dialogue_side_margin_pct, 30.0);
}

#[test]
fn a_missing_script_is_unreadable_not_an_empty_settings_object() {
    let s = Script::new();
    let err = s
        .run_on(&join(&s.dir, "nope.fountain"), None, &s.app_settings())
        .expect_err("refused");
    assert!(err.message().contains("cannot read"), "{}", err.message());
}

#[test]
fn a_malformed_set_is_bad_settings_and_does_not_write_the_sidecar() {
    let s = Script::new();
    let err = s.run(Some("{oops")).expect_err("refused");
    assert_eq!(code_of(&err), "bad-settings");
    assert!(!Path::new(&s.sidecar()).exists());
}

#[test]
fn a_plain_read_reports_what_a_prior_set_actually_saved() {
    let s = Script::new();
    s.ok(Some(r#"{"justifyText":true}"#));
    let result = s.ok(None);
    assert!(result.settings.justify_text);
    assert_eq!(result.preset, None);
}

#[test]
fn a_malformed_sidecar_already_on_disk_is_treated_as_absent_not_a_crash() {
    let s = Script::new();
    std::fs::write(s.sidecar(), "{ this is not json").unwrap();
    assert_eq!(s.ok(None).settings, DEFAULT_FORMAT_OPTIONS);
}

#[test]
fn set_as_a_json_array_is_bad_settings_not_silently_accepted() {
    let s = Script::new();
    let err = s.run(Some("[1,2,3]")).expect_err("refused");
    assert_eq!(code_of(&err), "bad-settings");
    assert!(!Path::new(&s.sidecar()).exists());
}

#[test]
fn a_directory_is_unreadable_not_treated_as_a_script_with_no_sidecar() {
    let s = Script::new();
    let err = s
        .run_on(&s.dir, None, &s.app_settings())
        .expect_err("refused");
    assert!(err.message().contains("cannot read"), "{}", err.message());
}

// ---- app-wide defaults underneath the sidecar ----

#[test]
fn no_app_settings_file_app_defaults_is_exactly_the_shipped_defaults() {
    let s = Script::new();
    let result = s.ok(None);
    assert_eq!(result.app_defaults, DEFAULT_FORMAT_OPTIONS);
    // defaults stays Method Reader's own regardless of appDefaults.
    assert_eq!(result.defaults, DEFAULT_FORMAT_OPTIONS);
}

#[test]
fn an_untouched_script_with_app_defaults_set_reports_them_not_the_shipped_defaults() {
    let s = Script::new();
    let path = s.app_settings();
    store(
        &path,
        "formatDefaults",
        json!({ "dialogueSideMarginPct": 5 }),
    );
    let result = s.ok(None);
    assert_eq!(result.settings.dialogue_side_margin_pct, 5.0);
    assert_eq!(result.app_defaults.dialogue_side_margin_pct, 5.0);
    // The shipped object never moves.
    assert_eq!(
        result.defaults.dialogue_side_margin_pct,
        DEFAULT_FORMAT_OPTIONS.dialogue_side_margin_pct
    );
}

#[test]
fn a_sidecar_already_on_disk_outranks_the_app_defaults() {
    let s = Script::new();
    let path = s.app_settings();
    store(
        &path,
        "formatDefaults",
        json!({ "dialogueSideMarginPct": 5 }),
    );
    // Written directly, not through --set.
    std::fs::write(s.sidecar(), r#"{"dialogueSideMarginPct":12}"#).unwrap();
    assert_eq!(s.ok(None).settings.dialogue_side_margin_pct, 12.0);
    // Knobs the sidecar never mentioned still come from the app defaults.
    store(
        &path,
        "formatDefaults",
        json!({ "dialogueSideMarginPct": 5, "justifyText": true }),
    );
    assert!(s.ok(None).settings.justify_text);
}

#[test]
fn set_overlays_on_the_app_defaults_when_there_is_no_sidecar_yet() {
    let s = Script::new();
    store(
        &s.app_settings(),
        "formatDefaults",
        json!({ "dialogueSideMarginPct": 5 }),
    );
    let result = s.ok(Some(r#"{"justifyText":true}"#));
    assert!(result.settings.justify_text);
    assert_eq!(result.settings.dialogue_side_margin_pct, 5.0);
}

#[test]
fn a_corrupt_app_settings_file_changes_nothing_shipped_defaults_throughout() {
    let s = Script::new();
    std::fs::write(s.app_settings(), "{ not json").unwrap();
    let result = s.ok(None);
    assert_eq!(result.settings, DEFAULT_FORMAT_OPTIONS);
    assert_eq!(result.app_defaults, DEFAULT_FORMAT_OPTIONS);
}

#[test]
fn the_answer_carries_keep_script_settings_for_the_settings_page() {
    let s = Script::new();
    // true when nothing is stored: keeping is the default.
    assert!(s.ok(None).keep_script_settings);
    store(&s.app_settings(), "keepScriptSettings", json!(false));
    assert!(!s.ok(None).keep_script_settings);
}

#[test]
fn with_no_app_settings_path_given_it_defaults_to_the_settings_file_the_cli_resolves() {
    // The Rust handler takes the path as a required field; the CLI resolves
    // it (here, the Sandbox's empty METHOD_READER_CONFIG_DIR), so this is checked
    // where the default lives: a script with no app settings file reads the
    // shipped defaults rather than failing.
    let sb = Sandbox::new();
    let fountain = sb.write("Script.fountain", "INT. ROOM - DAY\n\nA beat.\n");
    let ran = sb.run(&["settings", &fountain.to_string_lossy(), "--json"]);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert_eq!(
        ran.json()["appDefaults"],
        serde_json::to_value(DEFAULT_FORMAT_OPTIONS).unwrap()
    );
}

// ---- method-reader settings (through the CLI) ----

fn cli_script(sb: &Sandbox) -> String {
    sb.write("Script.fountain", "INT. ROOM - DAY\n\nA beat.\n")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn cli_json_prints_one_object_carrying_the_settings_and_the_presets() {
    let sb = Sandbox::new();
    let fountain = cli_script(&sb);
    let answer = sb.run(&["settings", &fountain, "--json"]).json();
    assert_eq!(answer["ok"], json!(true));
    assert_eq!(answer["settings"]["contdMode"], json!("auto"));
    assert_eq!(answer["presets"].as_array().unwrap().len(), 2);
    assert!(
        answer["sidecar"]
            .as_str()
            .unwrap()
            .contains("Script.method-reader.json")
    );
}

#[test]
fn cli_set_round_trips() {
    let sb = Sandbox::new();
    let fountain = cli_script(&sb);
    sb.run(&[
        "settings",
        &fountain,
        "--set",
        r#"{"showPageMarkers":true}"#,
        "--json",
    ]);
    let answer = sb.run(&["settings", &fountain, "--json"]).json();
    assert_eq!(answer["settings"]["showPageMarkers"], json!(true));
}

#[test]
fn cli_a_missing_script_fails_as_json_on_stdout_exit_1() {
    let sb = Sandbox::new();
    let ran = sb.run(&["settings", &sb.p("nope.fountain"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unreadable");
}

#[test]
fn a_file_named_settings_still_converts_rather_than_being_stolen() {
    // The bare verb word as argv[0], with the working folder holding a file
    // of that name: the shadowing rule sends it to the conversion.
    let sb = Sandbox::new();
    sb.write("settings", "x");
    let ran = sb.run(&["settings", "--json"]);
    assert_eq!(ran.error_code(), "unsupported-type");
}

#[test]
fn devices_rejects_set_it_belongs_to_settings_and_app_settings_not_every_verb() {
    let sb = Sandbox::new();
    let ran = sb.run(&["devices", "--set", "{}", "--json"]);
    assert_eq!(ran.error_code(), "usage");
}

#[test]
fn help_says_a_script_with_no_saved_settings_starts_from_the_app_defaults() {
    let ran = Sandbox::new().run(&["settings", "--help"]);
    assert!(
        ran.stdout
            .contains("starts from the format defaults you chose in the app")
    );
}

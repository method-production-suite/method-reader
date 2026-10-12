//! Port of tests/settings-app-defaults.test.ts.

mod common;

use common::{Scratch, write};
use serde_json::{Value, json};
use sp_engine::settings::app::write_app_settings;
use sp_engine::settings::app_defaults::{
    app_default_options, app_defaults_customized, keeps_script_settings,
};
use sp_model::{ContdMode, DEFAULT_FORMAT_OPTIONS};

fn store(path: &str, key: &str, value: Value) {
    write_app_settings(&vec![(key.into(), Some(value))], path).unwrap();
}

#[test]
fn no_settings_file_exactly_the_shipped_defaults() {
    let s = Scratch::new("app-defaults");
    assert_eq!(
        app_default_options(&s.join("settings.json")),
        DEFAULT_FORMAT_OPTIONS
    );
}

#[test]
fn a_stored_knob_overrides_the_shipped_default() {
    let s = Scratch::new("app-defaults");
    let path = s.join("settings.json");
    store(
        &path,
        "formatDefaults",
        json!({ "dialogueSideMarginPct": 5 }),
    );
    let result = app_default_options(&path);
    assert_eq!(result.dialogue_side_margin_pct, 5.0);
    assert_eq!(result.contd_mode, ContdMode::Auto);
}

#[test]
fn a_stored_partial_is_returned_full() {
    let s = Scratch::new("app-defaults");
    let path = s.join("settings.json");
    store(&path, "formatDefaults", json!({ "justifyText": true }));
    let full = serde_json::to_value(app_default_options(&path)).unwrap();
    let defaults = serde_json::to_value(DEFAULT_FORMAT_OPTIONS).unwrap();
    assert_eq!(
        full.as_object().unwrap().len(),
        defaults.as_object().unwrap().len()
    );
}

#[test]
fn an_out_of_range_stored_value_is_clamped() {
    let s = Scratch::new("app-defaults");
    let path = s.join("settings.json");
    store(
        &path,
        "formatDefaults",
        json!({ "dialogueSideMarginPct": 999 }),
    );
    assert_eq!(app_default_options(&path).dialogue_side_margin_pct, 30.0);
}

#[test]
fn format_defaults_that_are_not_an_object_read_as_absent() {
    for stored in [json!([1, 2, 3]), json!("nope"), json!(7), Value::Null] {
        let s = Scratch::new("app-defaults");
        let path = s.join("settings.json");
        store(&path, "formatDefaults", stored);
        assert_eq!(app_default_options(&path), DEFAULT_FORMAT_OPTIONS);
    }
}

#[test]
fn a_corrupt_settings_file_reads_as_the_shipped_defaults() {
    let s = Scratch::new("app-defaults");
    let path = s.join("settings.json");
    write(&path, "{ this is not json");
    assert_eq!(app_default_options(&path), DEFAULT_FORMAT_OPTIONS);
}

#[test]
fn other_keys_but_no_format_defaults_read_as_absent() {
    let s = Scratch::new("app-defaults");
    let path = s.join("settings.json");
    store(&path, "lastRoute", json!("kindle"));
    store(&path, "libraryPath", json!("/x"));
    assert_eq!(app_default_options(&path), DEFAULT_FORMAT_OPTIONS);
}

#[test]
fn customized_false_when_nothing_stored() {
    let s = Scratch::new("app-defaults");
    assert!(!app_defaults_customized(&s.join("settings.json")));
}

#[test]
fn customized_false_when_stored_equals_the_shipped_defaults() {
    let s = Scratch::new("app-defaults");
    let path = s.join("settings.json");
    store(
        &path,
        "formatDefaults",
        serde_json::to_value(DEFAULT_FORMAT_OPTIONS).unwrap(),
    );
    assert!(!app_defaults_customized(&path));
}

#[test]
fn customized_true_once_a_knob_differs() {
    let s = Scratch::new("app-defaults");
    let path = s.join("settings.json");
    store(
        &path,
        "formatDefaults",
        json!({ "justifyText": !DEFAULT_FORMAT_OPTIONS.justify_text }),
    );
    assert!(app_defaults_customized(&path));
}

#[test]
fn customized_false_for_a_non_object() {
    let s = Scratch::new("app-defaults");
    let path = s.join("settings.json");
    store(&path, "formatDefaults", json!("nope"));
    assert!(!app_defaults_customized(&path));
}

#[test]
fn keeps_true_when_nothing_stored() {
    let s = Scratch::new("app-defaults");
    assert!(keeps_script_settings(&s.join("settings.json")));
}

#[test]
fn keeps_false_only_when_false_is_stored() {
    let s = Scratch::new("app-defaults");
    let path = s.join("settings.json");
    store(&path, "keepScriptSettings", json!(false));
    assert!(!keeps_script_settings(&path));
    store(&path, "keepScriptSettings", json!(true));
    assert!(keeps_script_settings(&path));
}

#[test]
fn keeps_anything_not_a_boolean_reads_as_the_default() {
    for stored in [
        json!("false"),
        json!(0),
        Value::Null,
        json!({}),
        json!([]),
        json!("no"),
    ] {
        let s = Scratch::new("app-defaults");
        let path = s.join("settings.json");
        store(&path, "keepScriptSettings", stored.clone());
        assert!(keeps_script_settings(&path), "{stored}");
    }
}

#[test]
fn keeps_a_corrupt_settings_file_reads_as_the_default() {
    let s = Scratch::new("app-defaults");
    let path = s.join("settings.json");
    write(&path, "{ this is not json");
    assert!(keeps_script_settings(&path));
}

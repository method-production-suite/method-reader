//! Port of tests/cli-app-defaults.test.ts: a conversion's precedence
//! (flags > sidecar > app defaults > shipped defaults) through the real
//! CLI. Every assertion is a DISAGREEMENT: the same fixture, converted with
//! and without the app defaults, must produce different CSS. (The handler
//! half, reading the app defaults, is sp-engine's tests/settings_app_defaults.rs.)

mod common;

use std::path::Path;

use serde_json::{Value, json};

use common::{Sandbox, repo_root, zip_entries};

const APP_SIDE: i64 = 11;
const APP_CUE: i64 = 44;
const SIDECAR_SIDE: i64 = 3;
const FLAG_SIDE: i64 = 24;

/// format-defaults.json, with the guard every test here leans on: a
/// "distinctive" value that were the default would make every assertion
/// pass against an implementation that does nothing.
fn defaults() -> Value {
    let d: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("format-defaults.json")).unwrap(),
    )
    .unwrap();
    for v in [APP_SIDE, SIDECAR_SIDE, FLAG_SIDE] {
        assert_ne!(d["dialogueSideMarginPct"], json!(v));
    }
    assert_ne!(d["cueIndentPct"], json!(APP_CUE));
    assert_eq!(d["cueAlignment"], json!("centered"));
    d
}

fn shipped_side() -> String {
    defaults()["dialogueSideMarginPct"].to_string()
}

/// The stylesheet out of a built EPUB.
fn epub_css(path: &str) -> String {
    zip_entries(Path::new(path))
        .into_iter()
        .find(|(n, _)| n.ends_with(".css"))
        .map(|(_, d)| String::from_utf8_lossy(&d).into_owned())
        .unwrap_or_else(|| panic!("no stylesheet in {path}"))
}

/// The Sandbox's METHOD_READER_CONFIG_DIR holding `settings.json` with these
/// `formatDefaults`.
fn app_defaults(sb: &Sandbox, format_defaults: Value) {
    sb.write(
        "config/settings.json",
        json!({ "formatDefaults": format_defaults }).to_string(),
    );
}

/// The fixture copied to `scripts/<name>`, and its path (and `out/` made).
fn script(sb: &Sandbox, fixture: &str, name: &str) -> String {
    sb.mkdir("out");
    sb.copy_fixture(fixture, &format!("scripts/{name}"))
        .to_string_lossy()
        .into_owned()
}

fn convert(sb: &Sandbox, args: &[&str]) -> Value {
    let ran = sb.run(args);
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    ran.json()
}

fn epub_of(answer: &Value) -> String {
    epub_css(answer["epubPath"].as_str().expect("an epubPath"))
}

#[test]
fn no_sidecar_no_flags_the_app_defaults_reach_the_book() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "screenplay.pdf", "AppOnly.pdf");
    app_defaults(&sb, json!({ "dialogueSideMarginPct": APP_SIDE }));
    let out = sb.p("out/app-only.epub");
    let css = epub_of(&convert(
        &sb,
        &[&pdf, "-o", &out, "--no-fountain", "--json"],
    ));
    assert!(css.contains(&format!("margin-left: {APP_SIDE}%")));
    assert!(!css.contains(&format!("margin-left: {}%", shipped_side())));
}

#[test]
fn the_same_env_but_no_settings_file_at_that_path_the_shipped_defaults_reach_the_book() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "screenplay.pdf", "NoAppSettings.pdf");
    let out = sb.p("out/no-app-settings.epub");
    let css = epub_of(&convert(
        &sb,
        &[&pdf, "-o", &out, "--no-fountain", "--json"],
    ));
    assert!(css.contains(&format!("margin-left: {}%", shipped_side())));
    assert!(!css.contains(&format!("margin-left: {APP_SIDE}%")));
}

#[test]
fn a_corrupt_settings_file_changes_nothing_the_shipped_defaults_reach_the_book() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "screenplay.pdf", "Corrupt.pdf");
    sb.write("config/settings.json", "{ this is not json");
    let out = sb.p("out/corrupt.epub");
    let answer = convert(&sb, &[&pdf, "-o", &out, "--no-fountain", "--json"]);
    assert_eq!(answer["ok"], json!(true));
    assert!(epub_of(&answer).contains(&format!("margin-left: {}%", shipped_side())));
}

#[test]
fn a_sidecar_outranks_the_app_defaults_knob_by_knob() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "screenplay.pdf", "Sidecar.pdf");
    app_defaults(
        &sb,
        json!({
            "dialogueSideMarginPct": APP_SIDE,
            "cueIndentPct": APP_CUE,
            "cueAlignment": "indented",
        }),
    );
    // The sidecar only ever mentions ONE of those three knobs.
    sb.write(
        "scripts/Sidecar.method-reader.json",
        json!({ "dialogueSideMarginPct": SIDECAR_SIDE }).to_string(),
    );
    let out = sb.p("out/sidecar.epub");
    let css = epub_of(&convert(
        &sb,
        &[&pdf, "-o", &out, "--no-fountain", "--json"],
    ));
    assert!(css.contains(&format!("margin-left: {SIDECAR_SIDE}%")));
    assert!(!css.contains(&format!("margin-left: {APP_SIDE}%")));
    // The knobs it never mentioned are still the APP defaults.
    assert!(css.contains(&format!("margin-left: {APP_CUE}%")));
}

#[test]
fn an_explicit_options_json_flag_wins_over_both_the_sidecar_and_the_app_defaults() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "screenplay.pdf", "Flagged.pdf");
    app_defaults(&sb, json!({ "dialogueSideMarginPct": APP_SIDE }));
    sb.write(
        "scripts/Flagged.method-reader.json",
        json!({ "dialogueSideMarginPct": SIDECAR_SIDE }).to_string(),
    );
    let out = sb.p("out/flagged.epub");
    let flag = json!({ "dialogueSideMarginPct": FLAG_SIDE }).to_string();
    let css = epub_of(&convert(
        &sb,
        &[
            &pdf,
            "-o",
            &out,
            "--no-fountain",
            "--json",
            "--options-json",
            &flag,
        ],
    ));
    assert!(css.contains(&format!("margin-left: {FLAG_SIDE}%")));
    assert!(!css.contains(&format!("margin-left: {SIDECAR_SIDE}%")));
    assert!(!css.contains(&format!("margin-left: {APP_SIDE}%")));
}

/// torture.pdf has a dual-dialogue scene; a sideBySide render marks the
/// right-hand cue with ` ^` in the .fountain, a sequential one omits it.
fn fountain_of(answer: &Value) -> String {
    std::fs::read_to_string(answer["fountainPath"].as_str().expect("a fountainPath")).unwrap()
}

#[test]
fn a_sidecar_side_by_side_wins_over_app_defaults_sequential_through_the_real_precedence_chain() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "torture.pdf", "Dual.pdf");
    app_defaults(&sb, json!({ "dualDialogue": "sequential" }));
    sb.write(
        "scripts/Dual.method-reader.json",
        json!({ "dualDialogue": "sideBySide" }).to_string(),
    );
    let out = sb.p("out/dual-sidecar.epub");
    let answer = convert(&sb, &[&pdf, "-o", &out, "--json"]);
    assert!(fountain_of(&answer).contains(" ^"));
}

#[test]
fn an_explicit_options_json_flag_wins_the_same_way_over_a_sequential_sidecar_and_app_defaults() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "torture.pdf", "DualFlag.pdf");
    app_defaults(&sb, json!({ "dualDialogue": "sequential" }));
    sb.write(
        "scripts/DualFlag.method-reader.json",
        json!({ "dualDialogue": "sequential" }).to_string(),
    );
    let out = sb.p("out/dual-flag.epub");
    let flag = json!({ "dualDialogue": "sideBySide" }).to_string();
    let answer = convert(&sb, &[&pdf, "-o", &out, "--json", "--options-json", &flag]);
    assert!(fountain_of(&answer).contains(" ^"));
}

#[test]
fn help_says_a_conversion_starts_from_the_app_defaults_underneath_the_saved_settings() {
    let ran = Sandbox::new().run(&["--help"]);
    assert_eq!(ran.code, 0);
    let saved = ran
        .stdout
        .find("A script's saved settings")
        .expect("the saved-settings paragraph");
    let app = ran
        .stdout
        .find("the format defaults you chose in the app")
        .expect("the app-defaults line");
    assert!(app > saved);
}

//! Port of tests/conversion-settings.test.ts: a script's saved settings
//! reach the book this conversion produces, a library conversion pins the
//! settings it started from, and keepScriptSettings chooses whether it
//! does. Every assertion is a DISAGREEMENT: the value asserted is one the
//! defaults do not carry. (The functions underneath, library_output,
//! adopt_sidecar, keeps_script_settings and the sidecar itself, are
//! sp-engine's tests/library.rs, settings_app_defaults.rs and sidecar.rs.)

mod common;

use std::path::Path;

use serde_json::{Value, json};
use sp_engine::library::library_output;

use common::{Sandbox, repo_root, zip_entries};

const SIDE: i64 = 7;
const CUE: i64 = 41;
const APP_MARGIN: i64 = 19;

/// format-defaults.json, with the guard every test leans on: a
/// "distinctive" value that were the default would make every assertion
/// pass against an implementation that does nothing at all.
fn defaults() -> Value {
    let d: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("format-defaults.json")).unwrap(),
    )
    .unwrap();
    assert_ne!(d["dialogueSideMarginPct"], json!(SIDE));
    assert_ne!(d["dialogueSideMarginPct"], json!(APP_MARGIN));
    assert_ne!(d["cueIndentPct"], json!(CUE));
    assert_eq!(d["cueAlignment"], json!("centered"));
    d
}

fn tuning() -> String {
    format!(
        "{}\n",
        json!({ "dialogueSideMarginPct": SIDE, "cueIndentPct": CUE, "cueAlignment": "indented" })
    )
}

fn epub_css(path: &str) -> String {
    zip_entries(Path::new(path))
        .into_iter()
        .find(|(n, _)| n.ends_with(".css"))
        .map(|(_, d)| String::from_utf8_lossy(&d).into_owned())
        .unwrap_or_else(|| panic!("no stylesheet in {path}"))
}

fn css_of(answer: &Value) -> String {
    epub_css(answer["epubPath"].as_str().expect("an epubPath"))
}

/// screenplay.pdf copied to `scripts/<name>` (and `out/` made).
fn script(sb: &Sandbox, name: &str) -> String {
    sb.mkdir("out");
    sb.copy_fixture("screenplay.pdf", &format!("scripts/{name}"))
        .to_string_lossy()
        .into_owned()
}

fn read(p: &str) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn read_json(p: &str) -> Value {
    serde_json::from_str(&read(p)).unwrap()
}

/// The library is the Sandbox's own empty `library/`.
fn lib(sb: &Sandbox, rel: &str) -> String {
    sb.p(&format!("library/{rel}"))
}

/// The Sandbox's app settings file, written whole.
fn app_settings(sb: &Sandbox, settings: Value) {
    sb.write("config/settings.json", settings.to_string());
}

fn ok(sb: &Sandbox, args: &[&str]) -> (Value, String) {
    let ran = sb.run(args);
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    (ran.json(), ran.stderr)
}

// ---- a conversion renders with the script it is converting ----

#[test]
fn the_same_pdf_converts_differently_with_and_without_its_sidecar() {
    let d = defaults();
    let sb = Sandbox::new();
    let with = script(&sb, "With.pdf");
    sb.write("scripts/With.method-reader.json", tuning());
    let without = script(&sb, "Without.pdf");

    let (a, _) = ok(
        &sb,
        &[
            &with,
            "-o",
            &sb.p("out/with.epub"),
            "--no-fountain",
            "--json",
        ],
    );
    let (b, _) = ok(
        &sb,
        &[
            &without,
            "-o",
            &sb.p("out/without.epub"),
            "--no-fountain",
            "--json",
        ],
    );
    let tuned = css_of(&a);
    let plain = css_of(&b);
    assert_ne!(tuned, plain);
    assert!(tuned.contains(&format!("margin-left: {SIDE}%")));
    assert!(plain.contains(&format!("margin-left: {}%", d["dialogueSideMarginPct"])));
    assert!(!plain.contains(&format!("margin-left: {SIDE}%")));
    // The second key, and the one it enables.
    assert!(tuned.contains(&format!("margin-left: {CUE}%")));
    assert!(!plain.contains(&format!("margin-left: {CUE}%")));
}

#[test]
fn library_renders_with_the_sidecar_it_adopts_on_the_first_conversion() {
    defaults();
    let sb = Sandbox::new();
    let pdf = script(&sb, "Adopted.pdf");
    let beside = sb.write("scripts/Adopted.method-reader.json", tuning());
    let beside = beside.to_string_lossy();

    let (answer, stderr) = ok(&sb, &[&pdf, "--library", "--json"]);
    assert_eq!(answer["ok"], json!(true));
    assert!(css_of(&answer).contains(&format!("margin-left: {SIDE}%")));
    // Said out loud, and on stderr.
    assert!(stderr.contains("saved settings"), "{stderr}");
    assert!(stderr.contains(beside.as_ref()), "{stderr}");
    assert_eq!(answer["settingsPath"], json!(beside));
}

#[test]
fn a_sidecar_already_in_the_library_outranks_the_old_copy_beside_the_pdf() {
    defaults();
    let sb = Sandbox::new();
    let pdf = script(&sb, "Both.pdf");
    // The library's word, written the way `method-reader settings` writes it.
    let prefix = library_output(&pdf, &sb.p("library")).expect("the library folder");
    std::fs::write(format!("{prefix}.method-reader.json"), tuning()).unwrap();
    // The stale copy beside the PDF says 29.
    sb.write(
        "scripts/Both.method-reader.json",
        "{\"dialogueSideMarginPct\":29}\n",
    );

    let (answer, _) = ok(&sb, &[&pdf, "--library", "--json"]);
    assert_eq!(
        answer["settingsPath"],
        json!(format!("{prefix}.method-reader.json"))
    );
    let css = css_of(&answer);
    assert!(css.contains(&format!("margin-left: {SIDE}%")));
    assert!(!css.contains("margin-left: 29%"));
}

#[test]
fn an_explicit_options_json_wins_knob_by_knob_over_the_sidecar() {
    defaults();
    let sb = Sandbox::new();
    let pdf = script(&sb, "Flagged.pdf");
    sb.write("scripts/Flagged.method-reader.json", tuning());
    let flag = json!({ "dialogueSideMarginPct": 13 }).to_string();
    let (answer, stderr) = ok(
        &sb,
        &[
            &pdf,
            "-o",
            &sb.p("out/flagged.epub"),
            "--no-fountain",
            "--json",
            "--options-json",
            &flag,
        ],
    );
    let css = css_of(&answer);
    assert!(css.contains("margin-left: 13%"));
    assert!(!css.contains(&format!("margin-left: {SIDE}%")));
    // The knobs it does not name are still the script's.
    assert!(css.contains(&format!("margin-left: {CUE}%")));
    assert!(stderr.contains("override them"), "{stderr}");
}

#[test]
fn a_malformed_sidecar_is_ignored_out_loud_not_fatally() {
    let d = defaults();
    let sb = Sandbox::new();
    let pdf = script(&sb, "Broken.pdf");
    sb.write(
        "scripts/Broken.method-reader.json",
        "{\"dialogueSideMarginPct\": 7,,,\n",
    );
    let (answer, stderr) = ok(
        &sb,
        &[
            &pdf,
            "-o",
            &sb.p("out/broken.epub"),
            "--no-fountain",
            "--json",
        ],
    );
    assert_eq!(answer["ok"], json!(true));
    assert!(answer.get("settingsPath").is_none());
    // It converted at the defaults rather than half-applying a broken file.
    assert!(css_of(&answer).contains(&format!("margin-left: {}%", d["dialogueSideMarginPct"])));
    assert!(stderr.contains("not a settings object"), "{stderr}");
}

#[test]
fn no_sidecar_says_nothing_and_reports_nothing() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "Quiet.pdf");
    let (answer, stderr) = ok(
        &sb,
        &[
            &pdf,
            "-o",
            &sb.p("out/quiet.epub"),
            "--no-fountain",
            "--json",
        ],
    );
    assert_eq!(stderr, "");
    assert!(answer.get("settingsPath").is_none());
}

// ---- a library conversion saves the settings it started from ----

#[test]
fn the_first_library_conversion_of_an_untuned_script_pins_this_runs_settings_so_a_later_app_default_change_no_longer_reaches_it()
 {
    let d = defaults();
    let sb = Sandbox::new();
    app_settings(
        &sb,
        json!({ "formatDefaults": { "dialogueSideMarginPct": APP_MARGIN } }),
    );
    let pdf = script(&sb, "Pinned.pdf");

    let (answer, _) = ok(&sb, &[&pdf, "--library", "--json"]);
    assert_eq!(answer["ok"], json!(true));
    // The book itself was built at the app default.
    assert!(css_of(&answer).contains(&format!("margin-left: {APP_MARGIN}%")));

    let sidecar = lib(&sb, "Pinned/Pinned.method-reader.json");
    assert!(Path::new(&sidecar).exists());
    let pinned = read_json(&sidecar);
    // The FULL resolved options, not just the one knob.
    assert_eq!(pinned["dialogueSideMarginPct"], json!(APP_MARGIN));
    assert_eq!(pinned["cueAlignment"], d["cueAlignment"]);
    assert_eq!(
        pinned.as_object().unwrap().len(),
        d.as_object().unwrap().len()
    );

    // The regression proper: the app defaults change afterward, and this
    // script's settings do not follow.
    app_settings(
        &sb,
        json!({ "formatDefaults": { "dialogueSideMarginPct": 31 } }),
    );
    let (said, _) = ok(
        &sb,
        &[
            "settings",
            answer["fountainPath"].as_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(said["settings"]["dialogueSideMarginPct"], json!(APP_MARGIN));
}

#[test]
fn a_second_library_conversion_with_a_sidecar_already_present_leaves_it_unchanged_even_when_a_flag_overrides_a_knob()
 {
    defaults();
    let sb = Sandbox::new();
    app_settings(
        &sb,
        json!({ "formatDefaults": { "dialogueSideMarginPct": APP_MARGIN } }),
    );
    let pdf = script(&sb, "Twice.pdf");
    ok(&sb, &[&pdf, "--library", "--json"]);
    let sidecar = lib(&sb, "Twice/Twice.method-reader.json");
    let before = read(&sidecar);

    let flag = json!({ "dialogueSideMarginPct": 25 }).to_string();
    let (answer, _) = ok(&sb, &[&pdf, "--library", "--json", "--options-json", &flag]);
    // The flag won for THIS conversion...
    assert!(css_of(&answer).contains("margin-left: 25%"));
    // ...but flags do not save.
    assert_eq!(read(&sidecar), before);
}

#[test]
fn a_sidecar_beside_the_pdf_is_still_adopted_not_overwritten_by_the_pin() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "Adopted.pdf");
    sb.write(
        "scripts/Adopted.method-reader.json",
        "{\"cueIndentPct\":41}\n",
    );
    ok(&sb, &[&pdf, "--library", "--json"]);
    // Exactly the file that was beside the PDF, unexpanded.
    assert_eq!(
        read_json(&lib(&sb, "Adopted/Adopted.method-reader.json")),
        json!({ "cueIndentPct": 41 })
    );
}

#[test]
fn a_plain_conversion_without_library_writes_no_sidecar_beside_the_input() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "NoLibrary.pdf");
    ok(
        &sb,
        &[
            &pdf,
            "-o",
            &sb.p("out/no-library.epub"),
            "--no-fountain",
            "--json",
        ],
    );
    assert!(!sb.path("scripts/NoLibrary.method-reader.json").exists());
}

#[test]
fn a_conversion_that_fails_after_the_library_folder_opens_leaves_no_sidecar_behind() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "Blocked.pdf");
    // The folder claimed ahead of time, then a DIRECTORY where the book is
    // about to land: the rename onto it fails, so the conversion never
    // finishes.
    let prefix = library_output(&pdf, &sb.p("library")).expect("the library folder");
    std::fs::create_dir(format!("{prefix}.epub")).unwrap();

    let ran = sb.run(&[&pdf, "--library", "--json"]);
    assert_eq!(ran.code, 1, "{}", ran.stdout);
    // The book never landed, so its settings are not recorded either.
    assert!(!Path::new(&format!("{prefix}.method-reader.json")).exists());
    // Nor the book's own temp file.
    let folder = Path::new(&prefix).parent().unwrap();
    let temps: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(temps.is_empty(), "{temps:?}");
}

#[test]
fn a_flag_used_on_the_very_first_library_conversion_is_not_pinned_what_gets_saved_is_what_the_script_started_from()
 {
    defaults();
    let sb = Sandbox::new();
    app_settings(
        &sb,
        json!({ "formatDefaults": { "dialogueSideMarginPct": APP_MARGIN } }),
    );
    let pdf = script(&sb, "FlagFirst.pdf");
    let flag = json!({ "dialogueSideMarginPct": 5 }).to_string();
    let (answer, _) = ok(&sb, &[&pdf, "--library", "--json", "--options-json", &flag]);
    // The flag won for THIS conversion...
    assert!(css_of(&answer).contains("margin-left: 5%"));
    // ...but what is saved is the app default the script started from.
    let pinned = read_json(&lib(&sb, "FlagFirst/FlagFirst.method-reader.json"));
    assert_eq!(pinned["dialogueSideMarginPct"], json!(APP_MARGIN));
}

#[test]
fn a_library_conversion_of_a_fountain_input_saves_nothing_there_is_no_library_fountain_to_attach_it_to()
 {
    let sb = Sandbox::new();
    let input = sb.write(
        "scripts/Loose.fountain",
        "Title: Loose\n\nINT. ROOM - DAY\n\nAction holds.\n",
    );
    let (answer, _) = ok(&sb, &[&input.to_string_lossy(), "--library", "--json"]);
    assert_eq!(answer["ok"], json!(true));
    assert!(!Path::new(&lib(&sb, "Loose/Loose.method-reader.json")).exists());
}

// Not ported: "a sidecar write that fails does not fail the conversion, and
// says so once on stderr". The TypeScript blocks only the sidecar's temp
// file with a bun --preload monkeypatch of fs.writeFileSync; the Rust
// binary has no such seam (fsx::testing's fault is in process only), and no
// filesystem trick fails that one write without failing the book's too.

// ---- keepScriptSettings chooses whether a library conversion pins ----

#[test]
fn off_an_untuned_script_gets_no_sidecar_and_a_later_app_default_change_reaches_it() {
    defaults();
    let sb = Sandbox::new();
    app_settings(
        &sb,
        json!({
            "formatDefaults": { "dialogueSideMarginPct": APP_MARGIN },
            "keepScriptSettings": false,
        }),
    );
    let pdf = script(&sb, "Follows.pdf");
    let (answer, _) = ok(&sb, &[&pdf, "--library", "--json"]);
    // The book itself is still built at the app defaults.
    assert!(css_of(&answer).contains(&format!("margin-left: {APP_MARGIN}%")));
    assert!(!Path::new(&lib(&sb, "Follows/Follows.method-reader.json")).exists());

    // The app defaults move, and this script moves with them.
    app_settings(
        &sb,
        json!({ "formatDefaults": { "dialogueSideMarginPct": 27 } }),
    );
    let (said, _) = ok(
        &sb,
        &[
            "settings",
            answer["fountainPath"].as_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(said["settings"]["dialogueSideMarginPct"], json!(27));
}

#[test]
fn on_stored_explicitly_the_pin_is_written_the_same_as_when_the_key_is_absent() {
    defaults();
    let sb = Sandbox::new();
    app_settings(
        &sb,
        json!({
            "formatDefaults": { "dialogueSideMarginPct": APP_MARGIN },
            "keepScriptSettings": true,
        }),
    );
    let pdf = script(&sb, "Keeps.pdf");
    ok(&sb, &[&pdf, "--library", "--json"]);
    assert_eq!(
        read_json(&lib(&sb, "Keeps/Keeps.method-reader.json"))["dialogueSideMarginPct"],
        json!(APP_MARGIN)
    );
}

#[test]
fn switching_off_never_deletes_or_rewrites_a_sidecar_already_in_the_library_pinned_or_tuned() {
    defaults();
    let sb = Sandbox::new();
    app_settings(
        &sb,
        json!({ "formatDefaults": { "dialogueSideMarginPct": APP_MARGIN } }),
    );
    let pdf = script(&sb, "Already.pdf");

    // ON (absent): the first conversion pins.
    ok(&sb, &[&pdf, "--library", "--json"]);
    let sidecar = lib(&sb, "Already/Already.method-reader.json");
    let pinned = read(&sidecar);

    // OFF now, and the app defaults moved too.
    app_settings(
        &sb,
        json!({
            "keepScriptSettings": false,
            "formatDefaults": { "dialogueSideMarginPct": 31 },
        }),
    );
    let (second, _) = ok(&sb, &[&pdf, "--library", "--json"]);
    assert_eq!(read(&sidecar), pinned);
    assert!(css_of(&second).contains(&format!("margin-left: {APP_MARGIN}%")));

    // A partial, hand-tuned sidecar: not expanded, not removed.
    std::fs::write(&sidecar, "{\"cueIndentPct\":41}\n").unwrap();
    ok(&sb, &[&pdf, "--library", "--json"]);
    assert_eq!(read(&sidecar), "{\"cueIndentPct\":41}\n");
}

#[test]
fn off_a_sidecar_beside_the_pdf_is_still_adopted_because_that_one_is_tuning_not_a_pin() {
    let sb = Sandbox::new();
    app_settings(&sb, json!({ "keepScriptSettings": false }));
    let pdf = script(&sb, "Beside.pdf");
    sb.write(
        "scripts/Beside.method-reader.json",
        "{\"cueIndentPct\":41}\n",
    );
    ok(&sb, &[&pdf, "--library", "--json"]);
    assert_eq!(
        read_json(&lib(&sb, "Beside/Beside.method-reader.json")),
        json!({ "cueIndentPct": 41 })
    );
}

#[test]
fn a_sidecar_write_that_fails_does_not_fail_the_conversion_and_says_so_once_on_stderr() {
    // The debug binary's METHOD_READER_TEST_FAIL_PIN fails only the pin's save,
    // as the TypeScript's preload blocked only the sidecar's temp file.
    let mut sb = Sandbox::new();
    let pdf = script(&sb, "CannotPin.pdf");
    sb.set("METHOD_READER_TEST_FAIL_PIN", "1");
    let ran = sb.run(&[&pdf, "--library", "--json"]);
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    let answer = ran.json();
    assert_eq!(answer["ok"], true);
    // The book is written and correct...
    assert!(Path::new(answer["epubPath"].as_str().unwrap()).exists());
    // ...the save that could not happen leaves no sidecar...
    assert!(!Path::new(&lib(&sb, "CannotPin/CannotPin.method-reader.json")).exists());
    // ...and is said exactly once, not swallowed and not a stack trace.
    let lines: Vec<&str> = ran
        .stderr
        .trim()
        .lines()
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("could not save"), "{lines:?}");
}

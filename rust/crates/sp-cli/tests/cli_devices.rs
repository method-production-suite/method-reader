//! Port of the sp-cli half of tests/cli-devices.test.ts: verb dispatch
//! (`resolveCommand`, `VERBS`), the error envelope, and the settings file a
//! send remembers into. The handlers (devicesCommand, selectDevice,
//! sendCommand) are sp-device's, tested in sp-device/tests/commands.rs;
//! sp-device takes `remember` as a callback, and what this CLI hands it
//! writes the app settings file named by METHOD_READER_CONFIG_DIR.
//!
//! `resolve_command` lives in the binary, so it is driven through the
//! spawned CLI: a verb answers as itself (its own usage, its own handler's
//! refusal), and anything else reaches the conversion, which refuses a file
//! that is not a script as unsupported-type or one that is not there as
//! unreadable.

mod common;

use common::{Sandbox, Tablet, book_at};
use serde_json::{Value, json};

/// The conversion's own refusal for an input that is not a script.
const NOT_A_SCRIPT: &str = "unsupported-type";

fn remembered(sb: &Sandbox, rel: &str) -> Option<Value> {
    std::fs::read_to_string(sb.path(rel))
        .ok()
        .map(|t| serde_json::from_str(&t).expect("settings JSON"))
}

// ---- the error envelope -------------------------------------------------------

#[test]
fn a_contract_error_renders_the_exact_json_error_shape() {
    // The body is exactly two keys: the app decodes {code, message} and
    // nothing else.
    let mut sb = Sandbox::new();
    sb.mkdir("vols/Kindle/documents");
    sb.mkdir("vols/KOBOeReader/.kobo");
    let vols = sb.p("vols");
    sb.set("METHOD_READER_VOLUME_ROOTS", vols);
    let answer = sb
        .run(&["send", &book_at(&sb, "b/Script.epub"), "--json"])
        .json();
    let mut top: Vec<&String> = answer.as_object().unwrap().keys().collect();
    top.sort();
    assert_eq!(top, ["error", "ok"]);
    assert_eq!(answer["ok"], false);
    let mut keys: Vec<&String> = answer["error"].as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["code", "message"]);
    assert_eq!(answer["error"]["code"], "ambiguous-device");
    let message = answer["error"]["message"].as_str().unwrap();
    assert!(message.contains(&sb.p("vols/Kindle")), "{message}");
}

// ---- resolveCommand -----------------------------------------------------------

#[test]
fn the_bare_verb_dispatches_when_no_file_of_that_name_exists() {
    let sb = Sandbox::new();
    let plain = sb.run(&["devices"]);
    assert_eq!(plain.code, 0);
    assert_eq!(plain.stdout.trim(), "no devices connected");
    assert_eq!(
        sb.run(&["devices", "--json"]).json(),
        json!({ "ok": true, "devices": [] })
    );
    // send's own handler answers: the book is checked first.
    let send = sb.run(&["send", "Script.epub", "--device", "x", "--json"]);
    assert_eq!(send.error_code(), "unreadable");
}

#[test]
fn a_file_literally_named_devices_beats_the_verb() {
    // Without the exists() check this passes as a verb and the file becomes
    // silently unconvertible.
    let sb = Sandbox::new();
    sb.write("devices", "not a pdf");
    let plain = sb.run(&["devices"]);
    assert_eq!(plain.code, 1);
    assert!(plain.stderr.contains("unsupported input type"), "{plain:?}");
    assert_eq!(sb.run(&["devices", "--json"]).error_code(), NOT_A_SCRIPT);
}

#[test]
fn an_explicit_path_is_never_a_verb_even_with_no_such_file() {
    // `./devices` is how a person disambiguates when a verb would win.
    let sb = Sandbox::new();
    assert_eq!(sb.run(&["./devices", "--json"]).error_code(), NOT_A_SCRIPT);
    assert_eq!(
        sb.run(&["devices.pdf", "--json"]).error_code(),
        "unreadable"
    );
    assert_eq!(
        sb.run(&[&sb.p("elsewhere/send"), "--json"]).error_code(),
        NOT_A_SCRIPT
    );
}

#[test]
fn only_the_first_argument_can_be_a_verb() {
    // A script sent with --title send must still convert.
    let sb = Sandbox::new();
    assert_eq!(sb.run(&["--json", "devices"]).error_code(), NOT_A_SCRIPT);
    assert_eq!(
        sb.run(&["Script.pdf", "--title", "send", "--json"])
            .error_code(),
        "unreadable"
    );
}

#[test]
fn unknown_words_and_an_empty_argv_are_the_default_path() {
    let sb = Sandbox::new();
    assert_eq!(sb.run(&["frobnicate", "--json"]).error_code(), NOT_A_SCRIPT);
    // Nothing to convert: the conversion's usage, and a failure.
    let empty = sb.run(&[]);
    assert_eq!(empty.code, 1);
    assert!(empty.stdout.contains("Usage:"), "{empty:?}");
    let help = sb.run(&["--help"]);
    assert_eq!(help.code, 0);
    assert!(
        help.stdout
            .contains("method-reader <input.pdf | input.fountain>")
    );
}

#[test]
fn verbs_is_the_single_list_of_known_verbs() {
    // A pin, so that adding a verb is deliberate: every name here is a name
    // a person can no longer give a file without writing ./ in front of it.
    // Each answers its own --help; a word near one is still a file.
    let sb = Sandbox::new();
    for verb in [
        "devices",
        "send",
        "settings",
        "export",
        "update-decision",
        "update-should-check",
        "kfx-status",
        "kfx-install",
        "app-settings",
        "reveal",
        "routes",
        "route",
        "eject",
    ] {
        let ran = sb.run(&[verb, "--help", "--json"]);
        assert_eq!(ran.code, 0, "{verb}");
        let usage = ran.json()["usage"].as_str().unwrap().to_owned();
        assert!(usage.contains(&format!("method-reader {verb}")), "{verb}");
    }
    for word in [
        "update",
        "kfx",
        "device",
        "Devices",
        "remember",
        "eject-all",
    ] {
        assert_eq!(
            sb.run(&[word, "--json"]).error_code(),
            NOT_A_SCRIPT,
            "{word}"
        );
    }
}

// ---- send remembers the route that worked ---------------------------------------

/// A sandbox whose mount parent holds the named invented reader folders.
fn with_readers(dirs: &[&str]) -> Sandbox {
    let mut sb = Sandbox::new();
    sb.mkdir("vols");
    for d in dirs {
        sb.mkdir(&format!("vols/{d}"));
    }
    let vols = sb.p("vols");
    sb.set("METHOD_READER_VOLUME_ROOTS", vols);
    sb
}

fn docked(sb: &mut Sandbox, post: u16) -> Tablet {
    let tablet = Tablet::start(Some(200), Some(post));
    sb.set("METHOD_READER_REMARKABLE_ENDPOINT", tablet.url.clone());
    tablet
}

#[cfg(unix)]
fn chmod(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn a_volume_send_remembers_the_device_kind_never_its_volume_and_keeps_other_keys() {
    // A path would go stale the moment the reader was unplugged; the kind is
    // what the send panel can find again.
    let sb = with_readers(&["Kindle/documents"]);
    sb.write(
        "config/settings.json",
        r#"{"lastRoute":"save-epub","libraryPath":"/kept"}"#,
    );
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(
        remembered(&sb, "config/settings.json"),
        Some(json!({ "lastRoute": "device:kindle", "libraryPath": "/kept" }))
    );
}

#[test]
fn a_kobo_send_remembers_device_kobo() {
    let sb = with_readers(&["KOBOeReader/.kobo"]);
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(
        remembered(&sb, "config/settings.json"),
        Some(json!({ "lastRoute": "device:kobo" }))
    );
}

#[test]
fn a_remarkable_upload_remembers_remarkable() {
    let mut sb = Sandbox::new();
    let _tablet = docked(&mut sb, 200);
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(
        remembered(&sb, "config/settings.json"),
        Some(json!({ "lastRoute": "remarkable" }))
    );
}

#[cfg(unix)]
#[test]
fn a_failed_copy_remembers_nothing() {
    // A Kobo whose folder cannot be written to: the copy fails there.
    let sb = with_readers(&["KOBOeReader/.kobo"]);
    let kobo = sb.path("vols/KOBOeReader");
    chmod(&kobo, 0o555);
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    chmod(&kobo, 0o755);
    assert_eq!(ran.error_code(), "send-failed");
    assert!(!sb.path("config/settings.json").exists());
}

#[test]
fn a_failed_upload_remembers_nothing() {
    let mut sb = Sandbox::new();
    let _tablet = docked(&mut sb, 500);
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.error_code(), "send-failed");
    assert!(!sb.path("config/settings.json").exists());
}

#[test]
fn a_file_the_remarkable_cannot_read_remembers_nothing() {
    let mut sb = Sandbox::new();
    let _tablet = docked(&mut sb, 200);
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.azw3"), "--json"]);
    assert_eq!(ran.error_code(), "unsupported-file");
    assert!(!sb.path("config/settings.json").exists());
}

#[cfg(unix)]
#[test]
fn a_read_only_settings_folder_does_not_fail_the_send_the_book_is_on_the_reader() {
    let mut sb = with_readers(&["Kindle/documents"]);
    let folder = sb.mkdir("readonly");
    chmod(&folder, 0o555);
    let config = sb.p("readonly");
    sb.set("METHOD_READER_CONFIG_DIR", config);
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    chmod(&folder, 0o755);
    assert_eq!(ran.code, 0, "{ran:?}");
    let destination = ran.json()["destination"].as_str().unwrap().to_owned();
    assert_eq!(std::fs::read_to_string(destination).unwrap(), "book-bytes");
    assert!(!sb.path("readonly/settings.json").exists());
}

#[test]
fn a_settings_path_that_cannot_exist_its_parent_is_a_file_does_not_fail_the_send() {
    // ENOTDIR on every platform and for every user, root included, where a
    // read-only folder would not stop root.
    let mut sb = Sandbox::new();
    let blocker = sb.write("block/a-file", "x");
    sb.set(
        "METHOD_READER_CONFIG_DIR",
        format!("{}/config", blocker.display()),
    );
    let tablet = docked(&mut sb, 200);
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 0, "{ran:?}");
    assert_eq!(ran.json()["uploaded"], true);
    let posted: Vec<String> = tablet
        .requests()
        .into_iter()
        .filter(|r| r.starts_with("POST "))
        .collect();
    assert_eq!(posted, ["POST /upload"]);
}

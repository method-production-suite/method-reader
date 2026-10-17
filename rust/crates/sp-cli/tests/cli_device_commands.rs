//! Port of tests/cli-device-commands.test.ts: `method-reader devices` and
//! `method-reader send` through the spawned CLI, the rule that a file named like
//! a verb is converted, and --help / --version in --json.
//!
//! Every run is in a Sandbox: the mount parent is an empty scratch folder
//! (or one a test fills with invented readers), the reMarkable is a closed
//! port or a local stand-in, and the settings file a send remembers into is
//! in scratch. No test here can read a real mount, reach the real tablet
//! address or write the real settings.

mod common;

use common::{Sandbox, Tablet, book_at};
use serde_json::{Value, json};

/// A sandbox whose mount parent holds one invented Kobo.
fn with_kobo() -> Sandbox {
    let mut sb = Sandbox::new();
    sb.mkdir("vols/KOBOeReader/.kobo");
    let vols = sb.p("vols");
    sb.set("METHOD_READER_VOLUME_ROOTS", vols);
    sb
}

/// A sandbox whose mount parent holds one invented Kindle; and its volume.
fn with_kindle() -> (Sandbox, String) {
    let mut sb = Sandbox::new();
    sb.mkdir("vols/Kindle/documents");
    let vols = sb.p("vols");
    sb.set("METHOD_READER_VOLUME_ROOTS", vols);
    let volume = sb.p("vols/Kindle");
    (sb, volume)
}

/// The tablet stand-in, set as the sandbox's reMarkable.
fn docked(sb: &mut Sandbox, get: Option<u16>, post: Option<u16>) -> Tablet {
    let tablet = Tablet::start(get, post);
    sb.set("METHOD_READER_REMARKABLE_ENDPOINT", tablet.url.clone());
    tablet
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

// ---- method-reader devices ---------------------------------------------------------

#[test]
fn devices_json_reports_the_injected_device_and_exits_0() {
    let sb = with_kobo();
    let ran = sb.run(&["devices", "--json"]);
    assert_eq!(ran.code, 0);
    let result = ran.json();
    assert_eq!(result["ok"], true);
    let devices = result["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["kind"], "kobo");
    assert_eq!(devices[0]["name"], "KOBOeReader");
    assert_eq!(devices[0]["id"], devices[0]["volume"]);
    assert!(
        devices[0]["volume"]
            .as_str()
            .unwrap()
            .ends_with("KOBOeReader")
    );
}

#[test]
fn nothing_connected_is_ok_true_with_an_empty_list_and_exit_0() {
    // An empty list is an answer, not an error: an implementation that
    // reported no-devices here (as `send` correctly does) fails on BOTH.
    let sb = Sandbox::new();
    let ran = sb.run(&["devices", "--json"]);
    assert_eq!(ran.code, 0);
    assert_eq!(ran.json(), json!({ "ok": true, "devices": [] }));
}

#[test]
fn human_output_is_one_line_per_device_on_stdout() {
    let sb = with_kobo();
    let ran = sb.run(&["devices"]);
    assert_eq!(ran.code, 0);
    assert_eq!(ran.stdout.trim().lines().count(), 1);
    assert!(ran.stdout.contains("KOBOeReader"));
    assert!(ran.stdout.contains("kobo"));
}

#[test]
fn human_output_says_so_when_nothing_is_connected() {
    let sb = Sandbox::new();
    let ran = sb.run(&["devices"]);
    assert_eq!(ran.code, 0);
    assert_eq!(ran.stdout.trim(), "no devices connected");
}

#[test]
fn an_unknown_flag_on_devices_json_is_still_one_json_object() {
    // The pre-scan case: the parser fails before it could report the mode.
    let sb = Sandbox::new();
    let ran = sb.run(&["devices", "--json", "--no-such-flag"]);
    assert_eq!(ran.code, 1);
    let result = ran.json();
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "usage");
}

#[test]
fn devices_takes_no_positional_argument() {
    let sb = Sandbox::new();
    let ran = sb.run(&["devices", "extra", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
}

#[test]
fn a_real_remarkable_stub_appears_in_the_listing() {
    let mut sb = Sandbox::new();
    let _tablet = docked(&mut sb, Some(200), Some(200));
    assert_eq!(
        sb.run(&["devices", "--json"]).json()["devices"],
        json!([{ "id": "remarkable", "kind": "remarkable", "name": "reMarkable", "volume": null }])
    );
}

#[test]
fn quick_lists_mounted_readers_without_asking_for_a_remarkable() {
    let (mut sb, _) = with_kindle();
    let tablet = docked(&mut sb, Some(200), Some(200));
    let ran = sb.run(&["devices", "--quick", "--json"]);
    assert_eq!(ran.code, 0);
    let kinds: Vec<Value> = ran.json()["devices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["kind"].clone())
        .collect();
    assert_eq!(kinds, vec![json!("kindle")]);
    assert!(tablet.requests().is_empty());
}

// ---- method-reader send ------------------------------------------------------------

#[test]
fn send_json_reports_the_device_and_the_destination_and_the_bytes_moved() {
    let (sb, volume) = with_kindle();
    let file = book_at(&sb, "b/Script.epub");
    let ran = sb.run(&["send", &file, "--json"]);
    assert_eq!(ran.code, 0);
    let result = ran.json();
    assert_eq!(result["ok"], true);
    assert_eq!(
        result["device"],
        json!({ "id": volume, "kind": "kindle", "name": "Kindle" })
    );
    let destination = common::join(&volume, "documents/Script.epub");
    assert_eq!(result["destination"], destination.as_str());
    assert_eq!(read(&destination), "book-bytes");
    // It worked, so it is remembered: by kind, in the settings file under
    // METHOD_READER_CONFIG_DIR, as the send panel's choice next time.
    assert_eq!(
        serde_json::from_str::<Value>(&read(&sb.p("config/settings.json"))).unwrap(),
        json!({ "lastRoute": "device:kindle" })
    );
}

#[test]
fn a_send_that_fails_remembers_nothing() {
    let sb = Sandbox::new();
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "no-devices");
    assert!(!sb.path("config/settings.json").exists());
}

#[test]
fn a_remarkable_send_reports_uploaded_true_and_no_destination_key() {
    let mut sb = Sandbox::new();
    let tablet = docked(&mut sb, Some(200), Some(200));
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 0);
    let result = ran.json();
    assert_eq!(result["uploaded"], true);
    // The key must be ABSENT, not null: the field is omitted.
    assert!(result.get("destination").is_none(), "{result}");
    let posted: Vec<String> = tablet
        .requests()
        .into_iter()
        .filter(|r| r.starts_with("POST "))
        .collect();
    assert_eq!(posted, ["POST /upload"]);
}

#[test]
fn device_picks_one_of_several_and_the_others_are_untouched() {
    let mut sb = Sandbox::new();
    sb.mkdir("vols/Kindle/documents");
    sb.mkdir("vols/KOBOeReader/.kobo");
    let vols = sb.p("vols");
    sb.set("METHOD_READER_VOLUME_ROOTS", vols);
    let kobo = sb.p("vols/KOBOeReader");
    let ran = sb.run(&[
        "send",
        &book_at(&sb, "b/Script.epub"),
        "--device",
        &kobo,
        "--json",
    ]);
    assert_eq!(ran.code, 0);
    let result = ran.json();
    assert_eq!(result["device"]["kind"], "kobo");
    assert_eq!(result["destination"], common::join(&kobo, "Script.epub"));
    // Two devices were connected: an implementation that ignored --device
    // and took the first would have written into the Kindle instead.
    assert_eq!(read(&common::join(&kobo, "Script.epub")), "book-bytes");
    assert!(!sb.path("vols/Kindle/documents/Script.epub").exists());
}

#[test]
fn several_connected_and_no_device_is_ambiguous_device_naming_both() {
    let mut sb = Sandbox::new();
    sb.mkdir("vols/Kindle/documents");
    sb.mkdir("vols/KOBOeReader/.kobo");
    let vols = sb.p("vols");
    sb.set("METHOD_READER_VOLUME_ROOTS", vols);
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "ambiguous-device");
    let message = ran.error_message();
    assert!(message.contains(&sb.p("vols/Kindle")), "{message}");
    assert!(message.contains(&sb.p("vols/KOBOeReader")), "{message}");
}

#[test]
fn nothing_connected_is_no_devices() {
    let sb = Sandbox::new();
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "no-devices");
}

#[test]
fn an_id_nothing_matches_is_unknown_device() {
    let (sb, _) = with_kindle();
    let ran = sb.run(&[
        "send",
        &book_at(&sb, "b/Script.epub"),
        "--device",
        "/nope",
        "--json",
    ]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unknown-device");
}

#[test]
fn an_extension_remarkable_cannot_read_is_unsupported_file() {
    let mut sb = Sandbox::new();
    let tablet = docked(&mut sb, Some(200), Some(200));
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.azw3"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unsupported-file");
    assert!(!tablet.requests().iter().any(|r| r.starts_with("POST ")));
}

#[test]
fn a_tablet_that_refuses_the_upload_is_send_failed() {
    let mut sb = Sandbox::new();
    let _tablet = docked(&mut sb, Some(200), Some(500));
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "send-failed");
}

#[test]
fn no_file_argument_is_a_usage_error() {
    let sb = Sandbox::new();
    let ran = sb.run(&["send", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
}

#[test]
fn a_file_that_is_not_there_is_unreadable() {
    // Not no-devices: the file is checked before the device list is built.
    let sb = Sandbox::new();
    let ran = sb.run(&["send", &sb.p("ghost.epub"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unreadable");
}

#[test]
fn send_human_output_names_the_device_and_the_destination() {
    let (sb, volume) = with_kindle();
    let ran = sb.run(&["send", &book_at(&sb, "b/Script.epub")]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.contains("Kindle"));
    assert!(
        ran.stdout
            .contains(&common::join(&volume, "documents/Script.epub"))
    );
}

// ---- verb dispatch does not capture files -----------------------------------

#[test]
fn a_file_literally_named_devices_converts_instead_of_listing() {
    // `devices` has no extension, so the conversion rejects it as
    // unsupported-type: the proof that the FILE won. The verb would have
    // printed {"ok":true,"devices":[...]} and exited 0.
    let sb = Sandbox::new();
    let dir = sb.mkdir("shadow");
    sb.write("shadow/devices", "not a pdf");
    let ran = sb.run_in(&dir, &["devices", "--json"]);
    assert_eq!(ran.code, 1);
    let result = ran.json();
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "unsupported-type");
}

#[test]
fn the_bare_word_lists_devices_in_the_same_cwd_with_no_such_file() {
    // Same command, same cwd shape, only the file removed.
    let sb = Sandbox::new();
    let dir = sb.mkdir("shadow");
    let ran = sb.run_in(&dir, &["devices", "--json"]);
    assert_eq!(ran.code, 0);
    assert_eq!(ran.json(), json!({ "ok": true, "devices": [] }));
}

#[test]
fn dot_slash_devices_always_means_the_file() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("shadow");
    sb.write("shadow/devices", "not a pdf");
    let ran = sb.run_in(&dir, &["./devices", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unsupported-type");
}

#[test]
fn a_file_named_send_converts_rather_than_being_a_command() {
    // Without the shadowing rule this is a usage error from `send` with no
    // file argument; with it, the file is the input and its type is wrong.
    let sb = Sandbox::new();
    let dir = sb.mkdir("shadow");
    sb.write("shadow/send", "not a pdf");
    let ran = sb.run_in(&dir, &["send", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unsupported-type");
}

// ---- --json --help and --version --------------------------------------------

#[test]
fn json_help_emits_json_with_ok_true_and_non_empty_usage() {
    let sb = Sandbox::new();
    let ran = sb.run(&["--json", "--help"]);
    assert_eq!(ran.code, 0);
    let result = ran.json();
    assert_eq!(result["ok"], true);
    assert!(!result["usage"].as_str().unwrap().is_empty());
}

#[test]
fn json_version_emits_json_with_ok_true_and_matching_version() {
    let sb = Sandbox::new();
    let ran = sb.run(&["--json", "--version"]);
    assert_eq!(ran.code, 0);
    let result = ran.json();
    assert_eq!(result["ok"], true);
    assert_eq!(result["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn plain_help_without_json_prints_the_human_text_not_json() {
    let sb = Sandbox::new();
    let ran = sb.run(&["--help"]);
    assert_eq!(ran.code, 0);
    assert!(!ran.stdout.trim().starts_with('{'));
    assert!(ran.stdout.contains("Usage:"));
}

#[test]
fn devices_json_help_is_the_devices_usage_not_the_conversion_usage() {
    let sb = Sandbox::new();
    let ran = sb.run(&["devices", "--json", "--help"]);
    assert_eq!(ran.code, 0);
    let result = ran.json();
    assert_eq!(result["ok"], true);
    // A verb's help lists the verb's OWN flags; --device is send's.
    let usage = result["usage"].as_str().unwrap();
    assert!(usage.contains("method-reader devices"));
    assert!(!usage.contains("--mobi"));
    assert!(!usage.contains("--device"));
}

#[test]
fn send_json_help_is_the_send_usage_naming_device_and_not_mobi() {
    let sb = Sandbox::new();
    let ran = sb.run(&["send", "--json", "--help"]);
    assert_eq!(ran.code, 0);
    let result = ran.json();
    assert_eq!(result["ok"], true);
    let usage = result["usage"].as_str().unwrap();
    assert!(usage.contains("--device"));
    assert!(!usage.contains("--mobi"));
    assert!(!usage.contains("--options"));
}

#[test]
fn plain_send_help_prints_the_send_usage_as_human_text() {
    let sb = Sandbox::new();
    let ran = sb.run(&["send", "--help"]);
    assert_eq!(ran.code, 0);
    assert!(!ran.stdout.trim().starts_with('{'));
    assert!(ran.stdout.contains("--device"));
    assert!(!ran.stdout.contains("--mobi"));
}

// ---- flag before verb ---------------------------------------------------------

#[test]
fn json_devices_says_the_verb_must_come_first() {
    // Only the first argument can be a verb, so this reaches the
    // conversion and fails as unsupported-type, naming the cause and cure.
    let sb = Sandbox::new();
    let ran = sb.run(&["--json", "devices"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unsupported-type");
    assert_eq!(
        ran.error_message(),
        "unsupported input type \"\": expected .pdf, .fountain, or .txt. Did you mean `method-reader devices`? The verb must come first"
    );
}

#[test]
fn json_send_gets_the_hint_naming_send_not_devices() {
    let sb = Sandbox::new();
    assert!(
        sb.run(&["--json", "send"])
            .error_message()
            .contains("method-reader send")
    );
}

#[test]
fn an_extension_less_file_that_is_not_a_verb_gets_no_hint() {
    // A real script saved as `Makefile` must not be told to run a command
    // that does not exist.
    let sb = Sandbox::new();
    let ran = sb.run(&["--json", "Makefile"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unsupported-type");
    assert!(!ran.error_message().to_lowercase().contains("did you mean"));
}

#[test]
fn a_real_file_named_devices_gets_no_hint_either() {
    // Then the person meant the file: that is the shadowing rule.
    let sb = Sandbox::new();
    let dir = sb.mkdir("shadow");
    sb.write("shadow/devices", "not a pdf");
    let ran = sb.run_in(&dir, &["--json", "devices"]);
    assert!(!ran.error_message().to_lowercase().contains("did you mean"));
}

// ---- devices rejects flags it cannot act on ---------------------------------

#[test]
fn devices_without_device_still_lists_so_the_guard_is_not_blanket() {
    let sb = Sandbox::new();
    let ran = sb.run(&["devices", "--json"]);
    assert_eq!(ran.code, 0);
    assert_eq!(ran.json(), json!({ "ok": true, "devices": [] }));
}

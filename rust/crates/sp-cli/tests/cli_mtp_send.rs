//! Port of the two end-to-end runs in branch mtp-kindle's
//! tests/cli-mtp-send.test.ts: `send` to a Kindle that connects over MTP,
//! through the spawned CLI. MTP is off in this build
//! (sp_device::mtp::MTP_SHIPPED is false) and on only when
//! METHOD_READER_MTP_HELPER names a helper, which these runs do: a stand-in
//! shell script, beside stand-in `launchctl` and `pkill`, each logging what
//! it was asked. No real USB device, no real ptpcamerad, no real settings.
//! The in-process half is sp-device's (tests/commands.rs) and sp-engine's
//! (the route list).

#![cfg(unix)]

mod common;

use common::{Sandbox, book_at, script};
use serde_json::{Value, json};
use sp_device::camera_import::CAMERA_IMPORT_NOTE;

fn uid() -> String {
    String::from_utf8(
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .expect("id -u")
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned()
}

fn log_lines(sb: &Sandbox) -> Vec<String> {
    std::fs::read_to_string(sb.path("calls.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn lists_the_mtp_kindle_copies_through_the_helper_and_pauses_only_on_a_mac() {
    let mut sb = Sandbox::new();
    let log = sb.p("calls.log");
    let kindle = json!({ "vendorId": 6473, "productId": 39297, "serial": "G000INVENTED01", "model": "Kindle", "kindle": true });
    let listing = json!({ "ok": true, "devices": [kindle] }).to_string();
    let helper = script(
        &sb.path("tools/method-reader-mtp"),
        &format!(
            "echo \"helper $*\" >> \"{log}\"\ncase \"$1\" in\n  list) printf '%s\\n' '{listing}' ;;\n  put) printf '%s\\n' '{{\"ok\":true,\"path\":\"documents/Field Station.kfx\",\"bytes\":3,\"method\":\"rename\"}}' ;;\nesac"
        ),
    );
    let launchctl = script(
        &sb.path("tools/launchctl"),
        &format!("echo \"launchctl $*\" >> \"{log}\""),
    );
    let pkill = script(
        &sb.path("tools/pkill"),
        &format!("echo \"pkill $*\" >> \"{log}\"; exit 1"),
    );
    sb.set("METHOD_READER_MTP_HELPER", helper.to_string_lossy());
    sb.set("METHOD_READER_LAUNCHCTL", launchctl.to_string_lossy());
    sb.set("METHOD_READER_PKILL", pkill.to_string_lossy());
    let file = book_at(&sb, "lib/Field Station.kfx");
    let darwin = cfg!(target_os = "macos");

    let listed = sb.run(&["devices", "--quick", "--json"]);
    let mut device = json!({
        "id": "mtp:G000INVENTED01", "kind": "kindle", "name": "Kindle", "volume": null, "transport": "mtp",
    });
    if darwin {
        device["copyNote"] = Value::String(CAMERA_IMPORT_NOTE.into());
    }
    assert_eq!(listed.json()["devices"], json!([device]));

    let sent = sb.run(&["send", &file, "--json"]);
    assert_eq!(sent.code, 0, "{sent:?}");
    assert_eq!(
        sent.json(),
        json!({
            "ok": true,
            "device": { "id": "mtp:G000INVENTED01", "kind": "kindle", "name": "Kindle" },
            "destination": "documents/Field Station.kfx",
            "safeToUnplug": true,
            "cameraImportPaused": darwin,
        })
    );

    let calls = log_lines(&sb);
    let wait = if darwin { " --wait-ms 5000" } else { "" };
    let put = format!("helper put G000INVENTED01 {file} documents/Field Station.kfx{wait}");
    assert!(calls.contains(&put), "{calls:?}");
    let others: Vec<&String> = calls.iter().filter(|c| !c.starts_with("helper")).collect();
    if darwin {
        let uid = uid();
        assert_eq!(
            others,
            [
                &format!("launchctl print-disabled user/{uid}"),
                &format!("launchctl disable user/{uid}/com.apple.ptpcamerad"),
                &"pkill -9 -x ptpcamerad".to_owned(),
                &format!("launchctl enable user/{uid}/com.apple.ptpcamerad"),
            ]
        );
        assert!(!sb.path("config/camera-import-paused").exists());
    } else {
        assert!(others.is_empty(), "{others:?}");
    }
    assert_eq!(
        serde_json::from_str::<Value>(
            &std::fs::read_to_string(sb.path("config/settings.json")).unwrap()
        )
        .unwrap()["lastRoute"],
        "device:kindle"
    );
}

#[test]
fn a_marker_a_crashed_run_left_behind_is_repaired_by_the_next_run_whatever_it_is() {
    let mut sb = Sandbox::new();
    let log = sb.p("calls.log");
    let launchctl = script(
        &sb.path("tools/launchctl"),
        &format!("echo \"launchctl $*\" >> \"{log}\""),
    );
    sb.set("METHOD_READER_LAUNCHCTL", launchctl.to_string_lossy());
    sb.write("config/camera-import-paused", "{\"uid\":501}\n");
    let ran = sb.run(&["--version", "--json"]);
    assert_eq!(ran.code, 0);
    if cfg!(target_os = "macos") {
        assert_eq!(
            log_lines(&sb),
            [format!(
                "launchctl enable user/{}/com.apple.ptpcamerad",
                uid()
            )]
        );
        assert!(!sb.path("config/camera-import-paused").exists());
    } else {
        assert!(!sb.path("calls.log").exists());
    }
}

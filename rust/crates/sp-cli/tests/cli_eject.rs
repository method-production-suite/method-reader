//! Port of the spawned half of tests/cli-eject.test.ts: refusals and --help
//! only. The handler and the real runner are sp-device's
//! (sp-device/tests/eject.rs). The Sandbox's mount parent is empty, so the
//! engine lists no reader and nothing here can reach a real eject tool.

mod common;

use common::Sandbox;
use serde_json::json;

#[test]
fn a_volume_the_engine_does_not_list_is_unknown_device_on_stdout_as_json() {
    let sb = Sandbox::new();
    let ran = sb.run(&["eject", "/Volumes/Backup", "--json"]);
    assert_ne!(ran.code, 0);
    assert_eq!(
        ran.json(),
        json!({
            "ok": false,
            "error": { "code": "unknown-device", "message": "not a reader Method Reader can see: /Volumes/Backup" },
        })
    );
}

#[test]
fn no_volume_or_two_is_usage() {
    let sb = Sandbox::new();
    for args in [vec!["eject", "--json"], vec!["eject", "/a", "/b", "--json"]] {
        assert_eq!(sb.run(&args).error_code(), "usage", "{args:?}");
    }
}

#[test]
fn eject_help_describes_the_verb() {
    let sb = Sandbox::new();
    let ran = sb.run(&["eject", "--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.contains("method-reader eject <volume>"));
}

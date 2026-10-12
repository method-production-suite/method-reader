//! The Rust app must trust the key and ask the endpoint every Tauri window
//! was built with, or the first Rust release strands one side. Read from
//! desktop/src-tauri/tauri.conf.json while that file exists; at the cutover
//! (wave 4) it is deleted with the window and this test goes with it.

use std::path::Path;

#[test]
fn the_constants_are_the_tauri_window_s() {
    let conf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../desktop/src-tauri/tauri.conf.json");
    let text = std::fs::read_to_string(&conf).expect("desktop/src-tauri/tauri.conf.json");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["identifier"], sp_update::IDENTIFIER);
    assert_eq!(v["plugins"]["updater"]["pubkey"], sp_update::PUBKEY);
    assert_eq!(
        v["plugins"]["updater"]["endpoints"],
        serde_json::json!([sp_update::ENDPOINT])
    );
    // No window ever turned these on, so the Rust release may rely on
    // their defaults (the signed version is optional for a window).
    assert!(
        v["plugins"]["updater"]
            .get("requireSignedVersion")
            .is_none()
    );
    assert!(
        v["plugins"]["updater"]
            .get("dangerousInsecureTransportProtocol")
            .is_none()
    );
}

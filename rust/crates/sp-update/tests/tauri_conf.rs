//! The Rust app must trust the key every Tauri window was built with, and
//! know their identifier (now [`sp_update::LEGACY_IDENTIFIER`]: 0.13 took
//! its own and moves what the old one kept), or one side is stranded. The
//! endpoint those windows ask is [`sp_update::LEGACY_ENDPOINT`]: the
//! repository moved, GitHub redirects it, and Method Reader asks the new
//! one itself. The values
//! below are desktop/src-tauri/tauri.conf.json's as shipped in 0.11.1 (the
//! file went with the Tauri window at the cutover; `git show
//! v0.11.1:desktop/src-tauri/tauri.conf.json` has it). No window ever set
//! `requireSignedVersion` or `dangerousInsecureTransportProtocol`, so the
//! Rust release may rely on their defaults.

/// `identifier`, `plugins.updater.pubkey` and `plugins.updater.endpoints`
/// of every Tauri window from 0.7.1 to 0.11.1.
const TAURI_IDENTIFIER: &str = "com.darkwell.screepub";
const TAURI_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEU0RDA2NjhENTRGNkVCMzQKUldRMDYvWlVqV2JRNUQzRkpQMmVJZlQ4bTdaT2M5cmN2V05oVTJGdURUY0NwRDBRbEQ1c013UnYK";
const TAURI_ENDPOINTS: &[&str] =
    &["https://github.com/ssandweiss/screepub/releases/latest/download/latest.json"];

#[test]
fn the_constants_are_the_tauri_window_s() {
    assert_eq!(sp_update::LEGACY_IDENTIFIER, TAURI_IDENTIFIER);
    assert_eq!(sp_update::IDENTIFIER, "com.methodreader.desktop");
    assert_eq!(sp_update::PUBKEY, TAURI_PUBKEY);
    assert_eq!(&[sp_update::LEGACY_ENDPOINT], TAURI_ENDPOINTS);
    assert_ne!(sp_update::ENDPOINT, sp_update::LEGACY_ENDPOINT);
}

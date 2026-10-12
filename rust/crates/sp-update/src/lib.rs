//! Screepub's updater, in Rust: the transport half that
//! tauri-plugin-updater 2.12.0 did for the Tauri window (fetch
//! `latest.json`, verify the minisign signature, download, swap the
//! bundle) and the restart tauri-plugin-process did, plus the engine gate
//! the window's `app.js` kept (refuse engine calls once the bundle is
//! swapped, wait for quiet before restarting).
//!
//! WHETHER a release is newer stays sp-app's call
//! (`model::update_compare`), exactly as it was the engine's in the Tauri
//! window: this crate answers the plugin's question (semver, `>`), and the
//! window applies its own comparator on top.
//!
//! Every rule here was read off the plugin's source, cited per function, so
//! the Rust app treats `latest.json` exactly as the 0.11.x windows do. The
//! design is docs/superpowers/specs/2026-10-11-rust-release-design.md.

pub mod flow;
pub mod gate;
pub mod install;
pub mod manifest;
pub mod restart;
pub mod sqlite;
pub mod target;
pub mod transport;
pub mod verify;
pub mod webkit;

pub use manifest::{Platform, Release};
pub use target::{Installer, TargetSet};
pub use transport::{Config, Update, UpdateError, check, download};

/// Where every Screepub, Tauri or Rust, looks for a new version. Equal to
/// `plugins.updater.endpoints[0]` in desktop/src-tauri/tauri.conf.json (a
/// test holds them together while that file exists).
pub const ENDPOINT: &str =
    "https://github.com/ssandweiss/screepub/releases/latest/download/latest.json";

/// The minisign public key every Screepub trusts, as tauri.conf.json
/// carries it: base64 of the key file's two lines. Its key id is
/// E4D0668D54F6EB34.
pub const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEU0RDA2NjhENTRGNkVCMzQKUldRMDYvWlVqV2JRNUQzRkpQMmVJZlQ4bTdaT2M5cmN2V05oVTJGdURUY0NwRDBRbEQ1c013UnYK";

/// The bundle identifier the Tauri window has had since 0.8.0, and the
/// Rust app keeps.
pub const IDENTIFIER: &str = "com.darkwell.screepub";

/// The endpoint this build was baked with. A release bakes [`ENDPOINT`];
/// the handover rehearsal (tools/rehearse-handover.sh) bakes a local one by
/// setting `SCREEPUB_UPDATE_ENDPOINT` while it compiles, the way Tauri
/// bakes tauri.conf.json into the window. Nothing reads it at run time.
pub fn baked_endpoint() -> &'static str {
    option_env!("SCREEPUB_UPDATE_ENDPOINT").unwrap_or(ENDPOINT)
}

/// The public key this build was baked with: [`PUBKEY`], or the
/// rehearsal's throwaway key (`SCREEPUB_UPDATE_PUBKEY` at compile time).
pub fn baked_pubkey() -> &'static str {
    option_env!("SCREEPUB_UPDATE_PUBKEY").unwrap_or(PUBKEY)
}

/// Whether this build may fetch over plain HTTP: only a rehearsal build
/// (`SCREEPUB_UPDATE_INSECURE=1` at compile time), the counterpart of the
/// plugin's `dangerousInsecureTransportProtocol`.
pub fn baked_insecure() -> bool {
    option_env!("SCREEPUB_UPDATE_INSECURE") == Some("1")
}

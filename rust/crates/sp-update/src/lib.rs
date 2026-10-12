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

/// Whether this is a handover-rehearsal build (`--features rehearsal`,
/// which only `cargo xtask rehearse-handover prepare` passes). Only such a
/// build reads the `SCREEPUB_UPDATE_*` values below; any other build that
/// is compiled with one set refuses to build (build.rs), and would ignore
/// it anyway: a release can never carry a throwaway key, a local endpoint
/// or plain HTTP by accident (review item O10).
pub const REHEARSAL: bool = cfg!(feature = "rehearsal");

/// The endpoint this build was baked with: [`ENDPOINT`], or in a
/// rehearsal build the local one `SCREEPUB_UPDATE_ENDPOINT` named while it
/// compiled, the way Tauri bakes tauri.conf.json into the window. Nothing
/// reads it at run time.
pub fn baked_endpoint() -> &'static str {
    match option_env!("SCREEPUB_UPDATE_ENDPOINT") {
        Some(local) if REHEARSAL => local,
        _ => ENDPOINT,
    }
}

/// The public key this build was baked with: [`PUBKEY`], or in a
/// rehearsal build its throwaway key (`SCREEPUB_UPDATE_PUBKEY` at compile
/// time).
pub fn baked_pubkey() -> &'static str {
    match option_env!("SCREEPUB_UPDATE_PUBKEY") {
        Some(throwaway) if REHEARSAL => throwaway,
        _ => PUBKEY,
    }
}

/// Whether this build may fetch over plain HTTP: only a rehearsal build
/// compiled with `SCREEPUB_UPDATE_INSECURE=1`, the counterpart of the
/// plugin's `dangerousInsecureTransportProtocol`.
pub fn baked_insecure() -> bool {
    REHEARSAL && option_env!("SCREEPUB_UPDATE_INSECURE") == Some("1")
}

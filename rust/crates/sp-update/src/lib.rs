//! Method Reader's updater, in Rust: the transport half that
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

/// Where Method Reader looks for a new version: the latest release of
/// the repository the project lives in now.
pub const ENDPOINT: &str =
    "https://github.com/method-production-suite/method-reader/releases/latest/download/latest.json";

/// Where every copy up to 0.12.x (the Screepub name, Tauri or Rust) looks,
/// as `plugins.updater.endpoints[0]` in desktop/src-tauri/tauri.conf.json
/// had it. GitHub redirects it to [`ENDPOINT`]'s repository once the
/// repository moves, which is how those copies reach Method Reader.
/// Nothing in this build asks it; tests/tauri_conf.rs holds it to the
/// Tauri windows' value.
pub const LEGACY_ENDPOINT: &str =
    "https://github.com/ssandweiss/screepub/releases/latest/download/latest.json";

/// The minisign public key every copy, Screepub or Method Reader,
/// trusts, as tauri.conf.json carries it: base64 of the key file's two
/// lines. Its key id is E4D0668D54F6EB34.
pub const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEU0RDA2NjhENTRGNkVCMzQKUldRMDYvWlVqV2JRNUQzRkpQMmVJZlQ4bTdaT2M5cmN2V05oVTJGdURUY0NwRDBRbEQ1c013UnYK";

/// Method Reader's bundle identifier, from 0.13. The app's data folder is
/// named after it on every platform (Application Support, %APPDATA%,
/// $XDG_CONFIG_HOME), and macOS keys its permission grants to it.
pub const IDENTIFIER: &str = "com.methodreader.desktop";

/// The identifier every copy up to 0.12.x had, the Tauri windows from 0.8.0
/// and the first Rust app: their data and their WebKit storage sit under
/// it. Still known, so 0.13 can find what they kept and move it once.
pub const LEGACY_IDENTIFIER: &str = "com.darkwell.screepub";

/// The 0.7.x Tauri window's identifier: a copy that skipped every release
/// since keeps its update answer under it.
pub const LEGACY_TAURI_IDENTIFIER: &str = "com.darkwell.screepub.desktop";

/// Whether this is a handover-rehearsal build (`--features rehearsal`,
/// which only `cargo xtask rehearse-handover prepare` passes). Only such a
/// build reads the `METHOD_READER_UPDATE_*` values below; any other build that
/// is compiled with one set refuses to build (build.rs), and would ignore
/// it anyway: a release can never carry a throwaway key, a local endpoint
/// or plain HTTP by accident (review item O10).
pub const REHEARSAL: bool = cfg!(feature = "rehearsal");

/// The endpoint this build was baked with: [`ENDPOINT`], or in a
/// rehearsal build the local one `METHOD_READER_UPDATE_ENDPOINT` named while it
/// compiled, the way Tauri bakes tauri.conf.json into the window. Nothing
/// reads it at run time.
pub fn baked_endpoint() -> &'static str {
    match option_env!("METHOD_READER_UPDATE_ENDPOINT") {
        Some(local) if REHEARSAL => local,
        _ => ENDPOINT,
    }
}

/// The public key this build was baked with: [`PUBKEY`], or in a
/// rehearsal build its throwaway key (`METHOD_READER_UPDATE_PUBKEY` at compile
/// time).
pub fn baked_pubkey() -> &'static str {
    match option_env!("METHOD_READER_UPDATE_PUBKEY") {
        Some(throwaway) if REHEARSAL => throwaway,
        _ => PUBKEY,
    }
}

/// Whether this build may fetch over plain HTTP: only a rehearsal build
/// compiled with `METHOD_READER_UPDATE_INSECURE=1`, the counterpart of the
/// plugin's `dangerousInsecureTransportProtocol`.
pub fn baked_insecure() -> bool {
    REHEARSAL && option_env!("METHOD_READER_UPDATE_INSECURE") == Some("1")
}

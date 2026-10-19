//! Method Reader's side of the suite's updater, `method-update` (the
//! method-libs repository): everything that crate leaves to the app.
//!
//! - Reader's identity: the bundle identifiers, the endpoint, the public
//!   key, the bundle names, now and before the rename;
//! - [`update_config`], the [`AppConfig`] every check, download, swap and
//!   sweep is handed;
//! - the handover rehearsal's compile-time values, read here and only in a
//!   rehearsal build (build.rs refuses them in any other).
//!
//! The update work itself (latest.json, minisign, the swap, the restart,
//! the engine gate) is method-update's. The one-time import of the Tauri
//! window's update answer is the window's (sp-app's `legacy` module).
//! Which release is newer, and when to ask, stay sp-app's calls
//! (`model::update`), as they were before the move.

pub use method_update::{AppConfig, BundleNames};

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
/// lines. Its key id is E4D0668D54F6EB34. This is the key's one home: the
/// window, the release tools and the tag-time gate all read it here.
pub const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEU0RDA2NjhENTRGNkVCMzQKUldRMDYvWlVqV2JRNUQzRkpQMmVJZlQ4bTdaT2M5cmN2V05oVTJGdURUY0NwRDBRbEQ1c013UnYK";

/// Method Reader's bundle identifier, from 0.13. The app's data folder is
/// named after it on every platform (Application Support, %APPDATA%,
/// $XDG_DATA_HOME), and macOS keys its permission grants to it. The
/// updater never reads it; the window does, to find its own data.
pub const IDENTIFIER: &str = "com.methodreader.desktop";

/// The identifier every copy up to 0.12.x had, the Tauri windows from 0.8.0
/// and the first Rust app: their data and their WebKit storage sit under
/// it. Still known, so 0.13 can find what they kept and move it once.
pub const LEGACY_IDENTIFIER: &str = "com.darkwell.screepub";

/// The 0.7.x Tauri window's identifier: a copy that skipped every release
/// since keeps its update answer under it.
pub const LEGACY_TAURI_IDENTIFIER: &str = "com.darkwell.screepub.desktop";

/// The app's name as people see it, which the updater's sentences carry.
pub const DISPLAY_NAME: &str = "Method Reader";

/// The Mac bundle's name from 0.13 on.
pub const BUNDLE_NAME: &str = "Method Reader.app";

/// The name every bundle up to 0.12.x had, and the one the first Method
/// Reader still runs under (its 0.12.x updater swapped it in place). Kept
/// so the swap can rename it once and the sweep can find its leftovers.
pub const LEGACY_BUNDLE_NAME: &str = "Screepub.app";

/// Every name a Method Reader bundle carried before 0.13: `Screepub.app`,
/// and `Screepub Desktop.app`, the Tauri window's Mac name, which every
/// 0.12.x and 0.13.0 update kept because each swapped the bundle in place.
/// [`rename_old_bundle`] moves either to [`BUNDLE_NAME`] once.
pub const OLD_BUNDLE_NAMES: [&str; 2] = [LEGACY_BUNDLE_NAME, "Screepub Desktop.app"];

/// What [`rename_old_bundle`] did: the bundle's old and new folders, and the
/// running executable's path inside the new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    pub from: std::path::PathBuf,
    pub to: std::path::PathBuf,
    pub exe: std::path::PathBuf,
}

/// The bundle `exe` runs from, renamed to [`BUNDLE_NAME`] in the same
/// folder when it still has one of the [`OLD_BUNDLE_NAMES`]. Only then, and
/// only when nothing is called `Method Reader.app` there yet and macOS has
/// not translocated the app (a quarantined app run where it was
/// downloaded). Any other name is the person's own choice and stays. A
/// rename the folder refuses (read-only, a disk image) changes nothing.
///
/// macOS keeps answering the launch path for this process's executable, so
/// the caller re-launches from [`Renamed::exe`] before anything reads its
/// resources, and puts the folder back with [`undo_rename`] if it cannot.
pub fn rename_old_bundle(exe: &std::path::Path) -> Option<Renamed> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let from = contents.parent()?;
    if macos.file_name()? != "MacOS" || contents.file_name()? != "Contents" {
        return None;
    }
    let name = from.file_name()?.to_str()?;
    if !OLD_BUNDLE_NAMES.contains(&name) {
        return None;
    }
    if from
        .components()
        .any(|c| c.as_os_str() == "AppTranslocation")
    {
        return None;
    }
    let to = from.parent()?.join(BUNDLE_NAME);
    if std::fs::symlink_metadata(&to).is_ok() {
        return None;
    }
    let inside = exe.strip_prefix(from).ok()?.to_path_buf();
    std::fs::rename(from, &to).ok()?;
    Some(Renamed {
        from: from.to_path_buf(),
        exe: to.join(inside),
        to,
    })
}

/// Replaces this process with the program at [`Renamed::exe`], given the
/// same arguments (exec: same process, the new path). Returns only when
/// that failed, with the reason.
#[cfg(unix)]
pub fn relaunch(renamed: &Renamed) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    std::process::Command::new(&renamed.exe)
        .args(std::env::args_os().skip(1))
        .exec()
}

/// Puts a [`rename_old_bundle`] back, for when the re-launch failed.
pub fn undo_rename(renamed: &Renamed) -> std::io::Result<()> {
    std::fs::rename(&renamed.to, &renamed.from)
}

/// What every request says is asking.
pub const USER_AGENT: &str = concat!("method-reader-update/", env!("CARGO_PKG_VERSION"));

/// The prefix of the rehearsal's compile-time variables:
/// `METHOD_READER_UPDATE_ENDPOINT`, `_PUBKEY` and `_INSECURE`.
pub const REHEARSAL_PREFIX: &str = "METHOD_READER_UPDATE";

/// Whether this is a handover-rehearsal build (`--features rehearsal`,
/// which only `cargo xtask rehearse-handover prepare` passes, through
/// sp-app). Only such a build reads the `METHOD_READER_UPDATE_*` values;
/// any other build that is compiled with one set refuses to build
/// (build.rs), and would ignore it anyway: a release can never carry a
/// throwaway key, a local endpoint or plain HTTP by accident (review item
/// O10).
pub const REHEARSAL: bool = cfg!(feature = "rehearsal");

/// The endpoint and key this build was baked with: [`ENDPOINT`] and
/// [`PUBKEY`] over HTTPS, or in a rehearsal build the local endpoint, the
/// throwaway key and plain HTTP the `METHOD_READER_UPDATE_*` variables named
/// while it compiled, the way Tauri bakes tauri.conf.json into the window.
/// Nothing reads them at run time.
pub fn baked() -> method_update::Baked {
    method_update::baked!(
        prefix = "METHOD_READER_UPDATE",
        rehearsal = REHEARSAL,
        endpoint = ENDPOINT,
        pubkey = PUBKEY,
    )
}

/// The names Method Reader's bundle may have: `Method Reader.app`, and
/// `Screepub.app` from before the rename.
pub fn bundle_names() -> BundleNames {
    BundleNames::new(BUNDLE_NAME).with_legacy(LEGACY_BUNDLE_NAME)
}

/// Everything method-update needs to know about Method Reader: its name,
/// its bundle names, its user agent, the endpoint and key it was baked
/// with, and the suite's sentences in its name ("Method Reader has been
/// updated. Restart it to keep going." and the rest).
pub fn update_config() -> AppConfig {
    AppConfig::new(DISPLAY_NAME, BUNDLE_NAME, USER_AGENT, baked())
        .with_legacy_bundle_name(LEGACY_BUNDLE_NAME)
}

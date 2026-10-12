//! The facts about THIS machine that a frozen TypeScript answer depends on.
//!
//! The answers the TypeScript CLI gave were recorded once, on the owner's
//! Mac, and frozen as data. Most do not depend on the machine at all, but a
//! few do: whether Calibre, its KFX plugin and Kindle Previewer are
//! installed (kfx-status, export --check for kindle and kfx) and which Mac
//! apps the send routes can use (Books, Send to Kindle, the mail handler).
//! Those answers are compared only where these facts read the same as they
//! did when the answers were frozen; anywhere else the comparison is
//! skipped, saying so.
//!
//! Every fact here is read WITHOUT the engine (a file that exists or not,
//! the one `defaults read` the engine also asks), so a machine where the
//! Rust engine misreads its own state still compares and fails.
//!
//! Shared by sp-engine's tests and xtask's parity harness (`#[path]`).

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The operating system, as Node spells it.
pub fn os() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(windows) {
        "win32"
    } else {
        "linux"
    }
}

fn exists(p: &str) -> bool {
    Path::new(p).exists()
}

fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h ^= u64::from(x);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The folder Calibre itself would use for its settings, by its own rule.
pub fn real_calibre_config() -> PathBuf {
    if let Ok(set) = std::env::var("CALIBRE_CONFIG_DIRECTORY")
        && !set.trim().is_empty()
        && !set.starts_with("/dev/null")
    {
        return PathBuf::from(set.trim());
    }
    let home = std::env::home_dir().unwrap_or_default();
    if cfg!(target_os = "macos") {
        return home.join("Library/Preferences/calibre");
    }
    if cfg!(windows) {
        return std::env::var_os("APPDATA")
            .map_or_else(|| home.join("AppData").join("Roaming"), PathBuf::from)
            .join("calibre");
    }
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(x) if !x.trim().is_empty() => PathBuf::from(x.trim()).join("calibre"),
        _ => home.join(".config").join("calibre"),
    }
}

/// Whether a Calibre tool is where the engine looks for one.
fn calibre_installed() -> bool {
    let mut places = vec![
        "/Applications/calibre.app/Contents/MacOS/calibre-customize".to_owned(),
        "/opt/homebrew/bin/calibre-customize".to_owned(),
        "/usr/local/bin/calibre-customize".to_owned(),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            places.push(dir.join("calibre-customize").to_string_lossy().into_owned());
        }
    }
    places.iter().any(|p| exists(p))
}

/// The answer to the one question the engine asks Launch Services about
/// the mail handler, as a digest.
fn mail_handler() -> String {
    if !cfg!(target_os = "macos") {
        return "none".into();
    }
    match std::process::Command::new("defaults")
        .args([
            "read",
            "com.apple.LaunchServices/com.apple.launchservices.secure",
            "LSHandlers",
        ])
        .output()
    {
        Ok(o) => {
            let mut all = o.stdout.clone();
            all.extend_from_slice(&o.stderr);
            all.extend_from_slice(o.status.code().unwrap_or(-1).to_string().as_bytes());
            fnv(&all)
        }
        Err(_) => "unreadable".into(),
    }
}

/// The facts, as one JSON object. `home` is the HOME the engine is given
/// (Send to Kindle may sit in ~/Applications).
pub fn facts(home: Option<&Path>) -> serde_json::Value {
    let home = home
        .map(Path::to_path_buf)
        .or_else(std::env::home_dir)
        .unwrap_or_default();
    serde_json::json!({
        "os": os(),
        "calibre": calibre_installed(),
        "kfxPlugin": real_calibre_config().join("plugins").join("KFX Output.zip").exists(),
        "previewer": exists("/Applications/Kindle Previewer 3.app"),
        "books": exists("/System/Applications/Books.app") || exists("/Applications/Books.app"),
        "sendToKindle": exists("/Applications/Send to Kindle.app")
            || home.join("Applications/Send to Kindle.app").exists(),
        "mailHandler": mail_handler(),
    })
}

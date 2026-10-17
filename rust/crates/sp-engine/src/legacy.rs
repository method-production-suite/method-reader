//! The app was called Screepub until 0.13. Its settings folder
//! (Application Support/Screepub, %APPDATA%\Screepub, ~/.config/screepub)
//! and its default library (Documents/Screepub) move to Method Reader's
//! names ONCE, here: on first use, when the new folder is not there and the
//! old one is, the old one is renamed. Never over a folder already there,
//! never a delete, and a move that fails changes nothing: the path rules
//! (`app_settings_path`, `platform_library_default`) keep using an old
//! folder for as long as it is the only one.
//!
//! A run pointed at its own settings folder (METHOD_READER_CONFIG_DIR, or
//! the 0.12 SCREEPUB_CONFIG_DIR) moves nothing: that is every test, the
//! release smoke and the screenshot tools, none of which may touch the
//! person's own folders. The library moves only when the person never
//! chose a folder of their own (or chose the old default itself), and then
//! every saved absolute path under the old folder, in the settings file and
//! in each script folder's source.json, is rewritten to the new one.

use serde_json::Value;

use crate::env::Env;
use crate::fsx;
use crate::jsjson;
use crate::library::{
    LEGACY_LIBRARY_VAR, LIBRARY_VAR, SOURCE_FILE, legacy_library_default_folder,
    library_default_folder, resolved_if_absolute,
};
use crate::nodepath::host;
use crate::settings::app::{
    LEGACY_CONFIG_DIR_VAR, Patch, app_settings_path, legacy_settings_folder, read_app_settings,
    settings_folder, write_app_settings,
};

/// What [`move_legacy_folders`] moved, each as (from, to).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyMove {
    pub settings: Option<(String, String)>,
    pub library: Option<(String, String)>,
}

/// Move the 0.12 folders to the new names, once. Never fails; what could
/// not move stays in use where it is.
pub fn move_legacy_folders(platform: &str, env: &Env) -> LegacyMove {
    let mut moved = LegacyMove::default();
    if env.get(crate::settings::app::CONFIG_DIR_VAR).is_some()
        || env.get(LEGACY_CONFIG_DIR_VAR).is_some()
    {
        return moved;
    }
    let from = legacy_settings_folder(platform, env);
    let to = settings_folder(platform, env);
    if move_folder(&from, &to) {
        moved.settings = Some((from, to));
    }
    if env.get(LIBRARY_VAR).is_some() || env.get(LEGACY_LIBRARY_VAR).is_some() {
        return moved;
    }
    let settings_path = app_settings_path(platform, env);
    let from = legacy_library_default_folder(platform, env);
    let to = library_default_folder(platform, env);
    let chosen = match read_app_settings(&settings_path).get("libraryPath") {
        Some(Value::String(chosen)) => resolved_if_absolute(platform, chosen),
        _ => None,
    };
    if chosen.as_ref().is_some_and(|c| *c != from) {
        return moved;
    }
    if move_folder(&from, &to) {
        rewrite_settings(platform, &settings_path, &from, &to);
        rewrite_sources(platform, &to, &from);
        moved.library = Some((from, to));
    }
    moved
}

/// Rename `from` to `to` when `to` is not there and `from` is a folder.
fn move_folder(from: &str, to: &str) -> bool {
    if fsx::exists(to) || !fsx::is_dir(from) {
        return false;
    }
    std::fs::rename(from, to).is_ok()
}

/// `path` with the folder `from` at its head swapped for `to`, by the
/// platform's separator; `None` when `path` is not under `from`.
fn moved_path(platform: &str, path: &str, from: &str, to: &str) -> Option<String> {
    if path == from {
        return Some(to.to_owned());
    }
    let windows = platform == "win32";
    path.strip_prefix(from)
        .filter(|rest| rest.starts_with('/') || (windows && rest.starts_with('\\')))
        .map(|rest| format!("{to}{rest}"))
}

/// Every string in `value` under `from`, moved to `to`. True if any was.
fn rewrite_value(platform: &str, value: &mut Value, from: &str, to: &str) -> bool {
    match value {
        Value::String(s) => match moved_path(platform, s, from, to) {
            Some(new) => {
                *s = new;
                true
            }
            None => false,
        },
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |any, v| rewrite_value(platform, v, from, to) | any),
        Value::Object(map) => map
            .values_mut()
            .fold(false, |any, v| rewrite_value(platform, v, from, to) | any),
        _ => false,
    }
}

/// The settings file's keys that hold a path under the old library, moved.
fn rewrite_settings(platform: &str, settings_path: &str, from: &str, to: &str) {
    let mut patch: Patch = Vec::new();
    for (key, mut value) in read_app_settings(settings_path) {
        if rewrite_value(platform, &mut value, from, to) {
            patch.push((key, Some(value)));
        }
    }
    if !patch.is_empty() {
        let _ = write_app_settings(&patch, settings_path);
    }
}

/// Each script folder's source.json under the moved library, its recorded
/// source moved when the PDF lived inside the library.
fn rewrite_sources(platform: &str, library: &str, from: &str) {
    let Ok(entries) = std::fs::read_dir(library) else {
        return;
    };
    for entry in entries.flatten() {
        let folder = entry.path().to_string_lossy().into_owned();
        let marker = host::join(&[folder.as_str(), SOURCE_FILE]);
        let Ok(text) = fsx::read_text(&marker) else {
            continue;
        };
        let Some(mut value) = jsjson::parse(&text) else {
            continue;
        };
        if rewrite_value(platform, &mut value, from, library) {
            let text = format!("{}\n", jsjson::stringify_pretty(&value));
            let _ = fsx::write_atomic(&marker, text.as_bytes());
        }
    }
}

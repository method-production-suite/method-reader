//! The app was called Screepub until 0.13, under the bundle identifier
//! com.darkwell.screepub. What it kept moves to Method Reader's places
//! ONCE, here, on first use:
//!
//! - its settings folder (Application Support/Screepub, %APPDATA%\Screepub,
//!   ~/.config/screepub) becomes the app-data folder named after the new
//!   identifier, `com.methodreader.desktop`, in the same parent;
//! - what it kept under the old identifier's folder beside that
//!   (`com.darkwell.screepub`: the window's own storage, the update answer
//!   among it) joins it, entry by entry;
//! - its default library (Documents/Screepub) becomes Documents/Method
//!   Reader: a person's documents, not app data.
//!
//! A folder moves whole when the new one is not there; into a new folder
//! already there, each entry moves only when the new folder has none of
//! that name. Never over anything already there, never a delete (an old
//! folder emptied by the move stays, empty), and a move that fails changes
//! nothing: the path rules (`app_settings_path`, `app_data_path`,
//! `platform_library_default`) keep using what is still in an old folder
//! for as long as the new one lacks it.
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
    LEGACY_CONFIG_DIR_VAR, Patch, app_settings_path, legacy_app_data_folder,
    legacy_settings_folder, read_app_settings, settings_folder, write_app_settings,
};

/// What [`move_legacy_folders`] moved, each as (from, to).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyMove {
    /// The settings folder, moved whole.
    pub settings: Option<(String, String)>,
    /// What the old identifier's folder held: the folder itself when it
    /// moved whole, else each entry that moved into the new folder.
    pub app_data: Vec<(String, String)>,
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
        moved.settings = Some((from, to.clone()));
    }
    moved.app_data = merge_folder(platform, &legacy_app_data_folder(platform, env), &to);
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
        // The settings must follow, or the app would look for a library
        // that is gone: if they cannot be saved, the folder goes back.
        if rewrite_settings(platform, &settings_path, &from, &to) {
            rewrite_sources(platform, &to, &from);
            moved.library = Some((from, to));
        } else {
            let _ = std::fs::rename(&to, &from);
        }
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

/// `from` into `to`: the whole folder when `to` is not there, else each
/// entry of `from` that `to` has none of. What moved, as (from, to).
fn merge_folder(platform: &str, from: &str, to: &str) -> Vec<(String, String)> {
    if !fsx::is_dir(from) {
        return Vec::new();
    }
    if move_folder(from, to) {
        return vec![(from.to_owned(), to.to_owned())];
    }
    if !fsx::is_dir(to) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(from) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let join = |dir: &str, name: &str| {
        if platform == "win32" {
            crate::nodepath::win32::join(&[dir, name])
        } else {
            crate::nodepath::posix::join(&[dir, name])
        }
    };
    names
        .into_iter()
        .filter_map(|name| {
            let (old, new) = (join(from, &name), join(to, &name));
            (!fsx::exists(&new) && std::fs::rename(&old, &new).is_ok()).then_some((old, new))
        })
        .collect()
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
/// False when they needed rewriting and could not be saved.
fn rewrite_settings(platform: &str, settings_path: &str, from: &str, to: &str) -> bool {
    let mut patch: Patch = Vec::new();
    for (key, mut value) in read_app_settings(settings_path) {
        if rewrite_value(platform, &mut value, from, to) {
            patch.push((key, Some(value)));
        }
    }
    patch.is_empty() || write_app_settings(&patch, settings_path).is_ok()
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

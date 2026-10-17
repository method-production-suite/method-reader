//! Port of `src/settings/app.ts`: the app-wide settings file, one small
//! JSON file for knobs that belong to the app itself, not to any one
//! script. Several pieces share it (the last send route, the library
//! folder, the app-wide format defaults, the window's remembered choices),
//! and none owns it, so a write from one must keep the others' keys.
//!
//! It lives OUTSIDE the library on purpose: a file that recorded where to
//! find the library could not itself live inside the thing it locates.
//!
//! **Byte compatibility.** The file is read with JavaScript's rules and
//! written as `JSON.stringify(merged, null, 2)` plus a newline
//! ([`crate::jsjson`]), so a file the TypeScript app wrote reads back
//! unchanged and an untouched key is written back byte for byte.

use serde_json::{Map, Value};

use crate::env::{Env, js_trim, os_home};
use crate::errors::EngineError;
use crate::fsx;
use crate::jsjson;
use crate::nodepath::{host, posix, win32};

/// The readers the window can remember as "this is mine".
pub const READER_DEVICES: [&str; 5] = ["kindle", "apple", "kobo", "tolino", "remarkable"];

/// What the window's send panel leads with: reading the book, or getting
/// the script back to edit. Not in the TypeScript: a Rust window feature.
pub const PANEL_INTENTS: [&str; 2] = ["read", "edit"];

/// The settings file's contents: any JSON object.
pub type AppSettings = Map<String, Value>;

/// `homeFolder(env)`: HOME first, then USERPROFILE, then the OS's own
/// answer. The one copy of the rule for what `~` is.
pub fn home_folder(env: &Env) -> String {
    env.non_empty("HOME")
        .or_else(|| env.non_empty("USERPROFILE"))
        .map(str::to_owned)
        .unwrap_or_else(os_home)
}

/// `absoluteEnvFolder(raw)`: an environment variable naming a folder, as an
/// absolute path (the HOST's rules), or `None` when it is unset, blank or
/// relative.
pub fn absolute_env_folder(raw: Option<&str>) -> Option<String> {
    let value = js_trim(raw.unwrap_or(""));
    if !value.is_empty() && host::is_absolute(value) {
        Some(host::resolve(&[value]))
    } else {
        None
    }
}

/// The app's data folder, where settings.json and the window's own
/// storage live: named after the bundle identifier (sp_update::IDENTIFIER,
/// which sp-app's tests hold it to), as a Tauri app's is, under Application
/// Support (macOS), %APPDATA% (Windows) and $XDG_CONFIG_HOME (everywhere
/// else).
pub const APP_DATA_FOLDER: &str = "com.methodreader.desktop";
/// Where 0.12 and earlier kept settings.json: `Screepub` under Application
/// Support and %APPDATA%, `screepub` under $XDG_CONFIG_HOME. Still known:
/// it moves once into [`APP_DATA_FOLDER`]
/// ([`crate::legacy::move_legacy_folders`]) and is used until it has.
pub const LEGACY_SETTINGS_FOLDER: &str = "Screepub";
pub const LEGACY_SETTINGS_FOLDER_XDG: &str = "screepub";
/// The 0.12 bundle identifier's folder, beside the settings one: the 0.12
/// window kept its own storage (window-storage.json, the update answer
/// among it) there. Its entries move once into [`APP_DATA_FOLDER`].
pub const LEGACY_APP_DATA_FOLDER: &str = "com.darkwell.screepub";

/// The variable that puts the settings folder anywhere.
pub const CONFIG_DIR_VAR: &str = "METHOD_READER_CONFIG_DIR";
/// Its 0.12 name, still read when [`CONFIG_DIR_VAR`] is unset.
pub const LEGACY_CONFIG_DIR_VAR: &str = "SCREEPUB_CONFIG_DIR";

/// `env.NEW`, or `env.LEGACY` when NEW is unset: the rule for the variables
/// a person might have set under the 0.12 names.
pub fn env_or_legacy<'a>(env: &'a Env, name: &str, legacy: &str) -> Option<&'a str> {
    env.get(name).or_else(|| env.get(legacy))
}

/// The settings folder the environment names, when it names one:
/// METHOD_READER_CONFIG_DIR (or SCREEPUB_CONFIG_DIR, when that is unset),
/// as an absolute path.
pub fn config_dir_override(env: &Env) -> Option<String> {
    absolute_env_folder(env_or_legacy(env, CONFIG_DIR_VAR, LEGACY_CONFIG_DIR_VAR))
}

/// The platform's settings folder, named `name` (`xdg_name` on the
/// freedesktop systems), by that platform's path rules.
fn platform_settings_folder(platform: &str, env: &Env, name: &str, xdg_name: &str) -> String {
    let home = home_folder(env);
    match platform {
        "darwin" => posix::join(&[home.as_str(), "Library", "Application Support", name]),
        "win32" => {
            let app_data = js_trim(env.get("APPDATA").unwrap_or(""));
            let base = if app_data.is_empty() {
                win32::join(&[home.as_str(), "AppData", "Roaming"])
            } else {
                app_data.to_owned()
            };
            win32::join(&[base.as_str(), name])
        }
        _ => {
            let xdg = js_trim(env.get("XDG_CONFIG_HOME").unwrap_or(""));
            let config_home = if posix::is_absolute(xdg) {
                xdg.to_owned()
            } else {
                posix::join(&[home.as_str(), ".config"])
            };
            posix::join(&[config_home.as_str(), xdg_name])
        }
    }
}

/// `path` joined to `name` by the platform's rules.
fn platform_join(platform: &str, path: &str, name: &str) -> String {
    if platform == "win32" {
        win32::join(&[path, name])
    } else {
        posix::join(&[path, name])
    }
}

/// The platform's app-data folder called `name` (by identifier, so the
/// same name everywhere), whatever is on disk.
pub fn app_data_folder_named(platform: &str, env: &Env, name: &str) -> String {
    platform_settings_folder(platform, env, name, name)
}

/// Method Reader's app-data folder on this platform, whatever is on disk.
pub fn settings_folder(platform: &str, env: &Env) -> String {
    app_data_folder_named(platform, env, APP_DATA_FOLDER)
}

/// Screepub's settings folder on this platform, whatever is on disk.
pub fn legacy_settings_folder(platform: &str, env: &Env) -> String {
    platform_settings_folder(
        platform,
        env,
        LEGACY_SETTINGS_FOLDER,
        LEGACY_SETTINGS_FOLDER_XDG,
    )
}

/// The 0.12 identifier's app-data folder, whatever is on disk.
pub fn legacy_app_data_folder(platform: &str, env: &Env) -> String {
    app_data_folder_named(platform, env, LEGACY_APP_DATA_FOLDER)
}

/// The file `name` in the app's data folder: in METHOD_READER_CONFIG_DIR
/// when that is an absolute path; otherwise Method Reader's own, unless it
/// is not there yet and a 0.12 folder still holds one (the move had not
/// happened, or could not), which is then used where it is.
pub fn app_data_path(platform: &str, env: &Env, name: &str) -> String {
    if let Some(dir) = config_dir_override(env) {
        return host::join(&[dir.as_str(), name]);
    }
    let folder = settings_folder(platform, env);
    let new = platform_join(platform, &folder, name);
    if fsx::exists(&new) {
        return new;
    }
    [
        legacy_settings_folder(platform, env),
        legacy_app_data_folder(platform, env),
    ]
    .iter()
    .map(|old| platform_join(platform, old, name))
    .find(|old| fsx::exists(old))
    .unwrap_or(new)
}

/// `appSettingsPath(platform, env)`: folder + `settings.json`.
/// METHOD_READER_CONFIG_DIR wins everywhere, when it is an absolute path.
/// Otherwise Method Reader's app-data folder; Screepub's while only that
/// one is there, or while only it holds a settings.json (it had not moved
/// yet, or could not be moved).
pub fn app_settings_path(platform: &str, env: &Env) -> String {
    if config_dir_override(env).is_none() {
        let folder = settings_folder(platform, env);
        let legacy = legacy_settings_folder(platform, env);
        if !fsx::exists(&folder) && fsx::is_dir(&legacy) {
            return platform_join(platform, &legacy, "settings.json");
        }
    }
    app_data_path(platform, env, "settings.json")
}

/// `readAppSettings(path)`. Never fails: missing, unreadable, not JSON, or
/// not an object all read back as `{}`.
pub fn read_app_settings(path: &str) -> AppSettings {
    let Ok(text) = fsx::read_text(path) else {
        return Map::new();
    };
    match jsjson::parse(&text) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

/// One change to the file: `Some(value)` sets the key, `None` removes it
/// (the TypeScript's `undefined`).
pub type Patch = Vec<(String, Option<Value>)>;

/// `writeAppSettings(patch, path)`: reads, shallow-merges `patch` over what
/// is there (unknown keys kept), writes atomically, and creates the folder.
///
/// A file that is corrupt or unreadable reads as `{}`, so saving over it
/// REPLACES it with the patch alone (the owner's choice, 2026-10-01): a
/// damaged file heals on the next save instead of blocking every save.
pub fn write_app_settings(patch: &Patch, path: &str) -> Result<AppSettings, EngineError> {
    // One save at a time in this process: the window runs engine calls as
    // threads of one process, and two read-merge-writes at once would each
    // drop the other's change. (Poisoning carries no broken state here: the
    // file is only ever replaced whole.)
    static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = SAVING.lock().unwrap_or_else(|p| p.into_inner());
    let mut merged = read_app_settings(path);
    for (key, value) in patch {
        match value {
            // shift_remove, not remove: the keys after it keep their order,
            // as `delete` leaves a JavaScript object's.
            None => {
                merged.shift_remove(key);
            }
            Some(value) => {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    let folder = host::dirname(path);
    if !fsx::exists(&folder) {
        fsx::mkdir_all(&folder)?;
    }
    let text = format!(
        "{}\n",
        jsjson::stringify_pretty(&Value::Object(merged.clone()))
    );
    fsx::write_atomic(path, text.as_bytes())?;
    Ok(merged)
}

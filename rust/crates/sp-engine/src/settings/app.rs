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

/// `appSettingsPath(platform, env)`: folder + `settings.json`.
/// SCREEPUB_CONFIG_DIR wins everywhere, when it is an absolute path.
pub fn app_settings_path(platform: &str, env: &Env) -> String {
    if let Some(dir) = absolute_env_folder(env.get("SCREEPUB_CONFIG_DIR")) {
        return host::join(&[dir.as_str(), "settings.json"]);
    }
    let home = home_folder(env);
    match platform {
        "darwin" => posix::join(&[
            home.as_str(),
            "Library",
            "Application Support",
            "Screepub",
            "settings.json",
        ]),
        "win32" => {
            let app_data = js_trim(env.get("APPDATA").unwrap_or(""));
            let base = if app_data.is_empty() {
                win32::join(&[home.as_str(), "AppData", "Roaming"])
            } else {
                app_data.to_owned()
            };
            win32::join(&[base.as_str(), "Screepub", "settings.json"])
        }
        _ => {
            let xdg = js_trim(env.get("XDG_CONFIG_HOME").unwrap_or(""));
            let config_home = if posix::is_absolute(xdg) {
                xdg.to_owned()
            } else {
                posix::join(&[home.as_str(), ".config"])
            };
            posix::join(&[config_home.as_str(), "screepub", "settings.json"])
        }
    }
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

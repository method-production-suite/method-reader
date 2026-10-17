//! Port of `src/library.ts`: where a converted script's files live when
//! the caller did not say. The engine owns this path, so the CLI, the
//! window and anything later land in the same place.

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::env::{Env, js_trim, js_trim_start};
use crate::errors::EngineError;
use crate::fsx;
use crate::jsjson;
use crate::nodepath::{host, posix, win32};
use crate::settings::app::{
    absolute_env_folder, app_settings_path, env_or_legacy, home_folder, read_app_settings,
};

/// The library's default folder name, under the person's Documents.
pub const LIBRARY_FOLDER: &str = "Method Reader";
/// The 0.12 app's name for it, Screepub's. Still known: a default library
/// under the old name is used until it has moved
/// ([`crate::legacy::move_legacy_folders`]).
pub const LEGACY_LIBRARY_FOLDER: &str = "Screepub";

/// The variable that puts the library anywhere.
pub const LIBRARY_VAR: &str = "METHOD_READER_LIBRARY";
/// Its 0.12 name, still read when [`LIBRARY_VAR`] is unset.
pub const LEGACY_LIBRARY_VAR: &str = "SCREEPUB_LIBRARY";

/// Names this folder's script. One file per script folder, and the one name
/// a Library listing has to skip (or read, to show where a book came from).
pub const SOURCE_FILE: &str = "source.json";

/// `sha256(text)` as lowercase hex, of the text's UTF-8 bytes.
pub(crate) fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The user's Documents folder on a freedesktop system: XDG_DOCUMENTS_DIR,
/// else `user-dirs.dirs`, else `None` (the caller falls back to
/// ~/Documents).
fn xdg_documents(home: &str, env: &Env) -> Option<String> {
    let as_documents = |raw: &str| -> Option<String> {
        let trimmed = js_trim(raw);
        // `^"(.*)"$`: `.` stops at a line break, so a value holding one
        // keeps its quotes.
        let value = match trimmed.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            Some(inner) if !inner.contains(['\n', '\r', '\u{2028}', '\u{2029}']) => inner,
            _ => trimmed,
        };
        let expanded = match value.strip_prefix("$HOME") {
            Some(rest) => posix::join(&[home, rest]),
            None => value.to_owned(),
        };
        if !posix::is_absolute(&expanded) {
            return None;
        }
        let documents = posix::resolve(&[expanded.as_str()]);
        if documents == posix::resolve(&[home]) || documents == "/" {
            None
        } else {
            Some(documents)
        }
    };

    if let Some(from_env) = as_documents(env.get("XDG_DOCUMENTS_DIR").unwrap_or("")) {
        return Some(from_env);
    }
    let config_home = js_trim(env.get("XDG_CONFIG_HOME").unwrap_or(""));
    let config = if posix::is_absolute(config_home) {
        config_home.to_owned()
    } else {
        posix::join(&[home, ".config"])
    };
    let text = fsx::read_text(&posix::join(&[config.as_str(), "user-dirs.dirs"])).ok()?;
    let line = text
        .split('\n')
        .find(|l| js_trim_start(l).starts_with("XDG_DOCUMENTS_DIR="))?;
    let after = &line[line.find('=').map_or(0, |i| i + 1)..];
    as_documents(after)
}

/// `envLibraryOverride(env)`: METHOD_READER_LIBRARY (or SCREEPUB_LIBRARY,
/// when that is unset), resolved, or `None` when unset, blank or relative.
pub fn env_library_override(env: &Env) -> Option<String> {
    absolute_env_folder(env_or_legacy(env, LIBRARY_VAR, LEGACY_LIBRARY_VAR))
}

/// `resolvedIfAbsolute(platform, value)`: `value`, resolved, when it is
/// absolute by the PLATFORM's rules.
pub fn resolved_if_absolute(platform: &str, value: &str) -> Option<String> {
    if platform == "win32" {
        win32::is_absolute(value).then(|| win32::resolve(&[value]))
    } else {
        posix::is_absolute(value).then(|| posix::resolve(&[value]))
    }
}

/// `chosenLibraryPath(platform, env, settingsPath)`: the stored
/// `libraryPath`, only when `resolved_if_absolute` accepts it.
pub fn chosen_library_path(
    platform: &str,
    env: &Env,
    settings_path: Option<&str>,
) -> Option<String> {
    let path = settings_path
        .map(str::to_owned)
        .unwrap_or_else(|| app_settings_path(platform, env));
    match read_app_settings(&path).get("libraryPath") {
        Some(Value::String(chosen)) => resolved_if_absolute(platform, chosen),
        _ => None,
    }
}

/// The platform's Documents folder joined to `name`, by its path rules.
fn documents_folder(platform: &str, env: &Env, name: &str) -> String {
    let home = home_folder(env);
    match platform {
        "darwin" => posix::join(&[home.as_str(), "Documents", name]),
        "win32" => win32::join(&[home.as_str(), "Documents", name]),
        _ => {
            let documents = xdg_documents(&home, env)
                .unwrap_or_else(|| posix::join(&[home.as_str(), "Documents"]));
            posix::join(&[documents.as_str(), name])
        }
    }
}

/// Method Reader's default library folder, whatever is on disk.
pub fn library_default_folder(platform: &str, env: &Env) -> String {
    documents_folder(platform, env, LIBRARY_FOLDER)
}

/// Screepub's default library folder, whatever is on disk.
pub fn legacy_library_default_folder(platform: &str, env: &Env) -> String {
    documents_folder(platform, env, LEGACY_LIBRARY_FOLDER)
}

/// `platformLibraryDefault(platform, env)`: the library with no chosen
/// folder and no METHOD_READER_LIBRARY. Method Reader's folder; Screepub's
/// while only that one is there (it had not moved yet, or could not be
/// moved).
pub fn platform_library_default(platform: &str, env: &Env) -> String {
    let folder = library_default_folder(platform, env);
    let legacy = legacy_library_default_folder(platform, env);
    if !fsx::exists(&folder) && fsx::is_dir(&legacy) {
        legacy
    } else {
        folder
    }
}

/// `libraryRoot(platform, env, settingsPath)`: METHOD_READER_LIBRARY, else the
/// folder chosen in the app, else the platform default.
pub fn library_root(platform: &str, env: &Env, settings_path: Option<&str>) -> String {
    if let Some(over) = env_library_override(env) {
        return over;
    }
    if let Some(chosen) = chosen_library_path(platform, env, settings_path) {
        return chosen;
    }
    platform_library_default(platform, env)
}

/// The folder name a script would like: its own, undecorated.
fn stem_of(input: &str) -> String {
    let stem = host::basename_ext(input, &host::extname(input));
    let stem = js_trim(&stem);
    if stem.is_empty() || stem == "." || stem == ".." {
        "script".into()
    } else {
        stem.to_owned()
    }
}

/// The absolute path recorded in a folder's source.json, or `None` if the
/// folder is not a script folder this engine made.
fn recorded_source(folder: &str) -> Option<String> {
    let text = fsx::read_text(&host::join(&[folder, SOURCE_FILE])).ok()?;
    match jsjson::parse(&text)? {
        Value::Object(map) => match map.get("source") {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// `folderFor(source, root)`: the folder this input owns inside `root`,
/// free or already its own, hashed off the absolute path when the plain
/// name belongs to another script.
fn folder_for(source: &str, root: &str) -> Result<String, EngineError> {
    let stem = stem_of(source);
    let hash = sha256_hex(source);
    let ours =
        |folder: &str| !fsx::exists(folder) || recorded_source(folder).as_deref() == Some(source);
    let mut folder = host::join(&[root, stem.as_str()]);
    let mut attempt = 0usize;
    while !ours(&folder) {
        let width = if attempt == 0 { 8 } else { 8 + attempt * 4 };
        if width > 32 {
            return Err(EngineError::plain(format!(
                "too many scripts already claim the name \"{stem}\" in the library"
            )));
        }
        folder = host::join(&[root, &format!("{stem}-{}", &hash[..width])]);
        attempt += 1;
    }
    Ok(folder)
}

/// The same folder, created and marked as this script's.
fn script_folder(source: &str, root: &str) -> Result<String, EngineError> {
    let folder = folder_for(source, root)?;
    let made = !fsx::exists(&folder);
    fsx::mkdir_all(&folder)?;
    if recorded_source(&folder).as_deref() != Some(source) {
        let mut marker = serde_json::Map::new();
        marker.insert("source".into(), Value::String(source.into()));
        let text = format!("{}\n", jsjson::stringify_pretty(&Value::Object(marker)));
        if let Err(error) = fsx::write_atomic(
            &host::join(&[folder.as_str(), SOURCE_FILE]),
            text.as_bytes(),
        ) {
            if made {
                let _ = std::fs::remove_dir(&folder);
            }
            return Err(error);
        }
    }
    Ok(folder)
}

/// `libraryOutput(input, root)`: where this input's outputs go, as the path
/// PREFIX every companion file is built from: `<library>/<folder>/<stem>`.
/// Creates the folder.
pub fn library_output(input: &str, root: &str) -> Result<String, EngineError> {
    let source = host::resolve(&[input]);
    let folder = script_folder(&source, root)?;
    Ok(host::join(&[folder, stem_of(&source)]))
}

/// `existingLibraryOutput(input, root)`: the same prefix IF the library
/// already holds the script's folder, and `None` otherwise. Creates
/// nothing.
pub fn existing_library_output(input: &str, root: &str) -> Result<Option<String>, EngineError> {
    let source = host::resolve(&[input]);
    let folder = folder_for(&source, root)?;
    Ok(fsx::exists(&folder).then(|| host::join(&[folder, stem_of(&source)])))
}

/// `adoptSidecar(input, output)`: carry a sidecar from beside the PDF into
/// the library on the first library conversion. Copied, never moved, and
/// never over a sidecar already in the library.
///
/// The sidecar beside the PDF may carry the 0.12 name
/// (`<Stem>.screepub.json`); it lands under the new one. One already in the
/// library under either name is the script's tuning, and is kept.
pub fn adopt_sidecar(input: &str, output: &str) -> Result<bool, EngineError> {
    use crate::settings::sidecar::{LEGACY_SIDECAR_SUFFIX, SIDECAR_SUFFIX};
    let source = host::resolve(&[input]);
    let beside = |suffix: &str| {
        host::join(&[
            host::dirname(&source),
            format!("{}{suffix}", stem_of(&source)),
        ])
    };
    let landing = format!("{output}{SIDECAR_SUFFIX}");
    if fsx::exists(&landing) || fsx::exists(&format!("{output}{LEGACY_SIDECAR_SUFFIX}")) {
        return Ok(false);
    }
    let existing = [beside(SIDECAR_SUFFIX), beside(LEGACY_SIDECAR_SUFFIX)]
        .into_iter()
        .find(|p| fsx::exists(p));
    let Some(existing) = existing else {
        return Ok(false);
    };
    fsx::copy_atomic(&existing, &landing)?;
    Ok(true)
}

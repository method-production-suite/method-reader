//! Port of `src/library.ts`: where a converted script's files live when
//! the caller did not say. The engine owns this path, so the CLI, the
//! window and anything later land in the same place.

use serde_json::Value;
use sha2::{Digest, Sha256};
use sp_fdx::SceneMeasure;

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

/// The folder name a script would like: its own, undecorated. Also the
/// stem of every file in its folder (`<stem>.fountain`, `<stem>.epub`).
pub(crate) fn stem_of(input: &str) -> String {
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
pub(crate) fn recorded_source(folder: &str) -> Option<String> {
    let text = fsx::read_text(&host::join(&[folder, SOURCE_FILE])).ok()?;
    match jsjson::parse(&text)? {
        Value::Object(map) => match map.get("source") {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Whether `path` sits in a script folder this engine made (one holding a
/// source.json that names its source): a library `.fountain`, which the
/// window renders again on every tune, as opposed to a script someone
/// chose. Reads nothing but that marker.
pub fn in_script_folder(path: &str) -> bool {
    recorded_source(&host::dirname(&host::resolve(&[path]))).is_some()
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

/// source.json's key for the scene lengths a conversion measured
/// (crate::scene_lengths). Added in 0.14 beside `"source"`, which is all an
/// older version reads, so the marker stays readable both ways:
///
/// ```json
/// {
///   "scenes": {
///     "fountainSha256": "<sha256 of the .fountain they were measured with>",
///     "measured": [{ "eighths": 7, "page": "1" }, { "eighths": 14, "page": "2" }]
///   },
///   "source": "/path/to/Script.pdf"
/// }
/// ```
///
/// One entry per Scene Heading the FDX writes, in order; `page` is absent
/// when the page could not be told. The lengths live here and not in the
/// .fountain, which is the script people export and edit: an edited
/// .fountain no longer matches the digest, and its FDX then carries no
/// lengths rather than wrong ones.
pub const SCENES_KEY: &str = "scenes";

/// The marker beside a library .fountain: its folder's source.json.
fn marker_beside(fountain_path: &str) -> String {
    host::join(&[host::dirname(fountain_path), SOURCE_FILE.to_owned()])
}

/// Set `key` to `value` in the source.json beside `fountain_path`, or
/// remove it when `None`, keeping every other key. Only a library script
/// folder has that marker: anywhere else nothing is written. A marker that
/// would not change is not rewritten.
fn keep_in_marker(fountain_path: &str, key: &str, value: Option<Value>) -> Result<(), EngineError> {
    let path = marker_beside(fountain_path);
    let Ok(text) = fsx::read_text(&path) else {
        return Ok(());
    };
    let Some(Value::Object(mut marker)) = jsjson::parse(&text) else {
        return Ok(());
    };
    let before = marker.get(key).cloned();
    match value {
        Some(value) => {
            marker.insert(key.into(), value);
        }
        None => {
            marker.remove(key);
        }
    }
    if marker.get(key) == before.as_ref() {
        return Ok(());
    }
    let text = format!("{}\n", jsjson::stringify_pretty(&Value::Object(marker)));
    fsx::write_atomic(&path, text.as_bytes())
}

/// Keep `measures` (measured with `fountain_text`) in the source.json
/// beside `fountain_path`, or clear what is kept there when `None`. Only a
/// library script folder has that marker: anywhere else nothing is written.
pub fn save_scene_measures(
    fountain_path: &str,
    fountain_text: &str,
    measures: Option<&[SceneMeasure]>,
) -> Result<(), EngineError> {
    let scenes = measures.map(|measures| {
        let measured: Vec<Value> = measures
            .iter()
            .map(|m| {
                let mut entry = serde_json::Map::new();
                entry.insert("eighths".into(), Value::from(m.eighths));
                if let Some(page) = &m.page {
                    entry.insert("page".into(), Value::String(page.clone()));
                }
                Value::Object(entry)
            })
            .collect();
        let mut scenes = serde_json::Map::new();
        scenes.insert(
            "fountainSha256".into(),
            Value::String(sha256_hex(fountain_text)),
        );
        scenes.insert("measured".into(), Value::Array(measured));
        Value::Object(scenes)
    });
    keep_in_marker(fountain_path, SCENES_KEY, scenes)
}

/// The scene lengths kept beside `fountain_path`, when they were measured
/// with exactly `fountain_text`; `None` otherwise (no marker, an older
/// marker, another text, or an entry this version cannot read).
pub fn read_scene_measures(fountain_path: &str, fountain_text: &str) -> Option<Vec<SceneMeasure>> {
    let text = fsx::read_text(&marker_beside(fountain_path)).ok()?;
    let marker = jsjson::parse(&text)?;
    let scenes = marker.get(SCENES_KEY)?;
    if scenes.get("fountainSha256")?.as_str()? != sha256_hex(fountain_text) {
        return None;
    }
    scenes
        .get("measured")?
        .as_array()?
        .iter()
        .map(|entry| {
            let eighths = u32::try_from(entry.get("eighths")?.as_u64()?).ok()?;
            let page = match entry.get("page") {
                None => None,
                Some(Value::String(p)) => Some(p.clone()),
                Some(_) => return None,
            };
            Some(SceneMeasure { page, eighths })
        })
        .collect()
}

/// source.json's key for the page count a PDF conversion answered (its
/// `--json` `pages`: the PDF's pages, the title page among them). Added for the
/// Library (2026-11-02) beside `"source"` and `"scenes"`, and bound to the
/// .fountain the same way:
///
/// ```json
/// {
///   "pages": {
///     "count": 19,
///     "fountainSha256": "<sha256 of the .fountain it was counted with>"
///   },
///   "source": "/path/to/Script.pdf"
/// }
/// ```
///
/// The Library lists a reopened book with the page count its conversion
/// showed, which the .fountain alone cannot say: its `= pg N` markers start
/// on the first page of the script, and a title page carries none. An
/// edited .fountain no longer matches the digest, and its count is then
/// read from its markers rather than kept stale.
pub const PAGES_KEY: &str = "pages";

/// Keep `count` (the conversion of `fountain_text`'s PDF) in the
/// source.json beside `fountain_path`, or clear what is kept there when
/// `None` (a conversion from a .fountain, which has no PDF pages to count).
/// Only a library script folder has that marker: anywhere else nothing is
/// written.
pub fn save_page_count(
    fountain_path: &str,
    fountain_text: &str,
    count: Option<u32>,
) -> Result<(), EngineError> {
    let pages = count.map(|count| {
        let mut pages = serde_json::Map::new();
        pages.insert("count".into(), Value::from(count));
        pages.insert(
            "fountainSha256".into(),
            Value::String(sha256_hex(fountain_text)),
        );
        Value::Object(pages)
    });
    keep_in_marker(fountain_path, PAGES_KEY, pages)
}

/// The page count kept beside `fountain_path`, when it was counted for
/// exactly `fountain_text`; `None` otherwise (no marker, an older marker,
/// another text, or an entry this version cannot read).
pub fn read_page_count(fountain_path: &str, fountain_text: &str) -> Option<u32> {
    let text = fsx::read_text(&marker_beside(fountain_path)).ok()?;
    let pages = jsjson::parse(&text)?.get(PAGES_KEY)?.clone();
    if pages.get("fountainSha256")?.as_str()? != sha256_hex(fountain_text) {
        return None;
    }
    u32::try_from(pages.get("count")?.as_u64()?).ok()
}

/// source.json's key for when a library conversion converted: an RFC 3339
/// UTC time to the second. Added for the Library (2026-11-02) beside
/// `"source"`, `"scenes"` and `"pages"`:
///
/// ```json
/// {
///   "converted": "2026-11-02T18:00:00Z",
///   "source": "/path/to/Script.pdf"
/// }
/// ```
///
/// Written by every library conversion that writes the folder's .fountain
/// (from a PDF or from a .fountain elsewhere), so a reconversion moves it.
/// It is not bound to the .fountain's digest: it says when the script was
/// converted, which editing the .fountain afterwards does not change. The
/// Library lists it instead of the .fountain's modified time, which an edit
/// outside the app would move.
pub const CONVERTED_KEY: &str = "converted";

/// Keep `epoch_seconds` as the conversion time in the source.json beside
/// `fountain_path`. Only a library script folder has that marker: anywhere
/// else nothing is written.
pub fn save_converted_time(fountain_path: &str, epoch_seconds: i64) -> Result<(), EngineError> {
    keep_in_marker(
        fountain_path,
        CONVERTED_KEY,
        Some(Value::String(crate::recent::rfc3339(epoch_seconds))),
    )
}

/// `YYYY-MM-DDTHH:MM:SSZ`, the one shape [`save_converted_time`] writes.
fn is_utc_seconds(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

/// The conversion time kept beside `fountain_path`, as it was written;
/// `None` when there is none (no marker, an older marker, or a value not of
/// the shape this engine writes).
pub fn read_converted_time(fountain_path: &str) -> Option<String> {
    let text = fsx::read_text(&marker_beside(fountain_path)).ok()?;
    let converted = jsjson::parse(&text)?
        .get(CONVERTED_KEY)?
        .as_str()?
        .to_owned();
    is_utc_seconds(&converted).then_some(converted)
}

/// source.json's key for the name a script is kept under in the library,
/// when someone renamed it (`library-rename`, 2026-11-03). Added beside
/// `"source"`, `"scenes"`, `"pages"` and `"converted"`:
///
/// ```json
/// {
///   "name": "The Lamp Keeper",
///   "source": "/path/to/lampkeeper_v3_FINAL.pdf"
/// }
/// ```
///
/// Nothing on disk is renamed by it: the folder and its files keep the
/// source's stem, which is how a conversion finds the folder again. The
/// library listing calls the script by it, and a conversion into the
/// folder titles the book by it unless `--title` says otherwise. It is
/// not bound to the .fountain's digest: it names the script, which editing
/// the .fountain does not change. A reconversion keeps it, as it keeps
/// every key it does not write.
pub const NAME_KEY: &str = "name";

/// The longest name a script is kept under, in characters.
pub const MAX_NAME_CHARS: usize = 200;

/// Whether `c` breaks a line: what a one-line name must not hold.
fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// Why `name` cannot be a kept name, or `None` when it can. `name` is
/// already trimmed and not empty.
pub fn kept_name_refusal(name: &str) -> Option<String> {
    if name.chars().any(is_line_break) {
        return Some("a script's name is one line: it cannot hold a line break".into());
    }
    let chars = name.chars().count();
    if chars > MAX_NAME_CHARS {
        return Some(format!(
            "a script's name can be at most {MAX_NAME_CHARS} characters long, and this one is {chars}"
        ));
    }
    None
}

/// Keep `name` as the script's name in the source.json beside
/// `fountain_path`, or clear it when `None`. Only a library script folder
/// has that marker: anywhere else nothing is written.
pub fn save_kept_name(fountain_path: &str, name: Option<&str>) -> Result<(), EngineError> {
    keep_in_marker(
        fountain_path,
        NAME_KEY,
        name.map(|n| Value::String(n.to_owned())),
    )
}

/// The name kept beside `fountain_path`, trimmed; `None` when there is
/// none (no marker, an older marker, or a value [`save_kept_name`] would
/// not have written: not text, blank, too long, or more than one line).
pub fn read_kept_name(fountain_path: &str) -> Option<String> {
    let text = fsx::read_text(&marker_beside(fountain_path)).ok()?;
    let marker = jsjson::parse(&text)?;
    let name = js_trim(marker.get(NAME_KEY)?.as_str()?);
    (!name.is_empty() && kept_name_refusal(name).is_none()).then(|| name.to_owned())
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

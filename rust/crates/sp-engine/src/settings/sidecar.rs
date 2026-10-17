//! Port of `src/settings/sidecar.ts`: per-script formatting overrides,
//! stored beside the script's .fountain as `<Stem>.method-reader.json`. Absent
//! sidecar = the caller's base. A sidecar under the 0.12 name,
//! `<Stem>.screepub.json`, is read while the new one is missing, and
//! replaced by it the first time the settings are saved.

use serde_json::{Map, Value};
use sp_model::{FormatOptions, resolve_format_options};

use crate::errors::EngineError;
use crate::fsx;
use crate::jsjson;
use crate::nodepath::host;

/// What a sidecar's name adds to the script's stem.
pub const SIDECAR_SUFFIX: &str = ".method-reader.json";
/// The 0.12 app's suffix, Screepub's: still read, never written.
pub const LEGACY_SIDECAR_SUFFIX: &str = ".screepub.json";

/// `<Stem><suffix>` beside `fountain_path`. The stem is taken with
/// `extname`, the same rule the library names a script's folder and its
/// `.fountain` with.
fn beside(fountain_path: &str, suffix: &str) -> String {
    let name = host::basename(fountain_path);
    let stem = host::basename_ext(&name, &host::extname(&name));
    host::join(&[host::dirname(fountain_path), format!("{stem}{suffix}")])
}

/// `sidecarPath(fountainPath)`: `<Stem>.method-reader.json` beside the
/// file it belongs to: where the settings are written.
pub fn sidecar_path(fountain_path: &str) -> String {
    beside(fountain_path, SIDECAR_SUFFIX)
}

/// `<Stem>.screepub.json` beside the file: where the 0.12 app wrote them.
pub fn legacy_sidecar_path(fountain_path: &str) -> String {
    beside(fountain_path, LEGACY_SIDECAR_SUFFIX)
}

/// What a script's sidecar says: `None` when there is no sidecar to read,
/// `settings: None` when a file IS there but is not a settings object.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptSettingsRead {
    pub path: String,
    pub settings: Option<FormatOptions>,
}

/// `readScriptSettings(fountainPath, fallback)`.
pub fn read_script_settings(
    fountain_path: &str,
    fallback: &FormatOptions,
) -> Option<ScriptSettingsRead> {
    let mut path = sidecar_path(fountain_path);
    if !fsx::exists(&path) {
        path = legacy_sidecar_path(fountain_path);
    }
    let text = fsx::read_text(&path).ok()?;
    let settings = match jsjson::parse(&text) {
        Some(v @ Value::Object(_)) => Some(resolve_format_options(Some(&v), fallback)),
        _ => None,
    };
    Some(ScriptSettingsRead { path, settings })
}

/// `loadScriptSettings(fountainPath, fallback)`: the sidecar overlaid on
/// `fallback`, or `fallback` itself when there is nothing usable.
pub fn load_script_settings(fountain_path: &str, fallback: &FormatOptions) -> FormatOptions {
    read_script_settings(fountain_path, fallback)
        .and_then(|r| r.settings)
        .unwrap_or(*fallback)
}

/// `saveScriptSettings(settings, fountainPath)`: sorted keys, two-space
/// JSON and a newline, through a temp file and a rename. Written under the
/// new name; a sidecar under the 0.12 name beside it is then removed, so
/// the two can never disagree.
pub fn save_script_settings(
    settings: &FormatOptions,
    fountain_path: &str,
) -> Result<(), EngineError> {
    let path = sidecar_path(fountain_path);
    let Value::Object(map) = serde_json::to_value(settings).expect("options serialize") else {
        unreachable!("FormatOptions is a struct");
    };
    let mut entries: Vec<(String, Value)> = map.into_iter().collect();
    // JavaScript's `<` on strings: UTF-16 code units, which for these ASCII
    // keys is byte order.
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let sorted: Map<String, Value> = entries.into_iter().collect();
    let text = format!("{}\n", jsjson::stringify_pretty(&Value::Object(sorted)));
    fsx::write_atomic(&path, text.as_bytes())?;
    fsx::rm_force(&legacy_sidecar_path(fountain_path));
    Ok(())
}

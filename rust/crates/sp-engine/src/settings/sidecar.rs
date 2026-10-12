//! Port of `src/settings/sidecar.ts`: per-script formatting overrides,
//! stored beside the script's .fountain as `<Stem>.screepub.json`. Absent
//! sidecar = the caller's base.

use serde_json::{Map, Value};
use sp_model::{FormatOptions, resolve_format_options};

use crate::errors::EngineError;
use crate::fsx;
use crate::jsjson;
use crate::nodepath::host;

/// `sidecarPath(fountainPath)`: `<Stem>.screepub.json` beside the file it
/// belongs to. The stem is taken with `extname`, the same rule the library
/// names a script's folder and its `.fountain` with.
pub fn sidecar_path(fountain_path: &str) -> String {
    let name = host::basename(fountain_path);
    let stem = host::basename_ext(&name, &host::extname(&name));
    host::join(&[
        host::dirname(fountain_path),
        format!("{stem}.screepub.json"),
    ])
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
    let path = sidecar_path(fountain_path);
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
/// JSON and a newline, through a temp file and a rename.
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
    fsx::write_atomic(&path, text.as_bytes())
}

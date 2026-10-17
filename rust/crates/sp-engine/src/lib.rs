//! Method Reader's engine, beyond the conversion itself: the settings file and
//! a script's sidecar, the library, the export ladder and the send routes,
//! Calibre and the KFX toolchain, and the updater's version decisions. A
//! port of the TypeScript engine's `src/settings/*`, `src/library*.ts`,
//! `src/export/*`, `src/update/*` and the handlers in `src/cli-*.ts` the
//! `settings`, `app-settings`, `export`, `routes`, `route`, `kfx-*` and
//! `update-*` verbs call.
//!
//! Every answer serializes to the TypeScript's `--json` field for field;
//! [`jsjson::to_string`] writes it the way `JSON.stringify` does.
//!
//! Not here yet: the conversion (`src/convert.ts`, wave 3), device
//! detection and sending (`src/device/*`, the sp-device branch), and the
//! book makers (sp-azw3, sp-mobi, sp-fdx, sp-fountain), which the export
//! reaches through [`export::renderers::Renderers`].

pub mod commands;
pub mod convert;
pub mod env;
pub mod errors;
pub mod export;
pub mod fsx;
pub mod jsjson;
pub mod legacy;
pub mod library;
pub mod library_images;
/// node:path's functions: sp-fsutil's, the one copy (re-exported so the
/// engine's callers keep their paths).
pub use sp_fsutil::nodepath;
pub mod settings;
pub mod update;

/// The envelope the CLI prints for a successful `--json` answer:
/// `{"ok":true,...answer}`.
pub fn ok_envelope<T: serde::Serialize>(answer: &T) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("ok".into(), serde_json::Value::Bool(true));
    if let serde_json::Value::Object(fields) =
        serde_json::to_value(answer).expect("an answer always serializes")
    {
        for (k, v) in fields {
            map.insert(k, v);
        }
    }
    serde_json::Value::Object(map)
}

/// The envelope for a failure: `{"ok":false,"error":{code,message}}`.
pub fn error_envelope(error: &errors::JsonError) -> serde_json::Value {
    serde_json::json!({ "ok": false, "error": error })
}

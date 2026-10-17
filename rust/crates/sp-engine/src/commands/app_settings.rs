//! Port of `src/cli-app-settings.ts`: the `app-settings` verb. Where books
//! land, and what new scripts start from. Writes `libraryPath`,
//! `formatDefaults`, `keepScriptSettings` and the window's three remembered
//! choices (`readerDevice`, `animatePage`, `typewriterSounds`), and the
//! Rust window's `panelIntent` (read or edit, new behavior, not in the
//! TypeScript); never `lastRoute`. Returns a result; the CLI owns stdout.

use serde::Serialize;
use serde_json::Value;
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions, resolve_format_options};

use crate::env::{Env, host_platform};
use crate::errors::{CliError, EngineError, ErrorCode, plain_reason};
use crate::fsx;
use crate::jsjson;
use crate::library::{
    chosen_library_path, env_library_override, library_root, platform_library_default,
    resolved_if_absolute,
};
use crate::settings::app::{
    PANEL_INTENTS, Patch, READER_DEVICES, app_settings_path, home_folder, read_app_settings,
    write_app_settings,
};
use crate::settings::app_defaults::{
    app_default_options, app_defaults_customized, keeps_script_settings,
};

/// The only keys this verb may write.
const WRITABLE_KEYS: [&str; 7] = [
    "libraryPath",
    "formatDefaults",
    "keepScriptSettings",
    "readerDevice",
    "animatePage",
    "typewriterSounds",
    "panelIntent",
];

/// `AppSettingsOptions`.
#[derive(Debug, Clone, Default)]
pub struct AppSettingsOptions {
    /// A JSON object with any of the writable keys; each may be `null` to
    /// reset it. `None`: a plain read.
    pub set: Option<String>,
}

/// `AppSettingsDeps`: where the file is, which platform, which
/// environment. Each defaults to this machine's.
#[derive(Debug, Clone, Default)]
pub struct AppSettingsDeps {
    pub settings_path: Option<String>,
    pub platform: Option<String>,
    pub env: Option<Env>,
}

/// `AppSettingsLibraryInfo`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsLibraryInfo {
    /// The folder conversions will actually use.
    pub path: String,
    /// The stored choice, resolved, or null.
    pub chosen: Option<String>,
    /// The folder with no choice and no METHOD_READER_LIBRARY.
    pub platform_default: String,
    /// True when METHOD_READER_LIBRARY is overriding `path`.
    pub from_env: bool,
}

/// `AppSettingsResult`, in the TypeScript's field order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsResult {
    pub file: String,
    pub library: AppSettingsLibraryInfo,
    pub home: String,
    pub format_defaults: FormatOptions,
    pub shipped_defaults: FormatOptions,
    pub customized: bool,
    pub keep_script_settings: bool,
    pub reader_device: Option<String>,
    pub animate_page: bool,
    pub typewriter_sounds: bool,
    /// "read" (the default) or "edit".
    pub panel_intent: String,
}

fn bad(message: impl Into<String>) -> EngineError {
    CliError::new(ErrorCode::BadSettings, message).into()
}

/// The pure check for `libraryPath`: `None` for null (reset), the resolved
/// path for a good absolute string.
fn checked_library_path(value: &Value, platform: &str) -> Result<Option<String>, EngineError> {
    match value {
        Value::Null => Ok(None),
        Value::String(s) => resolved_if_absolute(platform, s)
            .map(Some)
            .ok_or_else(|| bad("the library folder must be a full path")),
        _ => Err(bad("libraryPath must be a full path or null")),
    }
}

/// `accessSync(path, W_OK)`.
fn check_writable(path: &str) -> Result<(), EngineError> {
    #[cfg(unix)]
    {
        rustix::fs::access(path, rustix::fs::Access::WRITE_OK).map_err(|errno| {
            let io = std::io::Error::from_raw_os_error(errno.raw_os_error());
            EngineError::Fs(crate::errors::FsError::from_io(&io, "access", path, None))
        })
    }
    #[cfg(not(unix))]
    {
        let meta = std::fs::metadata(path).map_err(|e| {
            EngineError::Fs(crate::errors::FsError::from_io(&e, "access", path, None))
        })?;
        // libuv's access(W_OK) on Windows refuses a READONLY file but
        // ignores the attribute on a folder, where Windows sets it on any
        // customized folder (Documents, Music, a OneDrive folder, one with
        // its own icon) without making it unwritable. Reading it there
        // refused exactly the folders a person is likely to pick.
        if meta.permissions().readonly() && !meta.is_dir() {
            return Err(EngineError::Fs(crate::errors::FsError::with_code(
                "EPERM",
                format!("EPERM: operation not permitted, access '{path}'"),
            )));
        }
        Ok(())
    }
}

/// The one side effect for `libraryPath`, run LAST: create it and check it
/// is writable.
fn created_writable_folder(resolved: &str) -> Result<String, EngineError> {
    fsx::mkdir_all(resolved)
        .and_then(|()| check_writable(resolved))
        .map_err(|err| {
            bad(format!(
                "cannot use {resolved} as the library: {}",
                plain_reason(&err)
            ))
        })?;
    Ok(resolved.to_owned())
}

fn validated_format_defaults(value: &Value) -> Result<Option<Value>, EngineError> {
    match value {
        Value::Null => Ok(None),
        Value::Object(_) => Ok(Some(
            serde_json::to_value(resolve_format_options(Some(value), &DEFAULT_FORMAT_OPTIONS))
                .expect("options serialize"),
        )),
        _ => Err(bad("formatDefaults must be an object or null")),
    }
}

fn validated_switch(name: &str, value: &Value) -> Result<Option<Value>, EngineError> {
    match value {
        Value::Null => Ok(None),
        Value::Bool(_) => Ok(Some(value.clone())),
        _ => Err(bad(format!("{name} must be true, false or null"))),
    }
}

fn validated_reader_device(value: &Value) -> Result<Option<Value>, EngineError> {
    match value {
        Value::Null => Ok(None),
        Value::String(s) if READER_DEVICES.contains(&s.as_str()) => Ok(Some(value.clone())),
        _ => Err(bad(format!(
            "readerDevice must be one of {} or null",
            READER_DEVICES.join(", ")
        ))),
    }
}

fn validated_panel_intent(value: &Value) -> Result<Option<Value>, EngineError> {
    match value {
        Value::Null => Ok(None),
        Value::String(s) if PANEL_INTENTS.contains(&s.as_str()) => Ok(Some(value.clone())),
        _ => Err(bad("panelIntent must be read, edit or null")),
    }
}

fn build_answer(file: &str, platform: &str, env: &Env) -> AppSettingsResult {
    let stored = read_app_settings(file);
    let reader_device = match stored.get("readerDevice") {
        Some(Value::String(d)) if READER_DEVICES.contains(&d.as_str()) => Some(d.clone()),
        _ => None,
    };
    AppSettingsResult {
        file: file.to_owned(),
        library: AppSettingsLibraryInfo {
            path: library_root(platform, env, Some(file)),
            chosen: chosen_library_path(platform, env, Some(file)),
            platform_default: platform_library_default(platform, env),
            from_env: env_library_override(env).is_some(),
        },
        home: home_folder(env),
        format_defaults: app_default_options(file),
        shipped_defaults: DEFAULT_FORMAT_OPTIONS,
        customized: app_defaults_customized(file),
        keep_script_settings: keeps_script_settings(file),
        reader_device,
        animate_page: stored.get("animatePage") != Some(&Value::Bool(false)),
        typewriter_sounds: stored.get("typewriterSounds") == Some(&Value::Bool(true)),
        panel_intent: match stored.get("panelIntent") {
            Some(Value::String(i)) if PANEL_INTENTS.contains(&i.as_str()) => i.clone(),
            _ => "read".into(),
        },
    }
}

/// `appSettingsCommand(options, deps)`.
pub fn app_settings_command(
    options: &AppSettingsOptions,
    deps: &AppSettingsDeps,
) -> Result<AppSettingsResult, EngineError> {
    let platform = deps
        .platform
        .clone()
        .unwrap_or_else(|| host_platform().to_owned());
    let env = deps.env.clone().unwrap_or_else(Env::process);
    let file = deps
        .settings_path
        .clone()
        .unwrap_or_else(|| app_settings_path(&platform, &env));

    if let Some(set) = &options.set {
        let Some(parsed) = jsjson::parse(set) else {
            return Err(bad("--set is not valid JSON"));
        };
        let Value::Object(patch) = parsed else {
            return Err(bad("--set must be a JSON object"));
        };

        // Every key is checked before anything is validated or written.
        for key in jsjson::js_key_order(&patch) {
            if !WRITABLE_KEYS.contains(&key.as_str()) {
                return Err(CliError::new(
                    ErrorCode::Usage,
                    format!(
                        "app-settings --set takes only libraryPath, formatDefaults, keepScriptSettings, readerDevice, animatePage, typewriterSounds and panelIntent, not \"{key}\""
                    ),
                )
                .into());
            }
        }

        // An empty object changes nothing: it answers like a plain read.
        if patch.is_empty() {
            return Ok(build_answer(&file, &platform, &env));
        }

        // Phase 1: every pure check, for every key, before any one touches
        // disk.
        let mut write: Patch = Vec::new();
        let library_folder = match patch.get("libraryPath") {
            Some(v) => Some(checked_library_path(v, &platform)?),
            None => None,
        };
        if let Some(v) = patch.get("formatDefaults") {
            write.push(("formatDefaults".into(), validated_format_defaults(v)?));
        }
        if let Some(v) = patch.get("keepScriptSettings") {
            write.push((
                "keepScriptSettings".into(),
                validated_switch("keepScriptSettings", v)?,
            ));
        }
        if let Some(v) = patch.get("readerDevice") {
            write.push(("readerDevice".into(), validated_reader_device(v)?));
        }
        if let Some(v) = patch.get("animatePage") {
            write.push(("animatePage".into(), validated_switch("animatePage", v)?));
        }
        if let Some(v) = patch.get("typewriterSounds") {
            write.push((
                "typewriterSounds".into(),
                validated_switch("typewriterSounds", v)?,
            ));
        }

        if let Some(v) = patch.get("panelIntent") {
            write.push(("panelIntent".into(), validated_panel_intent(v)?));
        }

        // Phase 2: the one side effect, last.
        if let Some(folder) = library_folder {
            let value = match folder {
                None => None,
                Some(resolved) => Some(Value::String(created_writable_folder(&resolved)?)),
            };
            write.push(("libraryPath".into(), value));
        }

        write_app_settings(&write, &file).map_err(|err| {
            bad(format!(
                "cannot save the settings file {file}: {}",
                plain_reason(&err)
            ))
        })?;
    }

    Ok(build_answer(&file, &platform, &env))
}

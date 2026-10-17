//! Port of `src/cli-settings.ts`: the `settings` verb. A script's own
//! formatting, read and written through its sidecar. Returns a result; the
//! CLI owns stdout.

use serde::Serialize;
use serde_json::Value;
use sp_model::{
    DEFAULT_FORMAT_OPTIONS, DevicePresetId, FormatOptions, device_preset, matching_preset,
    resolve_format_options,
};

use crate::errors::{CliError, EngineError, ErrorCode};
use crate::fsx;
use crate::jsjson;
use crate::settings::app_defaults::{app_default_options, keeps_script_settings};
use crate::settings::sidecar::{load_script_settings, save_script_settings, sidecar_path};

/// One entry of `presets`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetEntry {
    pub id: DevicePresetId,
    pub display_name: &'static str,
    pub settings: FormatOptions,
}

/// `SettingsResult`, in the TypeScript's field order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResult {
    pub settings: FormatOptions,
    /// Method Reader's own shipped defaults, never the user's app-wide ones.
    pub defaults: FormatOptions,
    /// The user's app-wide format defaults.
    pub app_defaults: FormatOptions,
    pub preset: Option<DevicePresetId>,
    pub presets: Vec<PresetEntry>,
    pub sidecar: String,
    pub keep_script_settings: bool,
}

/// `SettingsOptions`.
#[derive(Debug, Clone)]
pub struct SettingsOptions {
    /// The script's .fountain: the sidecar is named after it.
    pub fountain: String,
    /// A PARTIAL FormatOptions as JSON, overlaid on what is stored.
    pub set: Option<String>,
    /// The app settings file the app-wide defaults are read from.
    pub app_settings_path: String,
}

/// `settingsCommand(options)`.
pub fn settings_command(options: &SettingsOptions) -> Result<SettingsResult, EngineError> {
    if !fsx::is_file(&options.fountain) {
        return Err(CliError::new(
            ErrorCode::Unreadable,
            format!("cannot read the script: {}", options.fountain),
        )
        .into());
    }

    let app_defaults = app_default_options(&options.app_settings_path);
    let mut settings = load_script_settings(&options.fountain, &app_defaults);

    if let Some(set) = &options.set {
        let Some(parsed) = jsjson::parse(set) else {
            return Err(CliError::new(ErrorCode::BadSettings, "--set is not valid JSON").into());
        };
        if !matches!(parsed, Value::Object(_)) {
            return Err(
                CliError::new(ErrorCode::BadSettings, "--set must be a JSON object").into(),
            );
        }
        settings = resolve_format_options(Some(&parsed), &settings);
        save_script_settings(&settings, &options.fountain)?;
    }

    Ok(SettingsResult {
        settings,
        defaults: DEFAULT_FORMAT_OPTIONS,
        app_defaults,
        preset: matching_preset(&settings),
        presets: DevicePresetId::ALL
            .into_iter()
            .map(|id| {
                let p = device_preset(id);
                PresetEntry {
                    id,
                    display_name: p.display_name,
                    settings: p.settings,
                }
            })
            .collect(),
        sidecar: sidecar_path(&options.fountain),
        keep_script_settings: keeps_script_settings(&options.app_settings_path),
    })
}

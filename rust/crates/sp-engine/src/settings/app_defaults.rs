//! Port of `src/settings/app-defaults.ts`: the app-wide format defaults,
//! the base every script starts from. One layer OVER Method Reader's shipped
//! defaults and one layer UNDER a script's own sidecar: explicit flags >
//! the script's sidecar > these > `DEFAULT_FORMAT_OPTIONS`, through the one
//! merge `resolve_format_options` already is.

use serde_json::Value;
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions, resolve_format_options};

use super::app::read_app_settings;

/// `formatDefaults` off the settings file, or `None` when the stored value
/// is not a plain object.
fn stored_format_defaults(settings_path: &str) -> Option<Value> {
    match read_app_settings(settings_path).get("formatDefaults") {
        Some(v @ Value::Object(_)) => Some(v.clone()),
        _ => None,
    }
}

/// `appDefaultOptions(settingsPath)`: the user's app-wide defaults as a
/// FULL, resolved `FormatOptions`, every knob present and clamped.
pub fn app_default_options(settings_path: &str) -> FormatOptions {
    resolve_format_options(
        stored_format_defaults(settings_path).as_ref(),
        &DEFAULT_FORMAT_OPTIONS,
    )
}

/// `appDefaultsCustomized(settingsPath)`: true once the defaults have moved
/// away from Method Reader's own, knob by knob.
pub fn app_defaults_customized(settings_path: &str) -> bool {
    app_default_options(settings_path) != DEFAULT_FORMAT_OPTIONS
}

/// `keepsScriptSettings(settingsPath)`: whether a `--library` conversion
/// that finds no sidecar saves the settings it started from. Only a stored
/// `false` turns it off; anything that is not a boolean is the default.
pub fn keeps_script_settings(settings_path: &str) -> bool {
    read_app_settings(settings_path).get("keepScriptSettings") != Some(&Value::Bool(false))
}

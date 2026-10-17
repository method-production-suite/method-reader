//! The product's name, and the two environment variables a person might
//! have set under the old one. METHOD_READER_CONFIG_DIR and
//! METHOD_READER_LIBRARY are read first; SCREEPUB_CONFIG_DIR and
//! SCREEPUB_LIBRARY (the LEGACY names, up to 0.12.x) only when the new one
//! is unset, the same rule the engine follows for its own settings and
//! library.

/// What the window calls itself: title, menus, About, messages.
pub const PRODUCT: &str = "Method Reader";

/// The settings folder override (the engine's, which the window's own
/// storage follows).
pub const CONFIG_DIR_VAR: &str = "METHOD_READER_CONFIG_DIR";
/// [`CONFIG_DIR_VAR`] under the old name, still read as a fallback.
pub const LEGACY_CONFIG_DIR_VAR: &str = "SCREEPUB_CONFIG_DIR";

/// The library folder override.
pub const LIBRARY_VAR: &str = "METHOD_READER_LIBRARY";
/// [`LIBRARY_VAR`] under the old name, still read as a fallback.
pub const LEGACY_LIBRARY_VAR: &str = "SCREEPUB_LIBRARY";

/// The value of `name`, or of `legacy` when `name` is unset or empty, with
/// the name it came from. None when neither has a value.
pub fn var_with_legacy(
    env: &dyn Fn(&str) -> Option<String>,
    name: &'static str,
    legacy: &'static str,
) -> Option<(String, &'static str)> {
    [name, legacy]
        .into_iter()
        .find_map(|n| env(n).filter(|v| !v.is_empty()).map(|v| (v, n)))
}

/// This process's own environment, as [`var_with_legacy`] reads it.
pub fn process_env(name: &str) -> Option<String> {
    std::env::var_os(name).map(|v| v.to_string_lossy().into_owned())
}

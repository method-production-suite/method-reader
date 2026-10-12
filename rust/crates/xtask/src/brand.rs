//! Reading brand/: the colours of brand/tokens.json, which
//! `tests/brand_tokens.rs` holds sp-app's theme.rs to.
//!
//! Until the Rust cutover this was `cargo xtask build-desktop-tokens`
//! (tools/build-desktop-tokens.ts before it), which generated the Tauri
//! window's desktop/ui/tokens.css from brand/tokens.json and the scale
//! blocks of brand/tokens.css. That file went with the Tauri window, so the
//! command and the generator retired; the egui window keeps its colours,
//! type scale, radii and motion in sp-app's theme.rs, and the checks that
//! still have a subject read brand/ through this.

use serde_json::Value;

/// The colours of brand/tokens.json, in file order: (name, light, dark).
pub fn brand_colors(tokens_json: &str) -> Result<Vec<(String, String, String)>, String> {
    let parsed: Value =
        serde_json::from_str(tokens_json).map_err(|e| format!("brand/tokens.json: {e}"))?;
    let colors = parsed
        .get("colors")
        .and_then(Value::as_object)
        .ok_or("brand/tokens.json has no colors object")?;
    colors
        .iter()
        .map(|(name, c)| {
            let mode = |m: &str| {
                c.get(m)
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| format!("brand/tokens.json: {name} has no {m} value"))
            };
            Ok((name.clone(), mode("light")?, mode("dark")?))
        })
        .collect()
}

//! The window's own desktop/ui/index.html, with the capture scripts
//! inserted (was tools/capture/page.ts).
//!
//! Served, never copied: a copy would be a second definition of the
//! window's page, and the day the window added a stylesheet the pictures
//! would quietly stop matching the app. So the server reads the real file on
//! every request and inserts four things:
//!
//! - `<base href="/desktop/ui/">`: relative URLs resolve as in the app
//! - `window.__CAPTURE__ = {...}`: which shot, where the demo PDF is, and
//!   the run's token for /engine
//! - bridge.js BEFORE main.js: `window.__TAURI__` exists when main.js makes
//!   its first engine call at boot
//! - steps.js AFTER main.js: drives the window into the shot
//!
//! Module scripts run in document order, which is the whole mechanism.

use serde_json::json;

pub struct CaptureConfig<'a> {
    pub shot: &'a str,
    pub demo_pdf: &'a str,
    /// The per-run secret the capture server's /engine requires.
    pub token: &'a str,
}

const MAIN: &str = r#"<script type="module" src="main.js"></script>"#;

/// The config as the page sees it, `{"shot":..,"demoPdf":..,"token":..}`.
pub fn config_json(cfg: &CaptureConfig<'_>) -> String {
    json!({ "shot": cfg.shot, "demoPdf": cfg.demo_pdf, "token": cfg.token }).to_string()
}

pub fn capture_index(html: &str, cfg: &CaptureConfig<'_>) -> Result<String, String> {
    let count = html.matches(MAIN).count();
    if count != 1 {
        return Err(format!(
            "capture: desktop/ui/index.html must load main.js exactly once as {MAIN}; \
             found {count}. The window changed shape; update rust/crates/xtask/src/capture/page.rs."
        ));
    }
    if !html.contains("<head>") {
        return Err("capture: desktop/ui/index.html has no <head>".into());
    }
    // Escaped so a `<` in a value cannot close the <script> early (the HTML
    // parser looks for the literal text `</script`, however it got there:
    // quoting inside the JS string does not stop it). Inserted as plain
    // text, so nothing in a value is ever read as a substitution pattern.
    let config = config_json(cfg).replace('<', "\\u003c");
    let head = format!(
        "<head>\n    <base href=\"/desktop/ui/\">\n    <script>window.__CAPTURE__ = {config};</script>"
    );
    let scripts = format!(
        "<script type=\"module\" src=\"/tools/capture/bridge.js\"></script>\n    {MAIN}\n    \
         <script type=\"module\" src=\"/tools/capture/steps.js\"></script>"
    );
    Ok(html.replacen("<head>", &head, 1).replacen(MAIN, &scripts, 1))
}

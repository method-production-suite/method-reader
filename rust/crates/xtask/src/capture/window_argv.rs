//! The window's argv builders, as desktop/ui/app.js spells them.
//!
//! The TypeScript gate imported app.js itself, so the gate was checked
//! against the window's real contract. Rust cannot import JavaScript, so
//! this is a copy of the builders the gate and its tests use, held to the
//! original two ways: tests/capture.rs compares each builder's SOURCE in
//! app.js with the text it was copied from ([`SOURCES`]), so an edit there
//! fails the suite until this file follows it; and, where Chrome is
//! installed, tests/capture_chrome.rs runs app.js's builders in Chrome and
//! compares their answers with these.
//!
//! JavaScript's `x ? flag : null, x` drops the flag of a falsy value but
//! keeps the value itself unless it is null, so an empty string rides along
//! without its flag here too.

use serde_json::json;

/// What the window's override button stands for.
pub const FORCE_FLAG: &str = "--force";

fn v(parts: &[Option<&str>]) -> Vec<String> {
    parts.iter().flatten().map(|s| (*s).to_string()).collect()
}

/// `Some(s)` when `s` is a non-empty string: JavaScript's truthiness.
fn truthy(s: Option<&str>) -> Option<&str> {
    s.filter(|s| !s.is_empty())
}

/// `value ? flag : null, value`, then the null filter: the flag only when
/// the value is truthy, but the value whenever it is not null, so an empty
/// string rides along without its flag, exactly as app.js builds it
/// (measured in Chrome by tests/capture_chrome.rs).
fn pair<'a>(flag: &'a str, value: Option<&'a str>) -> [Option<&'a str>; 2] {
    [truthy(value).map(|_| flag), value]
}

pub fn version() -> Vec<String> {
    v(&[Some("--version"), Some("--json")])
}

pub fn convert(path: &str, force: bool, options_json: Option<&str>) -> Vec<String> {
    let mut out = v(&[
        Some(path),
        Some("--json"),
        Some("--progress"),
        Some("--preview-inline"),
        Some("--library"),
        force.then_some(FORCE_FLAG),
    ]);
    out.extend(v(&pair("--options-json", options_json)));
    out
}

pub fn reconvert(fountain: &str, epub: &str, options_json: &str) -> Vec<String> {
    v(&[
        Some(fountain),
        Some("--json"),
        Some("--preview-inline"),
        Some("-o"),
        Some(epub),
        Some("--options-json"),
        Some(options_json),
    ])
}

pub fn devices(quick: bool) -> Vec<String> {
    v(&[Some("devices"), Some("--json"), quick.then_some("--quick")])
}

pub fn eject(volume: &str) -> Vec<String> {
    v(&[Some("eject"), Some(volume), Some("--json")])
}

pub fn reader_device(kind: &str) -> Vec<String> {
    let set = json!({ "readerDevice": kind }).to_string();
    v(&[
        Some("app-settings"),
        Some("--json"),
        Some("--set"),
        Some(&set),
    ])
}

pub fn send(file: &str, device_id: Option<&str>) -> Vec<String> {
    let mut out = v(&[Some("send"), Some(file), Some("--json")]);
    out.extend(v(&pair("--device", device_id)));
    out
}

pub fn settings(fountain: &str, set: Option<&str>) -> Vec<String> {
    let mut out = v(&[Some("settings"), Some(fountain), Some("--json")]);
    out.extend(v(&pair("--set", set)));
    out
}

pub fn export(
    epub: &str,
    for_format: &str,
    fountain: Option<&str>,
    options_json: Option<&str>,
    out: Option<&str>,
) -> Vec<String> {
    let mut args = v(&[
        Some("export"),
        Some(epub),
        Some("--json"),
        Some("--for"),
        Some(for_format),
    ]);
    args.extend(v(&pair("--fountain", fountain)));
    args.extend(v(&pair("--options-json", options_json)));
    args.extend(v(&pair("--out", out)));
    args
}

pub fn kindle_check(epub: &str) -> Vec<String> {
    v(&[
        Some("export"),
        Some(epub),
        Some("--json"),
        Some("--for"),
        Some("kindle"),
        Some("--check"),
    ])
}

pub fn kfx_status() -> Vec<String> {
    v(&[Some("kfx-status"), Some("--json")])
}

pub fn kfx_install() -> Vec<String> {
    v(&[Some("kfx-install"), Some("--json")])
}

pub fn app_settings(set: Option<&str>) -> Vec<String> {
    let mut out = v(&[Some("app-settings"), Some("--json")]);
    out.extend(v(&pair("--set", set)));
    out
}

pub fn routes(epub: &str, quick: bool) -> Vec<String> {
    v(&[
        Some("routes"),
        Some(epub),
        Some("--json"),
        quick.then_some("--quick"),
    ])
}

pub fn route(
    key: &str,
    epub: &str,
    out: Option<&str>,
    fountain: Option<&str>,
    options_json: Option<&str>,
) -> Vec<String> {
    let mut args = v(&[Some("route"), Some(key), Some(epub), Some("--json")]);
    args.extend(v(&pair("--out", out)));
    args.extend(v(&pair("--fountain", fountain)));
    args.extend(v(&pair("--options-json", options_json)));
    args
}

/// Each builder above, as app.js spelled it when it was copied, with runs
/// of whitespace folded to one space. tests/capture.rs finds the same
/// builder in app.js and compares; a difference means the window's contract
/// moved and this file must follow it.
pub const SOURCES: &[(&str, &str)] = &[
    ("version", "version: () => ['--version', '--json'],"),
    (
        "convert",
        "convert: (path, { force = false, optionsJson = null } = {}) => [path, '--json', '--progress', \
         '--preview-inline', '--library', force ? FORCE_FLAG : null, optionsJson ? '--options-json' : null, \
         optionsJson].filter((a) => a !== null),",
    ),
    (
        "reconvert",
        "reconvert: (fountain, epubPath, optionsJson) => [fountain, '--json', '--preview-inline', '-o', \
         epubPath, '--options-json', optionsJson],",
    ),
    (
        "devices",
        "devices: ({ quick = false } = {}) => ['devices', '--json', quick ? '--quick' : null].filter((a) => a !== null),",
    ),
    ("eject", "eject: (volume) => ['eject', volume, '--json'],"),
    (
        "readerDevice",
        "readerDevice: (kind) => ['app-settings', '--json', '--set', JSON.stringify({ readerDevice: kind })],",
    ),
    (
        "send",
        "send: (file, deviceId = null) => ['send', file, '--json', deviceId ? '--device' : null, deviceId] \
         .filter((a) => a !== null),",
    ),
    (
        "settings",
        "settings: (fountain, set = null) => ['settings', fountain, '--json', set ? '--set' : null, set]\
         .filter((a) => a !== null),",
    ),
    (
        "export",
        "export: (epub, { forFormat, fountain = null, optionsJson = null, out = null }) => ['export', epub, \
         '--json', '--for', forFormat, fountain ? '--fountain' : null, fountain, optionsJson ? '--options-json' \
         : null, optionsJson, out ? '--out' : null, out].filter((a) => a !== null),",
    ),
    (
        "kindleCheck",
        "kindleCheck: (epub) => ['export', epub, '--json', '--for', 'kindle', '--check'],",
    ),
    ("kfxStatus", "kfxStatus: () => ['kfx-status', '--json'],"),
    ("kfxInstall", "kfxInstall: () => ['kfx-install', '--json'],"),
    (
        "appSettings",
        "appSettings: (set = null) => ['app-settings', '--json', set ? '--set' : null, set].filter((a) => a !== null),",
    ),
    (
        "routes",
        "routes: (epub, { quick = false } = {}) => ['routes', epub, '--json', quick ? '--quick' : null]\
         .filter((a) => a !== null),",
    ),
    (
        "route",
        "route: (key, epub, { out = null, fountain = null, optionsJson = null } = {}) => ['route', key, epub, \
         '--json', out ? '--out' : null, out, fountain ? '--fountain' : null, fountain, optionsJson ? \
         '--options-json' : null, optionsJson].filter((a) => a !== null),",
    ),
];

/// Builder `name`'s source in app.js, whitespace folded, comments left
/// out: from `  name: ` to the line before the next builder or the end of
/// the `argv` object. None when app.js has no such builder.
pub fn source_in(app_js: &str, name: &str) -> Option<String> {
    let start = app_js.find("export const argv = {")?;
    let body = &app_js[start..];
    let end = body.find("\n};").unwrap_or(body.len());
    let body = &body[..end];
    let lines: Vec<&str> = body.lines().collect();
    let opener = format!("  {name}: ");
    let first = lines.iter().position(|l| l.starts_with(&opener))?;
    let mut text = String::new();
    for (i, line) in lines[first..].iter().enumerate() {
        // A new builder (two-space indent, a name and a colon) ends this one.
        if i > 0 && is_builder_start(line) {
            break;
        }
        let t = line.trim();
        if t.starts_with("/**") || t.starts_with('*') || t.starts_with("//") {
            continue;
        }
        text.push(' ');
        text.push_str(t);
    }
    let folded: Vec<&str> = text.split_whitespace().collect();
    Some(folded.join(" "))
}

fn is_builder_start(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("  ") else {
        return false;
    };
    if rest.starts_with(' ') {
        return false;
    }
    rest.split_once(": ")
        .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric()))
}

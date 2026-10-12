//! The device-request service, in Rust: services/device-requests/src/worker.ts
//! (spec 2026-10-01, decisions 14 and 15) ported to workers-rs, answering
//! every request the way the TypeScript Worker does.
//!
//! The window posts one of two small JSON forms to `POST /request`:
//!
//! - `{ kind: "device", device, version, platform }`: "My device isn't here"
//! - `{ kind: "report", route, worked, version, platform }`: "It worked" or not
//!
//! and this counts them. It stores no IP address, no user agent and nothing
//! else: a device request adds to its device's count, a report to its
//! route's, each with the day it was first and last seen (a day, never a
//! time). The rate limit is Cloudflare's rate-limit binding, whose counters
//! live at Cloudflare and are never written to D1.
//!
//! Everything here is plain Rust over two small traits ([`Limiter`],
//! [`Store`]), so it is tested natively; `glue` (wasm32 only) adapts
//! workers-rs's request, D1 and rate-limiter types to them.

pub mod js;

#[cfg(target_arch = "wasm32")]
mod glue;

use serde_json::{Map, Value, json};

pub const PATH: &str = "/request";
pub const MAX_BODY_BYTES: usize = 1024;
pub const MAX_DEVICE: usize = 80;
pub const MAX_VERSION: usize = 20;
/// Accepted requests per sender per minute, per Cloudflare location. Kept
/// equal to `limit` in wrangler.toml (a test holds them together).
pub const LIMIT_PER_MINUTE: u32 = 5;
pub const PLATFORMS: [&str; 3] = ["mac", "windows", "linux"];
pub const ROUTES: [&str; 5] = [
    "kobo",
    "tolino",
    "remarkable",
    "kindle-windows",
    "kindle-linux",
];
/// The Tauri window's own origins: macOS and Linux, then Windows (http and
/// https). The egui window sends no Origin at all; CORS is for webviews.
pub const ALLOWED_ORIGINS: [&str; 3] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

pub mod said {
    pub const NOT_FOUND: &str = "Nothing here. Send a POST to /request.";
    pub const TOO_LARGE: &str = "That request is too large.";
    pub const NOT_JSON: &str = "That request is not JSON.";
    pub const UNREADABLE: &str = "That request did not arrive whole. Try again.";
    pub const BAD_FORM: &str = "That request is not one this service takes.";
    pub const BAD_DEVICE: &str = "The device name must be 1 to 80 characters.";
    pub const TOO_MANY: &str = "Too many requests from here just now. Try again in a minute.";
    pub const BROKEN: &str = "Something went wrong on our side. Try again later.";
}

pub const INSERT_DEVICE: &str = "INSERT INTO device_requests (name_key, spelling, count, first_seen, last_seen) VALUES (?, ?, 1, ?, ?) ON CONFLICT (name_key) DO UPDATE SET count = count + 1, spelling = excluded.spelling, last_seen = excluded.last_seen";
pub const INSERT_REPORT: &str = "INSERT INTO route_reports (route, platform, worked, did_not_work, first_seen, last_seen) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (route, platform) DO UPDATE SET worked = worked + excluded.worked, did_not_work = did_not_work + excluded.did_not_work, last_seen = excluded.last_seen";

#[derive(Clone, Debug, PartialEq)]
pub enum Form {
    Device {
        device: String,
        version: String,
        platform: String,
    },
    Report {
        route: String,
        worked: bool,
        version: String,
        platform: String,
    },
}

/// One bound value: D1 receives JavaScript strings and numbers.
#[derive(Clone, Debug, PartialEq)]
pub enum Bind {
    Text(String),
    Number(f64),
}

/// Cloudflare's rate-limit binding: true while the key has places left.
#[allow(async_fn_in_trait)]
pub trait Limiter {
    async fn limit(&self, key: &str) -> Result<bool, String>;
}

/// D1: run one prepared statement with its values.
#[allow(async_fn_in_trait)]
pub trait Store {
    async fn run(&self, sql: &str, values: &[Bind]) -> Result<(), String>;
}

/// The request as the handler reads it. `body` is read only when the
/// handler gets that far, as the TypeScript did.
#[allow(async_fn_in_trait)]
pub trait Incoming {
    fn method(&self) -> String;
    /// `new URL(request.url).pathname`.
    fn path(&self) -> String;
    /// A header, case-insensitively, as `headers.get` answers.
    fn header(&self, name: &str) -> Option<String>;
    /// `await request.text()`; Err when the body cannot be read.
    async fn text(&mut self) -> Result<String, ()>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outgoing {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// None: no body at all (the 204).
    pub body: Option<String>,
}

impl Outgoing {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Lowercased, trimmed, every run of spaces one space.
pub fn normalise_name(name: &str) -> String {
    js::collapse_space(js::trim(name)).to_lowercase()
}

fn allowed(origin: Option<&str>) -> bool {
    origin.is_some_and(|o| ALLOWED_ORIGINS.contains(&o))
}

fn cors_headers(origin: Option<&str>) -> Vec<(String, String)> {
    match origin {
        Some(o) if allowed(Some(o)) => vec![
            ("access-control-allow-origin".into(), o.into()),
            ("vary".into(), "Origin".into()),
        ],
        _ => Vec::new(),
    }
}

fn answer(status: u16, body: Value, origin: Option<&str>) -> Outgoing {
    let mut headers = vec![(
        "content-type".to_string(),
        "application/json; charset=utf-8".to_string(),
    )];
    headers.extend(cors_headers(origin));
    Outgoing {
        status,
        headers,
        body: Some(body.to_string()),
    }
}

fn refuse(status: u16, error: &str, origin: Option<&str>) -> Outgoing {
    answer(status, json!({ "ok": false, "error": error }), origin)
}

fn has_exactly(o: &Map<String, Value>, keys: &[&str]) -> bool {
    o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k))
}

fn is_version(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        let n = js::length(s);
        (1..=MAX_VERSION).contains(&n)
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
    })
}

fn is_platform(v: &Value) -> bool {
    v.as_str().is_some_and(|s| PLATFORMS.contains(&s))
}

/// The request, checked, or the words for why not.
pub fn parse_form(body: &Value) -> Result<Form, &'static str> {
    let Some(o) = body.as_object() else {
        return Err(said::BAD_FORM);
    };
    match o.get("kind").and_then(Value::as_str) {
        Some("device") => {
            if !has_exactly(o, &["kind", "device", "version", "platform"]) {
                return Err(said::BAD_FORM);
            }
            if !is_version(&o["version"]) || !is_platform(&o["platform"]) {
                return Err(said::BAD_FORM);
            }
            let Some(device) = o["device"].as_str() else {
                return Err(said::BAD_DEVICE);
            };
            let name = js::trim(device);
            if !(1..=MAX_DEVICE).contains(&js::length(name)) {
                return Err(said::BAD_DEVICE);
            }
            Ok(Form::Device {
                device: name.into(),
                version: o["version"].as_str().unwrap_or_default().into(),
                platform: o["platform"].as_str().unwrap_or_default().into(),
            })
        }
        Some("report") => {
            if !has_exactly(o, &["kind", "route", "worked", "version", "platform"]) {
                return Err(said::BAD_FORM);
            }
            let route = o["route"].as_str().filter(|r| ROUTES.contains(r));
            let worked = o["worked"].as_bool();
            match (route, worked) {
                (Some(route), Some(worked))
                    if is_version(&o["version"]) && is_platform(&o["platform"]) =>
                {
                    Ok(Form::Report {
                        route: route.into(),
                        worked,
                        version: o["version"].as_str().unwrap_or_default().into(),
                        platform: o["platform"].as_str().unwrap_or_default().into(),
                    })
                }
                _ => Err(said::BAD_FORM),
            }
        }
        _ => Err(said::BAD_FORM),
    }
}

/// The statement and values a form writes, on the day `now_ms` falls in.
pub fn statement(form: &Form, now_ms: f64) -> (&'static str, Vec<Bind>) {
    let at = js::day_of(now_ms);
    match form {
        Form::Device { device, .. } => (
            INSERT_DEVICE,
            vec![
                Bind::Text(normalise_name(device)),
                Bind::Text(device.clone()),
                Bind::Text(at.clone()),
                Bind::Text(at),
            ],
        ),
        Form::Report {
            route,
            worked,
            platform,
            ..
        } => (
            INSERT_REPORT,
            vec![
                Bind::Text(route.clone()),
                Bind::Text(platform.clone()),
                Bind::Number(if *worked { 1.0 } else { 0.0 }),
                Bind::Number(if *worked { 0.0 } else { 1.0 }),
                Bind::Text(at.clone()),
                Bind::Text(at),
            ],
        ),
    }
}

/// `handleRequest`.
pub async fn handle_request(
    request: &mut impl Incoming,
    limiter: &impl Limiter,
    db: &impl Store,
    now_ms: f64,
) -> Outgoing {
    let origin = request.header("origin");
    let origin = origin.as_deref();
    if request.path() != PATH {
        return refuse(404, said::NOT_FOUND, origin);
    }
    let method = request.method();
    if method == "OPTIONS" {
        let headers = if allowed(origin) {
            let mut h = cors_headers(origin);
            h.push(("access-control-allow-methods".into(), "POST".into()));
            h.push(("access-control-allow-headers".into(), "content-type".into()));
            h.push(("access-control-max-age".into(), "86400".into()));
            h
        } else {
            Vec::new()
        };
        return Outgoing {
            status: 204,
            headers,
            body: None,
        };
    }
    if method != "POST" {
        return refuse(404, said::NOT_FOUND, origin);
    }

    let declared = js::number(request.header("content-length").as_deref().unwrap_or("0"));
    if declared > MAX_BODY_BYTES as f64 {
        return refuse(413, said::TOO_LARGE, origin);
    }
    let Ok(raw) = request.text().await else {
        return refuse(400, said::UNREADABLE, origin);
    };
    if raw.len() > MAX_BODY_BYTES {
        return refuse(413, said::TOO_LARGE, origin);
    }
    let kind = request.header("content-type").unwrap_or_default();
    if !js::is_json_type(&kind) {
        return refuse(400, said::NOT_JSON, origin);
    }
    let Ok(body) = serde_json::from_str::<Value>(&raw) else {
        return refuse(400, said::NOT_JSON, origin);
    };
    let form = match parse_form(&body) {
        Ok(f) => f,
        Err(words) => return refuse(400, words, origin),
    };

    let key = request
        .header("cf-connecting-ip")
        .unwrap_or_else(|| "unknown".into());
    match limiter.limit(&key).await {
        Ok(true) => {}
        Ok(false) => return refuse(429, said::TOO_MANY, origin),
        Err(_) => return refuse(500, said::BROKEN, origin),
    }
    let (sql, values) = statement(&form, now_ms);
    match db.run(sql, &values).await {
        Ok(()) => answer(200, json!({ "ok": true }), origin),
        Err(_) => refuse(500, said::BROKEN, origin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_by_lowercasing_trimming_and_collapsing_spaces() {
        assert_eq!(normalise_name("  Kobo   Libra\t2 "), "kobo libra 2");
    }

    #[test]
    fn the_device_limit_counts_utf16_units_as_javascript_does() {
        let form = |d: &str| {
            parse_form(&json!({"kind":"device","device":d,"version":"0.7.4","platform":"mac"}))
        };
        assert!(form(&"😀".repeat(40)).is_ok(), "80 units");
        assert_eq!(form(&"😀".repeat(41)), Err(said::BAD_DEVICE));
    }
}

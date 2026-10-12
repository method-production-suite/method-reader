//! Stand-ins for Cloudflare: a request, the rate-limit binding, and D1 as
//! an in-memory model of the two statements (and, in tests/sqlite.rs, as
//! real SQLite with the real schema.sql). Nothing reaches any network.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::PathBuf;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use serde_json::Value;
use sp_device_requests::{
    Bind, INSERT_DEVICE, INSERT_REPORT, Incoming, LIMIT_PER_MINUTE, Limiter, Outgoing, Store,
    handle_request,
};

pub const URL_PATH: &str = "/request";
pub const IP: &str = "203.0.113.77";
pub const ORIGIN: &str = "tauri://localhost";

/// Every stand-in answers at once, so one poll finishes a request.
pub fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

pub fn service_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../services/device-requests")
}

#[derive(Clone, Debug)]
pub struct FakeReq {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Result<String, ()>,
    pub read: bool,
}

impl Incoming for FakeReq {
    fn method(&self) -> String {
        self.method.clone()
    }
    fn path(&self) -> String {
        self.path.clone()
    }
    fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }
    async fn text(&mut self) -> Result<String, ()> {
        self.read = true;
        self.body.clone()
    }
}

/// The TypeScript test's `post(body, o)`.
#[derive(Default)]
pub struct Opts {
    pub ip: Option<String>,
    /// Some(None): no Origin header at all.
    pub origin: Option<Option<String>>,
    pub content_type: Option<String>,
    pub raw: Option<String>,
    pub path: Option<String>,
    pub method: Option<String>,
}

pub fn post(body: &Value, o: Opts) -> FakeReq {
    let mut headers = vec![
        (
            "content-type".to_string(),
            o.content_type.unwrap_or_else(|| "application/json".into()),
        ),
        (
            "cf-connecting-ip".to_string(),
            o.ip.unwrap_or_else(|| IP.into()),
        ),
    ];
    match o.origin {
        Some(None) => {}
        Some(Some(origin)) => headers.push(("origin".into(), origin)),
        None => headers.push(("origin".into(), ORIGIN.into())),
    }
    FakeReq {
        method: o.method.unwrap_or_else(|| "POST".into()),
        path: o.path.unwrap_or_else(|| URL_PATH.into()),
        headers,
        body: Ok(o.raw.unwrap_or_else(|| body.to_string())),
        read: false,
    }
}

pub fn request(method: &str, path: &str, headers: &[(&str, &str)]) -> FakeReq {
    FakeReq {
        method: method.into(),
        path: path.into(),
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        body: Ok(String::new()),
        read: false,
    }
}

pub fn device(name: Value, extra: &[(&str, Value)]) -> Value {
    let mut v = serde_json::json!({"kind": "device", "device": name, "version": "0.7.4", "platform": "mac"});
    for (k, x) in extra {
        v[*k] = x.clone();
    }
    v
}

pub fn report(extra: &[(&str, Value)]) -> Value {
    let mut v = serde_json::json!({"kind": "report", "route": "kobo", "worked": true, "version": "0.7.4", "platform": "windows"});
    for (k, x) in extra {
        v[*k] = x.clone();
    }
    v
}

/// The binding: LIMIT_PER_MINUTE calls per key, then refused; `keys`
/// records what it was asked about.
#[derive(Default)]
pub struct FakeLimiter {
    pub counts: RefCell<HashMap<String, u32>>,
    pub keys: RefCell<Vec<String>>,
    pub broken: bool,
}

impl Limiter for FakeLimiter {
    async fn limit(&self, key: &str) -> Result<bool, String> {
        if self.broken {
            return Err("limiter down".into());
        }
        self.keys.borrow_mut().push(key.into());
        let mut c = self.counts.borrow_mut();
        let n = c.entry(key.into()).or_default();
        *n += 1;
        Ok(*n <= LIMIT_PER_MINUTE)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceRow {
    pub name_key: String,
    pub spelling: String,
    pub count: i64,
    pub first_seen: String,
    pub last_seen: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReportRow {
    pub route: String,
    pub platform: String,
    pub worked: i64,
    pub did_not_work: i64,
    pub first_seen: String,
    pub last_seen: String,
}

/// D1, modelled: exactly the two upserts, each as SQLite runs it.
#[derive(Default)]
pub struct MemoryStore {
    pub devices: RefCell<BTreeMap<String, DeviceRow>>,
    pub reports: RefCell<BTreeMap<(String, String), ReportRow>>,
    pub broken: bool,
    pub statements: RefCell<Vec<(String, Vec<Bind>)>>,
}

fn text(b: &Bind) -> String {
    match b {
        Bind::Text(s) => s.clone(),
        Bind::Number(n) => n.to_string(),
    }
}

fn int(b: &Bind) -> i64 {
    match b {
        Bind::Number(n) => *n as i64,
        Bind::Text(s) => s.parse().unwrap_or(0),
    }
}

impl Store for MemoryStore {
    async fn run(&self, sql: &str, v: &[Bind]) -> Result<(), String> {
        if self.broken {
            return Err("D1 is down".into());
        }
        self.statements.borrow_mut().push((sql.into(), v.to_vec()));
        if sql == INSERT_DEVICE {
            let mut d = self.devices.borrow_mut();
            let key = text(&v[0]);
            match d.get_mut(&key) {
                Some(row) => {
                    row.count += 1;
                    row.spelling = text(&v[1]);
                    row.last_seen = text(&v[3]);
                }
                None => {
                    d.insert(
                        key.clone(),
                        DeviceRow {
                            name_key: key,
                            spelling: text(&v[1]),
                            count: 1,
                            first_seen: text(&v[2]),
                            last_seen: text(&v[3]),
                        },
                    );
                }
            }
            Ok(())
        } else if sql == INSERT_REPORT {
            let mut r = self.reports.borrow_mut();
            let key = (text(&v[0]), text(&v[1]));
            match r.get_mut(&key) {
                Some(row) => {
                    row.worked += int(&v[2]);
                    row.did_not_work += int(&v[3]);
                    row.last_seen = text(&v[5]);
                }
                None => {
                    r.insert(
                        key.clone(),
                        ReportRow {
                            route: key.0,
                            platform: key.1,
                            worked: int(&v[2]),
                            did_not_work: int(&v[3]),
                            first_seen: text(&v[4]),
                            last_seen: text(&v[5]),
                        },
                    );
                }
            }
            Ok(())
        } else {
            Err(format!("unexpected statement {sql}"))
        }
    }
}

/// One request through the handler, now or at a given time.
pub fn send(
    req: &mut FakeReq,
    limiter: &impl Limiter,
    store: &impl Store,
    now_ms: Option<f64>,
) -> (Outgoing, Option<Value>) {
    let now = now_ms.unwrap_or(1_790_935_200_000.0);
    let out = block_on(handle_request(req, limiter, store, now));
    let body = out
        .body
        .as_deref()
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_str(b).unwrap());
    (out, body)
}

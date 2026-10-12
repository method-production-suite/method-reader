//! workers-rs to the handler: the request, D1 (`DB`), the rate limiter
//! (`RATE_LIMITER`) and the clock, and the answer back out. Built only for
//! wasm32 (`worker-build`); everything it calls is tested natively.

use worker::wasm_bindgen::JsValue;
use worker::{Context, D1Database, Env, Headers, Request, Response, event};

use crate::{Bind, Incoming, Limiter, Outgoing, Store, handle_request};

struct Req(Request);

impl Incoming for Req {
    fn method(&self) -> String {
        self.0.method().to_string()
    }
    fn path(&self) -> String {
        self.0.path()
    }
    fn header(&self, name: &str) -> Option<String> {
        self.0.headers().get(name).ok().flatten()
    }
    async fn text(&mut self) -> Result<String, ()> {
        self.0.text().await.map_err(|_| ())
    }
}

struct Rate(worker::RateLimiter);

impl Limiter for Rate {
    async fn limit(&self, key: &str) -> Result<bool, String> {
        self.0
            .limit(key.to_string())
            .await
            .map(|o| o.success)
            .map_err(|e| e.to_string())
    }
}

struct D1(D1Database);

impl Store for D1 {
    async fn run(&self, sql: &str, values: &[Bind]) -> Result<(), String> {
        let values: Vec<JsValue> = values
            .iter()
            .map(|b| match b {
                Bind::Text(s) => JsValue::from_str(s),
                Bind::Number(n) => JsValue::from_f64(*n),
            })
            .collect();
        self.0
            .prepare(sql)
            .bind(&values)
            .map_err(|e| e.to_string())?
            .run()
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// A binding that is missing fails every write as a 500, as an undefined
/// `env.DB` did in the TypeScript.
struct Missing;

impl Limiter for Missing {
    async fn limit(&self, _key: &str) -> Result<bool, String> {
        Err("no RATE_LIMITER binding".into())
    }
}

impl Store for Missing {
    async fn run(&self, _sql: &str, _values: &[Bind]) -> Result<(), String> {
        Err("no DB binding".into())
    }
}

fn respond(out: Outgoing) -> worker::Result<Response> {
    let headers = Headers::new();
    for (k, v) in &out.headers {
        headers.set(k, v)?;
    }
    let response = match out.body {
        Some(body) => Response::ok(body)?,
        None => Response::empty()?,
    };
    Ok(response.with_status(out.status).with_headers(headers))
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> worker::Result<Response> {
    let now = worker::Date::now().as_millis() as f64;
    let mut req = Req(req);
    let out = match (env.rate_limiter("RATE_LIMITER"), env.d1("DB")) {
        (Ok(limiter), Ok(db)) => handle_request(&mut req, &Rate(limiter), &D1(db), now).await,
        (Ok(limiter), Err(_)) => handle_request(&mut req, &Rate(limiter), &Missing, now).await,
        (Err(_), Ok(db)) => handle_request(&mut req, &Missing, &D1(db), now).await,
        (Err(_), Err(_)) => handle_request(&mut req, &Missing, &Missing, now).await,
    };
    respond(out)
}

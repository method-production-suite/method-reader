//! tests/device-requests-worker.test.ts, ported: the handler called
//! directly against stand-ins for D1 and the rate limiter. The SQL itself
//! runs against real SQLite in tests/sqlite.rs.

mod common;

use common::*;
use serde_json::{Value, json};
use sp_device_requests::{ALLOWED_ORIGINS, LIMIT_PER_MINUTE, MAX_BODY_BYTES, said};

const OCT2_10: f64 = 1_790_935_200_000.0;
const OCT3_11: f64 = 1_791_025_200_000.0;

fn opts() -> Opts {
    Opts::default()
}

// ---------------------------------------------------------- a device request

#[test]
fn a_device_request_is_counted_and_answers_ok() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let (out, body) = send(
        &mut post(&device(json!("PocketBook Era"), &[]), opts()),
        &l,
        &s,
        None,
    );
    assert_eq!(out.status, 200);
    assert_eq!(body, Some(json!({"ok": true})));
    assert!(
        out.header("content-type")
            .unwrap()
            .starts_with("application/json")
    );
    let rows = s.devices.borrow();
    let row = rows.values().next().unwrap();
    assert_eq!(row.name_key, "pocketbook era");
    assert_eq!(row.spelling, "PocketBook Era");
    assert_eq!(row.count, 1);
}

#[test]
fn the_same_device_asked_again_adds_to_its_count_keeping_the_latest_spelling_and_both_days() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    send(
        &mut post(&device(json!("PocketBook Era"), &[]), opts()),
        &l,
        &s,
        Some(OCT2_10),
    );
    send(
        &mut post(
            &device(
                json!("  pocketbook    ERA "),
                &[("platform", json!("linux"))],
            ),
            Opts {
                ip: Some("198.51.100.1".into()),
                ..opts()
            },
        ),
        &l,
        &s,
        Some(OCT3_11),
    );
    {
        let rows = s.devices.borrow();
        assert_eq!(rows.len(), 1);
        let row = rows.values().next().unwrap();
        assert_eq!(row.count, 2);
        assert_eq!(row.spelling, "pocketbook    ERA");
        // The day only: a time to the millisecond could pick one request out.
        assert_eq!(row.first_seen, "2026-10-02");
        assert_eq!(row.last_seen, "2026-10-03");
    }
    send(
        &mut post(&device(json!("Boox Note"), &[]), opts()),
        &l,
        &s,
        Some(OCT3_11),
    );
    assert_eq!(s.devices.borrow().len(), 2);
}

#[test]
fn takes_80_characters_after_trimming_and_refuses_81_or_none() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let name = format!("  {}  ", "x".repeat(80));
    assert_eq!(
        send(&mut post(&device(json!(name), &[]), opts()), &l, &s, None)
            .0
            .status,
        200
    );
    for bad in ["x".repeat(81), String::new(), "    ".into()] {
        let (out, body) = send(
            &mut post(
                &device(json!(bad), &[]),
                Opts {
                    ip: Some("198.51.100.9".into()),
                    ..opts()
                },
            ),
            &l,
            &s,
            None,
        );
        assert_eq!(out.status, 400);
        let body = body.unwrap();
        assert_eq!(body["ok"], false);
        assert!(body["error"].is_string());
    }
}

// ------------------------------------------------------------------ a report

#[test]
fn a_report_counts_worked_and_did_not_work_per_route_and_platform() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    for r in [
        report(&[]),
        report(&[("worked", json!(false))]),
        report(&[("worked", json!(true))]),
        report(&[
            ("route", json!("kindle-linux")),
            ("platform", json!("linux")),
            ("worked", json!(false)),
        ]),
    ] {
        send(&mut post(&r, opts()), &l, &s, None);
    }
    let got: Vec<(String, String, i64, i64)> = s
        .reports
        .borrow()
        .values()
        .map(|r| {
            (
                r.route.clone(),
                r.platform.clone(),
                r.worked,
                r.did_not_work,
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("kindle-linux".into(), "linux".into(), 0, 1),
            ("kobo".into(), "windows".into(), 2, 1),
        ]
    );
}

#[test]
fn a_report_takes_every_route_the_window_asks_about() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    for route in [
        "kobo",
        "tolino",
        "remarkable",
        "kindle-windows",
        "kindle-linux",
    ] {
        let o = Opts {
            ip: Some(format!("198.51.100.{}", route.len())),
            ..opts()
        };
        assert_eq!(
            send(
                &mut post(&report(&[("route", json!(route))]), o),
                &l,
                &s,
                None
            )
            .0
            .status,
            200
        );
    }
}

// ------------------------------------------------------------------- refused

#[test]
fn refused_forms_are_400_and_store_nothing() {
    let bad: Vec<(&str, Value)> = vec![
        (
            "an unknown kind",
            json!({"kind": "vote", "device": "x", "version": "0.7.4", "platform": "mac"}),
        ),
        (
            "an extra field",
            device(json!("Boox"), &[("email", json!("a@b.c"))]),
        ),
        (
            "a missing field",
            json!({"kind": "device", "device": "Boox", "platform": "mac"}),
        ),
        ("a number for a name", device(json!(42), &[])),
        (
            "an unknown platform",
            device(json!("Boox"), &[("platform", json!("ios"))]),
        ),
        (
            "a version over 20 characters",
            device(json!("Boox"), &[("version", json!("1".repeat(21)))]),
        ),
        (
            "an empty version",
            device(json!("Boox"), &[("version", json!(""))]),
        ),
        (
            "an unknown route",
            report(&[("route", json!("kindle-mac"))]),
        ),
        ("worked as a string", report(&[("worked", json!("yes"))])),
        (
            "a report carrying a device",
            report(&[("device", json!("Kobo"))]),
        ),
        ("an array", json!([device(json!("Boox"), &[])])),
        ("null", Value::Null),
    ];
    for (what, body) in bad {
        let (l, s) = (FakeLimiter::default(), MemoryStore::default());
        let (out, answer) = send(&mut post(&body, opts()), &l, &s, None);
        assert_eq!(out.status, 400, "{what}");
        assert_eq!(answer.unwrap()["ok"], false, "{what}");
        assert!(s.devices.borrow().is_empty(), "{what}");
        assert!(s.reports.borrow().is_empty(), "{what}");
    }
}

#[test]
fn a_body_that_is_not_json_or_not_marked_json_is_400() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let raw = Opts {
        raw: Some("{not json".into()),
        ..opts()
    };
    assert_eq!(
        send(&mut post(&Value::Null, raw), &l, &s, None).0.status,
        400
    );
    let plain = Opts {
        content_type: Some("text/plain".into()),
        ..opts()
    };
    let (out, body) = send(&mut post(&device(json!("Boox"), &[]), plain), &l, &s, None);
    assert_eq!(out.status, 400);
    assert_eq!(body.unwrap()["error"], said::NOT_JSON);
}

#[test]
fn a_body_over_1_kb_is_413() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let big = device(json!("Boox"), &[("pad", json!("x".repeat(MAX_BODY_BYTES)))]).to_string();
    let (out, body) = send(
        &mut post(
            &Value::Null,
            Opts {
                raw: Some(big),
                ..opts()
            },
        ),
        &l,
        &s,
        None,
    );
    assert_eq!(out.status, 413);
    assert_eq!(body.unwrap()["ok"], false);
}

#[test]
fn a_declared_length_over_1_kb_is_413_before_the_body_is_read() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let mut req = post(&device(json!("Boox"), &[]), opts());
    req.headers.push(("content-length".into(), "2048".into()));
    req.body = Err(());
    let (out, _) = send(&mut req, &l, &s, None);
    assert_eq!(out.status, 413);
    assert!(!req.read);
}

#[test]
fn any_other_method_or_path_is_404() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let put = Opts {
        method: Some("PUT".into()),
        ..opts()
    };
    assert_eq!(
        send(&mut post(&device(json!("Boox"), &[]), put), &l, &s, None)
            .0
            .status,
        404
    );
    assert_eq!(
        send(&mut request("GET", URL_PATH, &[]), &l, &s, None)
            .0
            .status,
        404
    );
    for path in ["/", "/request/more"] {
        let o = Opts {
            path: Some(path.into()),
            ..opts()
        };
        assert_eq!(
            send(&mut post(&device(json!("Boox"), &[]), o), &l, &s, None)
                .0
                .status,
            404
        );
    }
    assert!(s.devices.borrow().is_empty());
}

#[test]
fn every_error_is_said_in_plain_words_without_an_em_dash() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let said: Vec<String> = [
        post(&device(json!(""), &[]), opts()),
        post(
            &Value::Null,
            Opts {
                raw: Some("x".repeat(2000)),
                ..opts()
            },
        ),
        post(
            &device(json!("Boox"), &[]),
            Opts {
                method: Some("PUT".into()),
                ..opts()
            },
        ),
    ]
    .into_iter()
    .map(|mut r| {
        send(&mut r, &l, &s, None).1.unwrap()["error"]
            .as_str()
            .unwrap()
            .to_string()
    })
    .collect();
    for line in said {
        assert!(line.chars().next().unwrap().is_ascii_uppercase(), "{line}");
        assert!(line.ends_with('.'), "{line}");
        assert!(!line.contains('\u{2014}'), "{line}");
    }
}

// ------------------------------------------------------------- the rate limit

#[test]
fn five_accepted_a_minute_per_sender_then_429_while_another_sender_still_gets_through() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    for i in 0..LIMIT_PER_MINUTE {
        let r = &mut post(&device(json!(format!("Boox {i}")), &[]), opts());
        assert_eq!(send(r, &l, &s, None).0.status, 200);
    }
    let (over, body) = send(
        &mut post(&device(json!("Boox again"), &[]), opts()),
        &l,
        &s,
        None,
    );
    assert_eq!(over.status, 429);
    assert_eq!(body.unwrap()["ok"], false);
    assert_eq!(s.devices.borrow().len(), LIMIT_PER_MINUTE as usize);
    let other = Opts {
        ip: Some("198.51.100.2".into()),
        ..opts()
    };
    assert_eq!(
        send(
            &mut post(&device(json!("Boox other"), &[]), other),
            &l,
            &s,
            None
        )
        .0
        .status,
        200
    );
}

#[test]
fn a_refused_request_does_not_use_up_a_place() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    for _ in 0..10 {
        send(&mut post(&device(json!(""), &[]), opts()), &l, &s, None);
    }
    assert!(l.keys.borrow().is_empty());
    assert_eq!(
        send(&mut post(&device(json!("Boox"), &[]), opts()), &l, &s, None)
            .0
            .status,
        200
    );
}

#[test]
fn the_limiter_is_keyed_on_the_connecting_address_and_nothing_of_it_reaches_d1() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    send(&mut post(&device(json!("Boox"), &[]), opts()), &l, &s, None);
    send(&mut post(&report(&[]), opts()), &l, &s, None);
    assert_eq!(*l.keys.borrow(), [IP, IP]);
    let everything = format!("{:?}", s.statements.borrow());
    assert!(!everything.contains(IP));
}

#[test]
fn no_connecting_address_is_keyed_as_unknown() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let mut r = post(&device(json!("Boox"), &[]), opts());
    r.headers.retain(|(k, _)| k != "cf-connecting-ip");
    send(&mut r, &l, &s, None);
    assert_eq!(*l.keys.borrow(), ["unknown"]);
}

#[test]
fn a_limiter_that_fails_is_a_500_in_plain_words_and_nothing_is_counted() {
    let l = FakeLimiter {
        broken: true,
        ..Default::default()
    };
    let s = MemoryStore::default();
    let (out, body) = send(&mut post(&device(json!("Boox"), &[]), opts()), &l, &s, None);
    assert_eq!(out.status, 500);
    assert!(!body.unwrap()["error"].as_str().unwrap().contains("limiter"));
    assert!(s.devices.borrow().is_empty());
}

#[test]
fn wrangler_binds_rate_limiter_at_the_limit_the_worker_says_per_60_seconds() {
    {
        let file = service_dir().join("wrangler.toml");
        let toml = std::fs::read_to_string(&file).unwrap();
        let block = toml
            .split("[[ratelimits]]")
            .nth(1)
            .expect("a [[ratelimits]] block");
        let block = block.split("\n[[").next().unwrap();
        let lines: Vec<&str> = block.lines().map(str::trim).collect();
        assert!(
            lines.contains(&"name = \"RATE_LIMITER\""),
            "{}",
            file.display()
        );
        let positive_integer = |l: &&str| {
            l.strip_prefix("namespace_id = \"")
                .and_then(|r| r.strip_suffix('"'))
                .is_some_and(|n| {
                    !n.is_empty() && !n.starts_with('0') && n.chars().all(|c| c.is_ascii_digit())
                })
        };
        assert!(lines.iter().any(positive_integer), "{}", file.display());
        assert!(
            lines.contains(&format!("limit = {LIMIT_PER_MINUTE}").as_str()),
            "{}",
            file.display()
        );
        assert!(lines.contains(&"period = 60"), "{}", file.display());
    }
}

/// What services/device-requests/wrangler.toml, the TypeScript Worker's,
/// bound (as of its deletion at the cutover): deploying this crate must
/// replace that Worker at the same address, on the same database.
const LIVE_BINDINGS: &[(&str, &str)] = &[
    ("name = ", "name = \"screepub-device-requests\""),
    ("binding = ", "binding = \"DB\""),
    (
        "database_name = ",
        "database_name = \"screepub-device-requests\"",
    ),
    (
        "database_id = ",
        "database_id = \"343f8b66-876c-40e7-8cb4-9dcc6ba544b0\"",
    ),
    ("namespace_id = ", "namespace_id = \"2610\""),
];

#[test]
fn the_rust_worker_binds_the_same_database_as_the_live_one() {
    let rs = std::fs::read_to_string(service_dir().join("wrangler.toml")).unwrap();
    let line = |t: &str, key: &str| {
        t.lines()
            .find(|l| l.trim_start().starts_with(key))
            .map(|l| l.trim().to_string())
    };
    for (key, live) in LIVE_BINDINGS {
        assert_eq!(line(&rs, key).as_deref(), Some(*live), "{key}");
    }
}

// ----------------------------------------------------------------------- CORS

#[test]
fn the_window_s_origins_may_post_json_and_a_preflight_is_answered() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    for origin in ALLOWED_ORIGINS {
        let (pre, body) = send(
            &mut request(
                "OPTIONS",
                URL_PATH,
                &[
                    ("origin", origin),
                    ("access-control-request-method", "POST"),
                    ("access-control-request-headers", "content-type"),
                ],
            ),
            &l,
            &s,
            None,
        );
        assert_eq!(pre.status, 204);
        assert_eq!(body, None);
        assert_eq!(pre.header("access-control-allow-origin"), Some(origin));
        assert_eq!(pre.header("access-control-allow-methods"), Some("POST"));
        assert_eq!(
            pre.header("access-control-allow-headers"),
            Some("content-type")
        );
        let o = Opts {
            origin: Some(Some(origin.into())),
            ip: Some(format!("{origin}-ip")),
            ..opts()
        };
        let (r, _) = send(&mut post(&device(json!("Boox"), &[]), o), &l, &s, None);
        assert_eq!(r.header("access-control-allow-origin"), Some(origin));
        assert_eq!(r.header("vary"), Some("Origin"));
    }
    assert_eq!(
        ALLOWED_ORIGINS,
        [
            "tauri://localhost",
            "http://tauri.localhost",
            "https://tauri.localhost"
        ]
    );
}

#[test]
fn any_other_origin_is_not_allowed_and_an_error_answer_carries_the_header_too() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let (pre, _) = send(
        &mut request("OPTIONS", URL_PATH, &[("origin", "https://evil.example")]),
        &l,
        &s,
        None,
    );
    assert_eq!(pre.header("access-control-allow-origin"), None);
    assert!(pre.headers.is_empty());
    let (refused, _) = send(&mut post(&device(json!(""), &[]), opts()), &l, &s, None);
    assert_eq!(refused.header("access-control-allow-origin"), Some(ORIGIN));
}

#[test]
fn a_preflight_anywhere_but_request_is_a_404() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let (r, _) = send(
        &mut request("OPTIONS", "/other", &[("origin", ORIGIN)]),
        &l,
        &s,
        None,
    );
    assert_eq!(r.status, 404);
}

#[test]
fn a_body_that_cannot_be_read_is_a_400_the_window_can_read_with_its_cors_header() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let mut req = request(
        "POST",
        URL_PATH,
        &[
            ("content-type", "application/json"),
            ("origin", ORIGIN),
            ("cf-connecting-ip", IP),
        ],
    );
    req.body = Err(());
    let (r, body) = send(&mut req, &l, &s, None);
    assert_eq!(r.status, 400);
    assert_eq!(body.unwrap()["ok"], false);
    assert_eq!(r.header("access-control-allow-origin"), Some(ORIGIN));
}

#[test]
fn a_database_that_fails_says_so_in_plain_words_as_a_500() {
    let l = FakeLimiter::default();
    let s = MemoryStore {
        broken: true,
        ..Default::default()
    };
    let (r, body) = send(&mut post(&device(json!("Boox"), &[]), opts()), &l, &s, None);
    assert_eq!(r.status, 500);
    let body = body.unwrap();
    assert_eq!(body["ok"], false);
    assert!(!body["error"].as_str().unwrap().contains("D1"));
}

#[test]
fn answers_are_the_typescript_s_json_byte_for_byte() {
    let (l, s) = (FakeLimiter::default(), MemoryStore::default());
    let (ok, _) = send(&mut post(&device(json!("Boox"), &[]), opts()), &l, &s, None);
    assert_eq!(ok.body.as_deref(), Some(r#"{"ok":true}"#));
    let (no, _) = send(&mut post(&device(json!(""), &[]), opts()), &l, &s, None);
    assert_eq!(
        no.body.as_deref(),
        Some(r#"{"ok":false,"error":"The device name must be 1 to 80 characters."}"#)
    );
    assert_eq!(
        ok.header("content-type"),
        Some("application/json; charset=utf-8")
    );
}

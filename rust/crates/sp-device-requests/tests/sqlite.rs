//! The two statements and the migration against REAL SQLite (D1 is
//! SQLite), with the live service's own services/device-requests/schema.sql
//! and migrations, through the sqlite3 command-line tool. Unix only: macOS
//! ships sqlite3 and rust.yml installs it on Linux; a missing tool fails
//! here rather than skipping, so green means the SQL ran.
#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::*;
use serde_json::{Value, json};
use sp_device_requests::{Bind, Store};

struct Sqlite {
    db: PathBuf,
    _dir: tempfile::TempDir,
}

fn sqlite3(db: &Path, sql: &str, json_out: bool) -> String {
    use std::io::Write;
    let mut c = Command::new("sqlite3");
    if json_out {
        c.arg("-json");
    }
    // On stdin: a script opening with a `--` comment would read as an option.
    let mut child = c
        .arg(db)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the sqlite3 tool (macOS ships it; on Linux: apt-get install sqlite3)");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(sql.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "sqlite3: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

impl Sqlite {
    fn with(sql: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("d1.sqlite3");
        sqlite3(&db, sql, false);
        Self { db, _dir: dir }
    }
    fn schema() -> Self {
        Self::with(&std::fs::read_to_string(service_dir().join("schema.sql")).unwrap())
    }
    fn query(&self, sql: &str) -> Vec<Value> {
        let text = sqlite3(&self.db, sql, true);
        if text.trim().is_empty() {
            return Vec::new();
        }
        serde_json::from_str::<Vec<Value>>(&text).unwrap()
    }
}

fn literal(b: &Bind) -> String {
    match b {
        Bind::Text(s) => format!("'{}'", s.replace('\'', "''")),
        Bind::Number(n) if n.fract() == 0.0 => format!("{}", *n as i64),
        Bind::Number(n) => n.to_string(),
    }
}

impl Store for Sqlite {
    async fn run(&self, sql: &str, values: &[Bind]) -> Result<(), String> {
        let mut out = String::new();
        let mut v = values.iter();
        for c in sql.chars() {
            if c == '?' {
                out.push_str(&literal(v.next().ok_or("too few values")?));
            } else {
                out.push(c);
            }
        }
        if v.next().is_some() {
            return Err("too many values".into());
        }
        sqlite3(&self.db, &out, false);
        Ok(())
    }
}

const OCT2_10: f64 = 1_790_935_200_000.0;
const OCT3_11: f64 = 1_791_025_200_000.0;

#[test]
fn a_device_asked_twice_is_one_row_with_both_days_in_real_sqlite() {
    let (l, db) = (FakeLimiter::default(), Sqlite::schema());
    send(
        &mut post(&device(json!("PocketBook Era"), &[]), Opts::default()),
        &l,
        &db,
        Some(OCT2_10),
    );
    let second = Opts {
        ip: Some("198.51.100.1".into()),
        ..Default::default()
    };
    send(
        &mut post(
            &device(
                json!("  pocketbook    ERA "),
                &[("platform", json!("linux"))],
            ),
            second,
        ),
        &l,
        &db,
        Some(OCT3_11),
    );
    send(
        &mut post(&device(json!("O'Reilly Reader"), &[]), Opts::default()),
        &l,
        &db,
        Some(OCT3_11),
    );
    assert_eq!(
        db.query("SELECT * FROM device_requests ORDER BY name_key"),
        [
            json!({"name_key": "o'reilly reader", "spelling": "O'Reilly Reader", "count": 1, "first_seen": "2026-10-03", "last_seen": "2026-10-03"}),
            json!({"name_key": "pocketbook era", "spelling": "pocketbook    ERA", "count": 2, "first_seen": "2026-10-02", "last_seen": "2026-10-03"}),
        ]
    );
}

#[test]
fn reports_add_up_per_route_and_platform_in_real_sqlite() {
    let (l, db) = (FakeLimiter::default(), Sqlite::schema());
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
        send(&mut post(&r, Opts::default()), &l, &db, None);
    }
    assert_eq!(
        db.query("SELECT route, platform, worked, did_not_work FROM route_reports ORDER BY route"),
        [
            json!({"route": "kindle-linux", "platform": "linux", "worked": 0, "did_not_work": 1}),
            json!({"route": "kobo", "platform": "windows", "worked": 2, "did_not_work": 1}),
        ]
    );
}

#[test]
fn the_tables_hold_nothing_beyond_their_counts_and_no_address() {
    let (l, db) = (FakeLimiter::default(), Sqlite::schema());
    send(
        &mut post(&device(json!("Boox"), &[]), Opts::default()),
        &l,
        &db,
        None,
    );
    send(&mut post(&report(&[]), Opts::default()), &l, &db, None);
    let tables: Vec<Value> =
        db.query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name");
    assert_eq!(
        tables,
        [
            json!({"name": "device_requests"}),
            json!({"name": "route_reports"})
        ]
    );
    let columns = |t: &str| -> Vec<String> {
        db.query(&format!("PRAGMA table_info({t})"))
            .iter()
            .map(|c| c["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        columns("device_requests"),
        ["name_key", "spelling", "count", "first_seen", "last_seen"]
    );
    assert_eq!(
        columns("route_reports"),
        [
            "route",
            "platform",
            "worked",
            "did_not_work",
            "first_seen",
            "last_seen"
        ]
    );
    let everything = format!(
        "{:?}{:?}",
        db.query("SELECT * FROM device_requests"),
        db.query("SELECT * FROM route_reports")
    );
    assert!(!everything.contains(IP));
}

#[test]
fn the_migration_drops_the_senders_and_rounds_dates_twice_safely() {
    let old = Sqlite::with(&format!(
        "CREATE TABLE device_requests (name_key TEXT PRIMARY KEY, spelling TEXT NOT NULL, count INTEGER NOT NULL,
           first_seen TEXT NOT NULL, last_seen TEXT NOT NULL);
         CREATE TABLE route_reports (route TEXT NOT NULL, platform TEXT NOT NULL, worked INTEGER NOT NULL DEFAULT 0,
           did_not_work INTEGER NOT NULL DEFAULT 0, first_seen TEXT NOT NULL, last_seen TEXT NOT NULL, PRIMARY KEY (route, platform));
         CREATE TABLE rate_limits (sender TEXT NOT NULL, hour INTEGER NOT NULL, count INTEGER NOT NULL, PRIMARY KEY (sender, hour));
         CREATE INDEX rate_limits_hour ON rate_limits (hour);
         INSERT INTO device_requests VALUES ('boox', 'Boox', 3, '2026-10-02T10:00:00.000Z', '2026-10-04T23:59:59.999Z');
         INSERT INTO route_reports VALUES ('kobo', 'mac', 1, 0, '2026-10-03T08:00:00.000Z', '2026-10-03T08:00:00.000Z');
         INSERT INTO rate_limits VALUES ('{}', 489000, 1);",
        "a".repeat(64)
    ));
    let migration =
        std::fs::read_to_string(service_dir().join("migrations/0001-no-senders-day-dates.sql"))
            .unwrap();
    sqlite3(&old.db, &migration, false);
    assert!(
        old.query("SELECT name FROM sqlite_master WHERE name LIKE 'rate_limits%'")
            .is_empty()
    );
    assert_eq!(
        old.query("SELECT * FROM device_requests"),
        [
            json!({"name_key": "boox", "spelling": "Boox", "count": 3, "first_seen": "2026-10-02", "last_seen": "2026-10-04"})
        ]
    );
    assert_eq!(
        old.query("SELECT * FROM route_reports"),
        [
            json!({"route": "kobo", "platform": "mac", "worked": 1, "did_not_work": 0, "first_seen": "2026-10-03", "last_seen": "2026-10-03"})
        ]
    );
    sqlite3(&old.db, &migration, false);
    let fresh = Sqlite::schema();
    sqlite3(&fresh.db, &migration, false);
}

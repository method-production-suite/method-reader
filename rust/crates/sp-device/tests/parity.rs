//! The TypeScript CLI against this crate, answer for answer: `devices`,
//! `eject` and `reveal` under --json, on fake volume roots
//! (SCREEPUB_VOLUME_ROOTS) and a stand-in tablet
//! (SCREEPUB_REMARKABLE_ENDPOINT), the overrides the TypeScript honored.
//! The Rust side's envelope is built here the way sp-cli builds it:
//! `{"ok":true,...answer}` or `{"ok":false,"error":{code,message}}`.
//!
//! The TypeScript's answers are frozen data (tests/data/parity.json): each
//! was asked of `src/cli.ts` once under bun, on the owner's Mac, with the
//! scratch folder spelled `$BASE` and the stand-in tablets' addresses
//! `$SILENT` and `$ANSWERING`, so each replays against this run's folder
//! and stand-ins. They are a Mac's answers, so this runs on macOS only.
//!
//! eject and reveal were asked ONLY what they refuse: a valid volume would
//! have made the TypeScript run diskutil, and a valid file would have opened
//! Finder. Every eject here names a volume the fake roots do not list, or a
//! relative one, and every reveal a relative path, a missing file or a
//! folder.

mod common;
use common::{Stub, s, scratch};
use serde::Serialize;
use serde_json::{Map, Value};
use sp_device::eject::{EjectDeps, eject_command};
use sp_device::list::ListDevicesOptions;
use sp_device::process::{Opener, OpenerOutput, RunOutput, Runner};
use sp_device::volumes::mounted_devices;
use sp_device::{CliError, Platform, commands::devices_command, reveal::reveal_command};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

/// Whether the frozen answers apply here: they are a Mac's.
fn frozen_here() -> bool {
    if cfg!(target_os = "macos") {
        return true;
    }
    eprintln!("parity: the TypeScript answers were frozen on macOS; skipped");
    false
}

fn recording() -> bool {
    std::env::var_os("SCREEPUB_RECORD_FROZEN").is_some()
}

const FROZEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/parity.json");

fn frozen_store() -> &'static BTreeMap<String, Value> {
    static STORE: OnceLock<BTreeMap<String, Value>> = OnceLock::new();
    STORE.get_or_init(|| {
        serde_json::from_str(include_str!("data/parity.json")).expect("tests/data/parity.json")
    })
}

/// `text` with each real value spelled as its placeholder.
fn encode(text: &str, subs: &[(&str, &str)]) -> String {
    subs.iter()
        .fold(text.to_owned(), |t, (real, mark)| t.replace(real, mark))
}

/// `text` with each placeholder read as this run's value.
fn decode(text: &str, subs: &[(&str, &str)]) -> String {
    subs.iter()
        .fold(text.to_owned(), |t, (real, mark)| t.replace(mark, real))
}

/// The TypeScript's answer for `key`: frozen, or, while recording, asked of
/// `bun src/cli.ts` and frozen. `subs` pairs this run's values with the
/// placeholders the frozen answers use.
fn ts(
    key: &str,
    args: &[&str],
    roots: &str,
    endpoint: &str,
    config: &Path,
    subs: &[(&str, &str)],
) -> String {
    let asked: Vec<String> = args.iter().map(|a| encode(a, subs)).collect();
    if recording() {
        let text = ts_live(args, roots, endpoint, config);
        static LOCK: Mutex<()> = Mutex::new(());
        let _held = LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut all: BTreeMap<String, Value> = std::fs::read_to_string(FROZEN)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        all.insert(
            key.to_owned(),
            serde_json::json!({ "argv": asked, "answer": encode(&text, subs) }),
        );
        let mut out = serde_json::to_string_pretty(&all).unwrap();
        out.push('\n');
        std::fs::write(FROZEN, out).unwrap();
        return text;
    }
    let v = frozen_store()
        .get(key)
        .unwrap_or_else(|| panic!("no frozen TypeScript answer for {key:?}"));
    let want: Vec<String> = serde_json::from_value(v["argv"].clone()).unwrap();
    assert_eq!(asked, want, "{key}: the case changed since it was frozen");
    decode(v["answer"].as_str().unwrap(), subs)
}

/// One run of `bun src/cli.ts <args>` with the overrides, its stdout's one
/// JSON object as text.
fn ts_live(args: &[&str], roots: &str, endpoint: &str, config: &Path) -> String {
    let out = Command::new("bun")
        .arg(repo_root().join("src").join("cli.ts"))
        .args(args)
        .current_dir(repo_root())
        .env("SCREEPUB_VOLUME_ROOTS", roots)
        .env("SCREEPUB_REMARKABLE_ENDPOINT", endpoint)
        // Never the real settings file or Calibre folder.
        .env("SCREEPUB_CONFIG_DIR", config)
        .env("CALIBRE_CONFIG_DIRECTORY", config.join("calibre"))
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap().trim().to_string();
    // An empty answer on both sides must not pass as agreement.
    assert!(
        text.starts_with(r#"{"ok":"#),
        "the TypeScript answered {text:?} to {args:?}"
    );
    text
}

fn ok(answer: &impl Serialize) -> String {
    let mut m = Map::new();
    m.insert("ok".into(), Value::Bool(true));
    if let Value::Object(fields) = serde_json::to_value(answer).unwrap() {
        m.extend(fields);
    }
    Value::Object(m).to_string()
}

fn failed(err: &CliError) -> String {
    let mut m = Map::new();
    m.insert("ok".into(), Value::Bool(false));
    m.insert("error".into(), serde_json::to_value(err).unwrap());
    Value::Object(m).to_string()
}

/// A runner and an opener that must never be reached: these cases refuse.
struct Never;
impl Runner for Never {
    fn run(&self, argv: &[String], _: Duration) -> Result<RunOutput, String> {
        panic!("parity must never run an eject tool: {argv:?}")
    }
}
impl Opener for Never {
    fn open(&self, argv: &[String]) -> Result<OpenerOutput, String> {
        panic!("parity must never open a file manager: {argv:?}")
    }
}

struct Fixture {
    _scratch: tempfile::TempDir,
    roots: Vec<String>,
    config: PathBuf,
    base: PathBuf,
}

/// Two mount parents: every vendor, a plain drive, a loose file, a dotted
/// name and a name with spaces, and a second parent holding one more reader.
fn fixture() -> Fixture {
    let sc = scratch("screepub-parity-devices-");
    let base = sc.path().to_path_buf();
    let a = base.join("mounts-a");
    let b = base.join("mounts-b");
    for (dir, subs) in [
        (a.join("Kindle"), vec!["documents"]),
        (a.join("NO NAME"), vec!["documents", "system"]),
        (a.join("KOBOeReader"), vec![".kobo"]),
        (a.join("tolino vision"), vec![]),
        (a.join("USB STICK"), vec!["documents"]),
        (a.join("Backup.2026"), vec![]),
        (b.join("Sams Kindle"), vec!["documents"]),
    ] {
        std::fs::create_dir_all(&dir).unwrap();
        for sub in subs {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
    }
    std::fs::write(a.join("loose-file.txt"), "x").unwrap();
    let config = base.join("config");
    std::fs::create_dir_all(&config).unwrap();
    Fixture {
        roots: vec![s(&a), s(&b)],
        config,
        base,
        _scratch: sc,
    }
}

fn delimited(roots: &[String]) -> String {
    roots.join(if cfg!(windows) { ";" } else { ":" })
}

fn rust_devices(roots: &[String], endpoint: &str, quick: bool) -> String {
    ok(&devices_command(
        &ListDevicesOptions {
            roots: Some(roots.to_vec()),
            remarkable_endpoint: Some(endpoint.into()),
            skip_remarkable: quick,
            ..Default::default()
        },
        Platform::host(),
    ))
}

#[test]
fn devices_answers_as_the_typescript_does() {
    if !frozen_here() {
        return;
    }
    let f = fixture();
    let silent = Stub::start(404);
    let answering = Stub::start(200);
    let all = delimited(&f.roots);
    let empty = s(&f.base.join("no-mounts"));
    std::fs::create_dir_all(&empty).unwrap();
    let cases: Vec<(&str, Vec<&str>, Vec<String>, &str)> = vec![
        (
            "every vendor, no tablet",
            vec!["devices", "--json"],
            f.roots.clone(),
            &silent.url,
        ),
        (
            "every vendor and a tablet",
            vec!["devices", "--json"],
            f.roots.clone(),
            &answering.url,
        ),
        (
            "--quick never asks the tablet",
            vec!["devices", "--quick", "--json"],
            f.roots.clone(),
            &answering.url,
        ),
        (
            "nothing plugged in",
            vec!["devices", "--json"],
            vec![empty.clone()],
            &silent.url,
        ),
        (
            "a tablet alone",
            vec!["devices", "--json"],
            vec![empty.clone()],
            &answering.url,
        ),
        (
            "a root that is not there",
            vec!["devices", "--json"],
            vec![s(&f.base.join("gone"))],
            "http://127.0.0.1:1",
        ),
    ];
    for (what, args, roots, endpoint) in cases {
        let roots_arg = if roots == f.roots {
            all.clone()
        } else {
            delimited(&roots)
        };
        let base = s(&f.base);
        let subs = [
            (base.as_str(), "$BASE"),
            (silent.url.as_str(), "$SILENT"),
            (answering.url.as_str(), "$ANSWERING"),
        ];
        let want = ts(
            &format!("devices / {what}"),
            &args,
            &roots_arg,
            endpoint,
            &f.config,
            &subs,
        );
        let got = rust_devices(&roots, endpoint, args.contains(&"--quick"));
        assert_eq!(got, want, "{what}");
    }
}

#[test]
fn eject_refuses_as_the_typescript_does() {
    if !frozen_here() {
        return;
    }
    let f = fixture();
    let all = delimited(&f.roots);
    let roots = f.roots.clone();
    let listed = move || mounted_devices(Some(&roots));
    // Volumes the fake roots do NOT list as readers (or that are relative):
    // the TypeScript refuses each before it could reach an eject tool.
    let not_listed = s(&f.base.join("mounts-a").join("USB STICK"));
    let elsewhere = s(&f.base.join("mounts-a").join("Kindle").join("documents"));
    let gone = s(&f.base.join("nowhere"));
    for volume in [
        "Volumes/Kindle",
        "Kindle",
        not_listed.as_str(),
        elsewhere.as_str(),
        gone.as_str(),
    ] {
        let base = s(&f.base);
        let subs = [(base.as_str(), "$BASE")];
        let want = ts(
            &format!("eject / {}", encode(volume, &subs)),
            &["eject", volume, "--json"],
            &all,
            "http://127.0.0.1:1",
            &f.config,
            &subs,
        );
        let got = match eject_command(
            volume,
            &EjectDeps {
                platform: Platform::host(),
                devices: Some(&listed),
                run: Arc::new(Never),
                timeout: Duration::from_secs(1),
            },
        ) {
            Ok(answer) => ok(&answer),
            Err(err) => failed(&err),
        };
        assert_eq!(got, want, "eject {volume}");
    }
}

#[test]
fn reveal_refuses_as_the_typescript_does() {
    if !frozen_here() {
        return;
    }
    let f = fixture();
    let missing = s(&f.base.join("gone.epub"));
    let folder = s(&f.base.join("mounts-a"));
    for path in [
        "relative/Script.epub",
        "Script.epub",
        missing.as_str(),
        folder.as_str(),
    ] {
        let base = s(&f.base);
        let subs = [(base.as_str(), "$BASE")];
        let want = ts(
            &format!("reveal / {}", encode(path, &subs)),
            &["reveal", path, "--json"],
            "",
            "http://127.0.0.1:1",
            &f.config,
            &subs,
        );
        let got = match reveal_command(path, Platform::host(), &Never) {
            Ok(answer) => ok(&answer),
            Err(err) => failed(&err),
        };
        assert_eq!(got, want, "reveal {path}");
    }
}

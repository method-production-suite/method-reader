//! The TypeScript CLI against this crate, answer for answer: `devices`,
//! `eject` and `reveal` under --json, on fake volume roots
//! (SCREEPUB_VOLUME_ROOTS) and a stand-in tablet
//! (SCREEPUB_REMARKABLE_ENDPOINT), the overrides the TypeScript honors.
//! The Rust side's envelope is built here the way sp-cli will build it:
//! `{"ok":true,...answer}` or `{"ok":false,"error":{code,message}}`.
//!
//! eject and reveal are asked ONLY what they refuse: a valid volume would
//! make the TypeScript run diskutil, and a valid file would open Finder.
//! Every eject here names a volume the fake roots do not list, or a relative
//! one, and every reveal a relative path, a missing file or a folder.
//!
//! Skipped (with a note) when bun is not installed, as on a CI runner
//! without it; `cargo xtask parity` is the harness that always has it.

mod common;
use common::{Stub, s, scratch};
use serde::Serialize;
use serde_json::{Map, Value};
use sp_device::eject::{EjectDeps, eject_command};
use sp_device::list::ListDevicesOptions;
use sp_device::process::{Opener, OpenerOutput, RunOutput, Runner};
use sp_device::volumes::mounted_devices;
use sp_device::{CliError, Platform, commands::devices_command, reveal::reveal_command};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

fn bun_here() -> bool {
    Command::new("bun")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
        && repo_root().join("src").join("cli.ts").exists()
}

/// One run of `bun src/cli.ts <args>` with the overrides, its stdout's one
/// JSON object as text.
fn ts(args: &[&str], roots: &str, endpoint: &str, config: &Path) -> String {
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
    if !bun_here() {
        eprintln!("parity: bun is not installed here; skipped");
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
        let want = ts(&args, &roots_arg, endpoint, &f.config);
        let got = rust_devices(&roots, endpoint, args.contains(&"--quick"));
        assert_eq!(got, want, "{what}");
    }
}

#[test]
fn eject_refuses_as_the_typescript_does() {
    if !bun_here() {
        eprintln!("parity: bun is not installed here; skipped");
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
        let want = ts(
            &["eject", volume, "--json"],
            &all,
            "http://127.0.0.1:1",
            &f.config,
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
    if !bun_here() {
        eprintln!("parity: bun is not installed here; skipped");
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
        let want = ts(
            &["reveal", path, "--json"],
            "",
            "http://127.0.0.1:1",
            &f.config,
        );
        let got = match reveal_command(path, Platform::host(), &Never) {
            Ok(answer) => ok(&answer),
            Err(err) => failed(&err),
        };
        assert_eq!(got, want, "reveal {path}");
    }
}

//! Parity against the TypeScript: what `src/cli.ts <verb> ... --json`
//! printed and this crate's handler for the same verb, on the same files,
//! must be the same line byte for byte (the Rust answer wrapped in the
//! CLI's envelope and written by `jsjson`, as `JSON.stringify` writes it),
//! and leave the same files behind.
//!
//! The TypeScript's side is frozen data (tests/data/parity-cli.json,
//! common::frozen): each case was run once under bun, in a scratch folder
//! of its own (its own fake HOME, its own METHOD_READER_CONFIG_DIR), and its
//! line and files were recorded with that folder spelled `$ROOT`. Here the
//! case is set up again in a new scratch folder, the Rust handler runs
//! there, and both are compared with `$ROOT` read as that folder.
//!
//! The answers are a Mac's, so this runs on macOS only. Real scripts and
//! devices are never involved; nothing here opens an app or a browser (no
//! `route` performer runs: only its refusals and `save-epub`).
//!
//! CALIBRE SPAWNS: the `kfx-status` case and the `export --for kindle|kfx
//! --check` cases run calibre-customize --list-plugins on the Rust side,
//! with CALIBRE_CONFIG_DIRECTORY on the scratch copy (common::calibre_env),
//! and are compared only on a machine with the Calibre the answers were
//! frozen with (common::machine). Every other case hands the engine the
//! Calibre guard path.

#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

mod common;

use std::path::Path;

use common::frozen::{self, Frozen};
use common::{CALIBRE_GUARD, Scratch, calibre_config_copy};
use serde_json::json;
use sp_engine::commands::app_settings::{
    AppSettingsDeps, AppSettingsOptions, app_settings_command,
};
use sp_engine::commands::export::{ExportDeps, ExportOptions, export_command};
use sp_engine::commands::kfx::{KfxCommandDeps, kfx_status_command};
use sp_engine::commands::routes::{RouteDeps, RouteOptions, route_command, routes_command};
use sp_engine::commands::settings::{SettingsOptions, settings_command};
use sp_engine::env::Env;
use sp_engine::errors::{EngineError, ErrorCode, JsonError};
use sp_engine::export::renderers::Books;
use sp_engine::export::route_facts::{RouteProbes, route_facts};
use sp_engine::jsjson;
use sp_engine::settings::app::app_settings_path;
use sp_engine::update::{update_decision_command, update_should_check_command};

/// The environment both sides get for a case rooted at `root`.
fn env_for(root: &str, calibre: bool) -> Env {
    let mut env = Env::new();
    if let Ok(path) = std::env::var("PATH") {
        env.set("PATH", path);
    }
    env.set("HOME", format!("{root}/home"));
    env.set("METHOD_READER_CONFIG_DIR", format!("{root}/cfg"));
    env.set(
        "CALIBRE_CONFIG_DIRECTORY",
        if calibre {
            calibre_config_copy().to_string_lossy().into_owned()
        } else {
            CALIBRE_GUARD.to_owned()
        },
    );
    env
}

/// The CLI's line for a Rust answer.
fn line<T: serde::Serialize>(result: Result<T, EngineError>) -> String {
    match result {
        Ok(answer) => jsjson::stringify(&sp_engine::ok_envelope(&answer)),
        Err(EngineError::Cli(e)) => jsjson::stringify(&sp_engine::error_envelope(&e.to_json())),
        Err(other) => panic!("not a contract error: {other:?}"),
    }
}

fn usage_line<T: serde::Serialize>(result: Result<T, String>) -> String {
    match result {
        Ok(answer) => jsjson::stringify(&sp_engine::ok_envelope(&answer)),
        Err(message) => jsjson::stringify(&sp_engine::error_envelope(&JsonError {
            code: ErrorCode::Usage,
            message,
        })),
    }
}

/// Every file under `dir`, path and bytes, sorted.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
            // The fake HOME holds bun's own transpiler cache, and no case
            // here writes there.
            if rel == "home" {
                continue;
            }
            if p.is_dir() {
                out.push((format!("{rel}/"), Vec::new()));
                walk(base, &p, out);
            } else {
                out.push((rel, std::fs::read(&p).unwrap_or_default()));
            }
        }
    }
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn reset(dir: &Path) {
    #[cfg(unix)]
    {
        // A test may have locked a folder; unlock before removing.
        let _ = std::process::Command::new("chmod")
            .args(["-R", "u+rwx"])
            .arg(dir)
            .status();
    }
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::create_dir_all(dir).unwrap();
}

/// The TypeScript's frozen answer for this case (common::frozen). The key is
/// the test's name and the case's label; `dir` says the case compares the
/// files it leaves.
fn ts(label: &str, root: &str, argv: &[&str], _env: &Env, dir: Option<&Path>) -> Frozen {
    let test = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .rsplit("::")
        .next()
        .unwrap_or("unnamed")
        .to_owned();
    let key = format!("{test} / {label}");
    let answer = frozen::answer(&key, root, argv);
    assert_eq!(
        answer.files.is_some(),
        dir.is_some(),
        "{key}: frozen with and without its files"
    );
    answer
}

/// One parity case: the same setup, the TypeScript's line, the Rust line,
/// and the files each left.
fn case(
    label: &str,
    calibre: bool,
    setup: &dyn Fn(&str),
    ts_args: &dyn Fn(&str) -> Vec<String>,
    rust: &dyn Fn(&str, &Env) -> String,
) {
    let s = Scratch::new("parity");
    let root = s.str();
    let env = env_for(&root, calibre);

    setup(&root);
    let args = ts_args(&root);
    let mut argv: Vec<&str> = args.iter().map(String::as_str).collect();
    argv.push("--json");
    let Frozen { run: ts, files } = ts(label, &root, &argv, &env, Some(&s.path));
    let ts_files = files.expect("a case keeps its files");

    reset(&s.path);
    setup(&root);
    let rust_line = rust(&root, &env);
    let rust_files = snapshot(&s.path);

    assert_eq!(
        rust_line,
        ts.stdout.trim_end(),
        "{label}: the answers differ (ts stderr: {})",
        ts.stderr
    );
    let ts_names: Vec<&String> = ts_files.iter().map(|f| &f.0).collect();
    let rust_names: Vec<&String> = rust_files.iter().map(|f| &f.0).collect();
    assert_eq!(rust_names, ts_names, "{label}: the files left differ");
    for (a, b) in ts_files.iter().zip(&rust_files) {
        assert!(
            a.1 == b.1,
            "{label}: {} differs:\nts:   {}\nrust: {}",
            a.0,
            String::from_utf8_lossy(&a.1),
            String::from_utf8_lossy(&b.1)
        );
    }
}

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_owned()).collect()
}

fn write(path: &str, text: &str) {
    common::write(path, text);
}

// ---- app-settings ----

/// A `--set` value, built from the case's root.
type SetFor = Box<dyn Fn(&str) -> String>;

/// A settings case: its label, its setup and its `--set`.
type SettingsCase<'a> = (&'a str, &'a dyn Fn(&str), Option<&'a str>);

fn app_settings_case(label: &str, setup: &dyn Fn(&str), set: Option<&dyn Fn(&str) -> String>) {
    case(
        label,
        false,
        setup,
        &|root| {
            let mut a = args(&["app-settings"]);
            if let Some(set) = set {
                a.push("--set".into());
                a.push(set(root));
            }
            a
        },
        &|root, env| {
            line(app_settings_command(
                &AppSettingsOptions {
                    set: set.map(|f| f(root)),
                },
                &AppSettingsDeps {
                    settings_path: None,
                    platform: None,
                    env: Some(env.clone()),
                },
            ))
        },
    );
}

#[test]
fn parity_app_settings() {
    require_frozen!();
    let none = |_: &str| {};
    let stored = |root: &str| {
        write(
            &format!("{root}/cfg/settings.json"),
            "{\n  \"lastRoute\": \"save-epub\",\n  \"future\": {\"b\": 1.0, \"2\": [1e21]},\n  \"formatDefaults\": {\"cueIndentPct\": 99, \"fontFamily\": \"serif\"},\n  \"readerDevice\": \"kobo\",\n  \"animatePage\": false,\n  \"typewriterSounds\": true\n}",
        )
    };
    app_settings_case("plain read, no file", &none, None);
    app_settings_case("plain read, a stored file", &stored, None);
    app_settings_case(
        "a corrupt file",
        &|root: &str| write(&format!("{root}/cfg/settings.json"), "{ nope"),
        None,
    );

    let sets: Vec<(&str, SetFor)> = vec![
        (
            "libraryPath",
            Box::new(|root: &str| json!({ "libraryPath": format!("{root}/books/") }).to_string()),
        ),
        (
            "libraryPath reset",
            Box::new(|_: &str| r#"{"libraryPath":null}"#.into()),
        ),
        (
            "formatDefaults",
            Box::new(|_: &str| {
                r#"{"formatDefaults":{"cueIndentPct":12.5,"justifyText":true,"dialogueSideMarginPct":-4}}"#.into()
            }),
        ),
        (
            "formatDefaults reset",
            Box::new(|_: &str| r#"{"formatDefaults":null}"#.into()),
        ),
        (
            "every switch",
            Box::new(|_: &str| {
                r#"{"keepScriptSettings":false,"readerDevice":"remarkable","animatePage":true,"typewriterSounds":false}"#.into()
            }),
        ),
        (
            "every reset",
            Box::new(|_: &str| {
                r#"{"keepScriptSettings":null,"readerDevice":null,"animatePage":null,"typewriterSounds":null}"#.into()
            }),
        ),
        ("empty object", Box::new(|_: &str| "{}".into())),
        ("not JSON", Box::new(|_: &str| "{nope".into())),
        ("an array", Box::new(|_: &str| "[1]".into())),
        (
            "lastRoute",
            Box::new(|_: &str| r#"{"lastRoute":"apple-books"}"#.into()),
        ),
        (
            "an unknown key after a good one",
            Box::new(|_: &str| r#"{"animatePage":true,"zzz":1}"#.into()),
        ),
        (
            "integer key first",
            Box::new(|_: &str| r#"{"animatePage":true,"7":1}"#.into()),
        ),
        (
            "relative library",
            Box::new(|_: &str| r#"{"libraryPath":"books"}"#.into()),
        ),
        (
            "library not a string",
            Box::new(|_: &str| r#"{"libraryPath":7}"#.into()),
        ),
        (
            "library under a file",
            Box::new(|root: &str| {
                json!({ "libraryPath": format!("{root}/afile/books") }).to_string()
            }),
        ),
        (
            "library is a file",
            Box::new(|root: &str| json!({ "libraryPath": format!("{root}/afile") }).to_string()),
        ),
        (
            "bad formatDefaults",
            Box::new(|_: &str| r#"{"formatDefaults":[1]}"#.into()),
        ),
        (
            "bad keep",
            Box::new(|_: &str| r#"{"keepScriptSettings":"false"}"#.into()),
        ),
        (
            "bad reader",
            Box::new(|_: &str| r#"{"readerDevice":"nook"}"#.into()),
        ),
        (
            "bad switch",
            Box::new(|_: &str| r#"{"typewriterSounds":1}"#.into()),
        ),
        (
            "bad library with a good format, nothing written",
            Box::new(|_: &str| {
                r#"{"formatDefaults":{"justifyText":true},"libraryPath":"rel"}"#.into()
            }),
        ),
    ];
    for (label, set) in &sets {
        let setup = |root: &str| {
            stored(root);
            write(&format!("{root}/afile"), "a file");
        };
        app_settings_case(label, &setup, Some(set.as_ref()));
    }
}

#[test]
fn parity_app_settings_with_the_library_override() {
    require_frozen!();
    // METHOD_READER_LIBRARY is set in the env both sides get, through a case of
    // its own: the env is built per case, so wrap it.
    let s = Scratch::new("parity");
    let root = s.str();
    let env = env_for(&root, false).with("METHOD_READER_LIBRARY", &format!("{root}/envlib/"));
    let ts = ts(
        "the library override",
        &root,
        &["app-settings", "--json"],
        &env,
        None,
    )
    .run;
    let rust = line(app_settings_command(
        &AppSettingsOptions::default(),
        &AppSettingsDeps {
            env: Some(env.clone()),
            ..Default::default()
        },
    ));
    assert_eq!(rust, ts.stdout.trim_end());
}

// ---- settings ----

#[test]
fn parity_settings() {
    require_frozen!();
    let script = |root: &str| format!("{root}/lib/My.Script.fountain");
    let base = |root: &str| write(&script(root), "Title: T\n\nINT. ROOM - DAY\n");
    let with_defaults = |root: &str| {
        base(root);
        write(
            &format!("{root}/cfg/settings.json"),
            r#"{"formatDefaults":{"dualDialogue":"sequential","dialogueSideMarginPct":10},"keepScriptSettings":false}"#,
        );
    };
    let with_sidecar = |root: &str| {
        base(root);
        write(
            &format!("{root}/lib/My.Script.method-reader.json"),
            r#"{"cueIndentPct": 40, "x": 1}"#,
        );
    };
    let sets: [SettingsCase; 9] = [
        ("read, nothing stored", &base, None),
        ("read over app defaults (a preset)", &with_defaults, None),
        ("read a sidecar", &with_sidecar, None),
        (
            "set one knob",
            &with_sidecar,
            Some(r#"{"elementSpacingEm":1.25,"fontFamily":"sans"}"#),
        ),
        (
            "set over app defaults",
            &with_defaults,
            Some(r#"{"justifyText":true}"#),
        ),
        (
            "set clamps",
            &base,
            Some(r#"{"cueIndentPct":1000,"elementSpacingEm":0}"#),
        ),
        ("set not JSON", &base, Some("{")),
        ("set an array", &base, Some("[]")),
        ("set nothing", &base, Some("{}")),
    ];
    for (label, setup, set) in sets {
        case(
            label,
            false,
            setup,
            &|root| {
                let mut a = args(&["settings", &script(root)]);
                if let Some(set) = set {
                    a.push("--set".into());
                    a.push(set.into());
                }
                a
            },
            &|root, env| {
                line(settings_command(&SettingsOptions {
                    fountain: script(root),
                    set: set.map(str::to_owned),
                    app_settings_path: app_settings_path(sp_engine::env::host_platform(), env),
                }))
            },
        );
    }
    case(
        "a script that is not there",
        false,
        &|_| {},
        &|root| args(&["settings", &format!("{root}/ghost.fountain")]),
        &|root, env| {
            line(settings_command(&SettingsOptions {
                fountain: format!("{root}/ghost.fountain"),
                set: None,
                app_settings_path: app_settings_path(sp_engine::env::host_platform(), env),
            }))
        },
    );
}

// ---- update-decision, update-should-check ----

#[test]
fn parity_update_decision() {
    require_frozen!();
    let pairs = [
        ("0.12.0", "0.11.1"),
        ("0.11.1", "0.11.1-3-gabcdef1"),
        ("0.11.1", "0.11.1-dirty"),
        ("v0.12.0+ci.4", "0.11.1-3-gabcdef1-dirty"),
        ("0.10.0", "0.9.0"),
        ("0.6.0-beta.10", "0.6.0-beta.9"),
        ("0.6.0-rc.1", "0.6.0-beta.2"),
        ("0.6.0-beta", "0.6.0-Beta"),
        ("0.6.0-a-1", "0.6.0-a1"),
        ("0.6.0-a_", "0.6.0-a."),
        ("not-a-version", "0.6.0"),
        ("0.6.0", ""),
        ("1", "1.0.0"),
        ("1.0.1", "1"),
        ("0.3.0-dev", "0.3.0"),
        ("  V1.2.3 ", "1.2.3-x\"y"),
    ];
    for (offered, current) in pairs {
        let s = Scratch::new("parity");
        let env = env_for(&s.str(), false);
        let ts = ts(
            &format!("{offered} vs {current}"),
            &s.str(),
            &[
                "update-decision",
                "--offered",
                offered,
                "--current",
                current,
                "--json",
            ],
            &env,
            None,
        )
        .run;
        let rust = usage_line(update_decision_command(Some(offered), Some(current)));
        assert_eq!(rust, ts.stdout.trim_end(), "{offered} vs {current}");
    }
    for (flags, offered, current) in [
        (
            vec!["update-decision", "--current", "1.0.0"],
            None,
            Some("1.0.0"),
        ),
        (
            vec!["update-decision", "--offered", "1.0.0"],
            Some("1.0.0"),
            None,
        ),
    ] {
        let s = Scratch::new("parity");
        let mut argv = flags.clone();
        argv.push("--json");
        let ts = ts(
            &argv.join(" "),
            &s.str(),
            &argv,
            &env_for(&s.str(), false),
            None,
        )
        .run;
        let rust = usage_line(update_decision_command(offered, current));
        assert_eq!(rust, ts.stdout.trim_end());
    }
}

#[test]
fn parity_update_should_check() {
    require_frozen!();
    // `now` is the clock on each side; every case here answers the same
    // whatever the clock reads, within a day of today.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64;
    let cases: [(bool, Option<&str>); 11] = [
        (false, None),
        (true, None),
        (true, Some("0")),
        (true, Some("")),
        (true, Some(" 12 ")),
        (true, Some("0x10")),
        (true, Some("abc")),
        (true, Some("Infinity")),
        (true, Some("1e400")),
        (true, Some("99999999999999")),
        (false, Some("abc")),
    ];
    for (opted, last) in cases {
        let s = Scratch::new("parity");
        let mut argv = vec!["update-should-check"];
        if opted {
            argv.push("--opted-in");
        }
        if let Some(l) = last {
            argv.push("--last-checked");
            argv.push(l);
        }
        argv.push("--json");
        let ts = ts(
            &format!("{opted} {last:?}"),
            &s.str(),
            &argv,
            &env_for(&s.str(), false),
            None,
        )
        .run;
        let rust = usage_line(update_should_check_command(opted, last, now));
        assert_eq!(rust, ts.stdout.trim_end(), "{opted} {last:?}");
    }
}

// ---- kfx-status ----

/// CALIBRE SPAWNS: calibre-customize --list-plugins, once per side, on the
/// scratch copy.
#[test]
fn parity_kfx_status() {
    require_frozen!();
    if !frozen::same_machine(None) {
        return;
    }
    let s = Scratch::new("parity");
    let env = env_for(&s.str(), true);
    let ts = ts(
        "kfx-status",
        &s.str(),
        &["kfx-status", "--json"],
        &env,
        None,
    )
    .run;
    let rust = jsjson::stringify(&sp_engine::ok_envelope(&kfx_status_command(
        &KfxCommandDeps {
            env: Some(env.clone()),
            ..Default::default()
        },
    )));
    assert_eq!(rust, ts.stdout.trim_end());
}

// ---- export ----

const SCRIPT: &str = "Title: Field Notes\nAuthor: A. Writer\n\nINT. STATION - NIGHT\n\n[[img: p001-01.png w=40]]\n\nThe lamp hums.\n\n[[fmt: f=sans]]\nMARA\n(quietly)\nIt is late.\n\nCUT TO:\n\nEXT. FIELD - DAY\n\nWind.\n";

fn book_setup(root: &str) {
    write(
        &format!("{root}/lib/Book.epub"),
        "PK\u{3}\u{4} a stand-in book",
    );
    write(&format!("{root}/lib/Book.fountain"), SCRIPT);
    write(
        &format!("{root}/lib/Plain.fountain"),
        "Title: P\n\nINT. A - DAY\n\nNothing of ours.\n",
    );
    write(
        &format!("{root}/lib/Book.method-reader.json"),
        r#"{"cueIndentPct": 21}"#,
    );
    std::fs::create_dir_all(format!("{root}/out")).unwrap();
    // One fixed time, with a fraction of a millisecond in it, so the book's
    // date reads the same on both sides and its JavaScript spelling is
    // tested too.
    let at = std::time::UNIX_EPOCH + std::time::Duration::new(1_767_225_600, 123_456_789);
    for name in [
        "Book.epub",
        "Book.fountain",
        "Plain.fountain",
        "Book.method-reader.json",
    ] {
        std::fs::File::options()
            .write(true)
            .open(format!("{root}/lib/{name}"))
            .unwrap()
            .set_modified(at)
            .unwrap();
    }
}

fn export_case(label: &str, calibre: bool, flags: &[&str]) {
    let flags: Vec<String> = flags.iter().map(|s| (*s).to_owned()).collect();
    let expand = |root: &str, f: &str| f.replace("$ROOT", root);
    case(
        label,
        calibre,
        &book_setup,
        &|root| {
            let mut a = vec!["export".to_owned()];
            a.extend(flags.iter().map(|f| expand(root, f)));
            a
        },
        &|root, env| {
            let mut o = ExportOptions::default();
            let mut positional = None;
            let mut it = flags.iter().map(|f| expand(root, f));
            while let Some(f) = it.next() {
                match f.as_str() {
                    "--for" => o.for_ = it.next(),
                    "--fountain" => o.fountain = it.next(),
                    "--options-json" => o.options_json = it.next(),
                    "--out" => o.out = it.next(),
                    "--check" => o.check = true,
                    _ => positional = Some(f),
                }
            }
            o.epub = positional.unwrap_or_default();
            let deps = ExportDeps {
                env: Some(env.clone()),
                ..Default::default()
            };
            line(export_command(&o, &deps, &Books))
        },
    );
}

#[test]
fn parity_export() {
    require_frozen!();
    let b = "$ROOT/lib/Book.epub";
    let f = "$ROOT/lib/Book.fountain";
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("epub", vec![b]),
        ("epub --check", vec![b, "--for", "epub", "--check"]),
        ("epub --out", vec![b, "--out", "$ROOT/out/Copy.EPUB"]),
        ("epub --out onto itself", vec![b, "--out", b]),
        (
            "epub --out a new folder",
            vec![b, "--out", "$ROOT/out/new/deeper/c.epub"],
        ),
        ("fountain", vec![b, "--for", "fountain", "--fountain", f]),
        (
            "fountain --check",
            vec![b, "--for", "fountain", "--fountain", f, "--check"],
        ),
        (
            "fountain --out strips our notes",
            vec![
                b,
                "--for",
                "fountain",
                "--fountain",
                f,
                "--out",
                "$ROOT/out/clean.fountain",
            ],
        ),
        (
            "fountain --out with nothing of ours copies",
            vec![
                b,
                "--for",
                "fountain",
                "--fountain",
                "$ROOT/lib/Plain.fountain",
                "--out",
                "$ROOT/out/plain.fountain",
            ],
        ),
        (
            "fountain --out onto the book's own script",
            vec![b, "--for", "fountain", "--fountain", f, "--out", f],
        ),
        ("fdx", vec![b, "--for", "fdx", "--fountain", f]),
        (
            "fdx --out",
            vec![
                b,
                "--for",
                "fdx",
                "--fountain",
                f,
                "--out",
                "$ROOT/out/Edit.fdx",
            ],
        ),
        (
            "fdx --check, not built",
            vec![b, "--for", "fdx", "--fountain", f, "--check"],
        ),
        ("azw3 --check", vec![b, "--for", "azw3", "--check"]),
        (
            "mobi --check",
            vec![b, "--for", "mobi", "--fountain", f, "--check"],
        ),
        ("unknown --for", vec![b, "--for", "pdf"]),
        ("no book", vec!["$ROOT/lib/Ghost.epub"]),
        (
            "--check with --out",
            vec![b, "--check", "--out", "$ROOT/out/x.epub"],
        ),
        ("relative --out", vec![b, "--out", "x.epub"]),
        (
            "wrong extension",
            vec![b, "--for", "azw3", "--out", "$ROOT/out/x.kfx"],
        ),
        (
            "wrong extension, a vowel",
            vec![b, "--out", "$ROOT/out/x.mobi"],
        ),
        ("mobi without a script", vec![b, "--for", "mobi"]),
        ("fountain without a script", vec![b, "--for", "fountain"]),
        (
            "fountain that is not one",
            vec![b, "--for", "fountain", "--fountain", "$ROOT/lib/Book.epub"],
        ),
        ("fdx without a script", vec![b, "--for", "fdx"]),
        (
            "a script that is not there",
            vec![b, "--for", "fdx", "--fountain", "$ROOT/lib/Ghost.fountain"],
        ),
        ("bad options-json", vec![b, "--options-json", "{"]),
        ("options-json an array", vec![b, "--options-json", "[]"]),
    ];
    for (label, flags) in cases {
        export_case(label, false, &flags);
    }
}

/// CALIBRE SPAWNS: calibre-customize --list-plugins, once per side per
/// case, on the scratch copy. --check builds nothing.
#[test]
fn parity_export_kindle_check() {
    require_frozen!();
    if !frozen::same_machine(None) {
        return;
    }
    export_case(
        "kindle --check",
        true,
        &["$ROOT/lib/Book.epub", "--for", "kindle", "--check"],
    );
    export_case(
        "kfx --check",
        true,
        &["$ROOT/lib/Book.epub", "--for", "kfx", "--check"],
    );
}

// ---- routes, route ----

fn this_machines_facts(env: &Env) -> sp_engine::export::routes::RouteFacts {
    route_facts(
        &RouteProbes {
            devices: Some(Box::new(Vec::new)),
            env: Some(env.clone()),
            ..Default::default()
        },
        &sp_device::ListDevicesOptions {
            skip_remarkable: true,
            ..Default::default()
        },
    )
}

#[test]
fn parity_routes_quick() {
    require_frozen!();
    if !frozen::same_machine(None) {
        return;
    }
    for last in [
        None,
        Some("\"save-kindle\""),
        Some("\"apple-books\""),
        Some("7"),
        Some("\"device:kobo\""),
    ] {
        let s = Scratch::new("parity");
        let root = s.str();
        let env = env_for(&root, false);
        book_setup(&root);
        if let Some(last) = last {
            write(
                &format!("{root}/cfg/settings.json"),
                &format!("{{\"lastRoute\":{last}}}"),
            );
        }
        let epub = format!("{root}/lib/Book.epub");
        let ts = ts(
            &format!("lastRoute {last:?}"),
            &root,
            &["routes", &epub, "--quick", "--json"],
            &env,
            None,
        )
        .run;
        let answer = ts.json();
        // The TypeScript scans this machine's mounted readers; the Rust
        // device scan is the sp-device branch's. With a reader plugged in
        // the two cannot agree yet, and that says nothing about routes.
        if answer["routes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.get("device").is_some())
        {
            eprintln!("skipped: a reader is connected to this machine");
            return;
        }
        // The settings file the env names, as the CLI would read it.
        let rust = line(routes_command(
            &epub,
            &|| this_machines_facts(&env),
            Some(&app_settings_path(sp_engine::env::host_platform(), &env)),
        ));
        assert_eq!(rust, ts.stdout.trim_end(), "lastRoute {last:?}");
    }
}

fn route_case(label: &str, flags: &[&str]) {
    let flags: Vec<String> = flags.iter().map(|s| (*s).to_owned()).collect();
    let expand = |root: &str, f: &str| f.replace("$ROOT", root);
    case(
        label,
        false,
        &book_setup,
        &|root| {
            let mut a = vec!["route".to_owned()];
            a.extend(flags.iter().map(|f| expand(root, f)));
            a
        },
        &|root, env| {
            let mut o = RouteOptions::default();
            let mut positionals = Vec::new();
            let mut it = flags.iter().map(|f| expand(root, f));
            while let Some(f) = it.next() {
                match f.as_str() {
                    "--fountain" => o.fountain = it.next(),
                    "--options-json" => o.options_json = it.next(),
                    "--out" => o.out = it.next(),
                    _ => positionals.push(f),
                }
            }
            o.key = positionals.first().cloned().unwrap_or_default();
            o.epub = positionals.get(1).cloned();
            let deps = RouteDeps {
                settings_path: Some(app_settings_path(sp_engine::env::host_platform(), env)),
                // Never a real app: any open is a test failure.
                open: Some(Box::new(|argv: &[String]| panic!("would open {argv:?}"))),
                export_deps: Some(ExportDeps {
                    env: Some(env.clone()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            line(route_command(&o, deps, &Books))
        },
    );
}

#[test]
fn parity_route() {
    require_frozen!();
    let b = "$ROOT/lib/Book.epub";
    let cases: Vec<(&str, Vec<&str>)> = vec![
        (
            "save-epub",
            vec!["save-epub", b, "--out", "$ROOT/out/Saved.epub"],
        ),
        ("save-epub onto itself", vec!["save-epub", b, "--out", b]),
        ("a device key", vec!["device:kindle", b]),
        ("remarkable", vec!["remarkable", b]),
        ("an unknown key", vec!["nope", b]),
        ("setup with a book", vec!["kindle-email-setup", b]),
        (
            "setup with --out",
            vec!["kindle-email-setup", "--out", "$ROOT/out/x.epub"],
        ),
        (
            "setup with --fountain",
            vec![
                "kindle-email-setup",
                "--fountain",
                "$ROOT/lib/Book.fountain",
            ],
        ),
        ("no book", vec!["apple-books"]),
        (
            "a book that is not there",
            vec!["apple-books", "$ROOT/lib/Ghost.epub"],
        ),
        ("save without --out", vec!["save-epub", b]),
        (
            "a performer with --out",
            vec!["send-to-kindle", b, "--out", "$ROOT/out/x.epub"],
        ),
        (
            "save-epub with --fountain",
            vec![
                "save-epub",
                b,
                "--out",
                "$ROOT/out/x.epub",
                "--fountain",
                "$ROOT/lib/Book.fountain",
            ],
        ),
        (
            "save-epub with --options-json",
            vec![
                "save-epub",
                b,
                "--out",
                "$ROOT/out/x.epub",
                "--options-json",
                "{}",
            ],
        ),
        (
            "save-epub relative --out",
            vec!["save-epub", b, "--out", "x.epub"],
        ),
        (
            "save-epub wrong extension",
            vec!["save-epub", b, "--out", "$ROOT/out/x.azw3"],
        ),
        (
            "save-kindle relative --out",
            vec!["save-kindle", b, "--out", "x.azw3"],
        ),
    ];
    for (label, flags) in cases {
        route_case(label, &flags);
    }
}

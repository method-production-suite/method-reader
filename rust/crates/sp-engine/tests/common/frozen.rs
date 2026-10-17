//! The TypeScript CLI's answers, frozen as data: tests/data/parity-cli.json.
//!
//! parity_cli.rs once ran `bun src/cli.ts` beside each Rust handler. The
//! TypeScript is gone, so what it answered was recorded first, case by
//! case, on the owner's Mac: the line it printed, its exit code, and every
//! file it left in the case's scratch folder, with that folder spelled
//! `$ROOT` so the answer replays in any other scratch folder.
//!
//! The answers are a macOS machine's, so the comparisons run on macOS only.
//! The few that also depend on what is installed (Calibre, its plugin,
//! Kindle Previewer, the Mac apps a route can use) carry the facts they were
//! recorded under ([`super::machine`]) and are compared only where the
//! facts still read the same.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use super::TsRun;

/// Every frozen answer, by case key.
fn store() -> &'static BTreeMap<String, Value> {
    static STORE: OnceLock<BTreeMap<String, Value>> = OnceLock::new();
    STORE.get_or_init(|| {
        serde_json::from_str(include_str!("../data/parity-cli.json"))
            .expect("tests/data/parity-cli.json is JSON")
    })
}

/// Whether frozen answers apply on this machine at all.
pub fn here() -> bool {
    cfg!(target_os = "macos")
}

/// Skip a frozen comparison off macOS, saying so.
#[macro_export]
macro_rules! require_frozen {
    () => {
        if !$crate::common::frozen::here() {
            eprintln!("skipped: the TypeScript answers were frozen on macOS");
            return;
        }
    };
}

/// Whether this machine reads as it did when the answers were frozen, for
/// the answers that depend on what is installed.
pub fn same_machine(home: Option<&std::path::Path>) -> bool {
    let now = super::machine::facts(home);
    let then = &store()["$machine"];
    if &now != then {
        eprintln!(
            "skipped: this machine is not the one the answer was frozen on ({now} against {then})"
        );
        return false;
    }
    true
}

fn decode(text: &str, root: &str) -> String {
    renamed(&text.replace("$ROOT", root))
}

/// A frozen answer with the product's name as it is now. The TypeScript
/// answered as Screepub; the app is Method Reader since 0.13, and that is
/// the only thing a frozen answer is read differently for: the name in a
/// message, the default folder's name, the sidecar's suffix, the command's
/// name. The `builtBy` value "screepub" is a token of the --json contract,
/// not a name, and is left as it was frozen.
pub fn renamed(text: &str) -> String {
    const KEEP: &str = "\u{0}builtBy\u{0}";
    text.replace("\"builtBy\":\"screepub\"", KEEP)
        .replace(".screepub.json", ".method-reader.json")
        .replace("SCREEPUB_", "METHOD_READER_")
        .replace("Screepub", "Method Reader")
        .replace("screepub ", "method-reader ")
        .replace(KEEP, "\"builtBy\":\"screepub\"")
}

/// One frozen run: what the TypeScript printed, and the files it left
/// (path relative to the case's root, bytes), or `None` when the case kept
/// no files.
pub struct Frozen {
    pub run: TsRun,
    pub files: Option<Vec<(String, Vec<u8>)>>,
}

fn bytes_of(v: &Value, root: &str) -> Vec<u8> {
    if let Some(t) = v.get("text").and_then(Value::as_str) {
        return decode(t, root).into_bytes();
    }
    let hex = v["hex"].as_str().expect("text or hex");
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect()
}

/// The frozen answer for `key`, its scratch folder spelled `root`. The
/// arguments it was recorded with must be the ones the test asks with now,
/// so a case edited after the freeze cannot pass on another case's answer.
pub fn answer(key: &str, root: &str, argv: &[&str]) -> Frozen {
    let v = store()
        .get(key)
        .unwrap_or_else(|| panic!("no frozen TypeScript answer for {key:?}"));
    let want: Vec<String> = v["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(|a| decode(a.as_str().expect("an argument"), root))
        .collect();
    let asked: Vec<String> = argv.iter().map(|a| (*a).to_owned()).collect();
    assert_eq!(
        asked, want,
        "{key}: the case's arguments changed since its answer was frozen"
    );
    let files = v.get("files").map(|f| {
        f.as_array()
            .expect("files")
            .iter()
            .map(|e| {
                (
                    renamed(e["path"].as_str().expect("path")),
                    bytes_of(e, root),
                )
            })
            .collect()
    });
    Frozen {
        run: TsRun {
            stdout: decode(v["stdout"].as_str().unwrap_or_default(), root),
            stderr: decode(v["stderr"].as_str().unwrap_or_default(), root),
            code: v["code"].as_i64().unwrap_or(-1) as i32,
        },
        files,
    }
}

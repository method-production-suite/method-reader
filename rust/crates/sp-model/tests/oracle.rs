//! Every committed oracle dump reads into the model and writes back as the
//! same JSON: the proof that these types ARE the TypeScript's shapes, not a
//! guess at them. Real-script dumps (rust/oracle-real/, gitignored) are
//! checked too when they are present on this machine.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sp_model::{ParsedScreenplay, RawLine, Token};

fn rust_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn dumps(folder: &str) -> Vec<PathBuf> {
    let dir = rust_dir().join(folder);
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

fn read(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).expect("oracle JSON parses"))
}

/// Deserialize `wire` as `T`, serialize it back, and demand the same JSON.
fn round_trips<T: DeserializeOwned + Serialize>(wire: &Value, what: &str) {
    let typed: T = serde_json::from_value(wire.clone())
        .unwrap_or_else(|e| panic!("{what}: does not read into the model: {e}"));
    let back = serde_json::to_value(&typed).unwrap();
    assert!(
        back == *wire,
        "{what}: does not write back as the same JSON"
    );
}

fn check(folder: &str, name_in_messages: bool) -> usize {
    let mut checked = 0;
    for (i, dir) in dumps(folder).iter().enumerate() {
        // Real scripts are named by number only, never by title.
        let label = if name_in_messages {
            dir.file_name().unwrap().to_string_lossy().into_owned()
        } else {
            format!("real script {}", i + 1)
        };
        if let Some(v) = read(&dir.join("elements.json")) {
            round_trips::<ParsedScreenplay>(&v, &format!("{label}/elements.json"));
            checked += 1;
        }
        if let Some(v) = read(&dir.join("tokens.json")) {
            round_trips::<Vec<Token>>(&v, &format!("{label}/tokens.json"));
            checked += 1;
        }
        if let Some(v) = read(&dir.join("lines.json")) {
            round_trips::<Vec<RawLine>>(&v["lines"], &format!("{label}/lines.json"));
            checked += 1;
        }
    }
    checked
}

#[test]
fn committed_oracle_dumps_read_and_write_back_unchanged() {
    let checked = check("oracle", true);
    // 7 fixtures: 5 convert (elements, tokens, lines) and 2 are refused
    // before parsing (lines only).
    assert!(
        checked >= 17,
        "expected the committed dumps in rust/oracle/, checked {checked} files"
    );
}

#[test]
fn real_script_dumps_read_and_write_back_unchanged_when_present() {
    check("oracle-real", false);
}

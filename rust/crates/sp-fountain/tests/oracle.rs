//! Stage parity against the TypeScript engine's oracle dumps.

use std::path::{Path, PathBuf};

use serde_json::Value;
use sp_fountain::tokenizer::tokenize;

fn rust_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn dumps(folder: &str) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(rust_dir().join(folder)) else {
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

fn token_mismatches(folder: &str) -> (usize, Vec<String>) {
    let mut checked = 0;
    let mut bad = Vec::new();
    for (i, dir) in dumps(folder).iter().enumerate() {
        let Ok(text) = std::fs::read_to_string(dir.join("script.fountain")) else {
            continue;
        };
        let want: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("tokens.json")).unwrap())
                .unwrap();
        let got = serde_json::to_value(tokenize(&text).unwrap()).unwrap();
        checked += 1;
        if got != want {
            let a = got.as_array().unwrap();
            let b = want.as_array().unwrap();
            let at = (0..a.len().max(b.len()))
                .find(|&k| a.get(k) != b.get(k))
                .unwrap();
            bad.push(format!("{folder} #{} differs at token {at}", i + 1));
        }
    }
    (checked, bad)
}

#[test]
fn committed_tokens_match_fountain_js() {
    let (checked, bad) = token_mismatches("oracle");
    assert!(checked >= 5, "checked {checked}");
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn real_tokens_match_fountain_js_when_present() {
    let (checked, bad) = token_mismatches("oracle-real");
    eprintln!(
        "real scripts tokenized: {checked}, different: {}",
        bad.len()
    );
    assert!(bad.is_empty(), "{bad:?}");
}

//! Stage parity against the TypeScript engine's oracle dumps.

use std::path::{Path, PathBuf};

use serde_json::Value;
use sp_fountain::tokenizer::tokenize;
use sp_fountain::{TitleMeta, extract_title_meta, to_fountain};
use sp_model::{DEFAULT_FORMAT_OPTIONS, ParsedScreenplay, resolve_format_options};

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

/// What convert.ts hands toFountain: `resolveMeta(extractTitleMeta(...))`
/// with no `--title`/`--author`, and the dump's options (none: defaults).
fn serialize_like_convert(dir: &Path) -> Option<String> {
    let elements = std::fs::read_to_string(dir.join("elements.json")).ok()?;
    let screenplay: ParsedScreenplay = serde_json::from_str(&elements).unwrap();
    let meta: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("meta.json")).unwrap()).unwrap();
    let format = resolve_format_options(Some(&meta["options"]), &DEFAULT_FORMAT_OPTIONS);
    let detected = extract_title_meta(&screenplay.elements);
    let resolved = TitleMeta {
        title: Some(
            detected
                .title
                .unwrap_or_else(|| "Untitled Screenplay".to_owned()),
        ),
        author: detected.author,
    };
    Some(to_fountain(&screenplay, Some(&resolved), &format))
}

fn fountain_mismatches(folder: &str) -> (usize, Vec<String>) {
    let mut checked = 0;
    let mut bad = Vec::new();
    for (i, dir) in dumps(folder).iter().enumerate() {
        let Some(got) = serialize_like_convert(dir) else {
            continue;
        };
        let want = std::fs::read_to_string(dir.join("script.fountain")).unwrap();
        checked += 1;
        if got != want {
            let line = got
                .lines()
                .zip(want.lines())
                .position(|(a, b)| a != b)
                .unwrap_or(got.lines().count().min(want.lines().count()));
            bad.push(format!("{folder} #{} differs at line {}", i + 1, line + 1));
        }
    }
    (checked, bad)
}

#[test]
fn committed_fountain_is_byte_identical() {
    let (checked, bad) = fountain_mismatches("oracle");
    assert!(checked >= 5, "checked {checked}");
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn real_fountain_is_byte_identical_when_present() {
    let (checked, bad) = fountain_mismatches("oracle-real");
    eprintln!(
        "real scripts serialized: {checked}, different: {}",
        bad.len()
    );
    assert!(bad.is_empty(), "{bad:?}");
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

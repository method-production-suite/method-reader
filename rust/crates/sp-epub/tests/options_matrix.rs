//! Parity across option sets. The oracle dumps use the default options
//! only; tests/data/options-golden.json holds the TypeScript renderer's
//! answer (every entry's name, method and sha256, and the preview's sha256)
//! for each committed dump under each option set the TypeScript tests
//! exercise, plus sets aimed at the arithmetic edges (toFixed ties,
//! Math.round halves, fractional percentages, clamping). Regenerate with
//! tests/data/options-golden.ts. The real scripts' answers, when this
//! checkout has them, are in rust/oracle-real/sp-epub-options.json.

mod common;

use std::path::Path;

use common::{oracle_dirs, read_zip};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sp_epub::{
    BodyOptions, BookImage, BookMeta, build_epub, content_identifier, tokens_to_body,
    tokens_to_preview_html,
};
use sp_model::{DEFAULT_FORMAT_OPTIONS, Token, resolve_format_options};

fn sha256(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("a file")).expect("JSON")
}

/// Every (option set, what differs) for one dump against its golden.
fn differences(dir: &Path, golden: &Value, sets: &Value) -> Vec<String> {
    let fountain = std::fs::read_to_string(dir.join("script.fountain")).unwrap();
    let tokens: Vec<Token> = serde_json::from_value(read_json(&dir.join("tokens.json"))).unwrap();
    let answer = read_json(&dir.join("convert.json"));
    let epoch = read_json(&dir.join("meta.json"))["sourceDateEpoch"]
        .as_i64()
        .unwrap();
    let meta = BookMeta {
        title: answer["title"].as_str().unwrap().to_owned(),
        author: answer
            .get("author")
            .and_then(Value::as_str)
            .map(str::to_owned),
        language: None,
        identifier: Some(content_identifier(&fountain)),
    };
    let mut images: Vec<BookImage> = std::fs::read_dir(dir.join("images"))
        .map(|read| {
            read.filter_map(Result::ok)
                .map(|e| BookImage {
                    name: e.file_name().to_string_lossy().into_owned(),
                    bytes: std::fs::read(e.path()).unwrap(),
                })
                .collect()
        })
        .unwrap_or_default();
    images.sort_by(|a, b| a.name.cmp(&b.name));
    let by_name: Vec<(String, Vec<u8>)> = images
        .iter()
        .map(|i| (i.name.clone(), i.bytes.clone()))
        .collect();

    let mut out = Vec::new();
    let sets = sets.as_object().expect("option sets");
    assert_eq!(
        golden.as_object().unwrap().len(),
        sets.len(),
        "a golden per set"
    );
    for (name, partial) in sets {
        let format = resolve_format_options(Some(partial), &DEFAULT_FORMAT_OPTIONS);
        let body = tokens_to_body(
            &tokens,
            &BodyOptions {
                format,
                images: images.iter().map(|i| i.name.clone()).collect(),
                ..BodyOptions::default()
            },
        );
        let epub = build_epub(&meta, &body, &format, &images, epoch).expect("a book");
        let ours: Vec<Value> = read_zip(&epub)
            .iter()
            .map(|e| serde_json::json!([e.name, e.stored, sha256(&e.data)]))
            .collect();
        let want = &golden[name];
        if Value::Array(ours.clone()) != want["entries"] {
            let theirs = want["entries"].as_array().unwrap();
            let names = |v: &[Value]| v.iter().map(|e| e[0].clone()).collect::<Vec<_>>();
            if names(&ours) == names(theirs) {
                for (x, y) in ours.iter().zip(theirs) {
                    if x != y {
                        out.push(format!("{name}: {} differs", x[0]));
                    }
                }
            } else {
                out.push(format!(
                    "{name}: entries {:?} vs {:?}",
                    names(&ours),
                    names(theirs)
                ));
            }
        }
        let preview = tokens_to_preview_html(&tokens, &format, &by_name);
        if Value::String(sha256(preview.as_bytes())) != want["preview"] {
            out.push(format!("{name}: the preview differs"));
        }
    }
    out
}

#[test]
fn every_committed_fixture_matches_under_every_option_set() {
    let golden =
        read_json(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/options-golden.json"));
    let dirs = oracle_dirs("oracle");
    assert_eq!(
        dirs.len(),
        golden["fixtures"].as_object().unwrap().len(),
        "a golden per committed dump"
    );
    let mut failures = Vec::new();
    for dir in &dirs {
        let stem = dir.file_name().unwrap().to_string_lossy().into_owned();
        for d in differences(dir, &golden["fixtures"][&stem], &golden["optionSets"]) {
            failures.push(format!("{stem} {d}"));
        }
    }
    let sets = golden["optionSets"].as_object().unwrap().len();
    eprintln!(
        "committed fixtures: {} dumps x {sets} option sets",
        dirs.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_real_script_matches_under_every_option_set() {
    let path = common::repo_root().join("rust/oracle-real/sp-epub-options.json");
    if !path.is_file() {
        eprintln!(
            "no rust/oracle-real/sp-epub-options.json here; see tests/data/options-golden.ts"
        );
        return;
    }
    let golden = read_json(&path);
    let mut bad = 0;
    let mut checked = 0;
    for (n, dir) in oracle_dirs("oracle-real").iter().enumerate() {
        let stem = dir.file_name().unwrap().to_string_lossy().into_owned();
        let Some(want) = golden["fixtures"].get(&stem) else {
            continue;
        };
        checked += 1;
        let d = differences(dir, want, &golden["optionSets"]);
        if !d.is_empty() {
            bad += 1;
            // Option-set names and entry names only: no script text.
            eprintln!("real script {}: {}", n + 1, d.join("; "));
        }
    }
    eprintln!("real scripts: {checked} checked, {bad} differ");
    assert_eq!(bad, 0);
}

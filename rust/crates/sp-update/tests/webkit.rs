//! The update consent carried from the Tauri window's WebKit localStorage.
//! Fixtures: tests/data/make-webkit-fixtures.sh (real SQLite files made by
//! the sqlite3 tool, invented values).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sp_update::sqlite::{Database, Value, utf16};
use sp_update::webkit::{find_storage, import_update_consent, read_update_keys, website_data};

fn data(p: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(p)
}

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn the_older_layout_carries_all_four_values_and_nothing_else() {
    let got = import_update_consent(&data("webkit-legacy"));
    assert_eq!(
        got,
        map(&[
            ("updateOptIn", "true"),
            ("updateAsked", "true"),
            ("updateLastChecked", "1767225600000"),
            ("updateFound", "0.11.1"),
        ])
    );
}

#[test]
fn the_newer_layout_picks_the_tauri_origin_and_reads_the_write_ahead_log() {
    let root = data("webkit-modern");
    let file = find_storage(&root).unwrap();
    assert!(file.ends_with("a1/b2/LocalStorage/localstorage.sqlite3"));
    // Every row is still in the -wal file: the main file is its header.
    assert_eq!(std::fs::metadata(&file).unwrap().len(), 4096);
    assert_eq!(
        read_update_keys(&file).unwrap(),
        map(&[
            ("updateOptIn", "false"),
            ("updateAsked", "true"),
            ("updateFound", ""),
        ])
    );
}

#[test]
fn nothing_found_means_the_question_is_asked_again() {
    let empty = tempfile::tempdir().unwrap();
    assert!(import_update_consent(empty.path()).is_empty());
    assert!(import_update_consent(&empty.path().join("missing")).is_empty());
}

#[test]
fn two_unnamed_files_are_not_guessed_between() {
    let dir = tempfile::tempdir().unwrap();
    for d in ["x", "y"] {
        let ls = dir.path().join(d).join("LocalStorage");
        std::fs::create_dir_all(&ls).unwrap();
        std::fs::copy(data("big.sqlite3"), ls.join("localstorage.sqlite3")).unwrap();
    }
    assert_eq!(find_storage(dir.path()), None);
}

#[test]
fn a_damaged_file_is_nothing_not_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let ls = dir.path().join("LocalStorage");
    std::fs::create_dir_all(&ls).unwrap();
    let mut bytes = std::fs::read(data("big.sqlite3")).unwrap();
    bytes.truncate(3000);
    std::fs::write(ls.join("tauri_localhost_0.localstorage"), bytes).unwrap();
    assert!(import_update_consent(dir.path()).is_empty());
}

#[test]
fn interior_pages_and_overflow_chains_read_whole() {
    let db = Database::open(&data("big.sqlite3")).unwrap();
    let rows = db.rows("ItemTable").unwrap();
    assert_eq!(rows.len(), 401);
    let text = |v: &Value| match v {
        Value::Text(t) => t.clone(),
        Value::Blob(b) => utf16(b, false),
        other => panic!("{other:?}"),
    };
    assert_eq!(text(&rows[0][0]), "key1");
    assert_eq!(text(&rows[0][1]), "value1");
    assert_eq!(text(&rows[399][1]), "value400");
    assert_eq!(text(&rows[400][0]), "long");
    assert_eq!(text(&rows[400][1]), "x".repeat(3000));
}

#[test]
fn website_data_is_keyed_by_the_identifier() {
    assert_eq!(
        website_data(Path::new("/Users/a"), "com.darkwell.screepub"),
        PathBuf::from("/Users/a/Library/WebKit/com.darkwell.screepub/WebsiteData")
    );
}

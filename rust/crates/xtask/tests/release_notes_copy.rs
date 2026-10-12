//! Release notes stay readable: ported from tests/release-notes.test.ts.
//!
//! The judgment-free half of docs/release-notes-template.md. These terms
//! are wrong in reader-facing copy under every circumstance, so a machine
//! holds the line. Deliberately EXCLUDES "EPUB", "MOBI" and "orphaned
//! lines": the approved notes use all three correctly. The real rule ("name
//! a format only where the reader must act on it") is the template's.

use std::sync::LazyLock;

use regex::Regex;

const BANNED: &[&str] = &[
    "kepub",
    "sideload",
    "ragged-right",
    "keep-together",
    "rendering engine",
    "stylesheet",
    "\u{2014}", // em dash: house rule for user-facing copy
    // release.yml's `checks` job fails a tag whose notes name a checksum,
    // because the workflow appends the real ones; this runs in seconds.
    "sha256",
    "sha-256",
];

/// The other half of that workflow's `grep -niE 'sha-?256|[0-9a-f]{64}'`:
/// a pasted digest with no "sha256" near it. Case-insensitive, like grep -i.
static BARE_DIGEST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[0-9a-f]{64}").expect("regex"));

const WORD_CAP: usize = 350;

/// What release.yml would reject, decided here instead.
fn banned_terms(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut out: Vec<String> = BANNED
        .iter()
        .filter(|t| lower.contains(&t.to_lowercase()))
        .map(|t| t.to_string())
        .collect();
    if BARE_DIGEST.is_match(text) {
        out.push("a bare 64-hex checksum".into());
    }
    out
}

fn release_files() -> Vec<(String, String)> {
    let dir = xtask::paths::repo_root().join("docs/releases");
    let mut files: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("docs/releases")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .map(|p| {
            (
                p.file_name().expect("name").to_string_lossy().into_owned(),
                std::fs::read_to_string(&p).expect("read"),
            )
        })
        .collect();
    files.sort();
    files
}

#[test]
fn there_is_at_least_one_release_note_to_check() {
    // An empty directory would make every other assertion vacuously true.
    assert!(!release_files().is_empty());
}

#[test]
fn every_release_note_uses_no_jargon_from_the_banned_list() {
    for (file, text) in release_files() {
        assert_eq!(banned_terms(&text), Vec::<String>::new(), "{file}");
    }
}

#[test]
fn every_release_note_stays_under_the_word_cap() {
    for (file, text) in release_files() {
        let words = text.split_whitespace().count();
        assert!(words <= WORD_CAP, "{file}: {words} words");
    }
}

#[test]
fn a_raw_64_hex_digest_is_caught_even_with_no_sha256_beside_it() {
    let digest = "3f2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b";
    assert_eq!(digest.len(), 64);
    assert_eq!(
        banned_terms(&format!("Download it and check the digest: {digest}")),
        ["a bare 64-hex checksum"]
    );
    assert_eq!(banned_terms(&digest.to_uppercase()).len(), 1);
}

#[test]
fn ordinary_prose_is_not_a_false_positive() {
    assert!(
        banned_terms("Faster conversion, added dedicated Kobo defaults. 2026-09-13.").is_empty()
    );
}

#[test]
fn the_named_word_list_still_fires() {
    assert_eq!(banned_terms("The SHA256 is below."), ["sha256"]);
}

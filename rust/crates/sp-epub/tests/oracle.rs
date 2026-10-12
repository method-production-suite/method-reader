//! Stage parity against the oracle: for every dump with a book (the
//! committed fixtures in rust/oracle/, and the real scripts in
//! rust/oracle-real/ when this checkout has them), build the EPUB and the
//! preview from tokens.json, the options, the title and author the CLI
//! answered, the .fountain's identifier and the picture folder, then compare
//! with what the TypeScript engine wrote.
//!
//! The EPUB is compared entry by entry (name, order, stored or deflated,
//! contents byte for byte; the pictures are the dump's own files, so their
//! bytes are the decoded pixels' too), and then header by header: every
//! field JSZip writes but the compressed sizes and offsets, since the deflate
//! stream is zlib-rs's, not pako's (src/zip.rs). The preview is compared
//! byte for byte. Real scripts report counts only.

mod common;

use std::path::Path;

use common::{header_fields, oracle_dirs, read_zip};
use serde_json::Value;
use sp_epub::{
    BodyOptions, BookImage, BookMeta, build_epub, content_identifier, tokens_to_body,
    tokens_to_preview_html,
};
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions, Token, resolve_format_options};

struct Built {
    epub: Vec<u8>,
    preview: String,
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("a dump file")).expect("JSON")
}

/// The pictures the CLI wrote beside the .fountain, sorted by name.
fn pictures(dir: &Path) -> Vec<BookImage> {
    let Ok(read) = std::fs::read_dir(dir.join("images")) else {
        return Vec::new();
    };
    let mut out: Vec<BookImage> = read
        .filter_map(Result::ok)
        .map(|e| BookImage {
            name: e.file_name().to_string_lossy().into_owned(),
            bytes: std::fs::read(e.path()).expect("a picture"),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn build(dir: &Path) -> Built {
    let tokens: Vec<Token> =
        serde_json::from_value(read_json(&dir.join("tokens.json"))).expect("tokens");
    let answer = read_json(&dir.join("convert.json"));
    let meta_json = read_json(&dir.join("meta.json"));
    let format: FormatOptions =
        resolve_format_options(meta_json.get("options"), &DEFAULT_FORMAT_OPTIONS);
    let epoch = meta_json["sourceDateEpoch"]
        .as_i64()
        .expect("the dump's clock");
    let fountain = std::fs::read_to_string(dir.join("script.fountain")).expect("the .fountain");
    let meta = BookMeta {
        title: answer["title"].as_str().expect("a title").to_owned(),
        author: answer
            .get("author")
            .and_then(Value::as_str)
            .map(str::to_owned),
        language: None,
        identifier: Some(content_identifier(&fountain)),
    };
    let images = pictures(dir);
    let body = tokens_to_body(
        &tokens,
        &BodyOptions {
            format,
            images: images.iter().map(|i| i.name.clone()).collect(),
            ..BodyOptions::default()
        },
    );
    let epub = build_epub(&meta, &body, &format, &images, epoch).expect("a book");
    let by_name: Vec<(String, Vec<u8>)> = images
        .iter()
        .map(|i| (i.name.clone(), i.bytes.clone()))
        .collect();
    let preview = tokens_to_preview_html(&tokens, &format, &by_name);
    Built { epub, preview }
}

/// What differs between the TypeScript's book and ours, entry by entry, or
/// nothing. `say_names` is false for real scripts: no entry text is shown.
fn compare_epub(ts: &[u8], rust: &[u8], say_text: bool) -> Vec<String> {
    let a = read_zip(ts);
    let b = read_zip(rust);
    let mut out = Vec::new();
    let names = |z: &[common::Entry]| z.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
    if names(&a) != names(&b) {
        out.push(format!("entries: {:?} vs {:?}", names(&a), names(&b)));
        return out;
    }
    for (x, y) in a.iter().zip(&b) {
        if x.stored != y.stored {
            out.push(format!("{}: stored {} vs {}", x.name, x.stored, y.stored));
        }
        if x.data != y.data {
            let at = x
                .data
                .iter()
                .zip(&y.data)
                .position(|(p, q)| p != q)
                .unwrap_or(0);
            if say_text {
                let show = |d: &[u8]| {
                    String::from_utf8_lossy(&d[at.saturating_sub(80)..(at + 80).min(d.len())])
                        .into_owned()
                };
                out.push(format!(
                    "{}: contents differ at byte {at}\n--- TypeScript\n{}\n--- Rust\n{}",
                    x.name,
                    show(&x.data),
                    show(&y.data)
                ));
            } else {
                out.push(format!("{}: contents differ", x.name));
            }
        }
    }
    // The container itself: every header field JSZip writes but the
    // compressed size and offset (the deflate stream is zlib-rs's).
    let (ha, hb) = (header_fields(ts), header_fields(rust));
    if ha != hb {
        let at = ha.iter().zip(&hb).position(|(x, y)| x != y).unwrap_or(0);
        out.push(format!(
            "zip headers differ at entry {at}: {:?} vs {:?}",
            ha.get(at),
            hb.get(at)
        ));
    }
    out
}

fn first_difference(a: &str, b: &str) -> String {
    let at = a
        .bytes()
        .zip(b.bytes())
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    let cut = |s: &str| {
        let lo = s.floor_char_boundary(at.saturating_sub(80));
        let hi = s.ceil_char_boundary((at + 80).min(s.len()));
        s[lo..hi].to_owned()
    };
    format!(
        "at byte {at}\n--- TypeScript\n{}\n--- Rust\n{}",
        cut(a),
        cut(b)
    )
}

#[test]
fn every_committed_fixture_builds_the_oracle_book_byte_for_byte() {
    let dirs = oracle_dirs("oracle");
    assert!(
        dirs.len() >= 5,
        "the committed oracle has a book per fixture"
    );
    let mut failures = Vec::new();
    for dir in &dirs {
        let stem = dir.file_name().unwrap().to_string_lossy().into_owned();
        let built = build(dir);
        // For a look with other tools: SP_EPUB_DUMP=<folder>.
        if let Some(out) = std::env::var_os("SP_EPUB_DUMP") {
            std::fs::write(Path::new(&out).join(format!("{stem}.epub")), &built.epub).unwrap();
        }
        let ts = std::fs::read(dir.join("book.epub")).unwrap();
        for d in compare_epub(&ts, &built.epub, true) {
            failures.push(format!("{stem} book.epub {d}"));
        }
        let preview = std::fs::read_to_string(dir.join("book.preview.html")).unwrap();
        if preview != built.preview {
            failures.push(format!(
                "{stem} book.preview.html {}",
                first_difference(&preview, &built.preview)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn every_real_script_builds_the_oracle_book_byte_for_byte() {
    // rust/oracle-real/ is gitignored and local only (cargo xtask oracle);
    // a checkout without it has nothing to compare, which is not a pass.
    let dirs = oracle_dirs("oracle-real");
    if dirs.is_empty() {
        eprintln!(
            "rust/oracle-real/ has no books here; run `cargo xtask oracle` to compare real scripts"
        );
        return;
    }
    let mut epub_bad = 0;
    let mut preview_bad = 0;
    let mut notes = Vec::new();
    for (n, dir) in dirs.iter().enumerate() {
        let built = build(dir);
        let ts = std::fs::read(dir.join("book.epub")).unwrap();
        let d = compare_epub(&ts, &built.epub, false);
        if !d.is_empty() {
            epub_bad += 1;
            notes.push(format!("real script {}: {}", n + 1, d.join("; ")));
        }
        let preview = std::fs::read_to_string(dir.join("book.preview.html")).unwrap();
        if preview != built.preview {
            preview_bad += 1;
            notes.push(format!("real script {}: the preview differs", n + 1));
        }
    }
    eprintln!(
        "real scripts: {} books, {} EPUBs differ, {} previews differ",
        dirs.len(),
        epub_bad,
        preview_bad
    );
    assert!(notes.is_empty(), "{}", notes.join("\n"));
}

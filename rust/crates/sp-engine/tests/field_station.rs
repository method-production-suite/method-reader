//! The engine's half of tests/field-station.test.ts: the convertPdf title
//! and the walk over the .fountain's `= pg N` pages. The parser's half
//! (scenes, pages, cast, and the same walk over its elements) is
//! sp-parser/tests/field_station.rs.
//!
//! The TypeScript ran the CLI once and read its `--json` answer and its
//! .fountain; here `convert_pdf` is called once, in process, and both tests
//! read that one answer.

mod common;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use sp_engine::convert::{ConvertOptions, ConvertResult, convert_pdf};

fn field_station() -> &'static ConvertResult {
    static ANSWER: OnceLock<ConvertResult> = OnceLock::new();
    ANSWER.get_or_init(|| {
        let pdf = std::fs::read(common::repo_root().join("tests/fixtures/field-station.pdf"))
            .expect("field-station.pdf");
        convert_pdf(pdf, ConvertOptions::default()).expect("converts")
    })
}

/// site/index.html's window.SCENE, read out of the page itself.
fn site_scene() -> Vec<(String, String)> {
    let html = std::fs::read_to_string(common::repo_root().join("site/index.html"))
        .expect("site/index.html");
    let re = Regex::new(r"window\.SCENE = (\[[\s\S]*?\n\]);").unwrap();
    let m = re
        .captures(&html)
        .expect("site/index.html no longer declares window.SCENE");
    // Our own file, and an array literal of string pairs: read its string
    // literals in order and pair them up.
    let strings = string_literals(&m[1]);
    assert_eq!(strings.len() % 2, 0, "window.SCENE is pairs of strings");
    strings
        .chunks(2)
        .map(|p| (p[0].clone(), p[1].clone()))
        .collect()
}

/// Every single- or double-quoted string literal in `src`, in order.
fn string_literals(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = src.chars();
    while let Some(c) = chars.next() {
        if c != '\'' && c != '"' {
            continue;
        }
        let mut s = String::new();
        loop {
            match chars.next().expect("unterminated string literal") {
                '\\' => {
                    let e = chars.next().expect("escape");
                    s.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                }
                q if q == c => break,
                other => s.push(other),
            }
        }
        out.push(s);
    }
    out
}

// The same straightening the generator's ASCII_MAP does, plus the engine's
// own curly quotes back to straight, so the site's text and the book's text
// compare as what a reader sees.
fn plain(s: &str) -> String {
    let s = s
        .replace(['\u{2019}', '\u{2018}'], "'")
        .replace(['\u{201C}', '\u{201D}'], "\"")
        .replace('\u{2014}', "--")
        .replace('\u{2013}', "-")
        .replace('\u{2026}', "...");
    let ws = Regex::new(r"\s+").unwrap();
    ws.replace_all(&s, " ").trim().to_owned()
}

/// JavaScript's `parseInt(s, 10)` for the strings here: the leading digits.
fn parse_int(s: &str) -> Option<u32> {
    let digits: String = s
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// The .fountain split at its `= pg N` markers (`split(/^= pg (\d+)$/m)`),
/// each page's text made plain.
fn fountain_pages(text: &str) -> BTreeMap<u32, String> {
    let marker = Regex::new(r"(?m)^= pg (\d+)$").unwrap();
    let mut out = BTreeMap::new();
    let marks: Vec<_> = marker.captures_iter(text).collect();
    for (i, m) in marks.iter().enumerate() {
        let from = m.get(0).unwrap().end();
        let to = marks
            .get(i + 1)
            .map_or(text.len(), |n| n.get(0).unwrap().start());
        out.insert(m[1].parse().expect("a page number"), plain(&text[from..to]));
    }
    out
}

#[test]
fn the_engine_reads_it_as_a_feature_with_the_invented_title() {
    // The counts are the parser's (sp-parser/tests/field_station.rs); the
    // title is the engine's, read off the .fountain's title page.
    let answer = field_station();
    assert_eq!(answer.title.to_uppercase(), "FIELD STATION");
    let parsed = answer.screenplay.as_ref().expect("a PDF's parse");
    assert_eq!(parsed.scenes.len(), 11);
    assert_eq!(parsed.page_count, 19);
    assert_eq!(parsed.characters.len(), 4);
}

// What this proves: every row of the site's scene is in the book, in the
// site's order, on the printed page the site's own marker puts it on, and
// every speech is said by the character the site gives it to. The book is
// split at its `= pg N` page markers (registry 13a); walking SCENE, a `pg`
// row switches page, a cue is looked for together with its speech as
// `@CUE speech`, and each match must come after the one before it on that
// page. So a line that moves page, changes speaker, or swaps order fails,
// as does the site renumbering its pages or losing its scene.
#[test]
fn the_sites_scene_is_in_the_book_word_for_word_on_the_pages_the_site_says() {
    let pages = fountain_pages(&field_station().fountain_text);

    let scene = site_scene();
    assert_eq!(
        scene
            .iter()
            .filter(|(k, _)| k == "pg")
            .map(|(_, t)| parse_int(t).expect("a page number"))
            .collect::<Vec<_>>(),
        [14, 15, 16, 17, 18]
    );
    // Not a vacuous walk: the site's scene is five pages of rows, not a stub.
    assert!(scene.iter().filter(|(k, _)| k != "pg").count() >= 40);

    let mut page = 0;
    let mut from = 0;
    for (i, (kind, text)) in scene.iter().enumerate() {
        if kind == "pg" {
            page = parse_int(text).expect("a page number");
            from = 0;
            continue;
        }
        if kind == "d" {
            continue; // checked with its cue
        }
        let want = if kind == "c" {
            format!("@{} {}", plain(text), plain(&scene[i + 1].1))
        } else {
            plain(text)
        };
        let book = pages.get(&page).map_or("", String::as_str);
        let at = book
            .get(from..)
            .and_then(|rest| rest.find(&want))
            .map(|at| at + from);
        assert!(at.is_some(), "page {page}: {want}");
        from = at.unwrap() + want.len();
    }
}

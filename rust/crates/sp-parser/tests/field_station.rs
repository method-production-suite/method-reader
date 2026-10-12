//! Port of tests/field-station.test.ts.
//!
//! tests/fixtures/field-station.pdf is the invented feature every README and
//! site picture shows. Two things must hold for the pictures to be honest:
//! the engine reads it as a real screenplay (so the result screen's counts
//! and the scene index look like a feature), and the scene the SITE draws
//! really is in the book, word for word, on the pages the site says, so the
//! site's before-and-after and the window's pictures show one story.
//!
//! The TS file ran the CLI once and read its `--json` answer and its
//! .fountain. Here the parse comes from the oracle's dump of the PDF
//! (`common::parse`), and each test ports what the parser owns:
//!
//! MOVED: "the engine reads it as a feature, with the invented title and
//! cast": `ok` and `title` need sp-engine convertPdf (the title is read off
//! the .fountain's title page by fountain-js tokens), and port with that
//! crate. The counts (`pages`, `scenes`, `characters`, `topCharacters`) are
//! the parser's and port here, with the title checked as the parser's own
//! title-page element.
//!
//! MOVED: "the site's scene is in the book, word for word, on the pages the
//! site says": the walk over the .fountain's `= pg N` sections and `@CUE
//! speech` text needs sp-fountain toFountain, and ports with that crate. The
//! checks on site/index.html's own scene port as they stand, and the same
//! walk runs here over the parser's elements, split at the printed page mark
//! the parser reads on each page (the element the `= pg N` marker is written
//! from) with each cue joined to its speech as `@CUE speech`.

mod common;

use std::collections::BTreeMap;

use regex::Regex;
use sp_model::{ElementType, ParsedScreenplay};

/// site/index.html's window.SCENE, read out of the page itself.
fn site_scene() -> Vec<(String, String)> {
    let html = std::fs::read_to_string(common::rust_dir().join("../site/index.html"))
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

/// The book's text per printed page, from the parser's elements: each PDF
/// page is labelled by the printed page mark the parser reads on it, a cue
/// is written `@CUE` before its speech, and page marks and title-page
/// elements are left out, as the .fountain leaves them out of the text.
fn pages_from_elements(parsed: &ParsedScreenplay) -> BTreeMap<u32, String> {
    let mut by_pdf_page: BTreeMap<u32, (Option<u32>, Vec<String>)> = BTreeMap::new();
    for e in &parsed.elements {
        let entry = by_pdf_page.entry(e.page_num).or_default();
        if e.kind == ElementType::PageNumber {
            if entry.0.is_none() {
                entry.0 = parse_int(&e.text);
            }
            continue;
        }
        if e.is_title_page {
            continue;
        }
        if e.kind == ElementType::Character {
            entry.1.push(format!("@{}", e.text));
        } else {
            entry.1.push(e.text.clone());
        }
    }
    by_pdf_page
        .into_values()
        .filter_map(|(label, texts)| label.map(|n| (n, plain(&texts.join("\n")))))
        .collect()
}

mod field_station {
    use super::{common, pages_from_elements, parse_int, plain, site_scene};

    #[test]
    fn the_engine_reads_it_as_a_feature_with_the_invented_title_and_cast() {
        let parsed = common::parse("field-station");
        // `answer.ok` and `answer.title` are MOVED (see the file header); the
        // title as the parser reads it is a title-page element on page 1.
        assert!(
            parsed.elements.iter().any(|e| e.is_title_page
                && e.page_num == 1
                && e.text.to_uppercase() == "FIELD STATION"),
            "no FIELD STATION title on the title page"
        );
        assert_eq!(parsed.scenes.len(), 11);
        assert_eq!(parsed.page_count, 19);
        assert_eq!(parsed.characters.len(), 4);
        // The CLI's topCharacters: the first five of the parser's characters.
        let mut top: Vec<String> = parsed
            .characters
            .iter()
            .take(5)
            .map(|c| c.name.clone())
            .collect();
        top.sort();
        assert_eq!(top, ["DELACROIX", "IVERSEN", "MARA", "TEO"]);
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
        let pages = pages_from_elements(&common::parse("field-station"));

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
}

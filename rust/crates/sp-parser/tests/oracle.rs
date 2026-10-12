//! Stage parity against the TypeScript engine's own answers.
//!
//! For every dump in rust/oracle/ (committed) and rust/oracle-real/ (local,
//! gitignored, real scripts named by number only):
//!
//! 1. `pages.json` reads into [`DocumentFacts`] and writes back unchanged,
//!    which proves the input shape sp-pdf has to produce.
//! 2. `pages.json` → `lines.json`: [`extract_document`] gives the same lines,
//!    page count, repair count, picture placements and picture statistics.
//! 3. `lines.json` → `elements.json`: [`parse_lines`] gives the same
//!    elements, characters, scenes and page count.
//! 4. Both stages chained, from `pages.json` to `elements.json`.
//!
//! Pictures: the dump names each picture's source pixels by hash rather than
//! carrying them, so these checks judge pictures on their facts (see
//! images.rs, `facts_hash`). Where the dump carries its `pixels/` folder
//! (`<dataSha256>.bin`, as tools/oracle writes it), or `SP_PARSER_PIXELS`
//! names one, the pictures are decoded, encoded and compared by DECODED
//! PIXELS with the oracle's `images/` folder too (rust/PARITY.md, Decisions).

mod common;

use std::path::Path;

use serde_json::Value;
use sp_model::{ParsedScreenplay, RawImage, RawLine};
use sp_parser::extract::{ExtractResult, extract_document};
use sp_parser::facts::DocumentFacts;
use sp_parser::parse_lines;

use common::{decode_png, dumps, first_diff, read_json};

/// Fill in every picture's pixels from `<dir>/<dataSha256>.bin`. Keyed by
/// the hash, not the object id, because pdf.js numbers its document-wide
/// ids per process (`g_d0_...` in one run, `g_d4_...` in another). A
/// picture whose pixels are missing fails the whole check rather than
/// falling back to its facts, so the two modes never mix in one document.
fn load_pixels(facts: &mut DocumentFacts, dir: &Path) -> usize {
    let mut loaded = 0;
    for page in &mut facts.pages {
        for img in page.images.values_mut().flatten() {
            let Some(sha) = &img.data_sha256 else {
                continue;
            };
            let bytes = std::fs::read(dir.join(format!("{sha}.bin")))
                .unwrap_or_else(|_| panic!("SP_PARSER_PIXELS has no pixels for {sha}"));
            img.data = Some(bytes);
            loaded += 1;
        }
    }
    loaded
}

fn images_from_dump(lines: &Value) -> Vec<RawImage> {
    lines["images"]
        .as_array()
        .expect("lines.json images")
        .iter()
        .map(|v| {
            let mut v = v.clone();
            v.as_object_mut().expect("image object").remove("sha256");
            serde_json::from_value(v).expect("a RawImage")
        })
        .collect()
}

fn check_lines(label: &str, ours: &ExtractResult, want: &Value, pictures: Option<&Path>) {
    assert_eq!(
        Value::from(ours.page_count),
        want["pageCount"],
        "{label}: pageCount"
    );
    assert_eq!(
        Value::from(ours.spacing_repairs),
        want["spacingRepairs"],
        "{label}: spacingRepairs"
    );
    let want_lines = want["lines"].as_array().expect("lines");
    assert_eq!(ours.lines.len(), want_lines.len(), "{label}: line count");
    for (i, (a, b)) in ours.lines.iter().zip(want_lines).enumerate() {
        let a = serde_json::to_value(a).unwrap();
        assert!(a == *b, "{label}: line {i} differs\n ours: {a}\n want: {b}");
    }
    assert_eq!(
        serde_json::to_value(&ours.image_stats).unwrap(),
        want["imageStats"],
        "{label}: imageStats"
    );
    let want_images = want["images"].as_array().expect("images");
    assert_eq!(
        ours.images.len(),
        want_images.len(),
        "{label}: picture count"
    );
    for (a, b) in ours.images.iter().zip(want_images) {
        let mut b = b.clone();
        b.as_object_mut().unwrap().remove("sha256");
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            b,
            "{label}: picture {}",
            a.file
        );
        if let Some(dir) = pictures
            && !a.bytes.is_empty()
        {
            let theirs = std::fs::read(dir.join(&a.file)).expect("the oracle's picture");
            assert_eq!(
                decode_png(&a.bytes),
                decode_png(&theirs),
                "{label}: picture {} differs in its pixels",
                a.file
            );
        }
    }
}

fn check_elements(label: &str, ours: &ParsedScreenplay, want: &Value) {
    let ours = serde_json::to_value(ours).unwrap();
    let (oe, we) = (
        ours["elements"].as_array().unwrap(),
        want["elements"].as_array().unwrap(),
    );
    for (i, (a, b)) in oe.iter().zip(we).enumerate() {
        assert!(
            a == b,
            "{label}: element {i} differs\n ours: {a}\n want: {b}"
        );
    }
    assert_eq!(oe.len(), we.len(), "{label}: element count");
    assert!(
        ours["characters"] == want["characters"],
        "{label}: characters differ"
    );
    assert!(ours["scenes"] == want["scenes"], "{label}: scenes differ");
    assert_eq!(ours["pageCount"], want["pageCount"], "{label}: pageCount");
}

/// Every stage for one dump; returns how many stages it compared.
fn check_dump(label: &str, dir: &Path) -> usize {
    let mut stages = 0;
    let Some(pages) = read_json(&dir.join("pages.json")) else {
        return 0;
    };
    let mut facts: DocumentFacts = serde_json::from_value(pages.clone())
        .unwrap_or_else(|e| panic!("{label}: pages.json does not read into the facts: {e}"));
    let back = serde_json::to_value(&facts).unwrap();
    assert!(
        back == pages,
        "{label}: pages.json does not write back as the same JSON: {}",
        first_diff(&back, &pages, "").unwrap_or_default()
    );

    // The dump's own pixels/ folder (tools/oracle writes it), or a folder
    // named by SP_PARSER_PIXELS.
    let pixels = std::env::var_os("SP_PARSER_PIXELS")
        .map(std::path::PathBuf::from)
        .or_else(|| Some(dir.join("pixels")).filter(|p| p.is_dir()));
    let pictures = match &pixels {
        // A refused document writes no picture folder, so its pictures are
        // judged (the statistics still compare) but not compared as PNGs.
        Some(p) if load_pixels(&mut facts, p) > 0 && dir.join("images").is_dir() => {
            Some(dir.join("images"))
        }
        _ => None,
    };

    let want_lines = read_json(&dir.join("lines.json")).expect("lines.json beside pages.json");
    let ours = extract_document(&facts, None, None);
    check_lines(label, &ours, &want_lines, pictures.as_deref());
    stages += 1;

    if let Some(want) = read_json(&dir.join("elements.json")) {
        // lines.json → elements.json, from the oracle's own lines.
        let lines: Vec<RawLine> = serde_json::from_value(want_lines["lines"].clone()).unwrap();
        let images = images_from_dump(&want_lines);
        check_elements(label, &parse_lines(&lines, &images), &want);
        stages += 1;
        // And chained, from our own extraction.
        check_elements(
            &format!("{label} (chained)"),
            &parse_lines(&ours.lines, &ours.images),
            &want,
        );
        stages += 1;
    }
    stages
}

#[test]
fn committed_fixtures_match_the_oracle_at_both_stages() {
    let mut fixtures = 0;
    let mut stages = 0;
    for dir in dumps("oracle") {
        let label = dir.file_name().unwrap().to_string_lossy().into_owned();
        let n = check_dump(&label, &dir);
        if n > 0 {
            fixtures += 1;
            stages += n;
        }
    }
    // 7 fixtures: 5 convert (three stages each), 2 are refused before
    // parsing (lines only).
    assert_eq!(
        fixtures, 7,
        "expected the seven committed dumps in rust/oracle/"
    );
    assert_eq!(stages, 17);
}

#[test]
fn real_scripts_match_the_oracle_at_both_stages() {
    // Local only (rust/oracle-real/ is gitignored); named by number, never by
    // title.
    let mut checked = 0;
    for (i, dir) in dumps("oracle-real").iter().enumerate() {
        if check_dump(&format!("real script {}", i + 1), dir) > 0 {
            checked += 1;
        }
    }
    eprintln!("real scripts checked: {checked}");
}

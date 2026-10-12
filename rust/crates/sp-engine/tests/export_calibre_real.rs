//! The two checks of tests/export-calibre.test.ts that run REAL Calibre on
//! the engine's own EPUB: toKepub's koboSpan, and the dialogue column
//! surviving an exploded AZW3. A test binary of their own, so the Calibre
//! runs never share the machine with export_calibre.rs's timing checks
//! (cargo runs test binaries one at a time).

mod common;

use common::Scratch;
use sp_engine::export::calibre::{
    CALIBRE_FORMAT_GUARDS, CONVERSION_TIMEOUT_MS, calibre_tool, run_calibre, to_kepub,
};

// CALIBRE SPAWNS: ebook-convert (and calibre-debug -x), only ever with
// common::calibre_env, whose CALIBRE_CONFIG_DIRECTORY is a scratch COPY of
// the real folder; each test also proves the real folder did not change.
// Both skip, saying so, where Calibre is not installed.

/// Every file under `dir` (caches left out, as the copy leaves them), with
/// its size and time, so a test can prove the real folder was not touched.
fn snapshot(
    dir: &std::path::Path,
) -> std::collections::BTreeMap<std::path::PathBuf, (u64, Option<std::time::SystemTime>)> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in read.flatten() {
        let p = e.path();
        if p.file_name().is_some_and(|n| n == "caches") {
            continue;
        }
        if let Ok(m) = std::fs::symlink_metadata(&p) {
            if m.is_dir() {
                out.extend(snapshot(&p));
            } else {
                out.insert(p, (m.len(), m.modified().ok()));
            }
        }
    }
    out
}

/// The engine's EPUB of `text`, written into `dir` as book.epub.
fn engine_epub(dir: &Scratch, text: &str) -> String {
    let epub = dir.join("book.epub");
    let book = sp_engine::convert::convert_fountain(
        text,
        sp_engine::convert::ConvertOptions::default(),
        None,
    )
    .unwrap()
    .epub;
    std::fs::write(&epub, book).unwrap();
    epub
}

#[test]
fn to_kepub_names_its_output_kepub_epub_and_emits_kobo_span_markup() {
    let env = common::calibre_env();
    if calibre_tool("ebook-convert", &env).is_none() {
        eprintln!("skipped: Calibre's ebook-convert was not found");
        return;
    }
    let real = common::real_calibre_config();
    let before = snapshot(&real);
    let s = Scratch::new("kepub");
    let epub = engine_epub(&s, "Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n");
    let out = to_kepub(&epub, &env).unwrap();
    assert!(out.ends_with(".kepub.epub"));
    // The filename alone proves nothing: koboSpan is the one marker the
    // broken single-step form lacks, and it is inside deflated entries.
    let book = std::fs::read(&out).unwrap();
    assert!(
        common::epub::read_zip(&book)
            .iter()
            .filter(|e| !e.dir)
            .any(|e| String::from_utf8_lossy(&e.data).contains("koboSpan"))
    );
    assert_eq!(snapshot(&real), before, "the real Calibre folder changed");
}

/// Every file under `dir`, recursively.
fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(files_under(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// The dialogue column, read back out of a REAL AZW3 that Calibre made from
/// our EPUB and `calibre-debug -x` exploded: an OUTCOME check that the
/// column keeps its side margins through Calibre's reading of the book (the
/// first half of every KFX build), whatever Calibre does with the flag.
#[test]
fn calibres_reading_of_our_epub_keeps_the_dialogue_columns_side_margins() {
    let env = common::calibre_env();
    let (Some(convert), Some(debug)) = (
        calibre_tool("ebook-convert", &env),
        calibre_tool("calibre-debug", &env),
    ) else {
        eprintln!("skipped: Calibre's ebook-convert or calibre-debug was not found");
        return;
    };
    let real = common::real_calibre_config();
    let before = snapshot(&real);
    let s = Scratch::new("azw3-geometry");
    let speech = "JO\nEnough dialogue that the fake-margin heuristic sees a screenplay.\n\n";
    let epub = engine_epub(
        &s,
        &format!(
            "Title: Test\n\nINT. ROOM - DAY\n\nA hallway.\n\n{}",
            speech.repeat(24)
        ),
    );
    let azw3 = s.join("book.azw3");
    let mut args = vec![epub, azw3.clone()];
    args.extend(CALIBRE_FORMAT_GUARDS.iter().map(|g| (*g).to_owned()));
    run_calibre(&convert, &args, &env, CONVERSION_TIMEOUT_MS).unwrap();
    let exploded = s.join("exploded");
    run_calibre(
        &debug,
        &["-x".to_owned(), azw3, exploded.clone()],
        &env,
        CONVERSION_TIMEOUT_MS,
    )
    .unwrap();

    let files = files_under(std::path::Path::new(&exploded));
    let page_re = regex::Regex::new(r"(?i)\.x?html?$").unwrap();
    let pages: Vec<&std::path::PathBuf> = files
        .iter()
        .filter(|f| {
            page_re.is_match(&f.to_string_lossy())
                && std::fs::read_to_string(f).is_ok_and(|t| t.contains("dialogue-block"))
        })
        .collect();
    assert!(
        !pages.is_empty(),
        "no exploded content file mentions dialogue-block: {files:?}"
    );
    // Only stylesheets the page links, as the device would apply them.
    let page = std::fs::read_to_string(pages[0]).unwrap();
    let rules: Vec<String> = files
        .iter()
        .filter(|f| {
            f.extension().is_some_and(|e| e == "css")
                && page.contains(&*f.file_name().unwrap().to_string_lossy())
        })
        .filter_map(|f| {
            let css = std::fs::read_to_string(f).ok()?;
            let after = &css[css.find(".dialogue-block")?..];
            Some(after[..=after.find('}')?].to_owned())
        })
        .collect();
    assert!(
        !rules.is_empty(),
        "no linked stylesheet carries a .dialogue-block rule"
    );
    let margin = format!(
        "{}%",
        sp_model::DEFAULT_FORMAT_OPTIONS.dialogue_side_margin_pct
    );
    assert!(
        rules.join(" ").contains(&margin),
        "the dialogue column lost its {margin} side margins"
    );
    assert_eq!(snapshot(&real), before, "the real Calibre folder changed");
}

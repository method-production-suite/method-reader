//! The real engine behind the window: one conversion of the invented demo
//! script, in-process, exactly as the window asks for it (`Request::Convert`
//! is `<pdf> --json --progress --preview-inline --library`).
//!
//! The environment points at scratch folders: METHOD_READER_CONFIG_DIR
//! (never the real settings), METHOD_READER_LIBRARY (never the real
//! library), the same under their old SCREEPUB_ names the engine falls
//! back to, and a
//! Calibre guard path, so a Calibre started by mistake fails instead of
//! reaching the real one. The workspace forbids unsafe code, so the test
//! cannot set its own environment: it runs itself again as a child with
//! that environment, and the child does the work.

use serde_json::Value;
use sp_app::engine::{Engine, Request};

const CHILD: &str = "SP_APP_DIRECT_ENGINE_CHILD";

#[test]
fn a_conversion_streams_its_progress_and_answers_with_the_book() {
    let scratch = tempfile::tempdir().expect("scratch");
    let config = scratch.path().join("config");
    let library = scratch.path().join("library");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_converts", "--nocapture"])
        .env(CHILD, "1")
        .env("METHOD_READER_CONFIG_DIR", &config)
        .env("METHOD_READER_LIBRARY", &library)
        .env("SCREEPUB_CONFIG_DIR", &config)
        .env("SCREEPUB_LIBRARY", &library)
        .env(
            "CALIBRE_CONFIG_DIRECTORY",
            "/dev/null/method-reader-test-guard",
        )
        .output()
        .expect("the child runs");
    let said =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{said}");
    assert!(said.contains("1 passed"), "the child really ran: {said}");
}

#[test]
fn child_converts() {
    if std::env::var(CHILD).is_err() {
        return;
    }
    let library = std::env::var("METHOD_READER_LIBRARY").unwrap();
    let pdf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/field-station.pdf")
        .canonicalize()
        .expect("the committed fixture");
    let engine = sp_app::direct::DirectEngine::new();
    let mut lines: Vec<Value> = vec![];
    let reply = engine.run(
        &Request::Convert {
            path: pdf.to_string_lossy().into_owned(),
            force: false,
            options: None,
        },
        &mut |line| lines.push(line),
    );
    let answer = reply.answer.expect("an answer");
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["title"], "Field Station");
    let epub = answer["epubPath"].as_str().unwrap();
    assert!(epub.starts_with(&library), "{epub}");
    assert!(std::path::Path::new(epub).exists());
    assert!(
        answer["previewHtml"]
            .as_str()
            .unwrap()
            .contains("scene-heading"),
        "the book's pages ride in the answer"
    );

    // The page reached, and the first page for the page show.
    let first = lines
        .iter()
        .find(|l| l["stage"] == "page")
        .expect("the first page");
    assert!(first["page"].as_u64().is_some_and(|p| p >= 1), "{first}");
    assert!(!first["paragraphs"].as_array().unwrap().is_empty());
    assert!(
        lines
            .iter()
            .any(|l| l["stage"] == "parse" && l["page"].is_number()),
        "{lines:?}"
    );
    assert!(lines.iter().any(|l| l["stage"] == "render"));
    assert_eq!(
        engine.in_flight().load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

const SAMPLE_CHILD: &str = "SP_APP_DIRECT_ENGINE_SAMPLE_CHILD";

/// The sample, as the window converts it (src/app/sample.rs): the built-in
/// PDF into a folder of its own with `-o`, then remade from its
/// `.fountain`, and the library left empty.
#[test]
fn the_sample_converts_into_its_own_folder_and_reopens_from_its_fountain() {
    let scratch = tempfile::tempdir().expect("scratch");
    let config = scratch.path().join("config");
    let library = scratch.path().join("library");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_converts_the_sample", "--nocapture"])
        .env(SAMPLE_CHILD, "1")
        .env("METHOD_READER_CONFIG_DIR", &config)
        .env("METHOD_READER_LIBRARY", &library)
        .env("SCREEPUB_CONFIG_DIR", &config)
        .env("SCREEPUB_LIBRARY", &library)
        .env(
            "CALIBRE_CONFIG_DIRECTORY",
            "/dev/null/method-reader-test-guard",
        )
        .output()
        .expect("the child runs");
    let said =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{said}");
    assert!(said.contains("1 passed"), "the child really ran: {said}");
    assert_eq!(
        std::fs::read_dir(&library).unwrap().count(),
        0,
        "nothing in the library"
    );
}

#[test]
fn child_converts_the_sample() {
    if std::env::var(SAMPLE_CHILD).is_err() {
        return;
    }
    use sp_app::model::sample::{FOLDER, PDF, Paths};
    let config = std::env::var("METHOD_READER_CONFIG_DIR").unwrap();
    let p = Paths::in_folder(&std::path::Path::new(&config).join(FOLDER));
    std::fs::create_dir_all(&p.dir).unwrap();
    std::fs::write(&p.pdf, PDF).unwrap();
    let engine = sp_app::direct::DirectEngine::new();
    let mut lines: Vec<Value> = vec![];
    let answer = engine
        .run(
            &Request::ConvertTo {
                path: p.pdf.clone(),
                out: p.epub.clone(),
            },
            &mut |line| lines.push(line),
        )
        .answer
        .expect("an answer");
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["title"], "Field Station");
    assert_eq!(answer["epubPath"], p.epub.as_str());
    assert_eq!(answer["fountainPath"], p.fountain.as_str());
    assert!(answer["pages"].as_u64().is_some());
    assert!(
        answer["previewHtml"]
            .as_str()
            .unwrap()
            .contains("scene-heading")
    );
    assert!(lines.iter().any(|l| l["stage"] == "render"));
    assert!(std::path::Path::new(&p.fountain).is_file());

    let again = engine
        .run(
            &Request::ConvertTo {
                path: p.fountain.clone(),
                out: p.epub.clone(),
            },
            &mut |_| {},
        )
        .answer
        .expect("an answer");
    assert_eq!(again["ok"], true, "{again}");
    assert_eq!(again["epubPath"], p.epub.as_str());
    assert_eq!(again["previewHtml"], answer["previewHtml"], "the same book");
}

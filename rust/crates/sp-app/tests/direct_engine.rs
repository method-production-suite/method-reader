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

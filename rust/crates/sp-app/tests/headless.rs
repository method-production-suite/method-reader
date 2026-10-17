//! The window binary with no window: what xtask smoke-bundle runs inside a
//! built app. Scratch settings, and a Calibre guard path.

use std::process::Command;

use serde_json::Value;

fn window(scratch: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_method-reader-app"));
    // Both names, the new and the old one the engine still falls back to,
    // so no run can reach the real settings or library.
    c.env("METHOD_READER_CONFIG_DIR", scratch.join("config"))
        .env("METHOD_READER_LIBRARY", scratch.join("library"))
        .env("SCREEPUB_CONFIG_DIR", scratch.join("config"))
        .env("SCREEPUB_LIBRARY", scratch.join("library"))
        .env(
            "CALIBRE_CONFIG_DIRECTORY",
            "/dev/null/method-reader-test-guard",
        );
    c
}

fn json(out: &std::process::Output) -> Value {
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.trim()).unwrap_or_else(|_| panic!("one JSON object: {text}"))
}

#[test]
fn version_json_is_the_clis() {
    let scratch = tempfile::tempdir().unwrap();
    let out = window(scratch.path())
        .args(["--version", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        json(&out),
        serde_json::json!({"ok": true, "version": env!("CARGO_PKG_VERSION")})
    );
}

#[test]
fn smoke_converts_one_pdf_and_prints_the_result() {
    let scratch = tempfile::tempdir().unwrap();
    let epub = scratch.path().join("field-station.epub");
    let pdf = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../tests/fixtures/field-station.pdf"
    );
    let out = window(scratch.path())
        .args(["--smoke", pdf, "--out", epub.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let answer = json(&out);
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["title"], "Field Station");
    assert_eq!(answer["epubPath"], epub.to_str().unwrap());
    assert!(epub.exists());
}

#[test]
fn a_smoke_that_cannot_read_says_so_in_json() {
    let scratch = tempfile::tempdir().unwrap();
    let out = window(scratch.path())
        .args([
            "--smoke",
            "/nowhere/missing.pdf",
            "--out",
            scratch.path().join("x.epub").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(json(&out)["ok"], false);
}

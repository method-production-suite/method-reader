//! Ports tests/smoke-cli.test.ts: `cargo xtask smoke-cli`, every check on
//! a fake runner.

mod cli_build_common;

use std::cell::RefCell;

use cli_build_common::{Scratch, strings};
use serde_json::{Value, json};
use xtask::cli_build::RunResult;
use xtask::cli_build::smoke::{
    check_convert_result, check_devices, check_epub_bytes, check_jpx_picture, check_version,
    smoke_cli, sole_json,
};

fn ok(stdout: &str) -> RunResult {
    RunResult::ok(stdout, "")
}

// ── sole_json ───────────────────────────────────────────────────────────

#[test]
fn accepts_exactly_one_json_object() {
    assert_eq!(
        sole_json("{\"ok\":true}\n", "x").unwrap(),
        json!({"ok": true})
    );
}

#[test]
fn rejects_two_objects_which_is_the_json_contract_breaking() {
    let err = sole_json("{\"ok\":true}\n{\"ok\":false}\n", "convert").unwrap_err();
    assert!(err.contains("2 lines"), "{err}");
}

#[test]
fn rejects_empty_output_and_non_json() {
    assert!(sole_json("", "convert").unwrap_err().contains("0 lines"));
    assert!(
        sole_json("converting...\n", "convert")
            .unwrap_err()
            .contains("parseable JSON")
    );
}

// ── check_version ───────────────────────────────────────────────────────

#[test]
fn passes_only_on_the_exact_expected_line() {
    assert_eq!(
        check_version(&ok("method-reader 0.6.0 (Method Reader)\n"), "0.6.0"),
        Ok(())
    );
}

#[test]
fn a_stale_binary_reporting_the_old_version_fails() {
    assert!(
        check_version(&ok("method-reader 0.5.4 (Method Reader)\n"), "0.6.0")
            .unwrap_err()
            .contains("0.5.4")
    );
    // A substring match must not save it.
    assert!(check_version(&ok("method-reader 0.6.0-dirty (Method Reader)\n"), "0.6.0").is_err());
    // The 0.12 line, with no product name after the version, is not it.
    assert!(check_version(&ok("method-reader 0.6.0\n"), "0.6.0").is_err());
    let crashed = RunResult::failed(1, "boom");
    assert!(
        check_version(&crashed, "0.6.0")
            .unwrap_err()
            .contains("exited 1")
    );
}

// ── check_convert_result ────────────────────────────────────────────────

fn good() -> RunResult {
    ok("{\"ok\":true,\"epubPath\":\"/tmp/smoke.epub\",\"pages\":97}\n")
}

#[test]
fn passes_on_a_real_success_payload() {
    assert!(check_convert_result(&good(), "/tmp/smoke.epub").is_ok());
}

#[test]
fn a_reported_failure_fails_even_at_exit_0() {
    let failed = ok("{\"ok\":false,\"error\":{\"code\":\"not-screenplay\",\"message\":\"x\"}}\n");
    let err = check_convert_result(&failed, "/tmp/smoke.epub").unwrap_err();
    assert!(err.contains("not-screenplay"), "{err}");
}

#[test]
fn zero_pages_fails_an_empty_book_is_not_a_conversion() {
    let empty = ok("{\"ok\":true,\"epubPath\":\"/tmp/smoke.epub\",\"pages\":0}\n");
    assert!(
        check_convert_result(&empty, "/tmp/smoke.epub")
            .unwrap_err()
            .contains("pages")
    );
    let none = ok("{\"ok\":true,\"epubPath\":\"/tmp/smoke.epub\"}\n");
    assert!(
        check_convert_result(&none, "/tmp/smoke.epub")
            .unwrap_err()
            .contains("pages")
    );
}

#[test]
fn writing_somewhere_other_than_where_we_asked_fails() {
    let err = check_convert_result(&good(), "/tmp/other.epub").unwrap_err();
    assert!(err.contains("other.epub"), "{err}");
}

#[test]
fn a_non_zero_exit_fails_and_carries_stderr() {
    let crashed = RunResult::failed(134, "Segmentation fault");
    let err = check_convert_result(&crashed, "/tmp/smoke.epub").unwrap_err();
    assert!(err.contains("134") && err.contains("Segmentation"), "{err}");
}

#[test]
fn hands_back_the_answer_it_vetted_for_the_next_question_about_it() {
    assert_eq!(
        check_convert_result(&good(), "/tmp/smoke.epub").unwrap(),
        json!({"ok": true, "epubPath": "/tmp/smoke.epub", "pages": 97})
    );
}

// ── check_jpx_picture ───────────────────────────────────────────────────

#[test]
fn one_picture_carried_is_a_pass() {
    assert!(check_jpx_picture(&json!({"ok": true, "imageCount": 1, "warnings": []})).is_ok());
}

#[test]
fn a_decoder_that_did_not_load_fails_though_the_conversion_succeeded() {
    let lost = json!({
        "ok": true,
        "warnings": [
            "1 picture could not be read out of the PDF, so it was left out.",
            "Screepub could not unpack its picture decoders, so pictures stored as JPEG2000 or JBIG2 could not be read."
        ]
    });
    let err = check_jpx_picture(&lost).unwrap_err();
    assert!(
        err.contains("JPEG 2000 fixture carried no picture"),
        "{err}"
    );
    assert!(
        err.contains("could not unpack its picture decoders"),
        "{err}"
    );
}

#[test]
fn any_count_but_exactly_one_fails_the_fixture_holds_one_picture() {
    assert!(
        check_jpx_picture(&json!({"ok": true, "imageCount": 0}))
            .unwrap_err()
            .contains("carried 0")
    );
    assert!(
        check_jpx_picture(&json!({"ok": true, "imageCount": 2}))
            .unwrap_err()
            .contains("carried 2")
    );
    assert!(check_jpx_picture(&json!({"ok": true, "imageCount": "1"})).is_err());
}

// ── smoke_cli ───────────────────────────────────────────────────────────

struct Stage {
    scratch: Scratch,
}

impl Stage {
    /// A stand-in binary and fixtures: smoke_cli only checks the paths
    /// exist, and the fake runner answers in the binary's place.
    fn new() -> Self {
        let scratch = Scratch::new("smoke-cli");
        for name in ["screepub", "screenplay.pdf", "jpx.pdf"] {
            std::fs::write(scratch.join(name), "").unwrap();
        }
        Self { scratch }
    }
}

/// Answers every command the way a working binary would, except that the
/// JPEG 2000 conversion adds `jpx_answer`; writes a zip header wherever -o
/// points, and records every argv.
fn fake_binary<'a>(
    jpx: String,
    jpx_answer: Value,
    calls: &'a RefCell<Vec<Vec<String>>>,
) -> impl Fn(&[String]) -> RunResult + 'a {
    move |argv: &[String]| {
        calls.borrow_mut().push(argv.to_vec());
        if argv[1] == "--version" {
            return ok("method-reader 0.6.0 (Method Reader)\n");
        }
        if argv[1] == "devices" {
            return ok("{\"ok\":true,\"devices\":[]}\n");
        }
        let epub = &argv[argv.iter().position(|a| a == "-o").unwrap() + 1];
        std::fs::write(epub, [0x50, 0x4b, 0x03, 0x04]).unwrap();
        let mut answer = json!({"ok": true, "epubPath": epub, "pages": 3});
        if argv[1] == jpx
            && let Value::Object(extra) = &jpx_answer
        {
            for (k, v) in extra {
                answer[k] = v.clone();
            }
        }
        ok(&format!("{answer}\n"))
    }
}

#[test]
fn converts_the_jpeg_2000_fixture_as_well_as_the_script_and_passes_on_one_picture() {
    let s = Stage::new();
    let work = Scratch::new("smoke-pass");
    let jpx = s.scratch.join("jpx.pdf").display().to_string();
    let calls = RefCell::new(Vec::new());
    let run = fake_binary(jpx.clone(), json!({"imageCount": 1}), &calls);
    smoke_cli(
        &s.scratch.join("screepub"),
        &s.scratch.join("screenplay.pdf"),
        &s.scratch.join("jpx.pdf"),
        work.path(),
        "0.6.0",
        &run,
    )
    .unwrap();
    let calls = calls.borrow();
    let jpx_call = calls.iter().find(|c| c[1] == jpx).unwrap();
    assert_eq!(
        *jpx_call,
        strings(&[
            &s.scratch.join("screepub").display().to_string(),
            &jpx,
            "-o",
            &work.join("smoke-jpx.epub").display().to_string(),
            "--no-fountain",
            "--json"
        ])
    );
}

#[test]
fn fails_when_the_jpeg_2000_picture_did_not_come_through() {
    let s = Stage::new();
    let work = Scratch::new("smoke-fail");
    let calls = RefCell::new(Vec::new());
    let run = fake_binary(
        s.scratch.join("jpx.pdf").display().to_string(),
        json!({"warnings": ["Screepub could not unpack its picture decoders, so pictures stored as JPEG2000 or JBIG2 could not be read."]}),
        &calls,
    );
    let err = smoke_cli(
        &s.scratch.join("screepub"),
        &s.scratch.join("screenplay.pdf"),
        &s.scratch.join("jpx.pdf"),
        work.path(),
        "0.6.0",
        &run,
    )
    .unwrap_err();
    assert!(err.contains("JPEG 2000"), "{err}");
}

#[test]
fn a_missing_jpeg_2000_fixture_fails_before_anything_runs() {
    let s = Stage::new();
    let work = Scratch::new("smoke-missing");
    let calls = RefCell::new(Vec::new());
    let run = fake_binary(String::new(), json!({"imageCount": 1}), &calls);
    let err = smoke_cli(
        &s.scratch.join("screepub"),
        &s.scratch.join("screenplay.pdf"),
        &s.scratch.join("nope.pdf"),
        work.path(),
        "0.6.0",
        &run,
    )
    .unwrap_err();
    assert!(err.contains("no JPEG 2000 fixture"), "{err}");
    assert!(calls.borrow().is_empty());
}

// ── check_epub_bytes ────────────────────────────────────────────────────

#[test]
fn an_epub_is_a_zip_container() {
    assert!(check_epub_bytes(&[0x50, 0x4b, 0x03, 0x04, 0x14]).is_ok());
}

#[test]
fn a_0_byte_or_non_zip_output_fails() {
    assert!(check_epub_bytes(&[]).is_err());
    assert!(check_epub_bytes(b"<html>").is_err());
}

// ── check_devices ───────────────────────────────────────────────────────

#[test]
fn an_empty_device_list_is_a_pass_the_command_ran() {
    assert!(check_devices(&ok("{\"ok\":true,\"devices\":[]}\n")).is_ok());
    assert!(check_devices(&ok("{\"ok\":true,\"devices\":[{\"id\":\"kindle\"}]}\n")).is_ok());
}

#[test]
fn a_missing_array_a_reported_failure_or_a_crash_all_fail() {
    assert!(
        check_devices(&ok("{\"ok\":true}\n"))
            .unwrap_err()
            .contains("devices")
    );
    let failed = ok("{\"ok\":false,\"error\":{\"code\":\"internal\"}}\n");
    assert!(check_devices(&failed).unwrap_err().contains("internal"));
    assert!(
        check_devices(&RunResult::failed(1, "no"))
            .unwrap_err()
            .contains("exited 1")
    );
}

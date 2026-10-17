//! `cargo xtask smoke-cli`: run a built CLI binary and prove four things
//! about it: it reports its version, converts a script, carries a JPEG 2000
//! picture, and lists devices (was tools/smoke-cli.ts).
//!
//! Cross-compiling is not cross-testing: a Windows binary built on Linux has
//! never executed, so each OS runs its own artifact through this.
//!
//! ```text
//! cargo xtask smoke-cli --binary ./method-reader
//! ```

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{FlagKind, RunResult, Runner, TempDir, parse_flags, real_run, workspace_version};
use crate::paths;

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// What JavaScript's `String(value)` printed, for messages ported word for
/// word: `undefined` for a missing field, a string bare, anything else as JSON.
pub fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}

/// The --json contract is ONE parseable object on stdout, at every exit.
/// Checking only the first line would wave through a binary that printed a
/// progress line and then a result.
pub fn sole_json(stdout: &str, what: &str) -> Result<Value, String> {
    let lines: Vec<&str> = stdout
        .split('\n')
        .filter(|l| !l.trim().is_empty())
        .collect();
    if lines.len() != 1 {
        return Err(format!(
            "smoke: {what} printed {} lines on stdout under --json; the contract is exactly one",
            lines.len()
        ));
    }
    serde_json::from_str(lines[0]).map_err(|_| {
        format!(
            "smoke: {what} did not print parseable JSON: {}",
            clip(lines[0], 200)
        )
    })
}

/// `method-reader --version` must print exactly
/// `method-reader <expected> (Method Reader)`
/// (the `screepub` alias prints the same: it behaves exactly like the tool).
pub fn check_version(result: &RunResult, expected: &str) -> Result<(), String> {
    if result.exit_code != 0 {
        return Err(format!(
            "smoke: --version exited {}: {}",
            result.exit_code,
            clip(result.stderr.trim(), 500)
        ));
    }
    let line = result.stdout.trim();
    let want = format!("method-reader {expected} (Method Reader)");
    if line != want {
        return Err(format!(
            "smoke: --version printed \"{line}\", expected \"{want}\""
        ));
    }
    Ok(())
}

/// Returns the parsed answer, so a further question about the same
/// conversion is asked of the object this already vetted.
pub fn check_convert_result(result: &RunResult, epub_path: &str) -> Result<Value, String> {
    if result.exit_code != 0 {
        return Err(format!(
            "smoke: convert exited {}: {}",
            result.exit_code,
            clip(result.stderr.trim(), 500)
        ));
    }
    let json = sole_json(&result.stdout, "convert")?;
    if json.get("ok") != Some(&Value::Bool(true)) {
        return Err(format!("smoke: convert reported {json}"));
    }
    if json.get("epubPath").and_then(Value::as_str) != Some(epub_path) {
        return Err(format!(
            "smoke: convert wrote {}, expected {epub_path}",
            js_string(json.get("epubPath"))
        ));
    }
    let pages_ok = json
        .get("pages")
        .and_then(Value::as_f64)
        .is_some_and(|p| p > 0.0);
    if !pages_ok {
        return Err(format!(
            "smoke: convert reported {} pages; an empty book is not a conversion",
            js_string(json.get("pages"))
        ));
    }
    Ok(json)
}

/// tests/fixtures/jpx.pdf holds exactly one picture, stored as JPEG 2000.
/// When the decoder is missing the conversion still SUCCEEDS, with the
/// picture reported as a warning and the count absent, so the count is the
/// assertion; the warnings ride in the message, because on a runner nobody
/// can log into they are the whole diagnosis.
pub fn check_jpx_picture(json: &Value) -> Result<(), String> {
    let count = json.get("imageCount");
    if count.and_then(Value::as_f64) != Some(1.0) {
        let shown = match count {
            None | Some(Value::Null) => "no".to_string(),
            v => js_string(v),
        };
        let warnings = json
            .get("warnings")
            .filter(|w| !w.is_null())
            .map_or_else(|| "[]".to_string(), Value::to_string);
        return Err(format!(
            "smoke: the JPEG 2000 fixture carried {shown} picture(s), expected 1, so the decoder \
             did not load. Warnings: {warnings}"
        ));
    }
    Ok(())
}

/// An EPUB is a zip container. A CLI can report success and still have
/// written nothing usable.
pub fn check_epub_bytes(head: &[u8]) -> Result<(), String> {
    if head.starts_with(&[0x50, 0x4b, 0x03, 0x04]) {
        Ok(())
    } else {
        Err("smoke: the output is not a zip container; no EPUB was written".to_string())
    }
}

/// An empty list is a PASS: it proves the command ran on this OS. No runner
/// has an e-reader plugged into it.
pub fn check_devices(result: &RunResult) -> Result<(), String> {
    if result.exit_code != 0 {
        return Err(format!(
            "smoke: devices --json exited {}: {}",
            result.exit_code,
            clip(result.stderr.trim(), 500)
        ));
    }
    let json = sole_json(&result.stdout, "devices --json")?;
    if json.get("ok") != Some(&Value::Bool(true)) {
        return Err(format!("smoke: devices --json reported {json}"));
    }
    if !json.get("devices").is_some_and(Value::is_array) {
        return Err("smoke: devices --json carried no devices array".to_string());
    }
    Ok(())
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// The whole smoke, through `run`.
pub fn smoke_cli(
    binary: &Path,
    fixture: &Path,
    jpx_fixture: &Path,
    work_dir: &Path,
    expected_version: &str,
    run: Runner,
) -> Result<(), String> {
    if !binary.exists() {
        return Err(format!("smoke: no binary at {}", binary.display()));
    }
    if !fixture.exists() {
        return Err(format!("smoke: no fixture at {}", fixture.display()));
    }
    if !jpx_fixture.exists() {
        return Err(format!(
            "smoke: no JPEG 2000 fixture at {}",
            jpx_fixture.display()
        ));
    }
    let bin = binary.display().to_string();

    check_version(&run(&argv(&[&bin, "--version"])), expected_version)?;

    let epub = work_dir.join("smoke.epub");
    let epub_s = epub.display().to_string();
    let fixture_s = fixture.display().to_string();
    check_convert_result(
        &run(&argv(&[
            &bin,
            &fixture_s,
            "-o",
            &epub_s,
            "--no-fountain",
            "--json",
        ])),
        &epub_s,
    )?;
    if !epub.exists() {
        return Err(format!(
            "smoke: convert reported success but {epub_s} is not there"
        ));
    }
    let mut head = Vec::new();
    std::fs::File::open(&epub)
        .and_then(|f| f.take(8).read_to_end(&mut head))
        .map_err(|e| format!("smoke: cannot read {epub_s}: {e}"))?;
    check_epub_bytes(&head)?;

    // --no-fountain still reports imageCount: it counts what the conversion
    // carried out of the PDF, not what landed in a picture folder.
    let jpx_epub = work_dir.join("smoke-jpx.epub").display().to_string();
    let jpx_s = jpx_fixture.display().to_string();
    let json = check_convert_result(
        &run(&argv(&[
            &bin,
            &jpx_s,
            "-o",
            &jpx_epub,
            "--no-fountain",
            "--json",
        ])),
        &jpx_epub,
    )?;
    check_jpx_picture(&json)?;

    check_devices(&run(&argv(&[&bin, "devices", "--json"])))
}

/// `cargo xtask smoke-cli`. The expected version is the workspace's (what a
/// Rust `method-reader --version` reports) unless --expect-version names one,
/// which also lets it smoke a bun-compiled CLI whose package.json differs.
pub fn main(args: &[String]) -> u8 {
    match run_main(args) {
        Ok(line) => {
            println!("{line}");
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

fn run_main(args: &[String]) -> Result<String, String> {
    let flags = parse_flags(
        "smoke",
        args,
        &[
            ("binary", FlagKind::Value),
            ("fixture", FlagKind::Value),
            ("jpx-fixture", FlagKind::Value),
            ("expect-version", FlagKind::Value),
        ],
    )?;
    let binary = flags
        .value("binary")
        .ok_or("smoke: --binary <path> is required")?;
    let fixtures = paths::repo_root().join("tests").join("fixtures");
    let fixture = flags
        .value("fixture")
        .map_or_else(|| fixtures.join("screenplay.pdf"), PathBuf::from);
    let jpx = flags
        .value("jpx-fixture")
        .map_or_else(|| fixtures.join("jpx.pdf"), PathBuf::from);
    let version = match flags.value("expect-version") {
        Some(v) => v.to_string(),
        None => workspace_version(&paths::workspace_root()).map_err(|e| format!("smoke: {e}"))?,
    };
    // Removed when it drops, on a failure as well as a pass.
    let work =
        TempDir::new("method-reader-smoke-").map_err(|e| format!("smoke: no work folder: {e}"))?;
    smoke_cli(
        Path::new(binary),
        &fixture,
        &jpx,
        work.path(),
        &version,
        &real_run,
    )?;
    Ok(format!(
        "smoke: {binary} converts, decodes JPEG 2000, reports {version}, and lists devices"
    ))
}

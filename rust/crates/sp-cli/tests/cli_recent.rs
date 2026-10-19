//! recent.json from the CLI, which is also the window's Convert (the window
//! runs this same code with `--library`): which conversions record an
//! entry, under which path, and that a failed write never fails the
//! conversion. The file's own rules are sp-engine's tests/recent.rs.

mod common;

use std::path::Path;

use serde_json::Value;

use common::Sandbox;

fn script(sb: &Sandbox, rel: &str) -> String {
    sb.copy_fixture("screenplay.pdf", rel)
        .to_string_lossy()
        .into_owned()
}

fn recent(sb: &Sandbox) -> Option<Value> {
    let text = std::fs::read_to_string(sb.path("config/recent.json")).ok()?;
    Some(serde_json::from_str(&text).expect("recent.json is JSON"))
}

fn recent_paths(sb: &Sandbox) -> Vec<String> {
    recent(sb)
        .expect("a recent.json")
        .as_array()
        .expect("an array")
        .iter()
        .map(|e| e["path"].as_str().unwrap().to_owned())
        .collect()
}

fn ok(sb: &Sandbox, cwd: &Path, args: &[&str]) -> Value {
    let ran = sb.run_in(cwd, args);
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    ran.json()
}

#[test]
fn a_library_conversion_records_the_file_the_person_chose() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "scripts/Bright Angel.pdf");
    let answer = ok(&sb, &sb.root(), &[&pdf, "--library", "--json"]);

    let entries = recent(&sb).expect("a recent.json");
    let entries = entries.as_array().unwrap();
    assert_eq!(entries.len(), 1);
    let entry = entries[0].as_object().unwrap();
    let keys: Vec<&str> = entry.keys().map(String::as_str).collect();
    assert_eq!(keys, ["path", "opened"]);
    // The PDF itself, not the library's copy.
    assert_eq!(entry["path"], Value::String(pdf.clone()));
    let book = answer["epubPath"].as_str().unwrap();
    assert!(book.starts_with(&sb.p("library")), "{book}");
    let opened = entry["opened"].as_str().unwrap();
    assert_eq!(opened.len(), "2026-10-18T09:34:05Z".len(), "{opened}");
    assert!(
        opened.ends_with('Z') && opened.as_bytes()[10] == b'T',
        "{opened}"
    );
}

#[test]
fn a_relative_path_is_recorded_as_absolute_and_reopening_moves_it_up() {
    let sb = Sandbox::new();
    let one = script(&sb, "scripts/One.pdf");
    let two = script(&sb, "scripts/Two.pdf");
    let scripts = sb.path("scripts");
    ok(&sb, &scripts, &["One.pdf", "--library", "--json"]);
    ok(&sb, &scripts, &["Two.pdf", "--library", "--json"]);
    assert_eq!(recent_paths(&sb), [two.clone(), one.clone()]);
    ok(
        &sb,
        &scripts,
        &["One.pdf", "--library", "--json", "--force"],
    );
    assert_eq!(recent_paths(&sb), [one, two]);
}

#[test]
fn a_conversion_without_library_records_nothing() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "scripts/Beside.pdf");
    ok(&sb, &sb.root(), &[&pdf, "--json"]);
    sb.mkdir("out");
    let out = sb.p("out/Elsewhere.epub");
    ok(&sb, &sb.root(), &[&pdf, "--json", "-o", &out]);
    assert!(recent(&sb).is_none());
}

#[test]
fn the_sample_in_the_data_folder_is_never_recorded() {
    let sb = Sandbox::new();
    let sample = sb
        .copy_fixture("field-station.pdf", "config/Sample/Field Station.pdf")
        .to_string_lossy()
        .into_owned();
    // The window's way (-o into its own folder), and even --library.
    let out = sb.p("config/Sample/Field Station.epub");
    ok(&sb, &sb.root(), &[&sample, "--json", "-o", &out]);
    ok(&sb, &sb.root(), &[&sample, "--json", "--library"]);
    assert!(recent(&sb).is_none());
}

#[test]
fn a_recent_json_it_cannot_write_never_fails_the_conversion() {
    let sb = Sandbox::new();
    // A folder where the file should be: the rename cannot replace it.
    sb.mkdir("config/recent.json/blocked");
    let pdf = script(&sb, "scripts/Stubborn.pdf");
    let answer = ok(&sb, &sb.root(), &[&pdf, "--library", "--json"]);
    assert_eq!(answer["ok"], Value::Bool(true));
    assert!(Path::new(answer["epubPath"].as_str().unwrap()).exists());
    assert!(sb.path("config/recent.json/blocked").is_dir());
}

#[test]
fn the_json_answer_is_the_same_with_or_without_the_record() {
    // The same first conversion in two sandboxes, recording made to fail
    // in the second: the answer has the same keys, and no key about it.
    let recorded = Sandbox::new();
    let pdf = script(&recorded, "scripts/Same.pdf");
    let first = ok(&recorded, &recorded.root(), &[&pdf, "--library", "--json"]);
    assert!(recent(&recorded).is_some());
    let blocked = Sandbox::new();
    blocked.mkdir("config/recent.json/blocked");
    let pdf = script(&blocked, "scripts/Same.pdf");
    let second = ok(&blocked, &blocked.root(), &[&pdf, "--library", "--json"]);
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&first), keys(&second));
    assert!(!keys(&first).iter().any(|k| k.contains("recent")));
    assert_eq!(first["title"], second["title"]);
}

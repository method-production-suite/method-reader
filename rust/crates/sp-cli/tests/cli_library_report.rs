//! The `library-report` verb end to end
//! (docs/superpowers/specs/2026-11-04-conversion-report-design.md): a PDF
//! converted into a sandbox library, then reported. The counting rules are
//! sp-engine/tests/library_report.rs. Every name here is invented.

mod common;

use serde_json::Value;

use common::{Ran, Sandbox, join};

/// Run in the sandbox's `empty/` folder: a word is only a verb when no
/// file of that name is there.
fn run(sb: &Sandbox, args: &[&str]) -> Ran {
    sb.run_in(&sb.path("empty"), args)
}

fn ok(sb: &Sandbox, args: &[&str]) -> Value {
    let ran = run(sb, args);
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    ran.json()
}

/// The committed screenplay.pdf converted into the sandbox library;
/// answers the script's folder.
fn converted(sb: &Sandbox) -> String {
    let pdf = sb
        .copy_fixture("screenplay.pdf", "scripts/lamp_keeper.pdf")
        .to_string_lossy()
        .into_owned();
    ok(sb, &[&pdf, "--library", "--json"]);
    join(&sb.p("library"), "lamp_keeper")
}

#[test]
fn library_report_help_describes_the_verb_and_exits_0() {
    let sb = Sandbox::new();
    let ran = run(&sb, &["library-report", "--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.starts_with("method-reader library-report:"));
    assert!(
        ran.stdout
            .contains("method-reader library-report <folder> [--json]")
    );
    assert!(ran.stdout.contains("Reads only."));
}

#[test]
fn a_converted_script_reports_what_its_row_and_its_conversion_say() {
    let sb = Sandbox::new();
    let folder = converted(&sb);
    let row = ok(&sb, &["library", "--json"])["scripts"][0].clone();
    let report = ok(&sb, &["library-report", &folder, "--json"]);
    assert_eq!(report["ok"], Value::Bool(true));
    assert_eq!(report["folder"], Value::String(folder.clone()));
    assert_eq!(report["title"], row["title"]);
    assert_eq!(report["pages"], row["pages"]);
    assert_eq!(report["scenes"], row["scenes"]);

    let characters = report["characters"].as_array().expect("characters");
    assert!(!characters.is_empty());
    let speeches: Vec<u64> = characters
        .iter()
        .map(|c| c["speeches"].as_u64().unwrap())
        .collect();
    assert!(speeches.windows(2).all(|w| w[0] >= w[1]), "{speeches:?}");
    assert_eq!(
        speeches.iter().sum::<u64>(),
        report["speeches"].as_u64().unwrap()
    );
    let words: u64 = characters
        .iter()
        .map(|c| c["words"].as_u64().unwrap())
        .sum();
    assert_eq!(words, report["words"].as_u64().unwrap());
    assert!(words > 0);

    let plain = run(&sb, &["library-report", &folder]);
    assert_eq!(plain.code, 0, "{}", plain.stderr);
    let lines: Vec<&str> = plain.stdout.lines().collect();
    assert_eq!(lines[0], report["title"].as_str().unwrap());
    assert!(
        lines[1].contains(" scenes, ") && lines[1].ends_with("of dialogue"),
        "{}",
        lines[1]
    );
    assert_eq!(lines.len(), 2 + characters.len());
}

#[test]
fn a_folder_the_library_does_not_list_is_refused() {
    let sb = Sandbox::new();
    converted(&sb);
    for target in [sb.p("library"), sb.p("scripts")] {
        let ran = run(&sb, &["library-report", &target, "--json"]);
        assert_eq!(ran.error_code(), "not-a-script", "{target}");
    }
    let ran = run(&sb, &["library-report", "--json"]);
    assert_eq!(ran.error_code(), "usage");
}

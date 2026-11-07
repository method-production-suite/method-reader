//! The `library-sides` verb end to end
//! (docs/superpowers/specs/2026-11-05-sides-design.md): a PDF converted into
//! a sandbox library, then one character's sides made. Which scenes stay is
//! sp-engine/tests/library_sides.rs. Every name here is invented.

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
fn library_sides_help_describes_the_verb_and_exits_0() {
    let sb = Sandbox::new();
    let ran = run(&sb, &["library-sides", "--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.starts_with("method-reader library-sides:"));
    assert!(
        ran.stdout
            .contains("method-reader library-sides <folder> <character> [--json]")
    );
}

#[test]
fn a_listed_character_gets_a_book_in_both_forms() {
    let sb = Sandbox::new();
    let folder = converted(&sb);
    let report = ok(&sb, &["library-report", &folder, "--json"]);
    let name = report["characters"][0]["name"].as_str().unwrap().to_owned();
    let lower = name.to_lowercase();

    let sides = ok(&sb, &["library-sides", &folder, &lower, "--json"]);
    assert_eq!(sides["ok"], Value::Bool(true));
    assert_eq!(sides["folder"], Value::String(folder.clone()));
    assert_eq!(sides["name"], Value::String(name.clone()));
    assert_eq!(sides["ofScenes"], report["scenes"]);
    assert!(sides["scenes"].as_u64().unwrap() >= 1);
    let epub = sides["epub"].as_str().unwrap();
    assert!(std::path::Path::new(epub).is_file(), "{epub}");
    assert!(
        epub.ends_with(&format!("lamp_keeper sides - {name}.epub")),
        "{epub}"
    );

    let row = ok(&sb, &["library", "--json"])["scripts"][0].clone();
    assert_eq!(
        row["folder"],
        Value::String(folder.clone()),
        "still one script"
    );

    let plain = run(&sb, &["library-sides", &folder, &name]);
    assert_eq!(plain.code, 0);
    assert!(
        plain.stdout.starts_with(&format!("sides for {name}: ")),
        "{}",
        plain.stdout
    );
}

#[test]
fn a_stranger_and_a_missing_character_are_usage_errors() {
    let sb = Sandbox::new();
    let folder = converted(&sb);
    let stranger = run(&sb, &["library-sides", &folder, "NOBODY AT ALL", "--json"]);
    assert_ne!(stranger.code, 0);
    assert!(stranger.stdout.contains("usage") || stranger.stderr.contains("usage"));
    let missing = run(&sb, &["library-sides", &folder]);
    assert_ne!(missing.code, 0);
}

#[test]
fn library_lines_makes_the_whole_book_in_both_forms() {
    let sb = Sandbox::new();
    assert!(
        run(&sb, &["library-lines", "--help"])
            .stdout
            .contains("method-reader library-lines <folder> <character> [--json]")
    );
    let folder = converted(&sb);
    let report = ok(&sb, &["library-report", &folder, "--json"]);
    let who = &report["characters"][0];
    let name = who["name"].as_str().unwrap().to_owned();

    let lines = ok(
        &sb,
        &["library-lines", &folder, &name.to_lowercase(), "--json"],
    );
    assert_eq!(lines["name"], Value::String(name.clone()));
    assert_eq!(lines["speeches"], who["speeches"]);
    let epub = lines["epub"].as_str().unwrap();
    assert!(std::path::Path::new(epub).is_file(), "{epub}");
    assert!(
        epub.ends_with(&format!("lamp_keeper lines - {name}.epub")),
        "{epub}"
    );

    let plain = run(&sb, &["library-lines", &folder, &name]);
    assert_eq!(plain.code, 0);
    assert!(
        plain
            .stdout
            .starts_with(&format!("lines in bold for {name}: ")),
        "{}",
        plain.stdout
    );

    let stranger = run(&sb, &["library-lines", &folder, "NOBODY AT ALL", "--json"]);
    assert_ne!(stranger.code, 0);
}

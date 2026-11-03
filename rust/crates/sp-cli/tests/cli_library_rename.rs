//! The `library-rename` verb end to end
//! (docs/superpowers/specs/2026-11-03-library-rename-design.md): the name
//! kept in a sandbox library script's source.json, its book made again
//! under it, and every later book made into its folder titled by it. The
//! engine's half (the name's rules, the folder check, the MOBI) is
//! sp-engine/tests/library_rename.rs. Every name here is invented.

mod common;

use std::path::Path;

use serde_json::{Value, json};

use common::{Ran, Sandbox, join, zip_texts};

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

/// The committed screenplay.pdf, under a draft's working name, converted
/// into the sandbox library; answers the PDF and the script's folder.
fn converted(sb: &Sandbox) -> (String, String) {
    let pdf = sb
        .copy_fixture("screenplay.pdf", "scripts/draft_v3_FINAL.pdf")
        .to_string_lossy()
        .into_owned();
    ok(sb, &[&pdf, "--library", "--json"]);
    (pdf, join(&sb.p("library"), "draft_v3_FINAL"))
}

fn book(folder: &str) -> String {
    join(folder, "draft_v3_FINAL.epub")
}

/// The book's `dc:title`.
fn book_title(epub: &str) -> String {
    let opf = zip_texts(Path::new(epub), ".opf");
    let start = opf.find("<dc:title>").expect("a dc:title") + "<dc:title>".len();
    let end = opf[start..].find("</dc:title>").expect("its end") + start;
    opf[start..end].to_owned()
}

fn listed_title(sb: &Sandbox) -> Value {
    ok(sb, &["library", "--json"])["scripts"][0]["title"].clone()
}

#[test]
fn library_rename_help_describes_the_verb_and_exits_0() {
    let sb = Sandbox::new();
    let ran = run(&sb, &["library-rename", "--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.starts_with("method-reader library-rename:"));
    assert!(
        ran.stdout
            .contains("method-reader library-rename <folder> <name> [--json]")
    );
    assert!(ran.stdout.contains("Nothing is renamed on disk"));
    let json = run(&sb, &["library-rename", "-h", "--json"]).json();
    assert!(
        json["usage"]
            .as_str()
            .unwrap()
            .starts_with("method-reader library-rename:")
    );
}

#[test]
fn a_renamed_script_is_listed_and_its_book_titled_by_the_name_at_once() {
    let sb = Sandbox::new();
    let (_, folder) = converted(&sb);
    let title_page = book_title(&book(&folder));
    assert_eq!(listed_title(&sb), json!(title_page));

    let v = ok(
        &sb,
        &["library-rename", &folder, " Harbour Light ", "--json"],
    );
    assert_eq!(
        v,
        json!({"ok": true, "folder": folder, "title": "Harbour Light", "epub": book(&folder)})
    );
    assert_eq!(book_title(&book(&folder)), "Harbour Light", "made again");
    assert_eq!(listed_title(&sb), json!("Harbour Light"));
    // Nothing on disk is renamed.
    let listed = ok(&sb, &["library", "--json"]);
    assert_eq!(listed["scripts"][0]["name"], "draft_v3_FINAL");
    assert_eq!(listed["scripts"][0]["folder"], json!(folder));

    // In words, then cleared.
    let human = run(&sb, &["library-rename", &folder, "Low Tide"]);
    assert_eq!(human.code, 0, "{}", human.stderr);
    assert_eq!(human.stdout, format!("renamed {folder} to \"Low Tide\"\n"));
    assert_eq!(book_title(&book(&folder)), "Low Tide");
    let cleared = run(&sb, &["library-rename", &folder, ""]);
    assert_eq!(cleared.code, 0, "{}", cleared.stderr);
    assert_eq!(
        cleared.stdout,
        format!("{folder} is called by its title page again\n")
    );
    assert_eq!(book_title(&book(&folder)), title_page);
    assert_eq!(listed_title(&sb), json!(title_page));
    let v = ok(&sb, &["library-rename", &folder, "", "--json"]);
    assert_eq!(v["title"], json!(title_page));
}

#[test]
fn a_reconversion_of_the_same_pdf_keeps_the_name_and_a_title_flag_still_wins() {
    let sb = Sandbox::new();
    let (pdf, folder) = converted(&sb);
    ok(&sb, &["library-rename", &folder, "Harbour Light", "--json"]);

    let again = ok(&sb, &[&pdf, "--library", "--json"]);
    assert_eq!(again["title"], "Harbour Light");
    assert_eq!(book_title(&book(&folder)), "Harbour Light");
    let marker: Value =
        serde_json::from_str(&std::fs::read_to_string(join(&folder, "source.json")).unwrap())
            .unwrap();
    assert_eq!(marker["name"], "Harbour Light", "kept through it");
    assert_eq!(
        ok(&sb, &["library", "--json"])["scripts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let titled = ok(
        &sb,
        &[&pdf, "--library", "--title", "Given Outright", "--json"],
    );
    assert_eq!(titled["title"], "Given Outright");
    assert_eq!(book_title(&book(&folder)), "Given Outright");
    assert_eq!(
        listed_title(&sb),
        json!("Harbour Light"),
        "the name stays kept"
    );
}

#[test]
fn a_book_made_from_the_library_fountain_takes_the_name_only_in_its_own_folder() {
    let sb = Sandbox::new();
    let (_, folder) = converted(&sb);
    ok(&sb, &["library-rename", &folder, "Harbour Light", "--json"]);
    let fountain = join(&folder, "draft_v3_FINAL.fountain");

    // A reopen (the window's convert-to) and a tune (-o with options).
    let reopened = ok(&sb, &[&fountain, "--json", "-o", &book(&folder)]);
    assert_eq!(reopened["title"], "Harbour Light");
    let tuned = ok(
        &sb,
        &[
            &fountain,
            "--json",
            "-o",
            &book(&folder),
            "--options-json",
            "{}",
        ],
    );
    assert_eq!(tuned["title"], "Harbour Light");
    // The same .fountain made into a book anywhere else is not the
    // library's book, and goes by its title page.
    sb.mkdir("out");
    let elsewhere = sb.p("out/copy.epub");
    let other = ok(&sb, &[&fountain, "--json", "-o", &elsewhere]);
    assert_ne!(other["title"], "Harbour Light");
    assert_eq!(book_title(&elsewhere), other["title"].as_str().unwrap());
}

#[test]
fn a_name_that_starts_with_a_dash_goes_after_the_double_dash() {
    let sb = Sandbox::new();
    let (_, folder) = converted(&sb);
    let v = ok(
        &sb,
        &["library-rename", &folder, "--json", "--", "-30- Final"],
    );
    assert_eq!(v["title"], "-30- Final");
    assert_eq!(book_title(&book(&folder)), "-30- Final");
}

#[test]
fn a_bad_name_or_a_folder_the_listing_would_not_list_is_refused_and_nothing_changes() {
    let sb = Sandbox::new();
    let (_, folder) = converted(&sb);
    let before = std::fs::read(book(&folder)).unwrap();
    let long = "x".repeat(201);
    for (args, code, message) in [
        (
            vec!["library-rename", &folder as &str, &long],
            "usage",
            "a script's name can be at most 200 characters long, and this one is 201",
        ),
        (
            vec!["library-rename", &folder, "Two\nLines"],
            "usage",
            "a script's name is one line: it cannot hold a line break",
        ),
        (
            vec!["library-rename", &folder],
            "usage",
            "expected one script folder in the library and its new name (see --help)",
        ),
        (
            vec!["library-rename", &folder, "a", "b"],
            "usage",
            "expected one script folder in the library and its new name (see --help)",
        ),
        (
            vec!["library-rename", "draft_v3_FINAL", "a"],
            "usage",
            "library-rename needs a script folder's full path, not \"draft_v3_FINAL\"",
        ),
    ] {
        let mut args = args;
        args.push("--json");
        let ran = run(&sb, &args);
        assert_eq!(ran.code, 1, "{args:?}");
        assert_eq!(ran.error_code(), code, "{args:?}");
        assert_eq!(ran.error_message(), message, "{args:?}");
    }
    let stray = sb.mkdir("library/Loose").to_string_lossy().into_owned();
    for target in [stray.clone(), sb.p("library"), sb.p("scripts")] {
        let ran = run(&sb, &["library-rename", &target, "x", "--json"]);
        assert_eq!(ran.error_code(), "not-a-script", "{target}");
    }
    let ran = run(&sb, &["library-rename", &folder, "x", "--quick", "--json"]);
    assert_eq!(ran.error_code(), "usage");
    assert_eq!(
        std::fs::read(book(&folder)).unwrap(),
        before,
        "the book untouched"
    );
    assert_ne!(listed_title(&sb), json!("x"));
    assert!(std::fs::read_dir(&stray).unwrap().next().is_none());
}

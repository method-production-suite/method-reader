//! `cargo xtask build-desktop-notes`: ported from tests/desktop-notes.test.ts.
//!
//! The notes the window shows are the notes that were written: sp-app's
//! generated notes_release.rs is exactly what the generator makes of
//! docs/releases/<version>.md, for the version the repository is at. (The
//! Tauri window's notes.js and its notes-surface.js check went with the
//! Tauri window at the Rust cutover; sp-app's model/notes.rs
//! an_empty_section_is_never_drawn is the surface rule's twin.)

mod common;

use std::path::PathBuf;

use common::{Scratch, text, xtask};
use xtask::desktop_notes::{RUST_MODULE, current_version, parse_release_notes, render_notes_rust};

fn repo() -> PathBuf {
    xtask::paths::repo_root()
}

fn version() -> String {
    current_version(&repo()).expect("the repository's version")
}

fn markdown() -> String {
    common::read_lf(&repo().join(format!("docs/releases/{}.md", version())))
        .expect("the current release's notes")
}

/// A generated file as committed (LF, whatever the checkout did).
fn committed(file: &str) -> Option<String> {
    common::read_lf(&repo().join(file))
}

#[test]
fn the_committed_modules_are_exactly_what_the_generator_produces() {
    let notes = parse_release_notes(&version(), &markdown()).expect("the notes parse");
    assert_eq!(
        committed(RUST_MODULE).expect("sp-app's notes_release.rs"),
        render_notes_rust(&notes)
    );
}

#[test]
fn it_is_the_running_versions_notes_not_a_stale_file() {
    // The failure this catches is the one that actually happens: a release
    // bumps the version and nobody regenerates.
    let v = version();
    let rs = committed(RUST_MODULE).expect("notes_release.rs");
    assert!(rs.contains(&format!("version: \"{v}\"")));
}

#[test]
fn the_headline_and_every_section_title_survive_the_trip() {
    let md = markdown();
    let notes = parse_release_notes(&version(), &md).expect("parse");
    assert!(notes.headline.chars().count() > 10);
    let titles: Vec<String> = md
        .split('\n')
        .filter_map(|l| l.strip_prefix("## "))
        .map(|t| t.trim().to_string())
        .collect();
    let got: Vec<String> = notes.sections.iter().map(|s| s.title.clone()).collect();
    assert_eq!(got, titles);
    assert!(!titles.is_empty());
}

#[test]
fn every_bullet_keeps_its_lead_and_its_body_bold_led_or_plain() {
    // Every "- " line, bold or plain, is a bullet: a count that only
    // measures the bold form agrees with a parser that only keeps the bold
    // form, which is how 0.5.4's caveat was once dropped unnoticed.
    let md = markdown();
    let notes = parse_release_notes(&version(), &md).expect("parse");
    let items: Vec<_> = notes.sections.iter().flat_map(|s| s.items.iter()).collect();
    let bullets: Vec<&str> = md.split('\n').filter(|l| l.starts_with("- ")).collect();
    let bold = bullets.iter().filter(|l| l.starts_with("- **")).count();
    assert_eq!(items.len(), bullets.len());
    assert_eq!(items.iter().filter(|i| !i.lead.is_empty()).count(), bold);
    assert_eq!(
        items.iter().filter(|i| i.lead.is_empty()).count(),
        bullets.len() - bold
    );
    for item in items {
        assert!(!item.body.is_empty());
        assert!(!item.lead.contains("**"));
        assert!(!item.body.contains("**"));
        assert!(!item.body.contains("<!--"));
    }
}

#[test]
fn a_plain_good_to_know_caveat_is_not_dropped() {
    // Driven by a literal fixture, not by the current release file, so the
    // rare plain form stays checked whatever the next notes look like.
    let caveat = "That last change has been read on a Kindle, but not yet with a very long \
                  speech at a large font size. If a character name ever ends up alone at the \
                  foot of a page, that is the case worth reporting.";
    let md = [
        "# Screepub 9.9.9",
        "",
        "A headline paragraph, because the template has one.",
        "",
        "## Good to know",
        "",
        "- That last change has been read on a Kindle, but not yet with a very long",
        "  speech at a large font size. If a character name ever ends up alone at",
        "  the foot of a page, that is the case worth reporting. <!-- caveat: registry-8b -->",
        "",
    ]
    .join("\n");
    let notes = parse_release_notes("9.9.9", &md).expect("parse");
    let section = notes
        .sections
        .iter()
        .find(|s| s.title == "Good to know")
        .expect("the section");
    assert_eq!(section.items.len(), 1);
    assert_eq!(section.items[0].lead, "");
    assert_eq!(section.items[0].body, caveat);
}

#[test]
fn the_live_release_file_keeps_a_good_to_know_section_with_bullets_in_it() {
    let notes = parse_release_notes(&version(), &markdown()).expect("parse");
    let good = notes
        .sections
        .iter()
        .find(|s| s.title == "Good to know")
        .expect("a Good to know section");
    assert!(!good.items.is_empty());
    for item in &good.items {
        assert!(item.body.chars().count() > 20);
    }
}

#[test]
fn a_line_under_a_heading_that_is_neither_bullet_nor_continuation_is_refused() {
    let md = [
        "# Screepub 9.9.9",
        "",
        "A headline.",
        "",
        "## A section",
        "",
        "A stray paragraph, not a bullet.",
        "",
    ]
    .join("\n");
    assert!(parse_release_notes("9.9.9", &md).is_err());
}

#[test]
fn a_heading_with_no_bullets_under_it_is_refused() {
    let md = [
        "# Screepub 9.9.9",
        "",
        "A headline.",
        "",
        "## Empty",
        "",
        "## A section",
        "",
        "- **A lead.** A body.",
        "",
    ]
    .join("\n");
    assert!(parse_release_notes("9.9.9", &md).is_err());
}

#[test]
fn a_plain_good_to_know_bullet_becomes_an_item_with_no_lead() {
    let md = [
        "# Screepub 9.9.9",
        "",
        "A headline.",
        "",
        "## Good to know",
        "",
        "- A caveat with no bold claim, spanning",
        "  a second line. <!-- caveat: registry-1 -->",
        "",
    ]
    .join("\n");
    let notes = parse_release_notes("9.9.9", &md).expect("parse");
    assert_eq!(notes.sections[0].items[0].lead, "");
    assert_eq!(
        notes.sections[0].items[0].body,
        "A caveat with no bold claim, spanning a second line."
    );
}

#[test]
fn a_wrapped_bullet_is_joined_into_one_sentence() {
    let md = [
        "# Screepub 9.9.9",
        "",
        "A headline.",
        "",
        "## A section",
        "",
        "- **A lead.** A body that runs on",
        "  across a second line and a",
        "  third.",
        "",
    ]
    .join("\n");
    let notes = parse_release_notes("9.9.9", &md).expect("parse");
    assert_eq!(
        notes.sections[0].items[0].body,
        "A body that runs on across a second line and a third."
    );
}

#[test]
fn the_modules_are_plain_data_not_markup() {
    // A generator that emitted HTML would put a string of markup one
    // innerHTML away from the page.
    let rs = committed(RUST_MODULE).expect("notes_release.rs");
    assert!(!rs.contains("<p"));
    assert!(!rs.contains("<div"));
    assert!(
        committed(RUST_MODULE)
            .expect("notes_release.rs")
            .contains("pub const RELEASE: Release")
    );
}

#[test]
fn the_rust_module_is_valid_rust_holding_the_same_strings() {
    // A string the Debug escaper would mangle (quotes, a backslash, a
    // non-ASCII letter) must come back as itself in the generated literal.
    let md = "# Screepub 9.9.9\n\nA \"quoted\" headline \\ caf\u{e9}.\n\n## A section\n\n- **A lead.** A body.\n";
    let notes = parse_release_notes("9.9.9", md).expect("parse");
    let rs = render_notes_rust(&notes);
    assert!(rs.contains(r#"headline: "A \"quoted\" headline \\ café.","#));
}

#[test]
fn the_command_writes_the_windows_notes_and_nothing_else() {
    // A scratch repository shaped like the real one after the Rust
    // cutover: rust/Cargo.toml for the version, the notes, sp-app's folder.
    let dir = Scratch::new("desktop-notes");
    dir.write(
        "rust/Cargo.toml",
        "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"9.9.9\"\n",
    );
    let md = "# Screepub 9.9.9\n\nA headline.\n\n## A section\n\n- **A lead.** A body.\n";
    dir.write("docs/releases/9.9.9.md", md);
    std::fs::create_dir_all(dir.join("rust/crates/sp-app/src/model")).expect("folders");
    let repo_arg = dir.path().to_string_lossy().into_owned();
    let out = xtask(&["build-desktop-notes", "--repo", &repo_arg], &[], None, None);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout).trim(),
        format!("wrote {RUST_MODULE} from docs/releases/9.9.9.md")
    );
    let notes = parse_release_notes("9.9.9", md).expect("parse");
    assert_eq!(dir.read(RUST_MODULE), render_notes_rust(&notes));
    assert!(!dir.join("desktop").exists());
}

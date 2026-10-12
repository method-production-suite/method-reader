//! THE SWIFT APP STAYS RETIRED (was tests/app-references.test.ts; its
//! history and reasoning are in that file at any tag before the cutover).
//!
//! Two halves. app/ holds nothing git would commit: the plans under
//! docs/superpowers/ still edit app/Sources/... by line number, because
//! they are history, and a session following one would quietly recreate
//! the directory. And nothing outside the dated records names the Swift
//! app: a new reference is a new coupling to code that is not there.
//!
//! A reference is NOT textual proximity to "app". It is an occurrence, in
//! a file outside the dated records, of a token that names the Swift app
//! itself: its directory, a Swift source file, a Swift module or the
//! kit-check executable, or a command that builds it (PATTERNS below).
//! `cd app` is deliberately not one: release.yml once staged Tauri bundles
//! in a folder of that name, and a sweep that flagged it would have told
//! the next person to delete working code.
//!
//! Provenance is still allowed, in words: "the Swift app's converter",
//! with any tag before F3 (v0.7.4, say) for a reader who wants the original.
//!
//! Not swept: the dated records (docs/superpowers/, docs/history/,
//! docs/adr/, docs/releases/, docs/retired-coverage.md), app/ itself (the
//! first half answers for it), and this file, which names every token in
//! the list by construction.

mod repo_common;

use fancy_regex::Regex;
use repo_common::*;
use std::process::Command;

/// The dated records, app/ and this file; see above.
const NOT_SWEPT: &[&str] = &[
    r"^app/",
    r"^docs/superpowers/",
    r"^docs/history/",
    r"^docs/adr/",
    r"^docs/releases/",
    r"^docs/retired-coverage\.md$",
    r"^rust/crates/xtask/tests/app_references\.rs$",
];

/// Binary files: no prose to rot, and reading them as text lies.
const NOT_TEXT: &str =
    r"\.(pdf|png|jpg|jpeg|zip|ico|icns|woff2?|ttf|otf|lock|gz|bin|epub|mobi|azw3|kfx)$";

fn patterns() -> Vec<(&'static str, Regex)> {
    vec![
        // Not preceded by a path or word character, so desktop/app/ or
        // `myapp/` would not match; `app-bundles-*` has no slash.
        (
            "a path into the deleted directory",
            Regex::new(r"(?<![A-Za-z0-9_./-])app/").unwrap(),
        ),
        // The YAML form with no slash that a naive grep misses entirely.
        (
            "a workflow step that runs inside the deleted directory",
            Regex::new(r"(?m)working-directory:[ \t]*app[ \t]*$").unwrap(),
        ),
        (
            "a Swift module or the kit-check executable",
            Regex::new(r"ScreepubKit|ScreepubApp|KitCheck|kit-check|KFXKit").unwrap(),
        ),
        (
            "a Swift source file",
            Regex::new(r"\b[A-Za-z][A-Za-z0-9-]*\.swift\b").unwrap(),
        ),
        (
            "a command that builds the Swift app",
            Regex::new(r"swift (?:build|run)|build-app\.sh|build-lib\.sh").unwrap(),
        ),
        // There was a sixth rule, Screepub.app and Screepub-macOS: the
        // window took both names at the identifier release, so neither says
        // "the Swift app" any more.
    ]
}

fn count_in(text: &str) -> usize {
    patterns()
        .iter()
        .map(|(_, re)| re.find_iter(text).filter_map(Result::ok).count())
        .sum()
}

fn git(args: &[&str]) -> std::process::Output {
    let out = Command::new("git")
        .args(args)
        .current_dir(root())
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {}", said(&out));
    out
}

fn split_nul(out: std::process::Output) -> Vec<String> {
    String::from_utf8(out.stdout)
        .unwrap()
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Only what git already tracks under the pathspecs.
fn git_tracked(pathspecs: &[&str]) -> Vec<String> {
    let mut args = vec!["ls-files", "-z", "--cached", "--"];
    args.extend(pathspecs);
    split_nul(git(&args))
}

/// TRACKED, plus untracked and not ignored. A bare `git ls-files` lists
/// only tracked files, so a brand-new one (the likeliest kind to carry a
/// new reference) would be invisible until staged; --exclude-standard
/// keeps .gitignore honoured, so build output and the real-script
/// fixtures stay out.
fn git_files(pathspecs: &[&str]) -> Vec<String> {
    let mut args = vec![
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
    ];
    if !pathspecs.is_empty() {
        args.push("--");
        args.extend(pathspecs);
    }
    split_nul(git(&args))
}

fn swept(f: &str) -> bool {
    !NOT_SWEPT
        .iter()
        .any(|p| Regex::new(p).unwrap().is_match(f).unwrap())
}

/// path -> reference count, for everything swept that has any.
fn scan() -> Vec<(String, usize, Vec<&'static str>)> {
    let binary = Regex::new(NOT_TEXT).unwrap();
    let mut found = vec![];
    for f in git_files(&[]) {
        if !swept(&f) || binary.is_match(&f).unwrap() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root().join(&f)) else {
            continue;
        };
        if text.contains('\0') {
            continue;
        }
        let n = count_in(&text);
        if n > 0 {
            let kinds = patterns()
                .iter()
                .filter(|(_, re)| re.is_match(&text).unwrap())
                .map(|(what, _)| *what)
                .collect();
            found.push((f, n, kinds));
        }
    }
    found
}

const FIX: &str = "\nThe SwiftUI Mac app was deleted at piece F3. Nothing outside the dated\nrecords (docs/superpowers/, docs/adr/, docs/releases/,\ndocs/retired-coverage.md) may point at it.\n\nIf you are following a plan under docs/superpowers/ that edits app/, the\nplan is history: the code it edits is gone. Do the work in rust/.\n\nIf the reference is provenance, say it in words: \"the Swift app's\nconverter\", and for a reader who wants the original, any tag before F3\n(v0.7.4, say) or the commit that ported it.";

#[test]
fn app_holds_nothing_git_would_commit() {
    // IGNORED files do not count, so a Mac that built the Swift app before
    // F3 passes as it stands (its build folders are in .gitignore, pinned
    // below). Anything else is the directory coming back.
    let files = git_files(&["app"]);
    if !files.is_empty() {
        let tracked = git_tracked(&["app"]);
        let title = if tracked.is_empty() {
            "app/ holds files that git would add. If they are left over from before F3, delete the folder: rm -rf app."
        } else {
            "app/ is back in git. It was deleted at F3 and must stay gone."
        };
        panic!(
            "{title}\n  {}\n{FIX}",
            files
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n  ")
        );
    }
}

#[test]
fn nothing_outside_the_dated_records_names_it() {
    let found = scan();
    if !found.is_empty() {
        let lines: Vec<String> = found
            .iter()
            .map(|(f, n, kinds)| format!("  {f}: {n} reference(s): {}", kinds.join("; ")))
            .collect();
        panic!(
            "Files name the retired Swift app.\n{}\n{FIX}",
            lines.join("\n")
        );
    }
}

#[test]
fn the_build_output_the_swift_app_left_on_macs_that_built_it_is_ignored() {
    // Without these rules every run would read thousands of SwiftPM build
    // files as "untracked", both halves above would fail with advice about
    // recreating the app, and a `git add -A` would stage them.
    let ignored = |p: &str| {
        Command::new("git")
            .args(["check-ignore", "-q", "--no-index", p])
            .current_dir(root())
            .status()
            .expect("git runs")
            .success()
    };
    for p in [
        "app/.build/x",
        "app/build/x",
        "app/dist/x",
        "app/Packages/KFXKit/.build/x",
    ] {
        assert!(ignored(p), "{p} is not ignored");
    }
    // And no wider: a rule that hid app/ itself would blind the first half.
    assert!(!ignored("app/Sources/Probe.swift"));
    assert!(!ignored("app/Package.swift"));
}

#[test]
fn a_file_that_is_not_committed_yet_is_still_swept() {
    // Asserted against the flags rather than by writing a file into the
    // repository mid-run, which a crash would leave behind.
    let source = read("rust/crates/xtask/tests/app_references.rs");
    assert!(source.contains("\"--others\""));
    assert!(source.contains("\"--exclude-standard\""));
    assert!(git_files(&[]).contains(&"README.md".to_string()));
}

#[test]
fn the_patterns_match_the_forms_a_naive_grep_misses() {
    assert!(
        count_in("      - run: swift run -c release kit-check\n        working-directory: app\n")
            >= 3
    );
    assert_eq!(count_in("Mirrors EbookConvert.swift exactly"), 1);
    assert_eq!(count_in("see `app/release.sh`"), 1);
    assert_eq!(count_in("app/dist/Screepub-macOS.dmg"), 1);
}

#[test]
fn provenance_in_words_is_not_a_reference() {
    // The failure message tells people to write it this way.
    assert_eq!(count_in("matching the Swift app's converter exactly"), 0);
    assert_eq!(
        count_in("the Swift app was deleted at piece F3; see it at tag v0.7.4"),
        0
    );
}

#[test]
fn the_names_the_window_took_over_are_not_references() {
    assert_eq!(count_in("download/Screepub-macOS.dmg"), 0);
    assert_eq!(count_in("/Applications/Screepub.app"), 0);
}

#[test]
fn the_patterns_do_not_match_a_folder_that_happens_to_be_called_app() {
    assert_eq!(count_in("          path: app\n"), 0);
    assert_eq!(count_in("          cd app\n          sha256sum *.deb\n"), 0);
    assert_eq!(count_in("pattern: app-bundles-*\n"), 0);
    assert_eq!(count_in("working-directory: desktop/src-tauri\n"), 0);
    assert_eq!(count_in("rust/crates/sp-app/src/main.rs"), 0);
}

#[test]
fn the_sweep_covers_the_whole_tree_not_an_empty_slice_of_it() {
    let files: Vec<String> = git_files(&[]).into_iter().filter(|f| swept(f)).collect();
    assert!(files.len() > 150, "{}", files.len());
    for must in [
        "README.md",
        "CLAUDE.md",
        ".github/workflows/weekly-toolchain.yml",
        "rust/Cargo.toml",
    ] {
        assert!(files.contains(&must.to_string()), "{must}");
    }
    assert!(!files.iter().any(|f| f.starts_with("docs/superpowers/")));
    assert!(!files.iter().any(|f| f.starts_with("docs/history/")));
    assert!(!files.contains(&"docs/retired-coverage.md".to_string()));
}

// ---- docs/retired-coverage.md: the coverage the deletion gave up -------

/// Each unreplaced kit-check section with the check count the spec
/// measured. Named here, not read from the document: a test that took its
/// expectations from the document under test would pass on an empty one.
const SECTIONS: &[(&str, usize)] = &[
    ("send-menu", 45),
    ("self-update-installer", 26),
    ("updater-version-compare", 17),
    ("update-selection", 17),
    ("update-decoding", 14),
    ("update-error-descriptions", 11),
    ("engine-cancellation", 10),
    ("mail-and-books", 5),
    ("feedback-url", 5),
    ("release-notes-parsing", 21),
];

fn retired() -> String {
    read("docs/retired-coverage.md")
}

#[test]
fn every_unreplaced_kit_check_section_is_present_with_its_check_count() {
    let text = retired();
    let mut missing = vec![];
    for (section, checks) in SECTIONS {
        let heading = Regex::new(&format!(r"(?m)^## .*`{section}`.*$")).unwrap();
        match heading.find(&text).unwrap() {
            None => missing.push(format!("{section}: no \"## `{section}`\" heading")),
            Some(m) if !m.as_str().contains(&checks.to_string()) => missing.push(format!(
                "{section}: heading does not state its {checks} checks: {}",
                m.as_str()
            )),
            Some(_) => {}
        }
    }
    assert_eq!(missing, Vec::<String>::new());
}

#[test]
fn every_section_carries_an_explicit_decision() {
    let text = retired();
    let rows: Vec<&str> = text.lines().filter(|l| l.starts_with("| `")).collect();
    assert!(rows.len() >= SECTIONS.len());
    for row in rows {
        assert!(
            row.contains("`port`") || row.contains("`accept-loss`"),
            "no port/accept-loss decision in: {row}"
        );
    }
}

#[test]
fn the_decisions_are_marked_as_recommendations_not_as_settled() {
    let text = retired();
    let lower = text.to_lowercase();
    assert!(lower.contains("recommendation"));
    assert!(text.contains("RECOMMENDED"));
    assert!(
        Regex::new(r"not (yet )?(a )?decision|awaiting|has not been decided|no decision")
            .unwrap()
            .is_match(&lower)
            .unwrap()
    );
}

#[test]
fn it_says_what_a_user_loses_not_only_what_a_test_loses() {
    let text = retired();
    assert!(text.contains("What a user loses"));
    assert!(text.contains("Cost to port"));
}

#[test]
fn it_points_at_the_spec_and_at_the_gate_it_serves() {
    let text = retired();
    assert!(text.contains("2026-09-14-retire-swiftui-design.md"));
    assert!(text.contains("Gate 2"));
}

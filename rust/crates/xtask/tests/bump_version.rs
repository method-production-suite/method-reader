//! `cargo xtask bump-version`: ported from tests/bump-version.test.ts.
//!
//! The release commit's version bump, every version file and the window's
//! notes, or nothing at all. These run the tool on scratch copies of the
//! real files, so they check the files it will really meet; and on scratch
//! repos shaped like the repository before the Rust cutover (the Bun
//! engine and the Tauri window, no rust/) and after it (rust/ alone).

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::{Scratch, text, xtask};
use xtask::bump_version::{
    GROUPS, Kind, bump_version, cargo_table_version, set_cargo_lock_entry, set_cargo_lock_version,
    set_cargo_toml_version, set_json_version, version_files, workspace_crates,
};
use xtask::desktop_notes::{parse_release_notes, render_notes_module, render_notes_rust};

const GOOD_NOTES: &str = "# Screepub 9.8.7

**One line that says what changed.** And a second sentence in the same paragraph.

## Better

- **A claim.** Its explanation,
  wrapped onto a second line.

## Good to know

- A plain caveat.
";

fn root() -> std::path::PathBuf {
    xtask::paths::repo_root()
}

fn real(path: &str) -> String {
    common::read_lf(&root().join(path)).unwrap_or_else(|| panic!("{path} is missing"))
}

fn current() -> String {
    let pkg: serde_json::Value = serde_json::from_str(&real("package.json")).expect("JSON");
    pkg["version"].as_str().expect("version").to_string()
}

/// Which of the real repository's version files a scratch repo copies.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    /// Everything the repository has today (the port branch: all three).
    Today,
    /// Before cutover: the Bun engine and the Tauri window, no rust/.
    BeforeCutover,
    /// After cutover: rust/ alone.
    AfterCutover,
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to.parent().expect("parent")).expect("folders");
    // As committed (LF), whatever the checkout did to line endings.
    let text = common::read_lf(from).unwrap_or_else(|| panic!("{} is missing", from.display()));
    std::fs::write(to, text).expect("copy");
}

/// A scratch repo holding copies of the real version files, plus notes.
fn scratch_repo(shape: Shape, notes: Option<&str>, version: &str) -> Scratch {
    let s = Scratch::new("bump-version");
    let r = root();
    let take_bun_tauri = shape != Shape::AfterCutover;
    let take_rust = shape != Shape::BeforeCutover;
    if take_bun_tauri {
        for f in [
            "package.json",
            "desktop/src-tauri/Cargo.toml",
            "desktop/src-tauri/Cargo.lock",
            "desktop/src-tauri/tauri.conf.json",
            "desktop/ui/notes.js",
        ] {
            copy(&r.join(f), &s.join(f));
        }
    }
    if take_rust {
        for f in [
            "rust/Cargo.toml",
            "rust/Cargo.lock",
            "rust/crates/sp-app/src/model/notes_release.rs",
        ] {
            copy(&r.join(f), &s.join(f));
        }
        for entry in std::fs::read_dir(r.join("rust/crates")).expect("crates") {
            let dir = entry.expect("entry").path();
            let manifest = dir.join("Cargo.toml");
            if manifest.is_file() {
                let name = dir.file_name().expect("name").to_owned();
                copy(
                    &manifest,
                    &s.join(Path::new("rust/crates").join(name).join("Cargo.toml")),
                );
            }
        }
    }
    std::fs::create_dir_all(s.join("docs/releases")).expect("docs");
    if let Some(n) = notes {
        s.write(&format!("docs/releases/{version}.md"), n);
    }
    s
}

fn repo(notes: Option<&str>) -> Scratch {
    scratch_repo(Shape::Today, notes, "9.8.7")
}

/// Every version file's text, keyed by path.
fn snapshot(dir: &Path) -> BTreeMap<String, Option<String>> {
    GROUPS
        .iter()
        .flat_map(|g| g.files.iter().map(|f| f.path))
        .map(|p| (p.to_string(), std::fs::read_to_string(dir.join(p)).ok()))
        .collect()
}

/// The lines of `after` that differ from `before`, which must have as many.
fn changed_lines(before: &str, after: &str) -> Vec<String> {
    let a: Vec<&str> = before.split('\n').collect();
    let b: Vec<&str> = after.split('\n').collect();
    assert_eq!(b.len(), a.len());
    b.iter()
        .zip(a.iter())
        .filter(|(x, y)| x != y)
        .map(|(x, _)| x.to_string())
        .collect()
}

// ── each file changes on its version line and nowhere else ────────────

#[test]
fn package_json_and_tauri_conf_the_top_level_version_formatting_kept() {
    for file in ["package.json", "desktop/src-tauri/tauri.conf.json"] {
        let before = real(file);
        let after = set_json_version(&before, "9.8.7", file).expect("bump");
        let changed = changed_lines(&before, &after);
        assert_eq!(changed.len(), 1, "{file}");
        assert!(changed[0].contains("\"version\": \"9.8.7\""), "{file}");
        let v: serde_json::Value = serde_json::from_str(&after).expect("JSON");
        assert_eq!(v["version"], "9.8.7");
    }
}

#[test]
fn a_json_file_the_tool_would_reformat_is_refused_not_rewritten() {
    let four = "{\n    \"name\": \"x\",\n    \"version\": \"1.0.0\"\n}\n";
    let e = set_json_version(four, "9.8.7", "odd.json").unwrap_err();
    assert!(e.contains("odd.json"));
    let e = set_json_version("{\n  \"name\": \"x\"\n}\n", "9.8.7", "none.json").unwrap_err();
    assert!(e.contains("none.json"));
}

#[test]
fn cargo_toml_the_package_version_not_a_dependencys() {
    let before = real("desktop/src-tauri/Cargo.toml");
    let after = set_cargo_toml_version(&before, "9.8.7").expect("bump");
    assert_eq!(changed_lines(&before, &after), vec!["version = \"9.8.7\""]);
    assert_eq!(
        cargo_table_version(&after, "package").as_deref(),
        Some("9.8.7")
    );

    let no_own =
        "[package]\nname = \"x\"\n\n[dependencies]\nserde = { version = \"1\" }\nfoo = \"2\"\n";
    assert!(
        set_cargo_toml_version(no_own, "9.8.7")
            .unwrap_err()
            .contains("[package]")
    );

    // A version line ABOVE [package] (a workspace table, say) stays put.
    let workspace_first = "[workspace.package]\nversion = \"0.0.1\"\n\n[package]\nname = \"x\"\nversion = \"1.0.0\"\n";
    assert_eq!(
        set_cargo_toml_version(workspace_first, "9.8.7").expect("bump"),
        workspace_first.replace("\"1.0.0\"", "\"9.8.7\"")
    );
}

#[test]
fn cargo_lock_the_screepub_desktop_entry_one_line_of_hundreds() {
    let before = real("desktop/src-tauri/Cargo.lock");
    assert!(before.matches("\nversion = ").count() > 100);
    let after = set_cargo_lock_version(&before, "9.8.7").expect("bump");
    assert_eq!(changed_lines(&before, &after), vec!["version = \"9.8.7\""]);
    assert!(after.contains("[[package]]\nname = \"screepub-desktop\"\nversion = \"9.8.7\"\n"));
}

#[test]
fn cargo_lock_without_exactly_one_screepub_desktop_entry_is_refused() {
    let entry = "[[package]]\nname = \"screepub-desktop\"\nversion = \"1.0.0\"\n";
    let other = "[[package]]\nname = \"other\"\nversion = \"1.0.0\"\n";
    assert!(
        set_cargo_lock_version(other, "9.8.7")
            .unwrap_err()
            .contains("screepub-desktop")
    );
    assert!(
        set_cargo_lock_version(&format!("{entry}\n{entry}"), "9.8.7")
            .unwrap_err()
            .contains("screepub-desktop")
    );
}

#[test]
fn rust_cargo_toml_the_workspace_package_version_and_no_other_line() {
    let before = real("rust/Cargo.toml");
    let after = xtask::bump_version::set_cargo_table_version(
        &before,
        "workspace.package",
        "9.8.7",
        "rust/Cargo.toml",
    )
    .expect("bump");
    assert_eq!(changed_lines(&before, &after), vec!["version = \"9.8.7\""]);
}

#[test]
fn rust_cargo_lock_every_crate_that_inherits_the_workspace_version_and_only_those() {
    let crates = workspace_crates(&root()).expect("crates");
    assert!(crates.contains(&"xtask".to_string()));
    assert!(crates.contains(&"sp-app".to_string()));
    // A crate with its own version keeps it.
    assert!(!crates.contains(&"sp-mtp".to_string()));
    let before = real("rust/Cargo.lock");
    let mut after = before.clone();
    for name in &crates {
        after = set_cargo_lock_entry(&after, name, "9.8.7", "rust/Cargo.lock").expect("bump");
    }
    let changed = changed_lines(&before, &after);
    assert_eq!(changed.len(), crates.len());
    assert!(changed.iter().all(|l| l == "version = \"9.8.7\""));
}

// ── bump_version ─────────────────────────────────────────────────────────

fn assert_bumped(dir: &Path, version: &str) {
    let files = version_files(dir);
    for f in &files {
        let text = std::fs::read_to_string(dir.join(f.path)).expect("read");
        match f.kind {
            Kind::Json => {
                let v: serde_json::Value = serde_json::from_str(&text).expect("JSON");
                assert_eq!(v["version"], version, "{}", f.path);
            }
            Kind::CargoTable(t) => {
                assert_eq!(
                    cargo_table_version(&text, t).as_deref(),
                    Some(version),
                    "{}",
                    f.path
                );
            }
            Kind::CargoLockEntry(name) => {
                assert!(text.contains(&format!("name = \"{name}\"\nversion = \"{version}\"\n")))
            }
            Kind::WorkspaceLockEntries => {
                for name in workspace_crates(dir).expect("crates") {
                    assert!(
                        text.contains(&format!("name = \"{name}\"\nversion = \"{version}\"\n")),
                        "{name}"
                    );
                }
            }
            Kind::Notes => {}
        }
    }
}

#[test]
fn sets_every_file_and_writes_the_windows_notes_from_the_release_notes() {
    let dir = repo(Some(GOOD_NOTES));
    let written = bump_version(dir.path(), "9.8.7", false).expect("bump");
    let expected: Vec<String> = version_files(dir.path())
        .iter()
        .map(|f| f.path.to_string())
        .collect();
    assert_eq!(written, expected);
    assert_bumped(dir.path(), "9.8.7");
    let notes = parse_release_notes("9.8.7", GOOD_NOTES).expect("parse");
    assert_eq!(dir.read("desktop/ui/notes.js"), render_notes_module(&notes));
    assert_eq!(
        dir.read("rust/crates/sp-app/src/model/notes_release.rs"),
        render_notes_rust(&notes)
    );
    // The crate with its own version keeps it.
    assert!(
        dir.read("rust/Cargo.lock")
            .contains("name = \"sp-mtp\"\nversion = \"0.1.0\"\n")
    );
}

#[test]
fn before_cutover_it_bumps_the_bun_engine_and_the_tauri_window_alone() {
    let dir = scratch_repo(Shape::BeforeCutover, Some(GOOD_NOTES), "9.8.7");
    let written = bump_version(dir.path(), "9.8.7", false).expect("bump");
    assert_eq!(
        written,
        vec![
            "package.json",
            "desktop/src-tauri/Cargo.toml",
            "desktop/src-tauri/Cargo.lock",
            "desktop/src-tauri/tauri.conf.json",
            "desktop/ui/notes.js",
        ]
    );
    assert_bumped(dir.path(), "9.8.7");
    assert!(!dir.join("rust").exists());
}

#[test]
fn after_cutover_it_bumps_the_rust_workspace_alone() {
    let dir = scratch_repo(Shape::AfterCutover, Some(GOOD_NOTES), "9.8.7");
    let written = bump_version(dir.path(), "9.8.7", false).expect("bump");
    assert_eq!(
        written,
        vec![
            "rust/Cargo.toml",
            "rust/Cargo.lock",
            "rust/crates/sp-app/src/model/notes_release.rs",
        ]
    );
    assert_bumped(dir.path(), "9.8.7");
    assert!(!dir.join("package.json").exists());
    assert!(!dir.join("desktop").exists());
}

#[test]
fn a_repository_with_no_version_files_is_refused() {
    let dir = Scratch::new("bump-version-empty");
    dir.write("docs/releases/9.8.7.md", GOOD_NOTES);
    let e = bump_version(dir.path(), "9.8.7", false).unwrap_err();
    assert!(e.contains("no version files"), "{e}");
}

#[test]
fn a_second_paragraph_before_the_first_heading_stops_it_and_nothing_is_written() {
    // The pre-lede line has to share the headline's paragraph. On its own
    // line it is a second paragraph.
    let notes = GOOD_NOTES.replace("\n\n**One line", "\n\n**A pre-lede line.**\n\n**One line");
    let dir = repo(Some(&notes));
    let before = snapshot(dir.path());
    let e = bump_version(dir.path(), "9.8.7", false).unwrap_err();
    assert!(e.contains("second paragraph"), "{e}");
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn a_cargo_lock_it_cannot_bump_stops_it_before_package_json_is_touched() {
    for (lock, name) in [
        ("desktop/src-tauri/Cargo.lock", "screepub-desktop"),
        ("rust/Cargo.lock", "sp-app"),
    ] {
        let dir = repo(Some(GOOD_NOTES));
        let renamed = dir
            .read(lock)
            .replace(&format!("name = \"{name}\"\n"), "name = \"renamed\"\n");
        dir.write(lock, renamed);
        let before = snapshot(dir.path());
        let e = bump_version(dir.path(), "9.8.7", false).unwrap_err();
        assert!(e.contains(name), "{e}");
        assert_eq!(snapshot(dir.path()), before);
    }
}

fn set_readonly(path: &Path, on: bool) {
    let mut p = std::fs::metadata(path).expect("meta").permissions();
    p.set_readonly(on);
    std::fs::set_permissions(path, p).expect("permissions");
}

#[test]
fn a_write_that_fails_part_way_is_undone_so_every_file_is_as_it_was() {
    // A read-only file late in the order fails after earlier files already
    // hold the new version: tauri.conf.json before cutover, rust/Cargo.lock
    // after it.
    for (shape, late) in [
        (Shape::Today, "desktop/src-tauri/tauri.conf.json"),
        (Shape::AfterCutover, "rust/Cargo.lock"),
    ] {
        let dir = scratch_repo(shape, Some(GOOD_NOTES), "9.8.7");
        let before = snapshot(dir.path());
        set_readonly(&dir.join(late), true);
        let result = bump_version(dir.path(), "9.8.7", false);
        set_readonly(&dir.join(late), false);
        assert!(result.is_err());
        assert_eq!(snapshot(dir.path()), before);
    }
}

#[test]
fn missing_notes_stop_it_and_nothing_is_written() {
    let dir = repo(None);
    let before = snapshot(dir.path());
    let e = bump_version(dir.path(), "9.8.7", false).unwrap_err();
    assert!(e.contains("docs/releases/9.8.7.md"));
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn a_version_the_release_workflow_would_reject_is_refused() {
    let dir = repo(Some(GOOD_NOTES));
    let before = snapshot(dir.path());
    for bad in ["v9.8.7", "9.8", "9.8.7.1", "9.8.7-", ""] {
        let e = bump_version(dir.path(), bad, false).unwrap_err();
        assert!(e.contains("version"), "{bad}: {e}");
    }
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn check_mode_reads_the_notes_the_way_the_window_will_and_writes_nothing() {
    let dir = repo(Some(GOOD_NOTES));
    let before = snapshot(dir.path());
    assert_eq!(
        bump_version(dir.path(), "9.8.7", true).expect("check"),
        Vec::<String>::new()
    );
    assert_eq!(snapshot(dir.path()), before);
    let bad = repo(Some(
        "# Screepub 9.8.7\n\nHeadline.\n\nA second paragraph.\n\n## X\n\n- y\n",
    ));
    let e = bump_version(bad.path(), "9.8.7", true).unwrap_err();
    assert!(e.contains("second paragraph"));
}

#[test]
fn the_real_repositorys_files_are_already_in_the_shape_it_needs() {
    // Bumping to the current version changes no byte of any version file:
    // proof the tool neither reformats nor misses a line on the real files.
    let v = current();
    let notes = real(&format!("docs/releases/{v}.md"));
    let dir = scratch_repo(Shape::Today, Some(&notes), &v);
    let before = snapshot(dir.path());
    bump_version(dir.path(), &v, false).expect("bump");
    assert_eq!(snapshot(dir.path()), before);
}

// ── the command ──────────────────────────────────────────────────────────

#[test]
fn it_names_every_file_it_wrote_and_fails_with_the_reason_and_nothing_written() {
    let dir = repo(Some(GOOD_NOTES));
    let repo_arg = dir.path().to_string_lossy().into_owned();
    let ok = xtask(
        &["bump-version", "9.8.7", "--repo", &repo_arg],
        &[],
        None,
        None,
    );
    assert_eq!(ok.status.code(), Some(0), "{}", text(&ok.stderr));
    let out = text(&ok.stdout);
    for f in version_files(dir.path()) {
        assert!(out.contains(f.path), "{}", f.path);
    }

    let bad = repo(None);
    let before = snapshot(bad.path());
    let bad_arg = bad.path().to_string_lossy().into_owned();
    let refused = xtask(
        &["bump-version", "9.8.7", "--repo", &bad_arg],
        &[],
        None,
        None,
    );
    assert_eq!(refused.status.code(), Some(1));
    assert!(text(&refused.stderr).contains("docs/releases/9.8.7.md"));
    assert_eq!(snapshot(bad.path()), before);
    assert!(!bad.join("docs/releases/9.8.7.md").exists());
}

// ── it covers what the release checks at a tag ──────────────────────────

#[test]
fn every_version_file_release_yml_compares_with_the_tag_is_one_this_tool_sets() {
    // The Rust release.yml compares the tag with ONE file, rust/Cargo.toml's
    // [workspace.package] version, through `sp-release version-check`.
    let checks = real(".github/workflows/release.yml");
    let step = &checks[checks
        .find("The workspace version is the tag's")
        .expect("the step")..];
    assert!(step.contains("sp-release -- version-check"), "{step}");
    assert!(step.contains("rust/Cargo.toml"), "{step}");
    let all: Vec<&str> = GROUPS
        .iter()
        .flat_map(|g| g.files.iter().map(|f| f.path))
        .collect();
    assert!(all.contains(&"rust/Cargo.toml"));
    assert!(all.contains(&"rust/Cargo.lock"));
}

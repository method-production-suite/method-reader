//! `cargo xtask review-screens` (was tests/review-screens.test.ts): which
//! pictures a capture changed, shown old beside new, so the owner can check
//! them at /release's first moment the way he checks the notes, including
//! that nothing real is in frame (spec 2026-09-22, part 5).
//!
//! Run against scratch git repositories, never this one: what changed is
//! asked of git, and the answer has to come from a history the test made.

use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine as _;
use xtask::capture::review::{ChangedPicture, changed_pictures, picture_dirs, review_page};
use xtask::capture::shots::{SHOTS, all_outputs};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "screepub-review-screens-{tag}-{}",
        xtask::capture::random_token()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Stand-ins for PNGs: the tool compares bytes and never decodes them.
fn png(label: &str) -> Vec<u8> {
    let mut b = vec![0x89];
    b.extend_from_slice(format!("PNG fake {label}").as_bytes());
    b
}

fn put(dir: &Path, path: &str, bytes: &[u8]) {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// A repository whose last commit holds the light drop picture in both
/// places, the dark one in the README's, and a stray non-picture file.
fn scratch_repo() -> Scratch {
    let s = scratch("repo");
    let dir = &s.0;
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "dev@example.com"]);
    git(dir, &["config", "user.name", "Dev"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    git(dir, &["config", "core.autocrlf", "false"]);
    put(dir, "assets/screens/drop-light.png", &png("drop old"));
    put(dir, "site/img/drop-light.png", &png("drop old"));
    put(dir, "assets/screens/drop-dark.png", &png("dark old"));
    put(dir, "assets/screens/notes.txt", b"not a picture");
    put(dir, "README.md", b"readme");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "first"]);
    s
}

type Row = (Vec<String>, Option<Vec<u8>>, Option<Vec<u8>>);

fn rows(found: &[ChangedPicture]) -> Vec<Row> {
    found
        .iter()
        .map(|p| (p.paths.clone(), p.before.clone(), p.after.clone()))
        .collect()
}

fn paths(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn it_looks_where_the_capture_tool_writes_and_nowhere_else() {
    let mut dirs: Vec<String> = all_outputs(SHOTS)
        .iter()
        .map(|p| {
            Path::new(p)
                .parent()
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    dirs.sort();
    dirs.dedup();
    assert_eq!(picture_dirs(), dirs);
}

#[test]
fn nothing_changed_an_empty_list() {
    let r = scratch_repo();
    assert!(changed_pictures(&r.0).unwrap().is_empty());
}

#[test]
fn the_same_new_picture_in_two_places_is_one_picture_with_both_paths() {
    let r = scratch_repo();
    put(&r.0, "assets/screens/drop-light.png", &png("drop new"));
    put(&r.0, "site/img/drop-light.png", &png("drop new"));
    let found = changed_pictures(&r.0).unwrap();
    assert_eq!(
        rows(&found),
        vec![(
            paths(&["assets/screens/drop-light.png", "site/img/drop-light.png"]),
            Some(png("drop old")),
            Some(png("drop new"))
        )]
    );
}

#[test]
fn new_and_removed_pictures_say_so_and_a_staged_change_still_counts() {
    let r = scratch_repo();
    put(&r.0, "assets/screens/hero-light.png", &png("hero"));
    std::fs::remove_file(r.0.join("assets/screens/drop-dark.png")).unwrap();
    put(&r.0, "site/img/drop-light.png", &png("site only"));
    git(&r.0, &["add", "site/img/drop-light.png"]);
    assert_eq!(
        rows(&changed_pictures(&r.0).unwrap()),
        vec![
            (
                paths(&["assets/screens/drop-dark.png"]),
                Some(png("dark old")),
                None
            ),
            (
                paths(&["assets/screens/hero-light.png"]),
                None,
                Some(png("hero"))
            ),
            (
                paths(&["site/img/drop-light.png"]),
                Some(png("drop old")),
                Some(png("site only"))
            ),
        ]
    );
}

#[test]
fn a_staged_rename_shows_the_old_path_going_as_well_as_the_new_one_arriving() {
    let r = scratch_repo();
    git(
        &r.0,
        &[
            "mv",
            "assets/screens/drop-dark.png",
            "assets/screens/drop-night.png",
        ],
    );
    assert_eq!(
        rows(&changed_pictures(&r.0).unwrap()),
        vec![
            (
                paths(&["assets/screens/drop-dark.png"]),
                Some(png("dark old")),
                None
            ),
            (
                paths(&["assets/screens/drop-night.png"]),
                None,
                Some(png("dark old"))
            ),
        ]
    );
}

#[test]
fn files_that_are_not_pictures_or_not_in_those_folders_are_not_its_business() {
    let r = scratch_repo();
    put(&r.0, "assets/screens/notes.txt", b"changed");
    put(&r.0, "README.md", b"changed");
    put(&r.0, "assets/other.png", &png("elsewhere"));
    assert!(changed_pictures(&r.0).unwrap().is_empty());
}

fn page() -> String {
    review_page(&[
        ChangedPicture {
            paths: paths(&["assets/screens/drop-light.png", "site/img/drop-light.png"]),
            before: Some(png("a")),
            after: Some(png("b")),
        },
        ChangedPicture {
            paths: paths(&["assets/screens/<new>.png"]),
            before: None,
            after: Some(png("c")),
        },
        ChangedPicture {
            paths: paths(&["assets/screens/gone.png"]),
            before: Some(png("d")),
            after: None,
        },
    ])
}

fn b64(label: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(png(label))
}

#[test]
fn every_picture_is_inside_the_page_old_beside_new_so_it_opens_anywhere() {
    let page = page();
    for label in ["a", "b", "c", "d"] {
        assert!(page.contains(&format!("data:image/png;base64,{}", b64(label))));
    }
    assert!(page.find(&b64("a")).unwrap() < page.find(&b64("b")).unwrap());
    for part in page.split("src=\"").skip(1) {
        assert!(
            part.starts_with("data:"),
            "a src that is not data: {}",
            &part[..20]
        );
    }
}

#[test]
fn the_page_names_every_path_escaped_and_says_which_are_new_or_gone() {
    let page = page();
    assert!(page.contains("assets/screens/drop-light.png"));
    assert!(page.contains("site/img/drop-light.png"));
    assert!(page.contains("assets/screens/&lt;new&gt;.png"));
    assert!(!page.contains("<new>"));
    assert!(page.contains("No picture before"));
    assert!(page.contains("Removed"));
}

#[test]
fn the_page_asks_for_the_confidentiality_check_in_plain_words_with_no_em_dash() {
    let page = page();
    assert!(page.contains("anything real in frame"));
    assert!(page.contains("Field Station"));
    assert!(!page.contains('\u{2014}') && !page.contains("&mdash;"));
}

struct Ran {
    code: i32,
    out: String,
    err: String,
}

fn run(dir: &Path, more: &[&str]) -> Ran {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("review-screens")
        .arg("--repo")
        .arg(dir)
        .args(more)
        .output()
        .unwrap();
    Ran {
        code: out.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

#[test]
fn the_command_with_nothing_changed_prints_one_line_and_no_page() {
    let r = scratch_repo();
    let s = scratch("out");
    let out = s.0.join("unchanged.html");
    let ran = run(&r.0, &["--out", &out.to_string_lossy()]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out, "No picture changed.\n");
    assert!(!out.exists());
}

#[test]
fn the_command_writes_the_page_and_names_each_picture_on_a_line_of_its_own() {
    let r = scratch_repo();
    put(&r.0, "assets/screens/drop-light.png", &png("drop new"));
    put(&r.0, "site/img/drop-light.png", &png("drop new"));
    put(&r.0, "assets/screens/hero-light.png", &png("hero"));
    let s = scratch("out");
    let out = s.0.join("changed.html");
    let ran = run(&r.0, &["--out", &out.to_string_lossy()]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let lines: Vec<&str> = ran.out.split('\n').collect();
    assert_eq!(
        lines,
        [
            format!("2 pictures changed. Old beside new: {}", out.display()).as_str(),
            "  changed  assets/screens/drop-light.png site/img/drop-light.png",
            "  new      assets/screens/hero-light.png",
            "",
        ]
    );
    assert!(
        std::fs::read_to_string(&out)
            .unwrap()
            .contains(&b64("hero"))
    );
}

#[test]
fn the_command_without_out_refuses_before_looking() {
    let r = scratch_repo();
    let ran = run(&r.0, &[]);
    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("--out"));
}

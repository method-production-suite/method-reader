//! No test leaves anything behind in the system temp folder (was
//! tests/temp-hygiene.test.ts, in its Rust form).
//!
//! About 23,000 screepub-* folders once piled up in $TMPDIR from test
//! helpers that made a fresh folder per test and never removed it. The
//! TypeScript rule was one scratch folder per file, removed in afterAll.
//! Rust tests have no afterAll; their equivalent is a value that removes
//! its folder when it is dropped, on a failing test's unwind as well as a
//! passing one's. So the rule, for every test source (each crate's tests/
//! and the `#[cfg(test)]` part of its src/):
//!
//!   1. `temp_dir()` is called only inside a constructor (`-> Self` in
//!      `impl T`, or `-> T`) of a type T that the same file gives an
//!      `impl Drop for T` calling `remove_dir_all` (or `remove_file`, for a
//!      single temp file). Everything else gets its folder from such a
//!      type, or from tempfile, whose TempDir removes itself.
//!   2. Nothing switches that removal off: no `.keep()` or `.into_path()`,
//!      except on a folder made with `tempdir_in` inside one that is itself
//!      removed (the TypeScript rule's "every other folder INSIDE the
//!      scratch one"), no `disable_cleanup(`, and no `mem::forget(` of
//!      anything but the guards listed in NOT_FOLDERS.
//!
//! Why a rule about the SOURCE and not a count of the temp folder: the
//! folder is shared, and another worktree's run or a real capture adds and
//! removes entries at the same moment. What this cannot see: a PROCESS a
//! test starts that writes there on its own account; the repository's
//! tools are held to that by tool_temp_folders.rs.

mod repo_common;

use regex::Regex;
use repo_common::*;
use std::path::{Path, PathBuf};

/// Files allowed more `temp_dir()` lines than their constructors, and why.
/// Each READS the temp folder's path and writes nothing there. Keep short.
const READS_TEMP_DIR: &[(&str, &str)] = &[
    (
        "sp-engine/tests/guards.rs",
        "asserts the Calibre settings copy lives under the temp folder",
    ),
    (
        "xtask/tests/capture_chrome.rs",
        "asserts the capture puts its Chrome profile under the temp folder",
    ),
];

/// Values a test forgets on purpose that are not folders, and why.
const NOT_FOLDERS: &[(&str, &str)] = &[(
    "pause_camera_import(",
    "the camera-import pause guard, forgotten to stand for a run that crashed",
)];

/// The lines that are code: comment lines dropped (as the TypeScript rule
/// did), line numbers kept as blanks.
fn code_lines(source: &str) -> Vec<String> {
    source
        .lines()
        .map(|l| {
            let t = l.trim_start();
            if t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') {
                String::new()
            } else {
                l.to_string()
            }
        })
        .collect()
}

/// The text of the block opened on line `i` (or the first `{` after it),
/// to its matching close.
fn block_from(lines: &[String], i: usize) -> String {
    let mut depth = 0i32;
    let mut started = false;
    let mut out = String::new();
    for line in &lines[i..] {
        out.push_str(line);
        out.push('\n');
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    started = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        if started && depth <= 0 {
            break;
        }
    }
    out
}

/// The types this file gives a Drop that removes their folder.
fn self_removing(lines: &[String]) -> Vec<String> {
    let re = Regex::new(r"impl\s+Drop\s+for\s+(\w+)").unwrap();
    let mut out = vec![];
    for (i, line) in lines.iter().enumerate() {
        if let Some(c) = re.captures(line)
            && (block_from(lines, i).contains("remove_dir_all")
                || block_from(lines, i).contains("remove_file"))
        {
            out.push(c[1].to_string());
        }
    }
    out
}

/// The type the function enclosing line `at` constructs, if it is a
/// constructor: `-> Self` inside `impl T`, or `-> T`.
fn constructed_by(lines: &[String], at: usize) -> Option<String> {
    let fn_line = (0..=at)
        .rev()
        .find(|&i| Regex::new(r"\bfn\s+\w+").unwrap().is_match(&lines[i]))?;
    // The signature, which rustfmt may split, up to its opening brace.
    let mut sig = String::new();
    for line in &lines[fn_line..=at] {
        sig.push_str(line);
        if line.contains('{') {
            break;
        }
    }
    let ret = Regex::new(r"->\s*(\w+)").unwrap().captures(&sig)?[1].to_string();
    if ret != "Self" {
        return Some(ret);
    }
    let imp = Regex::new(r"^\s*impl(?:<[^>]*>)?\s+(\w+)").unwrap();
    (0..fn_line)
        .rev()
        .find_map(|i| imp.captures(&lines[i]).map(|c| c[1].to_string()))
}

/// What is wrong with one test source, as sentences. Empty is clean.
fn temp_problems(source: &str, allowed_reads: usize) -> Vec<String> {
    let lines = code_lines(source);
    let owners = self_removing(&lines);
    let mut reads = allowed_reads;
    let mut problems = vec![];
    for (i, line) in lines.iter().enumerate() {
        if line.contains("temp_dir()") {
            match constructed_by(&lines, i) {
                Some(t) if owners.contains(&t) => {}
                _ if reads > 0 => reads -= 1,
                Some(t) => problems.push(format!(
                    "line {}: temp_dir() in a constructor of {t}, which has no Drop that removes its folder",
                    i + 1
                )),
                None => problems.push(format!(
                    "line {}: temp_dir() outside a self-removing scratch type: {}",
                    i + 1,
                    line.trim()
                )),
            }
        }
        // The statement so far: rustfmt puts a builder chain on lines of
        // its own, so look back to the line that starts it.
        let start = (0..=i)
            .rev()
            .find(|&j| j == 0 || lines[j - 1].trim_end().ends_with([';', '{', '}']))
            .unwrap_or(i);
        let statement = lines[start..=i].join("\n");
        for keep in [".keep()", ".into_path()"] {
            if line.contains(keep) && !statement.contains("tempdir_in(") {
                problems.push(format!(
                    "line {}: {keep} stops a temp folder removing itself",
                    i + 1
                ));
            }
        }
        if line.contains("disable_cleanup(") {
            problems.push(format!(
                "line {}: disable_cleanup( stops a temp folder removing itself",
                i + 1
            ));
        }
        if let Some(at) = line.find("mem::forget(") {
            let what = &line[at + "mem::forget(".len()..];
            if !NOT_FOLDERS.iter().any(|(call, _)| what.starts_with(call)) {
                problems.push(format!(
                    "line {}: mem::forget( stops a temp folder removing itself",
                    i + 1
                ));
            }
        }
    }
    problems
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every test source in the workspace: (path under rust/crates/, the test
/// code). For a src/ file, only what follows its `#[cfg(test)]`.
fn test_sources() -> Vec<(String, String)> {
    let crates = root().join("rust/crates");
    let mut out = vec![];
    for entry in std::fs::read_dir(&crates).expect("rust/crates") {
        let dir = entry.unwrap().path();
        for (sub, tests_only) in [("tests", false), ("src", true)] {
            let mut files = vec![];
            rust_files(&dir.join(sub), &mut files);
            for f in files {
                let text = std::fs::read_to_string(&f).expect("readable");
                let code = if tests_only {
                    match text.find("#[cfg(test)]") {
                        Some(at) => text[at..].to_string(),
                        None => continue,
                    }
                } else {
                    text
                };
                let rel = f
                    .strip_prefix(&crates)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, code));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_test_keeps_its_temp_folders_in_values_that_remove_them() {
    let sources = test_sources();
    assert!(sources.len() > 150, "{}", sources.len());
    let mut report = vec![];
    for (file, code) in &sources {
        // This file is skipped: the samples below are the rule's own cases.
        if file == "xtask/tests/temp_hygiene.rs" {
            continue;
        }
        let reads = usize::from(READS_TEMP_DIR.iter().any(|(f, _)| f == file));
        for p in temp_problems(code, reads) {
            report.push(format!("{file}: {p}"));
        }
    }
    assert_eq!(report, Vec::<String>::new());
}

#[test]
fn every_exception_still_names_a_file_that_reads_the_temp_folder() {
    // An exception outliving its reason would quietly widen the rule.
    let sources = test_sources();
    for (file, _) in READS_TEMP_DIR {
        let (_, code) = sources
            .iter()
            .find(|(f, _)| f == file)
            .unwrap_or_else(|| panic!("{file} is gone"));
        assert!(code.contains("temp_dir()"), "{file}");
        // Exactly one read: clean with the allowance, one problem without.
        assert_eq!(temp_problems(code, 1), Vec::<String>::new(), "{file}");
        assert_eq!(temp_problems(code, 0).len(), 1, "{file} reads it once");
    }
}

// The rule, run against sources written to break it, so a pattern that
// stops matching anything shows up here and not as a silent pass above.

const GOOD: &str = "struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(tag);
        Scratch(dir)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
";

#[test]
fn the_pattern_itself_passes() {
    assert_eq!(temp_problems(GOOD, 0), Vec::<String>::new());
    assert_eq!(
        temp_problems("#[test]\nfn no_temp_at_all() {}\n", 0),
        Vec::<String>::new()
    );
    // tempfile's TempDir removes itself.
    assert_eq!(
        temp_problems("let dir = tempfile::tempdir().unwrap();\n", 0),
        Vec::<String>::new()
    );
    // A free constructor of the type counts, as a method does.
    let free = GOOD.replace(
        "impl Scratch {\n    fn new(tag: &str) -> Self {",
        "fn scratch(tag: &str) -> Scratch {\n    {",
    );
    assert_eq!(temp_problems(&free, 0), Vec::<String>::new());
    // So does a signature rustfmt split over lines.
    let split = GOOD.replace(
        "fn new(tag: &str) -> Self {",
        "fn new(\n        tag: &str,\n    ) -> Self {",
    );
    assert_eq!(temp_problems(&split, 0), Vec::<String>::new());
}

#[test]
fn a_folder_made_straight_under_the_temp_folder_in_a_test_fails() {
    let src = format!(
        "{GOOD}#[test]\nfn t() {{\n    let dir = std::env::temp_dir().join(\"screepub-test\");\n}}\n"
    );
    assert!(temp_problems(&src, 0)[0].contains("outside a self-removing scratch type"));
}

#[test]
fn a_fixed_file_name_under_the_temp_folder_fails() {
    let src = format!(
        "{GOOD}fn out() -> PathBuf {{\n    std::env::temp_dir().join(\"screepub-read.epub\")\n}}\n"
    );
    assert!(temp_problems(&src, 0)[0].contains("PathBuf"));
}

#[test]
fn a_scratch_type_that_never_removes_its_folder_fails() {
    let no_drop = GOOD.split("impl Drop").next().unwrap();
    assert!(temp_problems(no_drop, 0)[0].contains("no Drop that removes"));
    let empty_drop = GOOD.replace("let _ = std::fs::remove_dir_all(&self.0);", "");
    assert!(temp_problems(&empty_drop, 0)[0].contains("no Drop that removes"));
}

#[test]
fn switching_tempfiles_removal_off_fails() {
    for keep in [
        "let p = tempfile::tempdir().unwrap().keep();",
        "let p = tempfile::tempdir().unwrap().into_path();",
        "let mut b = tempfile::Builder::new(); b.disable_cleanup(true);",
        "std::mem::forget(dir);",
    ] {
        assert_eq!(temp_problems(keep, 0).len(), 1, "{keep}");
    }
}

#[test]
fn keeping_a_folder_made_inside_a_removed_one_passes() {
    let inside = "    let parent = tempfile::Builder::new()\n        .prefix(\"test-\")\n        .tempdir_in(scratch)\n        .unwrap()\n        .keep();\n";
    assert_eq!(temp_problems(inside, 0), Vec::<String>::new());
    let outside = "    let parent = tempfile::Builder::new()\n        .prefix(\"test-\")\n        .tempdir()\n        .unwrap()\n        .keep();\n";
    assert_eq!(temp_problems(outside, 0).len(), 1);
}

#[test]
fn a_single_temp_file_that_removes_itself_passes() {
    let file = GOOD.replace("remove_dir_all", "remove_file");
    assert_eq!(temp_problems(&file, 0), Vec::<String>::new());
}

#[test]
fn forgetting_a_listed_guard_passes_and_anything_else_fails() {
    assert_eq!(
        temp_problems("    std::mem::forget(pause_camera_import(&d));\n", 0),
        Vec::<String>::new()
    );
    assert_eq!(temp_problems("    std::mem::forget(dir);\n", 0).len(), 1);
}

#[test]
fn a_commented_out_example_does_not_count() {
    let src = format!("{GOOD}// let dir = std::env::temp_dir().join(\"x\");\n/// .keep()\n");
    assert_eq!(temp_problems(&src, 0), Vec::<String>::new());
}

#[test]
fn an_allowed_read_is_allowed_once_not_twice() {
    let read = "    assert!(p.starts_with(std::env::temp_dir()));\n";
    let once = format!("{GOOD}#[test]\nfn t() {{\n{read}}}\n");
    assert_eq!(temp_problems(&once, 1), Vec::<String>::new());
    let twice = format!("{GOOD}#[test]\nfn t() {{\n{read}{read}}}\n");
    assert_eq!(temp_problems(&twice, 1).len(), 1);
}

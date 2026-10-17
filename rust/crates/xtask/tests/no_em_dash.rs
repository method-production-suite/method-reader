//! No em dash in any text a person reads (tests/no-em-dash.test.ts, the
//! engine's half and the repository's copy; the window's half is
//! sp-app/tests/no_em_dash.rs). The owner's rule for every message the
//! engine prints or puts in an answer, and for the pages and notes a
//! person reads: a colon or a period says the same thing plainly.
//!
//! Engine code: every crate's src/ but the window's (sp-app has its own
//! scan) and the developer tools' (xtask), with comments taken out first
//! and each file cut at its `#[cfg(test)]` module, so what is left is
//! the strings the crate can print or write. Every em dash left must be on
//! the list below, each a dash that is DATA rather than words (the parser
//! reading a script's own dashes: never change how a script's dashes are
//! read or rendered), and each entry must still be found, so the list
//! cannot go stale and hide a new dash behind an old excuse.

mod repo_common;

use repo_common::*;
use std::path::{Path, PathBuf};

const EM_DASH: char = '\u{2014}';

/// The source with every comment blanked, string and char literals kept
/// (the same reader as sp-app/tests/no_em_dash.rs).
fn code_only(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '/' if next == Some('/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if next == Some('*') => {
                let mut depth = 0;
                while i < chars.len() {
                    if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                        depth += 1;
                        i += 2;
                    } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if chars[i] == '\n' {
                            out.push('\n');
                        }
                        i += 1;
                    }
                }
            }
            'r' if matches!(next, Some('"') | Some('#'))
                && !chars
                    .get(i.wrapping_sub(1))
                    .is_some_and(|p| p.is_alphanumeric() || *p == '_') =>
            {
                // A raw string: r"...", r#"..."#, and so on.
                let mut j = i + 1;
                let mut hashes = 0;
                while chars.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                if chars.get(j) != Some(&'"') {
                    out.push(c);
                    i += 1;
                    continue;
                }
                let close: String = std::iter::once('"')
                    .chain(std::iter::repeat_n('#', hashes))
                    .collect();
                let rest: String = chars[j + 1..].iter().collect();
                let end = rest.find(&close).map_or(rest.len(), |e| e + close.len());
                let literal: String = chars[i..j + 1].iter().collect::<String>() + &rest[..end];
                i += literal.chars().count();
                out.push_str(&literal);
            }
            '"' => {
                out.push(c);
                i += 1;
                while i < chars.len() {
                    out.push(chars[i]);
                    if chars[i] == '\\' {
                        if let Some(&e) = chars.get(i + 1) {
                            out.push(e);
                        }
                        i += 2;
                        continue;
                    }
                    i += 1;
                    if chars[i - 1] == '"' {
                        break;
                    }
                }
            }
            '\'' => {
                // A char literal ('x', '\n', '\u{2014}') or a lifetime ('a).
                let close = if next == Some('\\') {
                    chars[i + 2..]
                        .iter()
                        .position(|&x| x == '\'')
                        .map(|p| i + 2 + p)
                } else if chars.get(i + 2) == Some(&'\'') {
                    Some(i + 2)
                } else {
                    None
                };
                match close {
                    Some(end) => {
                        out.extend(&chars[i..=end]);
                        i = end + 1;
                    }
                    None => {
                        out.push(c);
                        i += 1;
                    }
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// (file under rust/crates/, why, a piece of the one code line it is on)
const ALLOWED: &[(&str, &str, &str)] = &[
    (
        "sp-fountain/src/serialize.rs",
        "dashes a script puts between a heading and its inline text",
        "r\"^[\\s/:\u{2013}\u{2014}-]+\"",
    ),
    (
        "sp-parser/src/boilerplate.rs",
        "dashes in a revision stamp on the page, as scripts print them",
        "[\\s\u{2014}\u{2013}-]*)+$",
    ),
    (
        "sp-epub/src/css.rs",
        "a comment inside the EPUB's own stylesheet, not text a reader sees",
        "Original PDF page numbers \u{2014} a marginal reference",
    ),
    (
        "sp-epub/src/css.rs",
        "a comment inside the EPUB's own stylesheet, not text a reader sees",
        "docs/pagination-reference.md \u{a7}2 \u{2014}",
    ),
];

/// The crates whose strings the engine prints or writes: all but the
/// window's and the tools'.
fn engine_sources() -> Vec<(String, PathBuf)> {
    let crates = root().join("rust/crates");
    let mut out = vec![];
    for entry in std::fs::read_dir(&crates).expect("rust/crates") {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        if name == "sp-app" || name == "xtask" || !dir.join("src").is_dir() {
            continue;
        }
        let mut files = vec![];
        rust_files(&dir.join("src"), &mut files);
        for f in files {
            let rel = f
                .strip_prefix(&crates)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, f));
        }
    }
    out.sort();
    out
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("a source folder") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The file up to its unit-test module.
fn before_tests(source: &str) -> &str {
    source
        .find("#[cfg(test)]")
        .map_or(source, |at| &source[..at])
}

#[test]
fn the_scan_sees_strings_and_not_comments_or_unit_tests() {
    let source = format!(
        "// a {EM_DASH} b\n/* c {EM_DASH} d */\n/// e {EM_DASH}\nconst X: &str = \"f {EM_DASH} g\";\nconst Y: &str = r#\"h {EM_DASH}\"#;\n#[cfg(test)]\nmod tests {{ const Z: &str = \"{EM_DASH}\"; }}\n"
    );
    let lines: Vec<String> = code_only(before_tests(&source))
        .lines()
        .filter(|l| l.contains(EM_DASH))
        .map(str::to_string)
        .collect();
    assert_eq!(
        lines,
        vec![
            format!("const X: &str = \"f {EM_DASH} g\";"),
            format!("const Y: &str = r#\"h {EM_DASH}\"#;"),
        ]
    );
    let files = engine_sources();
    assert!(files.len() > 100, "the whole engine: {}", files.len());
    for must in [
        "sp-cli/src/main.rs",
        "sp-engine/src/lib.rs",
        "sp-parser/src/lib.rs",
        "sp-device-requests/src/lib.rs",
    ] {
        assert!(files.iter().any(|(r, _)| r == must), "{must}");
    }
    assert!(
        !files
            .iter()
            .any(|(r, _)| r.starts_with("sp-app/") || r.starts_with("xtask/"))
    );
}

#[test]
fn every_em_dash_in_engine_code_is_data_on_the_allow_list_and_nothing_else() {
    let mut found = vec![];
    let mut used = vec![false; ALLOWED.len()];
    for (name, file) in engine_sources() {
        let source = std::fs::read_to_string(&file).expect("readable");
        let code = code_only(before_tests(&source));
        for line in code.lines() {
            let escaped = line.to_lowercase().contains("\\u{2014}");
            if !line.contains(EM_DASH) && !escaped {
                continue;
            }
            match ALLOWED
                .iter()
                .position(|(f, _, on)| *f == name && line.contains(on))
            {
                Some(at) => used[at] = true,
                None => found.push(format!("{name}: {}", line.trim())),
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
    let stale: Vec<String> = ALLOWED
        .iter()
        .zip(used)
        .filter(|(_, u)| !u)
        .map(|((f, why, _), _)| format!("{f}: {why}"))
        .collect();
    assert_eq!(
        stale,
        Vec::<String>::new(),
        "every excuse still has its dash"
    );
}

/// The repository's copy a person reads: the README and its neighbours,
/// the site, the brand pages, the reader-facing docs and every release's
/// notes. (Developer notes under docs/ and the dated records are not
/// copy.)
fn copy_files() -> Vec<String> {
    let mut files: Vec<String> = [
        "README.md",
        "CONTRIBUTING.md",
        "SECURITY.md",
        "ROADMAP.md",
        "brand/README.md",
        "docs/developers.md",
        "docs/RELEASING.md",
        "docs/fountain-input.md",
        "docs/send-to-kindle.md",
        "docs/release-notes-template.md",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for (dir, ext) in [
        ("site", "html"),
        ("brand/components", "html"),
        ("docs/releases", "md"),
    ] {
        for entry in std::fs::read_dir(root().join(dir)).unwrap_or_else(|e| panic!("{dir}: {e}")) {
            let p = entry.unwrap().path();
            if p.extension().is_some_and(|e| e == ext) {
                files.push(format!(
                    "{dir}/{}",
                    p.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn the_copy_a_person_reads_carries_no_em_dash_in_any_spelling() {
    let files = copy_files();
    assert!(files.len() > 30, "{}", files.len());
    assert!(files.contains(&"site/index.html".to_string()));
    let spellings = ["\u{2014}", "&mdash;", "&#8212;", "&#x2014;", "&#X2014;"];
    let mut found = vec![];
    for f in files {
        let text = read(&f);
        for (n, line) in text.lines().enumerate() {
            if spellings.iter().any(|s| line.contains(s)) {
                found.push(format!("{f}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
}

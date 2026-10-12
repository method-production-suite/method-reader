//! No em dash in any text a person reads (tests/no-em-dash.test.ts, the
//! window's half): every string the window shows. A colon or a period says
//! the same thing plainly.
//!
//! Comments are out of scope, so each source has its comments taken out
//! first, and what is left is code: string and char literals. Every em dash
//! left must be on the list below, each a dash that is DATA rather than
//! words, and each entry must still be found, so the list cannot go stale
//! and hide a new dash behind an old excuse.

use std::path::{Path, PathBuf};

const EM_DASH: char = '\u{2014}';

/// (file under src/, why, a piece of the one line it is on)
const ALLOWED: &[(&str, &str, &str)] = &[(
    "model/read.rs",
    "the separator between a scene heading's place and time, as scripts write it",
    "matches!(chars[i + 1], '-' | '\u{2013}' | '\u{2014}')",
)];

/// The source with every comment blanked, string and char literals kept.
/// Line breaks survive, so line numbers do too.
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

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

#[test]
fn the_scan_sees_strings_and_not_comments() {
    let source = format!(
        "// a {EM_DASH} b\n/* c {EM_DASH} d */\n/// e {EM_DASH}\nconst X: &str = \"f {EM_DASH} g\";\nconst Y: &str = r#\"h {EM_DASH}\"#;\nfn f<'a>(_: &'a str) -> char {{ '{EM_DASH}' }}\nconst Z: &str = \"// not {EM_DASH} a comment\";\n"
    );
    let lines: Vec<String> = code_only(&source)
        .lines()
        .filter(|l| l.contains(EM_DASH))
        .map(str::to_string)
        .collect();
    assert_eq!(
        lines,
        vec![
            format!("const X: &str = \"f {EM_DASH} g\";"),
            format!("const Y: &str = r#\"h {EM_DASH}\"#;"),
            format!("fn f<'a>(_: &'a str) -> char {{ '{EM_DASH}' }}"),
            format!("const Z: &str = \"// not {EM_DASH} a comment\";"),
        ]
    );
    let mut files = vec![];
    rust_files(&src(), &mut files);
    assert!(files.len() > 20, "the whole window: {}", files.len());
}

#[test]
fn every_em_dash_in_window_code_is_data_on_the_allow_list_and_nothing_else() {
    let mut files = vec![];
    rust_files(&src(), &mut files);
    files.sort();
    let mut found = vec![];
    let mut used = vec![false; ALLOWED.len()];
    for file in files {
        let name = file
            .strip_prefix(src())
            .expect("under src")
            .to_string_lossy()
            .replace('\\', "/");
        let code = code_only(&std::fs::read_to_string(&file).expect("readable"));
        // The escaped form draws the same dash.
        for line in code
            .lines()
            .filter(|l| l.to_lowercase().contains("\\u{2014}"))
        {
            found.push(format!("{name}: {}", line.trim()));
        }
        for line in code.lines().filter(|l| l.contains(EM_DASH)) {
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
    let stale: Vec<_> = ALLOWED
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

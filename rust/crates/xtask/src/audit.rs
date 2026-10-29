//! `cargo xtask audit-corpus`: convert every real script and count the
//! shapes in the parser's element list that mean a speech was misread.
//!
//! Every misread found by reading a book on a Kindle (a wrapped
//! parenthetical, one printed deeper than its cue, a dual exchange that
//! swallowed the next cue) left one of these shapes behind, and none needs
//! a person to spot it. So the corpus can be asked before anyone reads it.
//!
//! It reads the scripts in root fixtures/ (gitignored, real) and any folder
//! named with --dir, and prints counts by number, never a title, unless
//! --names is given. Nothing it prints is meant for a commit.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::paths;

pub const USAGE: &str = "\
usage: cargo xtask audit-corpus [options]

Converts every real script and counts the shapes in the parser's output
that mean a speech was misread:

  bracketed-description  a line in brackets read as description, right
                         after a speech (a parenthetical lost); (MORE) is
                         left out, as the book drops it anyway
  split-bracket          description opening a bracket it does not close,
                         after a speech, and the line that closes it (a
                         wrapped parenthetical cut in two)
  name-in-speech         a speaker's name read as a line of someone's speech

Scripts: the PDFs in root fixtures/, plus every PDF under each --dir.

  --dir <folder>      also every PDF under <folder> (repeatable)
  --names             print each script's file name (local use only: real
                      titles must never reach a commit or a doc)
  --show              print each finding, with its page and the lines around it
  --rust-cli <path>   the Rust binary (default: build crates/sp-cli)

Exit: 0 nothing found, 1 something found, 2 could not run.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    BracketedDescription,
    SplitBracket,
    NameInSpeech,
}

impl Shape {
    pub const ALL: [Shape; 3] = [
        Shape::BracketedDescription,
        Shape::SplitBracket,
        Shape::NameInSpeech,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Shape::BracketedDescription => "bracketed-description",
            Shape::SplitBracket => "split-bracket",
            Shape::NameInSpeech => "name-in-speech",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub shape: Shape,
    /// Where in the element list.
    pub index: usize,
    pub page: u64,
}

fn kind(e: &Value) -> &str {
    e["type"].as_str().unwrap_or("")
}

fn text(e: &Value) -> &str {
    e["text"].as_str().unwrap_or("").trim()
}

/// Page numbers and pictures sit between a cue and its speech without
/// being either.
fn passes_over(e: &Value) -> bool {
    matches!(kind(e), "page-number" | "image")
}

fn in_speech(e: Option<&Value>) -> bool {
    e.is_some_and(|e| matches!(kind(e), "character" | "dialogue" | "parenthetical"))
}

/// A cue's name without its extensions: "SYD (CONT'D)" -> "SYD".
fn base_name(cue: &str) -> &str {
    cue.split(" (").next().unwrap_or(cue).trim()
}

fn unbalanced(s: &str) -> bool {
    s.matches('(').count() != s.matches(')').count()
}

/// Every misread shape in one script's elements, in the order they occur.
pub fn findings(elements: &[Value]) -> Vec<Finding> {
    let roster: Vec<&str> = elements
        .iter()
        .filter(|e| kind(e) == "character" && e["isTitlePage"] != Value::Bool(true))
        .map(|e| base_name(text(e)))
        .collect();
    let mut out = Vec::new();
    // The element before this one, page numbers and pictures passed over.
    let mut prev: Option<&Value> = None;
    let mut prev_split = false;
    for (i, e) in elements.iter().enumerate() {
        if passes_over(e) {
            continue;
        }
        let page = e["pageNum"].as_u64().unwrap_or(0);
        let mut found = |shape| {
            out.push(Finding {
                shape,
                index: i,
                page,
            })
        };
        let t = text(e);
        let mut split = false;
        match kind(e) {
            "action" if prev_split && t.contains(')') && !t.contains('(') => {
                found(Shape::SplitBracket);
            }
            "action" if in_speech(prev) => {
                if t.starts_with('(') && t.ends_with(')') && !unbalanced(t) {
                    if !t.eq_ignore_ascii_case("(MORE)") {
                        found(Shape::BracketedDescription);
                    }
                } else if t.starts_with('(') && !t.contains(')') {
                    found(Shape::SplitBracket);
                    split = true;
                }
            }
            "dialogue" if roster.contains(&t) => found(Shape::NameInSpeech),
            _ => {}
        }
        prev_split = split;
        prev = Some(e);
    }
    out
}

/// Every PDF under `dir`, at any depth, sorted.
fn pdfs_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.filter_map(Result::ok) {
        let p = entry.path();
        if p.is_dir() {
            pdfs_under(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")) {
            out.push(p);
        }
    }
}

/// Convert one script in a scratch folder, with the same guards as parity
/// (its own settings, library and home; Calibre pointed at nothing), and
/// read back the parser's elements.
fn elements_of(cli: &Path, pdf: &Path, work: &Path) -> Result<Vec<Value>, String> {
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work.join("home")).map_err(|e| e.to_string())?;
    let book = work.join("book.epub");
    let out = Command::new(cli)
        .arg(pdf)
        .arg("-o")
        .arg(&book)
        .args(["--no-fountain", "--debug", "--force"])
        .env("METHOD_READER_CONFIG_DIR", work.join("config"))
        .env("METHOD_READER_LIBRARY", work.join("library"))
        .env("CALIBRE_CONFIG_DIRECTORY", crate::parity::CALIBRE_GUARD)
        .env("HOME", work.join("home"))
        .env("USERPROFILE", work.join("home"))
        .output()
        .map_err(|e| format!("could not run the CLI: {e}"))?;
    if !out.status.success() {
        return Err("the conversion failed".into());
    }
    let raw = std::fs::read(work.join("book.elements.json"))
        .map_err(|e| format!("no elements written: {e}"))?;
    let v: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    Ok(v["elements"].as_array().cloned().unwrap_or_default())
}

pub fn main(args: &[String]) -> u8 {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut names = false;
    let mut show = false;
    let mut cli: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dir" => match it.next() {
                Some(d) => dirs.push(PathBuf::from(d)),
                None => {
                    eprintln!("xtask audit-corpus: --dir needs a folder");
                    return 2;
                }
            },
            "--rust-cli" => match it.next() {
                Some(p) => cli = Some(PathBuf::from(p)),
                None => {
                    eprintln!("xtask audit-corpus: --rust-cli needs a path");
                    return 2;
                }
            },
            "--names" => names = true,
            "--show" => show = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return 0;
            }
            other => {
                eprintln!("xtask audit-corpus: no option \"{other}\" (see --help)");
                return 2;
            }
        }
    }

    let mut scripts: Vec<PathBuf> = paths::real_fixtures()
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")))
        .collect();
    for d in &dirs {
        pdfs_under(d, &mut scripts);
    }
    let mut seen = Vec::new();
    scripts.retain(|p| {
        let key = p.canonicalize().unwrap_or_else(|_| p.clone());
        let fresh = !seen.contains(&key);
        seen.push(key);
        fresh
    });
    if scripts.is_empty() {
        eprintln!("xtask audit-corpus: no real scripts (root fixtures/ is empty; try --dir)");
        return 2;
    }

    let cli = match cli {
        Some(p) => p,
        None => match crate::parity::rust_cli(None) {
            Ok(Some(p)) => p,
            Ok(None) => {
                eprintln!("xtask audit-corpus: there is no sp-cli crate to build");
                return 2;
            }
            Err(e) => {
                eprintln!("xtask audit-corpus: {e}");
                return 2;
            }
        },
    };

    let work = std::env::temp_dir().join(format!("method-reader-audit-{}", std::process::id()));
    let mut totals = [0usize; 3];
    let mut failed = 0;
    for (n, pdf) in scripts.iter().enumerate() {
        let n = n + 1;
        let label = if names {
            format!(
                "{n:>3} {}",
                pdf.file_stem().unwrap_or_default().to_string_lossy()
            )
        } else {
            format!("{n:>3}")
        };
        let elements = match elements_of(&cli, pdf, &work) {
            Ok(e) => e,
            Err(e) => {
                failed += 1;
                println!("{label}  not converted: {e}");
                continue;
            }
        };
        let found = findings(&elements);
        let counts: Vec<String> = Shape::ALL
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let c = found.iter().filter(|f| f.shape == *s).count();
                totals[k] += c;
                format!("{} {c}", s.as_str())
            })
            .collect();
        let mark = if found.is_empty() { "ok   " } else { "FOUND" };
        println!("{label}  {mark}  {}", counts.join(", "));
        if show {
            for f in &found {
                println!("        p{} {}:", f.page, f.shape.as_str());
                let from = f.index.saturating_sub(1);
                let to = (f.index + 2).min(elements.len());
                for (j, e) in elements[from..to].iter().enumerate() {
                    let here = if from + j == f.index { ">" } else { " " };
                    println!("          {here} {:<13} {}", kind(e), text(e));
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    let total: usize = totals.iter().sum();
    let summary: Vec<String> = Shape::ALL
        .iter()
        .zip(totals)
        .map(|(s, c)| format!("{} {c}", s.as_str()))
        .collect();
    println!(
        "audit: {} scripts, {} not converted; {}",
        scripts.len(),
        failed,
        summary.join(", ")
    );
    u8::from(total > 0)
}

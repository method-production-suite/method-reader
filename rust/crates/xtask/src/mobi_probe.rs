//! `cargo xtask mobi-picture-probe --out <folder>`: was
//! tools/mobi-picture-probe.ts.
//!
//! Builds small books for one question only a device can answer: how large
//! a picture an older Kindle accepts in a .mobi. Writes five files into the
//! folder, creating it if need be:
//!
//! ```text
//! method-reader-picture-probe.mobi         all three pictures in one book
//! method-reader-picture-probe.epub         that book as an EPUB, to compare it with
//! method-reader-picture-probe-2kb.mobi     the 2 KB picture alone
//! method-reader-picture-probe-200kb.mobi   the 200 KB picture alone
//! method-reader-picture-probe-1400kb.mobi  the 1.4 MB plate alone
//! ```
//!
//! The books are sp-mobi's (`sp_mobi::probe`, the probe's builder as a
//! library) and sp-epub's, each built as a .fountain's books are built with
//! Method Reader's defaults, at a fixed clock, so every run writes the same
//! bytes. Prints the files and the three sizes. Exit codes: 0 done, 1
//! refused or failed.

use std::collections::HashSet;
use std::path::Path;

use sp_epub::{BodyOptions, BookImage, BookMeta, build_epub, content_identifier, tokens_to_body};
use sp_mobi::probe::{
    BUILT_AT, EPUB_FILE, MOBI_FILE, ProbeBooks, ProbePicture, build_probe, plain_size, probe_script,
};
use sp_model::{DEFAULT_FORMAT_OPTIONS, TokenType};

pub use sp_mobi::probe::{probe_pictures, single_mobi_file, single_title};

/// Everything one run writes: the books and the pictures they carry.
pub struct Probe {
    pub books: ProbeBooks,
    pub epub: Vec<u8>,
}

/// The combined book as an EPUB, built as convert.ts's renderBooks builds a
/// .fountain's EPUB with the defaults (pictures on), at [`BUILT_AT`].
pub fn probe_epub(pictures: &[ProbePicture]) -> Result<Vec<u8>, String> {
    let text = probe_script(pictures, "Picture Probe");
    let tokens = sp_fountain::tokenize(&text).map_err(|e| e.to_string())?;
    let find = |kinds: &[&str]| {
        tokens
            .iter()
            .find(|t| matches!(&t.kind, TokenType::Other(k) if kinds.contains(&k.as_str())))
            .and_then(|t| t.text.clone())
    };
    let meta = BookMeta {
        title: find(&["title"])
            .map(|t| strip_tags(&t))
            .unwrap_or_else(|| "Untitled Screenplay".into()),
        author: find(&["author", "authors"]),
        language: None,
        identifier: Some(content_identifier(&text)),
    };
    let format = DEFAULT_FORMAT_OPTIONS;
    let images: Vec<BookImage> = pictures
        .iter()
        .map(|p| BookImage {
            name: p.name.clone(),
            bytes: p.bytes.clone(),
        })
        .collect();
    let names: HashSet<String> = images.iter().map(|i| i.name.clone()).collect();
    let body = tokens_to_body(
        &tokens,
        &BodyOptions {
            format,
            images: names,
            ..BodyOptions::default()
        },
    );
    build_epub(&meta, &body, &format, &images, i64::from(BUILT_AT)).map_err(|e| e.to_string())
}

/// `title.replace(/<[^>]+>/g, '')`.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        match after.find('>') {
            Some(close) if close > 0 => {
                out.push_str(&rest[..open]);
                rest = &after[close + 1..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Builds every book.
pub fn build() -> Result<Probe, String> {
    let books = build_probe();
    let epub = probe_epub(&books.pictures)?;
    Ok(Probe { books, epub })
}

/// `n.toLocaleString('en-US')` for a whole number: thousands separated by commas.
fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What the tool prints after writing `probe` to `out`.
pub fn report(out: &str, probe: &Probe) -> String {
    let singles = &probe.books.singles;
    let width = singles
        .iter()
        .map(|s| s.file.len())
        .chain([MOBI_FILE.len(), EPUB_FILE.len()])
        .max()
        .unwrap_or(0);
    let mut lines = vec![format!("Wrote the probe to {out}:")];
    lines.push(format!(
        "  {MOBI_FILE:<width$}  Picture Probe, all three pictures"
    ));
    for s in singles {
        lines.push(format!(
            "  {:<width$}  {}, that picture alone",
            s.file, s.title
        ));
    }
    lines.push(format!(
        "  {EPUB_FILE:<width$}  Picture Probe as an EPUB, to compare"
    ));
    lines.push("Copy the four .mobi files to the Kindle's documents folder.".into());
    lines.push("The three pictures, in the order Picture Probe shows them:".into());
    for (i, p) in probe.books.pictures.iter().enumerate() {
        let plate = if p.full_page {
            ", a full-page plate"
        } else {
            ""
        };
        lines.push(format!(
            "  {}. about {} ({} bytes){plate}",
            i + 1,
            plain_size(p.bytes.len()),
            grouped(p.bytes.len())
        ));
    }
    lines.join("\n")
}

fn write_all(out: &Path, probe: &Probe) -> std::io::Result<()> {
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join(MOBI_FILE), &probe.books.mobi)?;
    std::fs::write(out.join(EPUB_FILE), &probe.epub)?;
    for s in &probe.books.singles {
        std::fs::write(out.join(&s.file), &s.mobi)?;
    }
    Ok(())
}

fn run(args: &[String]) -> Result<String, String> {
    let mut out: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--out" {
            out = it.next().cloned();
        } else if let Some(v) = a.strip_prefix("--out=") {
            out = Some(v.to_string());
        } else {
            return Err(format!("Unknown option '{a}'"));
        }
    }
    let out = out.ok_or("usage: cargo xtask mobi-picture-probe --out <folder>")?;
    let probe = build()?;
    write_all(Path::new(&out), &probe).map_err(|e| e.to_string())?;
    Ok(report(&out, &probe))
}

/// `cargo xtask mobi-picture-probe`.
pub fn main(args: &[String]) -> u8 {
    match run(args) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(e) => {
            eprintln!("mobi-picture-probe: {e}");
            1
        }
    }
}

//! Ports the halves of tests/mobi-picture-probe.test.ts that sp-mobi's
//! tests/probe.rs does not: the tool run as a real process, the way a
//! device pass runs it and judged by the files it writes, and the EPUB it
//! writes beside the .mobi files. The MOBI checks themselves (the single
//! books, titles and ids, sizes, markup order, the plate) are in
//! sp-mobi/tests/probe.rs against the same builder.

use std::path::{Path, PathBuf};
use std::process::Command;

use xtask::mobi_probe;

/// The single-picture books' files, smallest first, spelled out rather than
/// derived: these are the names the device check tells a person to copy.
const SINGLE_FILES: [&str; 3] = [
    "method-reader-picture-probe-2kb.mobi",
    "method-reader-picture-probe-200kb.mobi",
    "method-reader-picture-probe-1400kb.mobi",
];
const MOBI_FILE: &str = "method-reader-picture-probe.mobi";
const EPUB_FILE: &str = "method-reader-picture-probe.epub";

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "screepub-mobi-picture-probe-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("scratch");
        Scratch(p)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    mobi: Vec<u8>,
    epub: Vec<u8>,
    singles: Vec<Vec<u8>>,
}

fn run_probe(out: &Path) -> Run {
    let o = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["mobi-picture-probe", "--out"])
        .arg(out)
        .output()
        .expect("xtask runs");
    let read = |f: &str| std::fs::read(out.join(f)).unwrap_or_default();
    Run {
        code: o.status.code(),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        mobi: read(MOBI_FILE),
        epub: read(EPUB_FILE),
        singles: SINGLE_FILES.iter().map(|f| read(f)).collect(),
    }
}

/// The PNG records of a .mobi, in record order.
fn mobi_pictures(b: &[u8]) -> Vec<Vec<u8>> {
    let u16_at = |i: usize| usize::from(u16::from_be_bytes([b[i], b[i + 1]]));
    let u32_at = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
    let count = u16_at(76);
    let offsets: Vec<usize> = (0..count).map(|i| u32_at(78 + i * 8)).collect();
    (0..count)
        .map(|i| {
            let end = offsets.get(i + 1).copied().unwrap_or(b.len());
            &b[offsets[i]..end]
        })
        .filter(|r| r.starts_with(&[0x89, b'P', b'N', b'G']))
        .map(<[u8]>::to_vec)
        .collect()
}

fn epub_entry(epub: &[u8], name: &str) -> Vec<u8> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(epub)).expect("a zip");
    let mut f = zip.by_name(name).expect("the entry");
    let mut out = Vec::new();
    f.read_to_end(&mut out).expect("reads");
    out
}

#[test]
fn writes_the_mobi_the_epub_and_a_mobi_per_size_and_prints_the_files_and_sizes() {
    let s = Scratch::new("first");
    let run = run_probe(&s.0.join("out"));
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(run.stdout.contains(MOBI_FILE));
    assert!(run.stdout.contains(EPUB_FILE));
    for f in SINGLE_FILES {
        assert!(run.stdout.contains(f));
    }
    let names: Vec<String> = mobi_probe::probe_pictures()
        .iter()
        .map(mobi_probe::single_mobi_file)
        .collect();
    assert_eq!(names, SINGLE_FILES);
    for size in ["about 2 KB", "about 200 KB", "about 1.4 MB"] {
        assert!(run.stdout.contains(size), "{size}");
    }
    assert!(!run.mobi.is_empty() && !run.epub.is_empty());
    assert!(run.singles.iter().all(|b| !b.is_empty()));
}

#[test]
fn the_mobi_records_hold_the_three_pngs_in_order_each_the_epubs_own() {
    let s = Scratch::new("records");
    let run = run_probe(&s.0.join("out"));
    let records = mobi_pictures(&run.mobi);
    assert_eq!(records.len(), 3);
    let shipped: Vec<Vec<u8>> = ["p001-01.png", "p002-01.png", "p003-01.png"]
        .iter()
        .map(|n| epub_entry(&run.epub, &format!("OEBPS/images/{n}")))
        .collect();
    assert_eq!(records, shipped);
}

#[test]
fn every_run_writes_the_same_bytes_and_the_pictures_are_the_ones_the_tool_draws() {
    let s = Scratch::new("twice");
    let first = run_probe(&s.0.join("first"));
    let again = run_probe(&s.0.join("second"));
    assert!(first.mobi == again.mobi);
    assert!(first.epub == again.epub);
    assert!(first.singles == again.singles);
    let drawn: Vec<Vec<u8>> = mobi_probe::probe_pictures()
        .into_iter()
        .map(|p| p.bytes)
        .collect();
    assert!(mobi_pictures(&first.mobi) == drawn);
}

#[test]
fn the_epub_is_the_combined_book_titled_picture_probe() {
    let epub = mobi_probe::build().expect("builds").epub;
    let opf = String::from_utf8(epub_entry(&epub, "OEBPS/package.opf")).expect("utf-8");
    assert!(opf.contains("<dc:title>Picture Probe</dc:title>"), "{opf}");
    assert!(opf.contains("urn:screepub:"));
    assert!(opf.contains("2026-10-01T00:00:00Z"));
}

#[test]
fn without_out_it_refuses_with_its_usage_and_exit_1() {
    let o = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("mobi-picture-probe")
        .output()
        .expect("xtask runs");
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.starts_with("mobi-picture-probe: usage:"), "{err}");
}

#[test]
fn the_epub_validates_clean() {
    let Some(epubcheck) = which("epubcheck") else {
        eprintln!("skipped: epubcheck is not on PATH (brew install epubcheck)");
        return;
    };
    let s = Scratch::new("epubcheck");
    let out = s.0.join("out");
    let run = run_probe(&out);
    assert_eq!(run.code, Some(0));
    let check = Command::new(epubcheck)
        .arg(out.join(EPUB_FILE))
        .output()
        .expect("epubcheck runs");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(check.status.success(), "{text}");
    assert!(text.contains("No errors or warnings detected"), "{text}");
}

fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    })
}

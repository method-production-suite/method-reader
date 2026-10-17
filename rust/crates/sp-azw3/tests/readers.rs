//! Our AZW3, read by readers that are not ours: Calibre's KF8 reader and
//! debugging inspector (tests/azw3-calibre.test.ts), Calibre's AZW3 of the
//! same EPUB beside ours (azw3-compare), every real script
//! (azw3-corpus), and KindleUnpack (azw3-kindleunpack).
//!
//! Calibre runs ONLY through tests/common/calibre.rs's Sandbox, against a
//! scratch copy of its settings folder, and every test checks afterwards
//! that the real folder did not change. Each test skips cleanly where its
//! tool is absent. Real scripts are reported as counts only.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use common::calibre::{Sandbox, Scratch, calibre_tool, real_calibre_config};
use common::calibre_read::{calibre_to_epub, inspect_mobi, view_of_calibre_epub, view_of_our_epub};
use common::kf8_reader::read_kf8;
use common::{dumps, fixture_epub};
use sp_azw3::build_azw3_at;
use sp_azw3::epub::read_epub;

const T: u64 = 1_767_225_600;
const FIXTURES: [&str; 4] = ["field-station", "images", "torture", "screenplay"];
/// src/export/calibre.ts's CALIBRE_FORMAT_GUARDS.
const CALIBRE_FORMAT_GUARDS: [&str; 3] = [
    "--page-breaks-before=/",
    "--chapter-mark=none",
    "--disable-remove-fake-margins",
];

fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (u64, Option<SystemTime>)> {
    let mut out = BTreeMap::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in read.flatten() {
        let p = e.path();
        if let Ok(m) = std::fs::symlink_metadata(&p) {
            if m.is_dir() {
                out.extend(snapshot(&p));
            } else {
                out.insert(p, (m.len(), m.modified().ok()));
            }
        }
    }
    out
}

/// Runs `f` with Calibre's tools, then checks the real settings folder is
/// as it was. Skips (and says so) where Calibre is not installed.
fn with_calibre(label: &str, f: impl FnOnce(&Sandbox, &Path, &Path)) {
    let (Some(convert), Some(debug)) =
        (calibre_tool("ebook-convert"), calibre_tool("calibre-debug"))
    else {
        eprintln!("skipped: Calibre's ebook-convert or calibre-debug was not found");
        return;
    };
    let real = real_calibre_config();
    let before = snapshot(&real);
    let sandbox = Sandbox::new(label);
    f(&sandbox, &convert, &debug);
    drop(sandbox);
    assert_eq!(
        snapshot(&real),
        before,
        "the real Calibre folder must not change"
    );
}

fn sorted(images: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut v = images.to_vec();
    v.sort();
    v
}

#[test]
fn calibres_reader_opens_our_azw3() {
    with_calibre("azw3-calibre", |sandbox, convert, debug| {
        for name in FIXTURES {
            let dir = sandbox.dir().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let epub = fixture_epub(name);
            let bytes = build_azw3_at(&epub, T).unwrap();
            let azw3 = dir.join(format!("{name}.azw3"));
            std::fs::write(&azw3, &bytes).unwrap();

            let (read, out) = calibre_to_epub(sandbox, convert, &azw3);
            let read = read.unwrap_or_else(|| panic!("{name}: {out}"));
            // Calibre's reader warns, and carries on, where a file is off.
            for warning in [
                "incorrect insert positions",
                "Could not find",
                "invalid",
                "unprocessed index bytes",
            ] {
                assert!(
                    !out.to_lowercase().contains(&warning.to_lowercase()),
                    "{name}: {out}"
                );
            }
            let ours = view_of_our_epub(&epub);
            let theirs = view_of_calibre_epub(&read);
            assert_eq!(theirs.title, ours.title, "{name}");
            assert_eq!(theirs.authors, ours.authors, "{name}");
            assert_eq!(theirs.language, ours.language, "{name}");
            assert_eq!(theirs.toc, ours.toc, "{name}");
            assert_eq!(theirs.blocks, ours.blocks, "{name}");
            assert_eq!(theirs.text, ours.text, "{name}");
            assert_eq!(sorted(&theirs.images), sorted(&ours.images), "{name}");
            assert_eq!(theirs.cover, ours.cover, "{name}");

            let (run, entries) = inspect_mobi(sandbox, debug, &azw3);
            assert_eq!(run.code, 0, "{}", run.out);
            let book = read_kf8(&bytes).unwrap();
            let want = vec![
                ("skel".to_owned(), book.skel.len()),
                ("chunks".to_owned(), book.chunks.len()),
                ("ncx".to_owned(), book.ncx.len()),
                ("guide".to_owned(), book.guide.len()),
            ];
            assert_eq!(entries, want, "{name}");
        }
    });
}

#[test]
fn ours_beside_calibres_azw3() {
    with_calibre("azw3-compare", |sandbox, convert, _| {
        let mut books: Vec<(String, Vec<u8>)> = ["field-station", "images"]
            .iter()
            .map(|n| ((*n).to_owned(), fixture_epub(n)))
            .collect();
        // The largest real script, where the dumps exist (counted, never named).
        if let Some(largest) = dumps("oracle-real")
            .into_iter()
            .max_by_key(|d| std::fs::metadata(d.join("book.epub")).map_or(0, |m| m.len()))
        {
            books.push((
                "largest real script".to_owned(),
                std::fs::read(largest.join("book.epub")).unwrap(),
            ));
        }
        for (label, epub) in books {
            let dir = sandbox.dir().join(label.replace(' ', "-"));
            std::fs::create_dir_all(&dir).unwrap();
            let epub_path = dir.join("book.epub");
            std::fs::write(&epub_path, &epub).unwrap();
            let t0 = std::time::Instant::now();
            let ours = build_azw3_at(&epub, T).unwrap();
            let ours_ms = t0.elapsed().as_millis();
            let theirs_path = dir.join("calibre.azw3");
            let mut args = vec![epub_path.to_str().unwrap(), theirs_path.to_str().unwrap()];
            args.extend(CALIBRE_FORMAT_GUARDS);
            let t1 = std::time::Instant::now();
            let run = sandbox.run(convert, &args, &dir);
            let theirs_ms = t1.elapsed().as_millis();
            assert_eq!(run.code, 0, "{}", run.out);
            let theirs = std::fs::read(&theirs_path).unwrap();
            let a = read_kf8(&ours).unwrap();
            let b = read_kf8(&theirs).unwrap();
            eprintln!(
                "AZW3 {label}: ours {ours_ms} ms, {} bytes, {} records; Calibre {theirs_ms} ms, {} bytes, {} records",
                ours.len(),
                a.records.len(),
                theirs.len(),
                b.records.len()
            );
            // The same table of contents, and Calibre's one more file is its
            // generated inline Table of Contents page.
            assert_eq!(
                a.ncx.iter().map(|n| &n.label).collect::<Vec<_>>(),
                b.ncx.iter().map(|n| &n.label).collect::<Vec<_>>(),
                "{label}"
            );
            assert_eq!(a.skel.len(), b.skel.len() - 1, "{label}");
        }
    });
}

#[test]
fn every_real_script_makes_an_azw3_that_reads_back_whole() {
    let dirs = dumps("oracle-real");
    if dirs.is_empty() {
        eprintln!("skipped: no rust/oracle-real dumps");
        return;
    }
    let mut structural = 0;
    let mut calibre = 0;
    let mut failures = Vec::new();
    let check = |sandbox: Option<(&Sandbox, &Path)>,
                 structural: &mut usize,
                 calibre: &mut usize,
                 failures: &mut Vec<String>| {
        for (i, dir) in dirs.iter().enumerate() {
            // Numbered, never named.
            let label = format!("script {}", i + 1);
            let epub = std::fs::read(dir.join("book.epub")).unwrap();
            let azw3 = build_azw3_at(&epub, T).unwrap();
            if let Err(e) = read_kf8(&azw3) {
                failures.push(format!("{label}: structural reader: {e}"));
                continue;
            }
            *structural += 1;
            let Some((sandbox, convert)) = sandbox else {
                continue;
            };
            let book_dir = sandbox.dir().join(format!("book-{}", i + 1));
            std::fs::create_dir_all(&book_dir).unwrap();
            let path = book_dir.join("book.azw3");
            std::fs::write(&path, &azw3).unwrap();
            let (read, _) = calibre_to_epub(sandbox, convert, &path);
            let Some(read) = read else {
                failures.push(format!("{label}: Calibre could not read it"));
                continue;
            };
            let ours = view_of_our_epub(&epub);
            let theirs = view_of_calibre_epub(&read);
            if theirs.text == ours.text
                && theirs.toc == ours.toc
                && theirs.blocks.len() == ours.blocks.len()
                && theirs.images.len() == ours.images.len()
            {
                *calibre += 1;
            } else {
                failures.push(format!(
                    "{label}: Calibre read it back different (text {}, toc {}/{}, blocks {}/{}, pictures {}/{})",
                    theirs.text == ours.text,
                    theirs.toc.len(),
                    ours.toc.len(),
                    theirs.blocks.len(),
                    ours.blocks.len(),
                    theirs.images.len(),
                    ours.images.len()
                ));
            }
        }
    };
    let mut ran_calibre = false;
    with_calibre("azw3-corpus", |sandbox, convert, _| {
        ran_calibre = true;
        check(
            Some((sandbox, convert)),
            &mut structural,
            &mut calibre,
            &mut failures,
        );
    });
    if !ran_calibre {
        check(None, &mut structural, &mut calibre, &mut failures);
    }
    eprintln!(
        "AZW3 corpus: {} scripts, {structural} structural, {calibre} through Calibre",
        dirs.len()
    );
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn kindleunpack_reads_our_azw3() {
    // METHOD_READER_KINDLEUNPACK names KindleUnpack's folder (the one holding
    // lib/kindleunpack.py); python3 must be on the PATH. KindleUnpack never
    // touches Calibre, and python3 runs isolated (-I).
    let ku = std::env::var("METHOD_READER_KINDLEUNPACK").unwrap_or_default();
    let python = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("python3"))
            .find(|p| p.is_file())
    });
    let (false, Some(python)) = (
        ku.is_empty() || !Path::new(&ku).join("lib/kindleunpack.py").is_file(),
        python,
    ) else {
        eprintln!(
            "skipped: METHOD_READER_KINDLEUNPACK does not name KindleUnpack, or python3 is missing"
        );
        return;
    };
    let scratch = Scratch::new("azw3-ku");
    let runner = scratch.path.join("ku_run.py");
    std::fs::write(
        &runner,
        "import sys\nsys.path.insert(0, sys.argv[1])\nfrom lib.kindleunpack import unpackBook\nunpackBook(sys.argv[2], sys.argv[3], epubver='3')\n",
    )
    .unwrap();
    for name in FIXTURES {
        let dir = scratch.path.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let epub = fixture_epub(name);
        let azw3 = dir.join(format!("{name}.azw3"));
        std::fs::write(&azw3, build_azw3_at(&epub, T).unwrap()).unwrap();
        let out = dir.join("out");
        let run = Command::new(&python)
            .args([
                "-I",
                runner.to_str().unwrap(),
                &ku,
                azw3.to_str().unwrap(),
                out.to_str().unwrap(),
            ])
            .current_dir(&scratch.path)
            .output()
            .unwrap();
        let log = format!(
            "{}\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(run.status.success(), "{log}");
        for bad in ["Traceback", "Error", "incorrect", "mismatch"] {
            assert!(!log.contains(bad), "{log}");
        }
        let ours = read_epub(&epub).unwrap();
        let warnings: Vec<&str> = log
            .lines()
            .filter(|l| l.to_lowercase().contains("warning"))
            .collect();
        if ours.images.is_empty() {
            assert!(
                warnings
                    .iter()
                    .all(|w| w.contains("does not contain a recognised resource")),
                "{log}"
            );
        } else {
            assert!(warnings.is_empty(), "{log}");
        }
        let rebuilt = std::fs::read(out.join("mobi8").join(format!("{name}.epub"))).unwrap();
        let theirs = view_of_calibre_epub(&rebuilt);
        let want = view_of_our_epub(&epub);
        assert_eq!(theirs.title, want.title);
        assert_eq!(theirs.authors, want.authors);
        assert_eq!(theirs.toc, want.toc);
        assert_eq!(theirs.blocks, want.blocks);
        assert_eq!(theirs.text, want.text);
        assert_eq!(sorted(&theirs.images), sorted(&want.images));
        assert_eq!(theirs.cover, want.cover);
        let opf = std::fs::read_to_string(out.join("mobi8/OEBPS/content.opf")).unwrap();
        let id = ours.identifier.unwrap();
        assert!(opf.contains(&format!(
            "<meta name=\"ASIN\" content=\"{}\" />",
            id.rsplit(':').next().unwrap()
        )));
        assert!(opf.contains("<meta name=\"cdeType\" content=\"EBOK\" />"));
    }
}

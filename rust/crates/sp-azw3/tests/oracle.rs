//! Stage parity: the AZW3 built from the oracle's book.epub is the
//! TypeScript CLI's book.azw3, byte for byte, for every committed fixture
//! (rust/oracle/) and every real script dumped locally (rust/oracle-real/,
//! gitignored). Real scripts are reported as counts only.

mod common;

use common::{dumps, epoch};
use sp_azw3::build_azw3_at;

#[test]
fn every_committed_fixture_is_byte_identical() {
    let dirs = dumps("oracle");
    assert!(
        dirs.len() >= 5,
        "the committed oracle holds the fixtures' books"
    );
    for dir in &dirs {
        let epub = std::fs::read(dir.join("book.epub")).expect("book.epub");
        let want = std::fs::read(dir.join("book.azw3")).expect("book.azw3");
        let got = build_azw3_at(&epub, epoch(dir)).expect("the oracle's EPUB builds");
        let name = dir.file_name().unwrap_or_default().to_string_lossy();
        assert!(got == want, "{name}: the AZW3 differs from the oracle's");
    }
}

#[test]
fn every_real_script_is_byte_identical() {
    let dirs = dumps("oracle-real");
    let mut same = 0;
    let mut differ = Vec::new();
    for (i, dir) in dirs.iter().enumerate() {
        let epub = std::fs::read(dir.join("book.epub")).expect("book.epub");
        let want = std::fs::read(dir.join("book.azw3")).expect("book.azw3");
        // Numbered, never named.
        match build_azw3_at(&epub, epoch(dir)) {
            Ok(got) if got == want => same += 1,
            _ => differ.push(i + 1),
        }
    }
    eprintln!("AZW3 real scripts: {same} of {} byte-identical", dirs.len());
    assert!(
        differ.is_empty(),
        "real scripts whose AZW3 differs: {differ:?}"
    );
}

//! Stage parity: the MOBI rebuilt from the oracle's tokens is the
//! TypeScript CLI's book.mobi, byte for byte, for every committed fixture
//! (rust/oracle/) and every real script dumped locally (rust/oracle-real/,
//! gitignored). Real scripts are reported as counts only.

mod common;

use common::{dumps, rebuild};

#[test]
fn every_committed_fixture_is_byte_identical() {
    let dirs = dumps("oracle");
    assert!(
        dirs.len() >= 5,
        "the committed oracle holds the fixtures' books"
    );
    for dir in &dirs {
        let want = std::fs::read(dir.join("book.mobi")).expect("book.mobi");
        let got = rebuild(dir);
        let name = dir.file_name().unwrap_or_default().to_string_lossy();
        assert!(got == want, "{name}: the MOBI differs from the oracle's");
    }
}

#[test]
fn every_real_script_is_byte_identical() {
    let dirs = dumps("oracle-real");
    let mut same = 0;
    let mut differ = Vec::new();
    for (i, dir) in dirs.iter().enumerate() {
        let want = std::fs::read(dir.join("book.mobi")).expect("book.mobi");
        if rebuild(dir) == want {
            same += 1;
        } else {
            // Numbered, never named.
            differ.push(i + 1);
        }
    }
    eprintln!("MOBI real scripts: {same} of {} byte-identical", dirs.len());
    assert!(
        differ.is_empty(),
        "real scripts whose MOBI differs: {differ:?}"
    );
}

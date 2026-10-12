//! Stage parity: every oracle dump's tokens.json, written as an FDX, is its
//! book.fdx byte for byte. Committed dumps (rust/oracle/) always; real
//! scripts (rust/oracle-real/, gitignored) when this machine has them, and
//! reported by count only, never by name.

use std::path::{Path, PathBuf};

use sp_fdx::tokens_to_fdx;
use sp_model::Token;

fn rust_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every dump folder holding both a tokens.json and a book.fdx.
fn dumps(folder: &str) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(rust_dir().join(folder)) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("tokens.json").is_file() && p.join("book.fdx").is_file())
        .collect();
    out.sort();
    out
}

fn fdx_of(dir: &Path) -> (String, String) {
    let tokens: Vec<Token> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("tokens.json")).unwrap())
            .expect("tokens.json reads into the model");
    let want = std::fs::read_to_string(dir.join("book.fdx")).unwrap();
    (tokens_to_fdx(&tokens), want)
}

/// The first line that differs, for a readable failure.
fn first_difference(got: &str, want: &str) -> String {
    for (i, (g, w)) in got.lines().zip(want.lines()).enumerate() {
        if g != w {
            return format!("line {}:\n  got  {g}\n  want {w}", i + 1);
        }
    }
    format!(
        "lengths differ: {} lines against {}",
        got.lines().count(),
        want.lines().count()
    )
}

#[test]
fn every_committed_fixture_matches_its_book_fdx() {
    let dirs = dumps("oracle");
    // field-station, images, jpx, screenplay and torture: the two refused
    // fixtures have no book.
    assert_eq!(dirs.len(), 5, "committed dumps with an FDX");
    for dir in dirs {
        let (got, want) = fdx_of(&dir);
        let name = dir.file_name().unwrap().to_string_lossy();
        assert!(got == want, "{name}: {}", first_difference(&got, &want));
    }
}

#[test]
fn every_real_script_matches_its_book_fdx() {
    let dirs = dumps("oracle-real");
    let mut differ = 0;
    for (i, dir) in dirs.iter().enumerate() {
        let (got, want) = fdx_of(dir);
        if got != want {
            differ += 1;
            // By number only: a real script's words never reach output.
            eprintln!("real script {} of {}: FDX differs", i + 1, dirs.len());
        }
    }
    eprintln!(
        "real scripts: {} of {} FDX byte-identical",
        dirs.len() - differ,
        dirs.len()
    );
    assert_eq!(differ, 0, "real-script FDX differences");
}

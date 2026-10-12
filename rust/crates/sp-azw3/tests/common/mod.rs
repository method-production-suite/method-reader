//! What the sp-azw3 tests share: the oracle's folders, the structural KF8
//! reader and the PalmDOC decoder (independent of the writer), and the
//! isolated Calibre runner sp-mobi's tests use.

#![allow(dead_code)]

#[path = "../../../sp-mobi/tests/common/calibre.rs"]
pub mod calibre;
pub mod calibre_read;
pub mod kf8_reader;
pub mod palmdoc_decode;

use std::path::{Path, PathBuf};

/// rust/ (this crate is rust/crates/sp-azw3).
pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every dump folder under `rust/<name>/` that holds a book.azw3, sorted.
pub fn dumps(name: &str) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(workspace().join(name)) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("book.azw3").is_file())
        .collect();
    out.sort();
    out
}

/// A dump's frozen clock.
pub fn epoch(dir: &Path) -> u64 {
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("meta.json")).expect("meta.json"))
            .expect("JSON");
    meta["sourceDateEpoch"].as_u64().expect("an epoch")
}

/// The committed fixtures' EPUBs, as the oracle holds them
/// (tests/helpers/fixture-epubs.ts converted the PDFs; the oracle did that
/// once, with the CLI).
pub fn fixture_epub(name: &str) -> Vec<u8> {
    std::fs::read(workspace().join("oracle").join(name).join("book.epub")).expect("an oracle EPUB")
}

/// A copy with the PDB header's creation and modification dates zeroed.
pub fn mask_pdb_dates(b: &[u8]) -> Vec<u8> {
    let mut c = b.to_vec();
    c[36..44].fill(0);
    c
}

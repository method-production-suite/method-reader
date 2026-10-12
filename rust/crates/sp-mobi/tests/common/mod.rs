//! What the sp-mobi tests share: the oracle's folders and a reader of the
//! MOBI's own records (tests/helpers/mobi-records.ts), kept independent of
//! the writer so a test of one is not the other checking itself.

#![allow(dead_code)]

pub mod calibre;
pub mod sources;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sp_mobi::{
    MOBI_CARRIES_PICTURES, MobiInput, MobiMeta, build_mobi, mobi_art, tokens_to_mobi_html,
};
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions, Token, resolve_format_options};

/// rust/ (this crate is rust/crates/sp-mobi).
pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every dump folder under `rust/<name>/` that holds a book.mobi, sorted.
pub fn dumps(name: &str) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(workspace().join(name)) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("book.mobi").is_file())
        .collect();
    out.sort();
    out
}

fn json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("an oracle file")).expect("oracle JSON")
}

/// The MOBI the TypeScript CLI built for a dump, rebuilt here from its
/// tokens, the title and author its `--json` answer reports, its options
/// and its frozen clock.
pub fn rebuild(dir: &Path) -> Vec<u8> {
    let tokens: Vec<Token> =
        serde_json::from_slice(&std::fs::read(dir.join("tokens.json")).expect("tokens.json"))
            .expect("tokens");
    let convert = json(&dir.join("convert.json"));
    let meta = json(&dir.join("meta.json"));
    let format: FormatOptions = match meta.get("options") {
        Some(o) if !o.is_null() => resolve_format_options(Some(o), &DEFAULT_FORMAT_OPTIONS),
        _ => DEFAULT_FORMAT_OPTIONS,
    };
    let time = u32::try_from(meta["sourceDateEpoch"].as_u64().expect("an epoch")).expect("32 bits");
    let book = MobiMeta {
        title: convert["title"].as_str().expect("a title").to_owned(),
        author: convert
            .get("author")
            .and_then(Value::as_str)
            .map(str::to_owned),
    };
    // The CLI's MOBI is behind the picture gate: told it can ship nothing
    // while the gate is shut.
    let have: HashSet<String> = if MOBI_CARRIES_PICTURES {
        unimplemented!("the gate is open: hand the dump's pictures over")
    } else {
        HashSet::new()
    };
    let art = mobi_art(&tokens, &format, &have);
    let html = tokens_to_mobi_html(&tokens, &book, &format, &art);
    build_mobi(&MobiInput {
        title: book.title,
        author: book.author,
        html,
        images: Vec::new(),
        cover_index: art.cover_index,
        time: Some(time),
    })
    .expect("the oracle's book builds")
}

// ── record reader (tests/helpers/mobi-records.ts) ──

pub fn u16_at(b: &[u8], o: usize) -> usize {
    usize::from(u16::from_be_bytes([b[o], b[o + 1]]))
}

pub fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

pub fn ascii(b: &[u8], o: usize, len: usize) -> String {
    String::from_utf8_lossy(&b[o..o + len]).into_owned()
}

pub fn record_offsets(b: &[u8]) -> Vec<usize> {
    let count = u16_at(b, 76);
    (0..count).map(|i| u32_at(b, 78 + i * 8) as usize).collect()
}

/// Every record's bytes, record 0 first, cut at the offsets the PDB lists.
pub fn records(b: &[u8]) -> Vec<Vec<u8>> {
    let offsets = record_offsets(b);
    offsets
        .iter()
        .enumerate()
        .map(|(i, &o)| b[o..offsets.get(i + 1).copied().unwrap_or(b.len())].to_vec())
        .collect()
}

/// The MOBI header fields the tests read, by their offset in record 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub text_records: usize,
    pub first_non_book: u32,
    pub first_image: u32,
    pub first_content: usize,
    pub last_content: usize,
    pub fcis: u32,
    pub flis: u32,
}

pub fn header(b: &[u8]) -> Header {
    let r0 = record_offsets(b)[0];
    Header {
        text_records: u16_at(b, r0 + 8),
        first_non_book: u32_at(b, r0 + 80),
        first_image: u32_at(b, r0 + 108),
        first_content: u16_at(b, r0 + 192),
        last_content: u16_at(b, r0 + 194),
        fcis: u32_at(b, r0 + 200),
        flis: u32_at(b, r0 + 208),
    }
}

/// The picture records, in record order (`mobiPictures`).
pub fn mobi_pictures(b: &[u8]) -> Vec<Vec<u8>> {
    let offsets = record_offsets(b);
    let r0 = offsets[0];
    let first = u32_at(b, r0 + 108);
    if first == 0xffff_ffff {
        return Vec::new();
    }
    let last = u16_at(b, r0 + 194);
    (first as usize..=last)
        .map(|i| b[offsets[i]..offsets.get(i + 1).copied().unwrap_or(b.len())].to_vec())
        .collect()
}

/// The book's markup: its text records joined, cut at the stated length
/// (`mobiHtml`).
pub fn mobi_html(b: &[u8]) -> String {
    let offsets = record_offsets(b);
    let r0 = offsets[0];
    let length = u32_at(b, r0 + 4) as usize;
    let text_records = u16_at(b, r0 + 8);
    let mut joined = Vec::new();
    for i in 0..text_records {
        joined.extend_from_slice(&b[offsets[i + 1]..offsets[i + 2]]);
    }
    String::from_utf8(joined[..length].to_vec()).expect("UTF-8 markup")
}

/// One EXTH record: its type and its data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExthRecord {
    pub kind: u32,
    pub data: Vec<u8>,
}

/// Record 0's EXTH block, walked the way a reader walks it. Panics where the
/// block disagrees with itself.
pub fn exth_records(b: &[u8]) -> Vec<ExthRecord> {
    let r0 = u32_at(b, 78) as usize;
    assert_ne!(
        u32_at(b, r0 + 128) & 0x40,
        0,
        "the MOBI header says there is no EXTH block"
    );
    let at = r0 + 16 + u32_at(b, r0 + 20) as usize;
    assert_eq!(&b[at..at + 4], b"EXTH", "expected EXTH at {at}");
    let length = u32_at(b, at + 4) as usize;
    let count = u32_at(b, at + 8);
    let mut out = Vec::new();
    let mut p = at + 12;
    for _ in 0..count {
        let size = u32_at(b, p + 4) as usize;
        out.push(ExthRecord {
            kind: u32_at(b, p),
            data: b[p + 8..p + size].to_vec(),
        });
        p += size;
    }
    assert_eq!(
        p - at,
        length,
        "the EXTH block's length and its walk disagree"
    );
    out
}

/// The data of the one EXTH record of this type, or None.
pub fn exth(b: &[u8], kind: u32) -> Option<Vec<u8>> {
    let found: Vec<_> = exth_records(b)
        .into_iter()
        .filter(|r| r.kind == kind)
        .collect();
    assert!(
        found.len() <= 1,
        "EXTH {kind} is written {} times",
        found.len()
    );
    found.into_iter().next().map(|r| r.data)
}

/// A copy with the PDB header's creation and modification dates zeroed.
pub fn mask_pdb_dates(b: &[u8]) -> Vec<u8> {
    let mut c = b.to_vec();
    c[36..44].fill(0);
    c
}

pub fn sha256_hex(b: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

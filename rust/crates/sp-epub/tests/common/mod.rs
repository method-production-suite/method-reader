//! Shared by the sp-epub test files: where the oracle is, reading a zip
//! back, the recorded fountain-js tokens, and tests/css-rules.ts's
//! selector-exact rule reader.
#![allow(dead_code)]

use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sp_epub::{BodyOptions, BookBody, tokens_to_body};
use sp_model::{FormatOptions, Token};

/// The repository root (this crate is rust/crates/sp-epub).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("sp-epub sits three folders below the repo root")
        .to_path_buf()
}

/// Every dump folder under `rust/<which>/` that holds a book, sorted.
pub fn oracle_dirs(which: &str) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(repo_root().join("rust").join(which)) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("book.epub").is_file())
        .collect();
    out.sort();
    out
}

/// One zip entry as a reader sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub stored: bool,
    pub dir: bool,
    pub data: Vec<u8>,
}

pub fn read_zip(bytes: &[u8]) -> Vec<Entry> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a zip");
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).expect("an entry");
            let mut data = Vec::new();
            f.read_to_end(&mut data).expect("entry bytes");
            Entry {
                name: f.name().expect("a name").into_owned(),
                stored: f.compression() == zip::CompressionMethod::Stored,
                dir: f.is_dir(),
                data,
            }
        })
        .collect()
}

/// One entry's text.
pub fn entry_text(bytes: &[u8], name: &str) -> Option<String> {
    read_zip(bytes)
        .into_iter()
        .find(|e| e.name == name)
        .map(|e| String::from_utf8(e.data).expect("UTF-8"))
}

/// The compression method each entry's LOCAL header records, read off the
/// bytes (0 stored, 8 deflated), in order.
pub fn methods(bytes: &[u8]) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut i = 0;
    while i + 30 <= bytes.len() && u32_at(i) == 0x0403_4b50 {
        let name_len = usize::from(u16_at(i + 26));
        let name = String::from_utf8_lossy(&bytes[i + 30..i + 30 + name_len]).into_owned();
        out.push((name, u16_at(i + 8)));
        i += 30 + name_len + usize::from(u16_at(i + 28)) + u32_at(i + 18) as usize;
    }
    out
}

/// Every central-directory field JSZip writes but the compressed size and
/// the local header's offset, entry by entry, plus each local header's
/// copy of the same fields: (name, made by, needed, flags, method, time,
/// date, crc, size, extra length, comment length, external attributes).
pub fn header_fields(bytes: &[u8]) -> Vec<[u64; 11]> {
    let u16_at = |i: usize| u64::from(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
    let u32_at = |i: usize| {
        u64::from(u32::from_le_bytes([
            bytes[i],
            bytes[i + 1],
            bytes[i + 2],
            bytes[i + 3],
        ]))
    };
    let end = (0..=bytes.len() - 22)
        .rev()
        .find(|&i| u32_at(i) == 0x0605_4b50)
        .expect("an end-of-central-directory record");
    let count = u16_at(end + 10);
    let mut at = usize::try_from(u32_at(end + 16)).unwrap();
    let mut out = Vec::new();
    for _ in 0..count {
        assert_eq!(u32_at(at), 0x0201_4b50, "a central header");
        let name_len = usize::try_from(u16_at(at + 28)).unwrap();
        let extra_len = usize::try_from(u16_at(at + 30)).unwrap();
        let comment_len = usize::try_from(u16_at(at + 32)).unwrap();
        let local = usize::try_from(u32_at(at + 42)).unwrap();
        // The local header repeats bytes 6..30 of the central one from
        // offset 4: needed, flags, method, time, date, crc, sizes, lengths.
        assert_eq!(
            bytes[local + 4..local + 16],
            bytes[at + 6..at + 18],
            "local and central headers agree"
        );
        let name_hash = bytes[at + 46..at + 46 + name_len]
            .iter()
            .fold(0u64, |h, &b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
        out.push([
            name_hash,
            u16_at(at + 4),
            u16_at(at + 6),
            u16_at(at + 8),
            u16_at(at + 10),
            u16_at(at + 12),
            u16_at(at + 14),
            u32_at(at + 16),
            u32_at(at + 24),
            (extra_len as u64) << 16 | comment_len as u64,
            u32_at(at + 38),
        ]);
        at += 46 + name_len + extra_len + comment_len;
    }
    out
}

/// TRANSITIONAL (until sp-fountain's tokenizer lands): fountain-js's tokens
/// for a source the TypeScript tests parse, recorded by
/// tests/data/record-tokens.ts. A source missing from the recording is a
/// test written without regenerating it.
pub fn tokens(src: &str) -> Vec<Token> {
    static RECORDED: OnceLock<BTreeMap<String, Vec<Token>>> = OnceLock::new();
    let all = RECORDED.get_or_init(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fountain-tokens.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("the recording"))
            .expect("recorded tokens")
    });
    all.get(src)
        .unwrap_or_else(|| {
            panic!("no recorded tokens for {src:?}: rerun tests/data/record-tokens.ts")
        })
        .clone()
}

/// `tokensToBody(tokens(src), { format, images })`.
pub fn body_of(src: &str, format: &FormatOptions, images: &[&str]) -> BookBody {
    tokens_to_body(
        &tokens(src),
        &BodyOptions {
            format: *format,
            images: images
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<HashSet<_>>(),
            ..BodyOptions::default()
        },
    )
}

/// `tokensToBody(tokens(src))`.
pub fn default_body(src: &str) -> BookBody {
    tokens_to_body(&tokens(src), &BodyOptions::default())
}

// ── tests/css-rules.ts ─────────────────────────────────────

/// `/\/\*[\s\S]*?\*\//g` replaced with `''`.
pub fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(at) = rest.find("/*") {
        match rest[at + 2..].find("*/") {
            Some(end) => {
                out.push_str(&rest[..at]);
                rest = &rest[at + 2 + end + 2..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// `normalizeSelector`: comments stripped, whitespace runs collapsed,
/// trimmed.
pub fn normalize_selector(s: &str) -> String {
    strip_comments(s)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// One rule: its normalized selector and its declaration body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub selector: String,
    pub body: String,
}

/// `eachRule`: `/([^{}]+)\{([^}]*)\}/g` over the sheet.
pub fn each_rule(css: &str) -> Vec<Rule> {
    let mut out = Vec::new();
    let b = css.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // The selector: a maximal run of non-brace characters ending at `{`.
        let start = i;
        while i < b.len() && b[i] != b'{' && b[i] != b'}' {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        if b[i] == b'}' || i == start {
            i += 1;
            continue;
        }
        let open = i;
        // The body is `[^}]*`, which may hold a `{`. With no `}` left there
        // is no match here or anywhere after.
        let Some(len) = css[open + 1..].find('}') else {
            break;
        };
        let j = open + 1 + len;
        out.push(Rule {
            selector: normalize_selector(&css[start..open]),
            body: css[open + 1..j].to_owned(),
        });
        i = j + 1;
    }
    out
}

/// `ruleFor`: the rule whose selector is EXACTLY `selector`, as
/// `${selector} {${body}}`; panics when there is none.
pub fn rule_for(css: &str, selector: &str) -> String {
    let target = normalize_selector(selector);
    let hit = each_rule(css)
        .into_iter()
        .find(|r| r.selector == target)
        .unwrap_or_else(|| panic!("no rule for selector: {selector}"));
    format!("{} {{{}}}", hit.selector, hit.body)
}

/// A regex match, for the TypeScript's `toMatch`.
pub fn matches(haystack: &str, pattern: &str) -> bool {
    regex::Regex::new(pattern)
        .expect("a pattern")
        .is_match(haystack)
}

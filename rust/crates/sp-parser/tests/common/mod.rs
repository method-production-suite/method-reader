//! Shared by the integration tests: the oracle's folders, and a PNG reader
//! that is not the encoder under test.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde_json::Value;

/// rust/ (this crate is rust/crates/sp-parser).
pub fn rust_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The dump folders under rust/<folder>, sorted.
pub fn dumps(folder: &str) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(rust_dir().join(folder)) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

/// One committed fixture's dump folder.
pub fn oracle(fixture: &str) -> PathBuf {
    rust_dir().join("oracle").join(fixture)
}

pub fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).expect("oracle JSON parses"))
}

/// A committed fixture's PDF facts (`rust/oracle/<fixture>/pages.json`):
/// what pdf.js said about `tests/fixtures/<fixture>.pdf`, standing in for
/// the PDF itself until sp-pdf reads it.
pub fn facts(fixture: &str) -> sp_parser::facts::DocumentFacts {
    let v = read_json(&oracle(fixture).join("pages.json"))
        .unwrap_or_else(|| panic!("no oracle dump for {fixture}"));
    serde_json::from_value(v).expect("pages.json reads into DocumentFacts")
}

/// `extractDocument(tests/fixtures/<fixture>.pdf)`.
pub fn extract(fixture: &str) -> sp_parser::extract::ExtractResult {
    sp_parser::extract::extract_document(&facts(fixture), None, None)
}

/// `parseLines(...extractDocument(tests/fixtures/<fixture>.pdf))`.
pub fn parse(fixture: &str) -> sp_model::ParsedScreenplay {
    let doc = extract(fixture);
    sp_parser::parse_lines(&doc.lines, &doc.images)
}

/// The real-script dumps in rust/oracle-real/ (gitignored; empty on CI).
/// Refer to them by number only, never by name.
pub fn real_dumps() -> Vec<PathBuf> {
    dumps("oracle-real")
}

/// A real script's facts, from its dump folder.
pub fn facts_at(dir: &Path) -> sp_parser::facts::DocumentFacts {
    let v = read_json(&dir.join("pages.json")).expect("pages.json");
    serde_json::from_value(v).expect("pages.json reads into DocumentFacts")
}

/// Where two JSON values first differ, as a path and both sides: for
/// failure messages that point at the difference instead of printing both.
pub fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => {
                        if let Some(d) = first_diff(v, w, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                    None => return Some(format!("{path}.{k}: only ours has it ({v})")),
                }
            }
            y.keys()
                .find(|k| !x.contains_key(*k))
                .map(|k| format!("{path}.{k}: only theirs has it ({})", y[k]))
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                if let Some(d) = first_diff(v, w, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
            (x.len() != y.len()).then(|| format!("{path}: {} items against {}", x.len(), y.len()))
        }
        _ => (a != b).then(|| format!("{path}: ours {a}, theirs {b}")),
    }
}

/// A decoded PNG: its dimensions, colour type and every row's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub colour_type: u8,
    pub pixels: Vec<u8>,
    /// The filter type each row was written with.
    pub filters: Vec<u8>,
}

fn paeth(a: i32, b: i32, c: i32) -> i32 {
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Decode an 8-bit truecolour, non-interlaced PNG (all this engine writes),
/// checking every chunk's CRC on the way.
///
/// # Panics
/// On anything else, or a malformed file.
pub fn decode_png(bytes: &[u8]) -> Decoded {
    assert_eq!(
        &bytes[..8],
        &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
        "PNG signature"
    );
    let mut at = 8;
    let mut ihdr = None;
    let mut idat = Vec::new();
    let mut ended = false;
    while at < bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let kind = &bytes[at + 4..at + 8];
        let data = &bytes[at + 8..at + 8 + len];
        let crc = u32::from_be_bytes(bytes[at + 8 + len..at + 12 + len].try_into().unwrap());
        assert_eq!(
            sp_parser::png::crc32(&bytes[at + 4..at + 8 + len]),
            crc,
            "chunk CRC"
        );
        match kind {
            b"IHDR" => ihdr = Some(data.to_vec()),
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => ended = true,
            _ => {}
        }
        at += 12 + len;
    }
    assert!(ended, "IEND");
    let ihdr = ihdr.expect("IHDR");
    let width = u32::from_be_bytes(ihdr[0..4].try_into().unwrap());
    let height = u32::from_be_bytes(ihdr[4..8].try_into().unwrap());
    let (bit_depth, colour_type) = (ihdr[8], ihdr[9]);
    assert_eq!(
        (bit_depth, colour_type, ihdr[12]),
        (8, 2, 0),
        "8-bit truecolour, no interlace"
    );
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&idat).expect("IDAT inflates");
    let stride = width as usize * 3;
    assert_eq!(raw.len(), (stride + 1) * height as usize, "IDAT size");
    let mut pixels = vec![0u8; stride * height as usize];
    let mut filters = Vec::new();
    for y in 0..height as usize {
        let f = raw[y * (stride + 1)];
        filters.push(f);
        let line = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for i in 0..stride {
            let a = if i >= 3 {
                i32::from(pixels[y * stride + i - 3])
            } else {
                0
            };
            let b = if y > 0 {
                i32::from(pixels[(y - 1) * stride + i])
            } else {
                0
            };
            let c = if y > 0 && i >= 3 {
                i32::from(pixels[(y - 1) * stride + i - 3])
            } else {
                0
            };
            let p = match f {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) >> 1,
                4 => paeth(a, b, c),
                other => panic!("filter type {other}"),
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                pixels[y * stride + i] = ((i32::from(line[i]) + p) & 0xff) as u8;
            }
        }
    }
    Decoded {
        width,
        height,
        bit_depth,
        colour_type,
        pixels,
        filters,
    }
}

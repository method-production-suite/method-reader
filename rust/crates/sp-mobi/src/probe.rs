//! The MOBI picture probe's book builder, as a library function, so the
//! probe can move to xtask (ports tools/mobi-picture-probe.ts's builder).
//!
//! It builds small books for one question only a device can answer: how
//! large a picture an older Kindle accepts in a .mobi. The combined book
//! shows three pictures, each announced by a line of action that names its
//! size: about 2 KB, about 200 KB (past the single-record cap older Kindle
//! firmware is reported to have, about 127 KB), and about 1.4 MB as a
//! full-page plate. Each picture also gets a book of its own, for the
//! failure the combined one cannot report on: if one record too large makes
//! the Kindle refuse the whole book, the smaller sizes still answer.
//!
//! Every run writes the same bytes: the pictures are drawn from seeded
//! static, and each book is built at [`BUILT_AT`].
//!
//! One piece waits for a crate being ported beside this one: the probe's
//! EPUB (sp-epub), which the TypeScript tool writes beside the .mobi files
//! for comparison. [`probe_script`] is its input too.
//!
//! The PNGs are this module's own encoding (adaptive filters, zlib at level
//! 9 through miniz_oxide), so their bytes differ from node:zlib's while
//! their pixels do not (rust/PARITY.md's PNG decision); the sizes land in
//! the same ranges.

use std::collections::HashSet;

use sp_fountain::{is_safe_image_file, tokenize};
use sp_model::{DEFAULT_FORMAT_OPTIONS, Token, TokenType};

use crate::html::{MobiMeta, mobi_art, tokens_to_mobi_html};
use crate::writer::{MobiInput, build_mobi};

/// The combined book's file.
pub const MOBI_FILE: &str = "method-reader-picture-probe.mobi";
/// The combined book as an EPUB, which the TypeScript tool writes too.
pub const EPUB_FILE: &str = "method-reader-picture-probe.epub";
/// The build time every probe book records: 2026-10-01T00:00:00Z.
pub const BUILT_AT: u32 = 1_790_812_800;

/// One of the probe's pictures: the name its note gives it, and its PNG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbePicture {
    pub name: String,
    pub bytes: Vec<u8>,
    /// Set as a full-page plate, as the largest real plates are.
    pub full_page: bool,
    /// Its size as the single book that holds it alone is named: the file
    /// name part (`2kb`) and the title part (`2 KB`). Fixed rather than
    /// measured, so the files are named the same whatever the encoder.
    pub nominal_file: &'static str,
    pub nominal_title: &'static str,
}

/// The file the single book holding `picture` alone is written to.
pub fn single_mobi_file(picture: &ProbePicture) -> String {
    format!("method-reader-picture-probe-{}.mobi", picture.nominal_file)
}

/// The title of the single book holding `picture` alone.
pub fn single_title(picture: &ProbePicture) -> String {
    format!("Picture Probe {}", picture.nominal_title)
}

/// mulberry32: a small seeded generator, so every run draws the same static.
fn seeded(seed: u32) -> impl FnMut() -> u32 {
    let mut a = seed;
    move || {
        a = a.wrapping_add(0x6d2b_79f5);
        let mut t = a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        t ^ (t >> 14)
    }
}

/// A black frame 8 px wide around a checkerboard of 40 px squares in two
/// greys, with a centred `static_w` x `static_h` rectangle of RGB static.
/// The static sets the size: it does not compress.
fn draw_picture(
    width: usize,
    height: usize,
    static_w: usize,
    static_h: usize,
    seed: u32,
) -> Vec<u8> {
    const FRAME: usize = 8;
    const SQUARE: usize = 40;
    let mut next = seeded(seed);
    let x0 = (width - static_w) / 2;
    let y0 = (height - static_h) / 2;
    let mut rgb = vec![0u8; width * height * 3];
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) * 3;
            if x < FRAME || y < FRAME || x >= width - FRAME || y >= height - FRAME {
                continue;
            }
            if x >= x0 && x < x0 + static_w && y >= y0 && y < y0 + static_h {
                let [r0, r1, r2, _] = next().to_le_bytes();
                rgb[i] = r0;
                rgb[i + 1] = r1;
                rgb[i + 2] = r2;
                continue;
            }
            let light = ((x / SQUARE + y / SQUARE) & 1) == 0;
            let v = if light { 0xe0 } else { 0x60 };
            rgb[i] = v;
            rgb[i + 1] = v;
            rgb[i + 2] = v;
        }
    }
    encode_png(&rgb, width, height)
}

/// The three pictures, smallest first (sized by encoding, 2026-10-01: 13 x
/// 13 of static makes about 2 KB, 256 x 255 about 200 KB, and static across
/// the whole 600 x 800 plate inside its frame about 1.4 MB). The plate is an
/// older Kindle's own screen size, so its size in bytes is the only thing
/// about it that could be too large.
pub fn probe_pictures() -> Vec<ProbePicture> {
    vec![
        ProbePicture {
            name: "p001-01.png".to_owned(),
            bytes: draw_picture(480, 360, 13, 13, 1),
            full_page: false,
            nominal_file: "2kb",
            nominal_title: "2 KB",
        },
        ProbePicture {
            name: "p002-01.png".to_owned(),
            bytes: draw_picture(480, 360, 256, 255, 2),
            full_page: false,
            nominal_file: "200kb",
            nominal_title: "200 KB",
        },
        ProbePicture {
            name: "p003-01.png".to_owned(),
            bytes: draw_picture(600, 800, 584, 784, 3),
            full_page: true,
            nominal_file: "1400kb",
            nominal_title: "1.4 MB",
        },
    ]
}

/// A size as a person reads one: whole kilobytes under a megabyte,
/// megabytes to one decimal above, decimal units. JavaScript's
/// `Math.round` and `toFixed(1)`, ties rounding up.
pub fn plain_size(bytes: usize) -> String {
    #[allow(clippy::cast_precision_loss)]
    let b = bytes as f64;
    if bytes < 1_000_000 {
        return format!("{} KB", (b / 1000.0).round());
    }
    let x = b / 1_000_000.0;
    let tenths = x * 10.0;
    if tenths.fract() == 0.5 {
        return format!("{:.1} MB", (tenths.floor() + 1.0) / 10.0);
    }
    format!("{x:.1} MB")
}

/// The probe's script: a short scene per picture, each picture announced by
/// a line that names its size. Invented, like every fixture.
pub fn probe_script(pictures: &[ProbePicture], title: &str) -> String {
    let scenes = [
        "INT. TEST ROOM - DAY",
        "INT. LONG HALLWAY - NIGHT",
        "EXT. ROOFTOP - DAWN",
    ];
    let mut lines: Vec<String> = vec![
        format!("Title: {title}"),
        "Author: Marlow Penn".to_owned(),
        String::new(),
    ];
    let one = pictures.len() == 1;
    for (i, picture) in pictures.iter().enumerate() {
        lines.push(scenes[i].to_owned());
        lines.push(String::new());
        if i == 0 {
            lines.push(
                if one {
                    "A plain room, kept for one question: whether this Kindle shows the picture in it."
                } else {
                    "A plain room, kept for one question: which of three pictures this Kindle shows."
                }
                .to_owned(),
            );
            lines.push(String::new());
            lines.push(format!(
                "{} a black frame around a grey checkerboard with a patch of static in it. \
                 The bigger the file, the more of it is static.",
                if one {
                    "The picture is"
                } else {
                    "Each picture is"
                }
            ));
            lines.push(String::new());
        }
        let page = if picture.full_page {
            " It has a page to itself."
        } else {
            ""
        };
        lines.push(format!(
            "The next picture is about {}.{page}",
            plain_size(picture.bytes.len())
        ));
        lines.push(String::new());
        lines.push(format!(
            "[[img: {} w={}]]",
            picture.name,
            if picture.full_page { "100 full" } else { "60" }
        ));
        lines.push(String::new());
    }
    lines.push("The probe ends here.".to_owned());
    lines.push(String::new());
    lines.join("\n")
}

/// One single-picture book: its file, its title, its picture and its .mobi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingleProbe {
    pub file: String,
    pub title: String,
    pub picture: ProbePicture,
    pub mobi: Vec<u8>,
}

/// The probe's MOBI books and the pictures they carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeBooks {
    /// The combined book, all three pictures.
    pub mobi: Vec<u8>,
    pub pictures: Vec<ProbePicture>,
    /// Each picture alone, smallest first.
    pub singles: Vec<SingleProbe>,
}

/// `isPng`: the signature, an IHDR first, and nonzero dimensions.
fn is_png(bytes: &[u8]) -> bool {
    bytes.len() >= 29
        && bytes[..8] == PNG_SIGNATURE
        && &bytes[12..16] == b"IHDR"
        && bytes[16..20] != [0; 4]
        && bytes[20..24] != [0; 4]
}

/// The .mobi for one probe text, built as `convertFountain` builds a
/// .fountain's MOBI with Method Reader's defaults and the picture gate open
/// (`mobiPictures: true`): this probe is the device test that decides
/// whether the gate opens. `tokens` are fountain-js's tokens for `text`.
pub fn probe_mobi(tokens: &[Token], pictures: &[ProbePicture]) -> Vec<u8> {
    let format = DEFAULT_FORMAT_OPTIONS;
    let title = tokens
        .iter()
        .find(|t| t.kind == TokenType::Other("title".to_owned()))
        .and_then(|t| t.text.as_deref())
        .map(strip_tags);
    let author = tokens
        .iter()
        .find(|t| matches!(&t.kind, TokenType::Other(k) if k == "author" || k == "authors"))
        .and_then(|t| t.text.clone());
    let meta = MobiMeta {
        title: title.unwrap_or_else(|| "Untitled Screenplay".to_owned()),
        author,
    };
    // First bytes win for a name passed twice, as in renderBooks.
    let mut by_name: Vec<(&str, &[u8])> = Vec::new();
    for p in pictures {
        if is_safe_image_file(&p.name)
            && is_png(&p.bytes)
            && !by_name.iter().any(|(n, _)| *n == p.name)
        {
            by_name.push((&p.name, &p.bytes));
        }
    }
    let have: HashSet<String> = by_name.iter().map(|(n, _)| (*n).to_owned()).collect();
    let art = mobi_art(tokens, &format, &have);
    let html = tokens_to_mobi_html(tokens, &meta, &format, &art);
    let images = art
        .order
        .iter()
        .map(|name| {
            by_name
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, b)| b.to_vec())
                .expect("every picture in the order was handed over")
        })
        .collect();
    build_mobi(&MobiInput {
        title: meta.title,
        author: meta.author,
        html,
        images,
        cover_index: art.cover_index,
        time: Some(BUILT_AT),
    })
    .expect("the probe numbers its markup from its own picture list")
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

/// The books: the combined one with all three pictures, and each picture
/// alone in a book of its own, smallest first, each text tokenized as
/// `convertFountain` tokenizes it.
pub fn build_probe() -> ProbeBooks {
    let tokenize = |text: &str| tokenize(text).expect("the probe's own script tokenizes");
    let pictures = probe_pictures();
    let combined = probe_mobi(
        &tokenize(&probe_script(&pictures, "Picture Probe")),
        &pictures,
    );
    let singles = pictures
        .iter()
        .map(|picture| {
            let title = single_title(picture);
            let one = std::slice::from_ref(picture);
            SingleProbe {
                file: single_mobi_file(picture),
                mobi: probe_mobi(&tokenize(&probe_script(one, &title)), one),
                title,
                picture: picture.clone(),
            }
        })
        .collect();
    ProbeBooks {
        mobi: combined,
        pictures,
        singles,
    }
}

// ── PNG ──

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                0xedb8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(
        &u32::try_from(data.len())
            .expect("a chunk under 4 GiB")
            .to_be_bytes(),
    );
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[4..]);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

/// Each row under all five predictors, the cheapest by the spec's
/// minimum-sum-of-absolute-differences heuristic (src/parser/png.ts).
fn filter_row(row: &[u8], prev: &[u8], out: &mut Vec<u8>) {
    const BPP: usize = 3;
    let mut best: Option<(u64, Vec<u8>, u8)> = None;
    for kind in 0u8..5 {
        let mut buf = Vec::with_capacity(row.len());
        let mut score = 0u64;
        for i in 0..row.len() {
            let a = if i >= BPP { i32::from(row[i - BPP]) } else { 0 };
            let b = i32::from(prev[i]);
            let c = if i >= BPP {
                i32::from(prev[i - BPP])
            } else {
                0
            };
            let p = match kind {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) >> 1,
                _ => {
                    let pa = (b - c).abs();
                    let pb = (a - c).abs();
                    let pc = (a + b - 2 * c).abs();
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
            };
            let v = ((i32::from(row[i]) - p) & 0xff) as u8;
            buf.push(v);
            score += u64::from(if v < 128 { v } else { 255 - v + 1 });
        }
        if best.as_ref().is_none_or(|(s, _, _)| score < *s) {
            best = Some((score, buf, kind));
        }
    }
    let (_, buf, kind) = best.expect("five predictors tried");
    out.push(kind);
    out.extend_from_slice(&buf);
}

/// 8-bit RGB pixels as a PNG.
pub fn encode_png(rgb: &[u8], width: usize, height: usize) -> Vec<u8> {
    assert_eq!(
        rgb.len(),
        width * height * 3,
        "encode_png: RGB bytes for {width}x{height}"
    );
    let stride = width * 3;
    let mut raw = Vec::with_capacity((stride + 1) * height);
    let zero = vec![0u8; stride];
    for y in 0..height {
        let row = &rgb[y * stride..(y + 1) * stride];
        let prev = if y == 0 {
            &zero[..]
        } else {
            &rgb[(y - 1) * stride..y * stride]
        };
        filter_row(row, prev, &mut raw);
    }
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&u32::try_from(width).expect("a sane width").to_be_bytes());
    ihdr.extend_from_slice(&u32::try_from(height).expect("a sane height").to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit truecolour, deflate, adaptive, no interlace
    let idat = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9);
    [
        PNG_SIGNATURE.to_vec(),
        png_chunk(b"IHDR", &ihdr),
        png_chunk(b"IDAT", &idat),
        png_chunk(b"IEND", &[]),
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mulberry32_matches_javascript() {
        // Draws of mulberry32 as JavaScript computes them (bun 1.3.14).
        let mut next = seeded(1);
        assert_eq!(next(), 2_693_262_067);
        assert_eq!(next(), 11_749_833);
        let mut next = seeded(3);
        for _ in 0..100_000 {
            next();
        }
        assert_eq!(next(), 3_518_048_024);
    }

    #[test]
    fn plain_size_reads_as_a_person_reads_it() {
        assert_eq!(plain_size(1977), "2 KB");
        assert_eq!(plain_size(199_905), "200 KB");
        assert_eq!(plain_size(1_381_847), "1.4 MB");
        assert_eq!(plain_size(1_250_000), "1.3 MB");
    }

    #[test]
    fn strip_tags_like_the_regex() {
        assert_eq!(strip_tags("A <b>B</b> C"), "A B C");
        assert_eq!(strip_tags("a <> b"), "a <> b");
        assert_eq!(strip_tags("a < b"), "a < b");
    }
}

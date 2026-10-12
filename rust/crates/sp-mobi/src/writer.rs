//! Minimal MOBI 6 writer: a PalmDB ("BOOKMOBI") container holding
//! uncompressed UTF-8 HTML. Layout per the MobileRead MOBI documentation:
//! PDB header, record 0 (PalmDOC + MOBI header + EXTH + full name), text
//! records (at most 4096 bytes, split on UTF-8 boundaries), image records,
//! FLIS, FCIS, EOF. Kindles index sideloaded .mobi files, unlike sideloaded
//! EPUBs: this is the dependency-free USB transfer format.
//!
//! The header fields that name an image record follow two real MOBI 6
//! writers of the same book, calibre 9.11.0 and Amazon's kindlegen 2.9,
//! both read back with `calibre-debug --inspect-mobi` (measured
//! 2026-08-11). Where those two pin a meaning rather than merely agree on a
//! value, the measurement is at the field.
//!
//! Ports src/mobi/writer.ts.

use std::fmt;
use std::sync::LazyLock;

use sp_fountain::jsre::{Regex, group};

use crate::clock;
use crate::palmdb::{ByteWriter, exth_record, hash32, len32, palm_database};

/// What [`build_mobi`] packs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MobiInput {
    pub title: String,
    pub author: Option<String>,
    pub html: String,
    /// Picture records, in the order the HTML's `recindex` attributes count
    /// them: `recindex="00001"` is `images[0]`. [`crate::html::mobi_art`] is
    /// the one copy of that order; the renderer numbers from it, and the
    /// caller passes the bytes in it. None by default, which is the book this
    /// writer always built.
    pub images: Vec<Vec<u8>>,
    /// Which of `images` is the book's cover (registry #19b), 0-based,
    /// written as EXTH 201. `mobi_art`'s `cover_index`. The picture stays
    /// wherever the markup shows it: the cover names a record, it does not
    /// take one out of the flow. A book without one is byte for byte the
    /// book this writer built before covers.
    pub cover_index: Option<usize>,
    /// The build time written into the PalmDB header, in seconds since 1970.
    /// `None` reads [`clock::now_seconds`] (`SOURCE_DATE_EPOCH`, else now).
    pub time: Option<u32>,
}

/// A caller bug the writer refuses to pack, because the file would put the
/// wrong record on a page and nothing downstream could tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MobiError {
    /// The markup shows a `recindex` the picture records cannot answer.
    Recindex { recindex: String, pictures: usize },
    /// The cover names a picture past the last one.
    Cover { index: usize, pictures: usize },
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

impl fmt::Display for MobiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MobiError::Recindex { recindex, pictures } => write!(
                f,
                "The markup shows recindex=\"{recindex}\", but the book was given {pictures} picture{}, \
                 numbered from 00001, so that number would open a record that is not one of them.",
                plural(*pictures)
            ),
            MobiError::Cover { index, pictures } => write!(
                f,
                "The cover is picture {index}, counted from 0, but the book was given {pictures} picture{}, \
                 so that number would name a record that is not one of them.",
                plural(*pictures)
            ),
        }
    }
}

impl std::error::Error for MobiError {}

const TEXT_RECORD_SIZE: usize = 4096;

/// `/recindex="(\d+)"/g`.
static RECINDEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"recindex="(\d+)""#, "g"));

/// Split UTF-8 bytes into chunks of at most `size`, never inside a codepoint.
fn split_utf8(bytes: &[u8], size: usize) -> Vec<&[u8]> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let mut end = (start + size).min(bytes.len());
        // A UTF-8 continuation byte is 0b10xxxxxx: back up to a boundary.
        while end < bytes.len() && (bytes[end] & 0xc0) == 0x80 {
            end -= 1;
        }
        chunks.push(&bytes[start..end]);
        start = end;
    }
    chunks
}

/// The MOBI 6 file for `input`.
pub fn build_mobi(input: &MobiInput) -> Result<Vec<u8>, MobiError> {
    let text_bytes = input.html.as_bytes();
    let text_records = split_utf8(text_bytes, TEXT_RECORD_SIZE);
    let title_bytes = input.title.as_bytes();
    let author = input.author.as_deref().unwrap_or("");
    // NUL-separated, as the TypeScript template literal is (raw NULs there).
    let uid = hash32(&format!(
        "{}\0{}\0{}",
        input.title,
        author,
        text_bytes.len()
    ));
    let images = &input.images;

    // The markup and the records are one numbering with two ends (mobi_art),
    // and this is the one place both ends are in hand. A reader finds
    // recindex N at the first image record plus N - 1 and shows whatever
    // record sits there: past the last picture that is FLIS, and 00000 is
    // the last text record. So a number the records cannot answer puts the
    // wrong record on the page rather than leaving a picture out.
    let html: Vec<char> = input.html.chars().collect();
    let mut from = 0;
    while let Some(caps) = RECINDEX.find_from(&html, from) {
        from = caps[0].map_or(html.len(), |(_, end)| end);
        let n = group(&html, &caps, 1).unwrap_or_default();
        let n = n.as_str();
        let in_range = n
            .parse::<u128>()
            .ok()
            .is_some_and(|i| i >= 1 && i <= images.len() as u128);
        if !in_range {
            return Err(MobiError::Recindex {
                recindex: n.to_owned(),
                pictures: images.len(),
            });
        }
    }

    // The cover is named by the same kind of number, and fails the same way.
    if let Some(index) = input.cover_index
        && index >= images.len()
    {
        return Err(MobiError::Cover {
            index,
            pictures: images.len(),
        });
    }

    // ── EXTH ──
    let mut exth_records: Vec<Vec<u8>> = Vec::new();
    if !author.is_empty() {
        exth_records.push(exth_record(100, author.as_bytes()));
    }
    // 201, the cover, as a RECORD offset from the first image record (field
    // 108), 0-based: calibre wrote 0 with its cover in its first image
    // record, kindlegen 1 with a body picture first and the cover second
    // (measured 2026-08-11). Written without the 202 thumbnail both add,
    // because this engine has nothing to shrink one with. Only when there is
    // a cover, so a book without one keeps its old bytes.
    if let Some(index) = input.cover_index {
        exth_records.push(exth_record(201, &len32(index).to_be_bytes()));
    }
    exth_records.push(exth_record(501, b"EBOK"));
    exth_records.push(exth_record(503, title_bytes));
    exth_records.push(exth_record(524, b"en"));
    let exth_body: Vec<u8> = exth_records.concat();
    let mut exth = ByteWriter::new();
    exth.ascii("EXTH")
        .u32(len32(12 + exth_body.len()))
        .u32(len32(exth_records.len()))
        .bytes(&exth_body);
    let exth_pad = (4 - (exth.len() % 4)) % 4;
    exth.zeros(exth_pad);

    // ── record 0: PalmDOC(16) + MOBI(232) + EXTH + full name ──
    let mobi_header_length = 232; // spans [16, 248) in record 0
    let full_name_offset = 16 + mobi_header_length + exth.len();
    // Field 80, the first non-book record: the first record that is not
    // text. This writer emits no pad and no index records, so it is the one
    // straight after the text: the first picture, or FLIS without pictures.
    let first_non_book = 1 + text_records.len();
    let first_image = if images.is_empty() {
        0xffff_ffff
    } else {
        len32(first_non_book)
    };
    let flis_index = first_non_book + images.len();
    let fcis_index = flis_index + 1;

    let mut r0 = ByteWriter::new();
    // PalmDOC header
    r0.u16(1); // compression: none
    r0.u16(0);
    r0.u32(len32(text_bytes.len()));
    r0.u16(u16::try_from(text_records.len()).expect("at most 65535 text records"));
    r0.u16(TEXT_RECORD_SIZE as u16);
    r0.u16(0); // encryption: none
    r0.u16(0);
    // MOBI header (offsets are absolute within record 0)
    r0.ascii("MOBI");
    r0.u32(len32(mobi_header_length));
    r0.u32(2); // mobi type: book
    r0.u32(65001); // UTF-8
    r0.u32(uid);
    r0.u32(6); // version
    for _ in 0..10 {
        r0.u32(0xffff_ffff); // index fields 40..80
    }
    r0.u32(len32(first_non_book)); // 80: first non-book record
    r0.u32(len32(full_name_offset)); // 84
    r0.u32(len32(title_bytes.len())); // 88
    r0.u32(1033); // 92: locale en-US
    r0.u32(0); // 96: input language
    r0.u32(0); // 100: output language
    r0.u32(6); // 104: min version
    // 108: first image record, as an ABSOLUTE record number (both reference
    // writers wrote their first picture's own index); 0xffffffff is none.
    r0.u32(first_image);
    r0.u32(0); // 112: huffman record offset
    r0.u32(0); // 116: huffman record count
    r0.u32(0); // 120: huffman table offset
    r0.u32(0); // 124: huffman table length
    r0.u32(0x40); // 128: EXTH present
    r0.zeros(32); // 132..164 reserved
    r0.u32(0xffff_ffff); // 164
    r0.u32(0xffff_ffff); // 168: DRM offset
    r0.u32(0xffff_ffff); // 172: DRM count
    r0.u32(0); // 176: DRM size
    r0.u32(0); // 180: DRM flags
    r0.zeros(8); // 184..192
    r0.u16(1); // 192: first content record
    // 194: last content record, which is the last PICTURE, not the last text
    // record: both reference writers extend it over their pictures.
    r0.u16(u16::try_from(text_records.len() + images.len()).expect("at most 65535 records"));
    r0.u32(1); // 196
    r0.u32(len32(fcis_index)); // 200: FCIS record
    r0.u32(1); // 204
    r0.u32(len32(flis_index)); // 208: FLIS record
    r0.u32(1); // 212
    r0.zeros(8); // 216..224
    r0.u32(0xffff_ffff); // 224
    r0.u32(0); // 228: compilation count
    let pad = 16 + mobi_header_length - r0.len();
    r0.zeros(pad); // pad MOBI header to 248
    // EXTH + full name (2 nulls, pad to 4)
    r0.bytes(exth.as_slice());
    r0.bytes(title_bytes);
    r0.zeros(2);
    let pad = (4 - (r0.len() % 4)) % 4;
    r0.zeros(pad);
    let record0 = r0.into_vec();

    // ── FLIS / FCIS / EOF ──
    let mut flis = ByteWriter::new();
    flis.ascii("FLIS")
        .u32(8)
        .u16(65)
        .u16(0)
        .u32(0)
        .u32(0xffff_ffff)
        .u16(1)
        .u16(3)
        .u32(3)
        .u32(1)
        .u32(0xffff_ffff);
    let mut fcis = ByteWriter::new();
    fcis.ascii("FCIS")
        .u32(20)
        .u32(16)
        .u32(1)
        .u32(0)
        .u32(len32(text_bytes.len()))
        .u32(0)
        .u32(32)
        .u32(8)
        .u16(1)
        .u16(1)
        .u32(0);
    let eof: &[u8] = &[0xe9, 0x8e, 0x0d, 0x0a];

    // ── PDB container ──
    // Pictures ride between the text and FLIS, one record each, their bytes
    // as the caller handed them over.
    let mut records: Vec<&[u8]> = Vec::with_capacity(text_records.len() + images.len() + 4);
    records.push(&record0);
    records.extend(text_records.iter().copied());
    records.extend(images.iter().map(Vec::as_slice));
    records.push(flis.as_slice());
    records.push(fcis.as_slice());
    records.push(eof);
    let time = input
        .time
        .unwrap_or_else(|| clock_u32(clock::now_seconds()));
    Ok(palm_database(&input.title, uid, &records, time))
}

/// A time as the PalmDB stores it: 32 bits, as JavaScript's `>>> 0` keeps it.
pub fn clock_u32(seconds: u64) -> u32 {
    (seconds & 0xffff_ffff) as u32
}

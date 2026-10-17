//! The KF8 index records (INDX), their string tables (CNCX), and the number
//! encodings both use.
//!
//! Ported from Calibre 9.11.0's src/calibre/ebooks/mobi/writer8/index.py and
//! the helpers it takes from src/calibre/ebooks/mobi/utils.py (encint,
//! to_base, CNCX, align_block, utf8_text). Calibre is GPL-3, Copyright Kovid
//! Goyal; this port is licensed GPL-3.0-only (LICENSE-GPL-3.0) (see
//! THIRD-PARTY-NOTICES.md). Ports src/azw3/index.ts.
//!
//! An index is a header record, then one or more records of entries, then
//! the CNCX records holding its strings. Each entry is a short name, control
//! bytes saying which tags it carries, and the tags' values as
//! variable-width integers. A KF8 book carries four: the skeleton (one entry
//! per XHTML file), the fragment or "chunk" table (one per piece of body cut
//! out of a file), the guide, and the NCX (the table of contents).

use std::collections::HashMap;

use sp_fountain::js;
use sp_mobi::palmdb::ByteWriter;
use unicode_normalization::UnicodeNormalization;

use crate::Azw3Error;

/// A variable-width integer: 7 bits per byte, big-endian. Forward marks the
/// LAST byte with the high bit, so a reader going forward knows where it
/// ends; backward marks the first, for a reader coming from the end of a
/// record (utils.py's encint).
pub fn encint(value: u64, forward: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut v = value;
    loop {
        bytes.push((v & 0x7f) as u8);
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    // Collected least significant first, so index 0 is the last byte once
    // reversed.
    let at = if forward { 0 } else { bytes.len() - 1 };
    bytes[at] |= 0x80;
    bytes.reverse();
    bytes
}

const DIGITS: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// `num` in `base`, upper case, left-padded with zeros to `min_digits`
/// (utils.py's to_base). Record references in KF8 are base 32.
pub fn to_base(num: u64, base: u64, min_digits: usize) -> String {
    let mut digits = Vec::new();
    let mut n = num;
    loop {
        digits.push(DIGITS[(n % base) as usize]);
        n /= base;
        if n == 0 {
            break;
        }
    }
    while digits.len() < min_digits {
        digits.push(b'0');
    }
    digits.reverse();
    String::from_utf8(digits).expect("ASCII digits")
}

/// Pad to a multiple of 4 with zero bytes (utils.py's align_block).
pub fn align_block(mut raw: Vec<u8>) -> Vec<u8> {
    let extra = raw.len() % 4;
    if extra != 0 {
        raw.resize(raw.len() + 4 - extra, 0);
    }
    raw
}

/// A string as KF8 stores it: trimmed, NFC, UTF-8, never empty
/// (utils.py's utf8_text, which writes "Unknown" for an empty one).
pub fn utf8_text(text: &str) -> Vec<u8> {
    let t = js::trim(text);
    if t.is_empty() {
        b"Unknown".to_vec()
    } else {
        t.nfc().collect::<String>().into_bytes()
    }
}

/// The strings an index points at, packed into records of at most 64 KiB
/// less kindlegen's 1024-byte margin. An offset is the string's place in its
/// record plus 0x10000 for each record before it.
#[derive(Debug, Clone, Default)]
pub struct Cncx {
    pub records: Vec<Vec<u8>>,
    offsets: HashMap<String, u64>,
}

impl Cncx {
    pub const MAX_STRING_LENGTH: usize = 500;

    pub fn new<'a>(strings: impl IntoIterator<Item = &'a str>) -> Cncx {
        const RECORD_LIMIT: usize = 0x10000 - 1024;
        let mut cncx = Cncx::default();
        let mut buf: Vec<u8> = Vec::new();
        let mut offset: u64 = 0;
        for s in strings {
            if cncx.offsets.contains_key(s) {
                continue;
            }
            let clipped: String = s.chars().take(Self::MAX_STRING_LENGTH).collect();
            let utf8 = utf8_text(&clipped);
            let mut raw = encint(utf8.len() as u64, true);
            raw.extend_from_slice(&utf8);
            if buf.len() + raw.len() > RECORD_LIMIT {
                cncx.records.push(align_block(std::mem::take(&mut buf)));
                offset = cncx.records.len() as u64 * 0x10000;
            }
            buf.extend_from_slice(&raw);
            cncx.offsets.insert(s.to_owned(), offset);
            offset += raw.len() as u64;
        }
        if !buf.is_empty() {
            cncx.records.push(align_block(buf));
        }
        cncx
    }

    pub fn offset_of(&self, s: &str) -> Result<u64, Azw3Error> {
        self.offsets
            .get(s)
            .copied()
            .ok_or_else(|| Azw3Error::new(format!("The string table holds no \"{s}\".")))
    }
}

/// One tag an index's entries may carry. End-flag entries separate control
/// bytes and carry nothing (index.py's TagMeta).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagMeta {
    pub name: &'static str,
    pub number: u8,
    pub values_per_entry: u8,
    pub bitmask: u8,
    pub end_flag: u8,
}

const fn tag(name: &'static str, number: u8, values_per_entry: u8, bitmask: u8) -> TagMeta {
    TagMeta {
        name,
        number,
        values_per_entry,
        bitmask,
        end_flag: 0,
    }
}

const END_TAG: TagMeta = TagMeta {
    name: "eof",
    number: 0,
    values_per_entry: 0,
    bitmask: 0,
    end_flag: 1,
};

/// How far a value is shifted under its mask (index.py's mask_to_bit_shifts).
fn mask_shift(mut mask: u8) -> u32 {
    let mut shift = 0;
    while mask != 0 && mask & 1 == 0 {
        mask >>= 1;
        shift += 1;
    }
    shift
}

/// One index entry: its name and its tags' values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub name: Vec<u8>,
    pub tags: Vec<(&'static str, Vec<u64>)>,
}

impl IndexEntry {
    fn values(&self, name: &str) -> &[u64] {
        self.tags
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(&[], |(_, v)| v.as_slice())
    }
}

const HEADER_LENGTH: usize = 192;

fn u32_of(n: usize) -> u32 {
    u32::try_from(n).expect("an index under 4 GiB")
}

fn u16_of(n: usize) -> Result<u16, Azw3Error> {
    u16::try_from(n).map_err(|_| Azw3Error::new("An index field outgrew 16 bits."))
}

/// Every record of one index: the header, the entry records, then its CNCX
/// records (index.py's Index.__call__).
pub fn build_index(
    tag_types: &[TagMeta],
    entries: &[IndexEntry],
    cncx: &Cncx,
    control_byte_count: usize,
) -> Result<Vec<Vec<u8>>, Azw3Error> {
    // Control bytes: for each tag, how many entries' worth of values it has,
    // shifted under its mask; an end tag closes one control byte.
    let mut control_bytes: Vec<Vec<u8>> = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut cbs = Vec::new();
        let mut ans: u64 = 0;
        for t in tag_types {
            if t.end_flag == 1 {
                cbs.push((ans & 0xff) as u8);
                ans = 0;
                continue;
            }
            let nvals = entry.values(t.name).len() as u64;
            ans |= u64::from(t.bitmask)
                & ((nvals / u64::from(t.values_per_entry)) << mask_shift(t.bitmask));
        }
        if cbs.len() != control_byte_count {
            return Err(Azw3Error::new(format!(
                "The index entry {} is invalid.",
                String::from_utf8_lossy(&entry.name)
            )));
        }
        control_bytes.push(cbs);
    }

    const RECORD_LIMIT: usize = 0x10000 - HEADER_LENGTH - 1048; // kindlegen's margin
    let mut index_blocks: Vec<Vec<Vec<u8>>> = vec![Vec::new()];
    let mut idxt_blocks: Vec<Vec<usize>> = vec![Vec::new()];
    let mut block_sizes: Vec<usize> = vec![0];
    let mut record_counts: Vec<usize> = vec![0];
    let mut last_indices: Vec<Vec<u8>> = vec![Vec::new()];

    for (i, entry) in entries.iter().enumerate() {
        let name = &entry.name;
        let mut raw = vec![
            u8::try_from(name.len())
                .map_err(|_| Azw3Error::new("An index name over 255 bytes."))?,
        ];
        raw.extend_from_slice(name);
        raw.extend_from_slice(&control_bytes[i]);
        for t in tag_types {
            for &val in entry.values(t.name) {
                raw.extend(encint(val, true));
            }
        }
        let mut offset = *block_sizes.last().expect("one block");
        let idxt_pos = idxt_blocks.last().expect("one block").len() * 2;
        if offset + idxt_pos + raw.len() + 2 > RECORD_LIMIT {
            index_blocks.push(Vec::new());
            idxt_blocks.push(Vec::new());
            block_sizes.push(0);
            record_counts.push(0);
            last_indices.push(Vec::new());
            offset = 0;
        }
        let last = record_counts.len() - 1;
        record_counts[last] += 1;
        idxt_blocks[last].push(HEADER_LENGTH + offset);
        block_sizes[last] += raw.len();
        index_blocks[last].push(raw);
        last_indices[last] = name.clone();
    }

    let mut index_records = Vec::new();
    for (r, blocks) in index_blocks.iter().enumerate() {
        let index_block = align_block(blocks.concat());
        let mut idxt = ByteWriter::new();
        idxt.ascii("IDXT");
        for &p in &idxt_blocks[r] {
            idxt.u16(u16_of(p)?);
        }
        let idxt_block = align_block(idxt.into_vec());
        let mut head = ByteWriter::new();
        head.ascii("INDX")
            .u32(u32_of(HEADER_LENGTH))
            .zeros(4)
            .u32(1) // header type: 1 for an index record
            .zeros(4)
            .u32(u32_of(HEADER_LENGTH + index_block.len())) // where IDXT starts
            .u32(u32_of(record_counts[r]))
            .fill(8, 0xff)
            .zeros(156);
        let record = [head.into_vec(), index_block, idxt_block].concat();
        if record.len() > 0x10000 {
            return Err(Azw3Error::new("An index record outgrew 64 KiB."));
        }
        index_records.push(record);
    }

    // TAGX: each tag's number, values per entry, mask and end flag.
    let mut tagx = ByteWriter::new();
    tagx.ascii("TAGX")
        .u32(u32_of(12 + tag_types.len() * 4))
        .u32(u32_of(control_byte_count));
    for t in tag_types {
        tagx.u8(t.number)
            .u8(t.values_per_entry)
            .u8(t.bitmask)
            .u8(t.end_flag);
    }
    let tagx_bytes = tagx.into_vec();

    // The header's own entries: each record's last name and entry count,
    // pointed at by the header's IDXT.
    let mut geometry = ByteWriter::new();
    let mut header_idxt = ByteWriter::new();
    header_idxt.ascii("IDXT");
    let mut pos = HEADER_LENGTH + tagx_bytes.len();
    for (r, name) in last_indices.iter().enumerate() {
        let start = geometry.len();
        header_idxt.u16(u16_of(pos)?);
        geometry
            .u8(u8::try_from(name.len())
                .map_err(|_| Azw3Error::new("An index name over 255 bytes."))?)
            .bytes(name)
            .u16(u16_of(record_counts[r])?);
        pos += geometry.len() - start;
    }

    let tagx_aligned = align_block(tagx_bytes);
    let geometry_aligned = align_block(geometry.into_vec());
    let idxt_aligned = align_block(header_idxt.into_vec());
    let mut header = ByteWriter::new();
    header
        .ascii("INDX")
        .u32(u32_of(HEADER_LENGTH))
        .zeros(8)
        .u32(2) // index type, as Calibre writes it
        .u32(u32_of(
            HEADER_LENGTH + tagx_aligned.len() + geometry_aligned.len(),
        )) // IDXT offset
        .u32(u32_of(index_records.len()))
        .u32(65001) // UTF-8
        .u32(0xffff_ffff)
        .u32(u32_of(record_counts.iter().sum()))
        .u32(0) // ORDT offset
        .u32(0) // LIGT offset
        .u32(0) // ORDT/LIGT entries
        .u32(u32_of(cncx.records.len()))
        .zeros(124)
        .u32(u32_of(HEADER_LENGTH)) // TAGX offset
        .zeros(8)
        .bytes(&tagx_aligned)
        .bytes(&geometry_aligned)
        .bytes(&idxt_aligned);

    let mut out = vec![header.into_vec()];
    out.extend(index_records);
    out.extend(cncx.records.iter().cloned());
    Ok(out)
}

// ── The four KF8 indices ──

/// One skeleton-table row: a file's fragment count and where its skeleton
/// sits in the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkelEntry {
    pub file_number: usize,
    pub chunk_count: usize,
    pub start_pos: usize,
    pub length: usize,
}

/// One entry per XHTML file. Calibre: "Don't ask me why these entries have
/// to be repeated twice"; kindlegen writes them so.
pub fn skel_index(table: &[SkelEntry]) -> Result<Vec<Vec<u8>>, Azw3Error> {
    let types = [
        tag("chunk_count", 1, 1, 3),
        tag("geometry", 6, 2, 12),
        END_TAG,
    ];
    let entries: Vec<IndexEntry> = table
        .iter()
        .map(|s| IndexEntry {
            name: format!("SKEL{:010}", s.file_number).into_bytes(),
            tags: vec![
                (
                    "chunk_count",
                    vec![s.chunk_count as u64, s.chunk_count as u64],
                ),
                (
                    "geometry",
                    vec![
                        s.start_pos as u64,
                        s.length as u64,
                        s.start_pos as u64,
                        s.length as u64,
                    ],
                ),
            ],
        })
        .collect();
    build_index(&types, &entries, &Cncx::default(), 1)
}

/// One fragment-table row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkEntry {
    pub insert_pos: usize,
    pub selector: String,
    pub file_number: usize,
    pub sequence_number: usize,
    pub start_pos: usize,
    pub length: usize,
}

/// One entry per fragment: where it goes into its skeleton, the selector
/// naming the element it belongs to, and where its bytes sit.
pub fn chunk_index(table: &[ChunkEntry]) -> Result<Vec<Vec<u8>>, Azw3Error> {
    let types = [
        tag("cncx_offset", 2, 1, 1),
        tag("file_number", 3, 1, 2),
        tag("sequence_number", 4, 1, 4),
        tag("geometry", 6, 2, 8),
        END_TAG,
    ];
    let cncx = Cncx::new(table.iter().map(|c| c.selector.as_str()));
    let mut entries = Vec::with_capacity(table.len());
    for c in table {
        entries.push(IndexEntry {
            name: format!("{:010}", c.insert_pos).into_bytes(),
            tags: vec![
                ("cncx_offset", vec![cncx.offset_of(&c.selector)?]),
                ("file_number", vec![c.file_number as u64]),
                ("sequence_number", vec![c.sequence_number as u64]),
                ("geometry", vec![c.start_pos as u64, c.length as u64]),
            ],
        });
    }
    build_index(&types, &entries, &cncx, 1)
}

/// One guide entry: a named place (where reading starts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideEntry {
    pub kind: String,
    pub title: String,
    pub pos_fid: (usize, usize),
}

/// The guide. The Kindle wants it sorted by type (main.py sorts before
/// indexing).
pub fn guide_index(table: &[GuideEntry]) -> Result<Vec<Vec<u8>>, Azw3Error> {
    let types = [tag("title", 1, 1, 1), tag("pos_fid", 6, 2, 2), END_TAG];
    let cncx = Cncx::new(table.iter().map(|g| g.title.as_str()));
    let mut entries = Vec::with_capacity(table.len());
    for g in table {
        entries.push(IndexEntry {
            name: g.kind.clone().into_bytes(),
            tags: vec![
                ("title", vec![cncx.offset_of(&g.title)?]),
                ("pos_fid", vec![g.pos_fid.0 as u64, g.pos_fid.1 as u64]),
            ],
        });
    }
    build_index(&types, &entries, &cncx, 1)
}

/// One table-of-contents entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NcxEntry {
    pub index: usize,
    pub offset: usize,
    pub length: usize,
    pub label: String,
    pub depth: usize,
    pub parent: Option<usize>,
    pub first_child: Option<usize>,
    pub last_child: Option<usize>,
    pub pos_fid: (usize, usize),
}

/// The table of contents. Entry names are the entry's index in hex, at
/// least two digits, all the same width.
pub fn ncx_index(table: &[NcxEntry]) -> Result<Vec<Vec<u8>>, Azw3Error> {
    let types = [
        tag("offset", 1, 1, 1),
        tag("length", 2, 1, 2),
        tag("label", 3, 1, 4),
        tag("depth", 4, 1, 8),
        tag("parent", 21, 1, 16),
        tag("first_child", 22, 1, 32),
        tag("last_child", 23, 1, 64),
        tag("pos_fid", 6, 2, 128),
        END_TAG,
    ];
    let cncx = Cncx::new(table.iter().map(|e| e.label.as_str()));
    let largest = table.iter().map(|e| e.index).max().unwrap_or(0);
    let width = format!("{largest:x}").len().max(2);
    let mut entries = Vec::with_capacity(table.len());
    for e in table {
        let mut tags: Vec<(&'static str, Vec<u64>)> = vec![
            ("offset", vec![e.offset as u64]),
            ("length", vec![e.length as u64]),
            ("label", vec![cncx.offset_of(&e.label)?]),
            ("depth", vec![e.depth as u64]),
            ("pos_fid", vec![e.pos_fid.0 as u64, e.pos_fid.1 as u64]),
        ];
        if let Some(p) = e.parent {
            tags.push(("parent", vec![p as u64]));
        }
        if let Some(c) = e.first_child {
            tags.push(("first_child", vec![c as u64]));
        }
        if let Some(c) = e.last_child {
            tags.push(("last_child", vec![c as u64]));
        }
        entries.push(IndexEntry {
            name: format!("{:0width$X}", e.index).into_bytes(),
            tags,
        });
    }
    build_index(&types, &entries, &cncx, 1)
}

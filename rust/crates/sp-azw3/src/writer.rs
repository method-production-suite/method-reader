//! Method Reader's own AZW3 (KF8) writer: the EPUB in, a Kindle book out, with
//! no Calibre. BETA: tried in our tests and in Calibre's reader, not yet on
//! a Kindle (docs/formatting-options-log.md, the in-house AZW3 entry).
//!
//! Ported from Calibre 9.11.0's src/calibre/ebooks/mobi/writer8/ (main.py's
//! text records, FDST, indices and guide; mobi.py's record 0 and record
//! layout; exth.py), with writer2/resources.py's resource records. Calibre
//! is GPL-3, Copyright Kovid Goyal; this port is licensed
//! GPL-3.0-only (LICENSE-GPL-3.0) (see THIRD-PARTY-NOTICES.md). Ports src/azw3/writer.ts.
//!
//! The file, record by record:
//!   0                 the PalmDOC header, the 264-byte MOBI header (version
//!                     8), EXTH, the full title, 8192 bytes of padding
//!   1..n              text records: 4096 bytes of text each, compressed,
//!                     then trailing entries (UTF-8 overlap, TBS)
//!   (pad)             zeros, when the text records do not end on 4 bytes
//!   fragment INDX, skeleton INDX, guide INDX, NCX INDX, each with its CNCX
//!   resources         the pictures, in manifest order
//!   FDST, FLIS, FCIS, EOF
//!
//! Where this departs from Calibre, on purpose (each in the registry
//! entry): no generated inline Table of Contents page; no thumbnail record.
//! Pictures pass through as they are. The stylesheet passes through as the
//! EPUB has it; the landmarks' bodymatter becomes the guide's start-reading
//! entry; a text run too long for one fragment is never cut inside an
//! entity (chunk.rs).

use std::collections::HashMap;

use sp_mobi::clock;
use sp_mobi::palmdb::{ByteWriter, exth_number, exth_record, hash32, palm_database};

use crate::Azw3Error;
use crate::chunk::{ResourceRef, kf8_markup};
use crate::epub::{EpubBook, EpubTocEntry, read_epub, split_fragment};
use crate::index::{
    GuideEntry, NcxEntry, chunk_index, guide_index, ncx_index, skel_index, utf8_text,
};
use crate::palmdoc::palmdoc_compress;
use crate::revision::{AZW3_REVISION_EXTH, AZW3_WRITER_REVISION};
use crate::tbs::{TbsEntry, encode_trailing_data, trailing_byte_sequences};

const RECORD_SIZE: usize = 4096;
const NULL_INDEX: u32 = 0xffff_ffff;

/// Calibre's stand-in for a picture nothing shows (writer2/resources.py).
fn placeholder_gif() -> Vec<u8> {
    [
        &b"GIF89a"[..],
        &[
            0x01, 0x00, 0x01, 0x00, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff,
        ],
        &[
            0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x21, 0xfe, 0x20,
        ],
        b"calibre-placeholder-gif-for-azw3",
        &[0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00],
        &[0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b],
    ]
    .concat()
}

const FLIS: [u8; 36] = [
    b'F', b'L', b'I', b'S', 0, 0, 0, 8, 0, 0x41, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0, 1, 0,
    3, 0, 0, 0, 3, 0, 0, 0, 1, 0xff, 0xff, 0xff, 0xff,
];

fn fcis(text_length: u32) -> Vec<u8> {
    let mut w = ByteWriter::new();
    w.ascii("FCIS")
        .u32(0x14)
        .u32(0x10)
        .u32(0x02)
        .u32(0)
        .u32(text_length)
        .u32(0)
        .u32(0x28)
        .u32(0)
        .u32(0x28)
        .u32(8)
        .u16(1)
        .u16(1)
        .u32(0);
    w.into_vec()
}

const EOF: [u8; 4] = [0xe9, 0x8e, 0x0d, 0x0a];

/// What a picture's bytes are, read from its first bytes (Calibre asks
/// imghdr the same question).
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, 0x50, 0x4e, 0x47]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x47, 0x49, 0x46]) {
        Some("image/gif")
    } else {
        None
    }
}

/// The MOBI language field: Calibre's iana2mobi for English, the only
/// language our EPUB is written in (`/^en(-|$)/i`). Anything else is 0,
/// "neutral"; EXTH 524 still says the language.
fn language_code(lang: &str) -> u32 {
    let b = lang.as_bytes();
    let en = b.len() >= 2 && b[..2].eq_ignore_ascii_case(b"en") && (b.len() == 2 || b[2] == b'-');
    if en { 9 } else { 0 }
}

struct TextRecord<'a> {
    data: &'a [u8],
    overlap: &'a [u8],
}

/// Cut the text into 4096-byte records. A record that ends inside a UTF-8
/// character carries the rest of that character after its data, so the
/// record decodes on its own (utils.py's create_text_record).
fn text_records(text: &[u8]) -> Vec<TextRecord<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = (start + RECORD_SIZE).min(text.len());
        let mut overlap: &[u8] = &[];
        if end < text.len() {
            // Find the start of the last character in this record.
            let mut lead = end - 1;
            while lead > start && (text[lead] & 0xc0) == 0x80 {
                lead -= 1;
            }
            let b = text[lead];
            let size = if b >= 0xf0 {
                4
            } else if b >= 0xe0 {
                3
            } else if b >= 0xc0 {
                2
            } else {
                1
            };
            if lead + size > end {
                overlap = &text[end..lead + size];
            }
        }
        out.push(TextRecord {
            data: &text[start..end],
            overlap,
        });
        start += RECORD_SIZE;
    }
    out
}

struct FlatToc<'a> {
    entry: &'a EpubTocEntry,
    index: usize,
    depth: usize,
    parent: Option<usize>,
    children: Vec<usize>,
}

/// The TOC depth first, as Calibre's toc.iterdescendants() walks it.
fn flatten(toc: &[EpubTocEntry]) -> Vec<FlatToc<'_>> {
    fn walk<'a>(
        entries: &'a [EpubTocEntry],
        depth: usize,
        parent: Option<usize>,
        out: &mut Vec<FlatToc<'a>>,
    ) {
        for entry in entries {
            let index = out.len();
            out.push(FlatToc {
                entry,
                index,
                depth,
                parent,
                children: Vec::new(),
            });
            if let Some(p) = parent {
                out[p].children.push(index);
            }
            walk(&entry.children, depth + 1, Some(index), out);
        }
    }
    let mut out = Vec::new();
    walk(toc, 0, None, &mut out);
    out
}

fn u32_of(n: usize) -> u32 {
    u32::try_from(n).expect("a field under 4 GiB")
}

/// Build the AZW3 for an EPUB already read, with `time` (seconds since
/// 1970) as the PalmDB dates and, when the EPUB names no date, EXTH 106.
/// The book's documents are changed in place.
pub fn build_azw3_from_book(book: &mut EpubBook, time: u64) -> Result<Vec<u8>, Azw3Error> {
    // ── Resources: every picture, numbered from 1 in manifest order. ──
    let mut resources: HashMap<String, ResourceRef> = HashMap::new();
    let mut resource_bytes: Vec<Vec<u8>> = Vec::new();
    for img in &book.images {
        let mime = image_mime(&img.bytes).ok_or_else(|| {
            Azw3Error::new(format!(
                "The picture {} is not a PNG, JPEG or GIF.",
                img.href
            ))
        })?;
        resource_bytes.push(img.bytes.clone());
        resources.insert(
            img.href.clone(),
            ResourceRef {
                index: resource_bytes.len(),
                mime: mime.to_owned(),
            },
        );
    }
    let cover_offset = match &book.cover_href {
        Some(href) => Some(
            resources
                .get(href)
                .ok_or_else(|| {
                    Azw3Error::new(format!("The cover {href} is no picture of the book."))
                })?
                .index
                - 1,
        ),
        None => None,
    };

    let markup = kf8_markup(book, &resources)?;
    // A picture nothing shows becomes Calibre's placeholder; the cover
    // always counts as shown (resources.py's serialize).
    for (i, img) in book.images.iter().enumerate() {
        if !markup.used_images.contains(&img.href) && book.cover_href.as_ref() != Some(&img.href) {
            resource_bytes[i] = placeholder_gif();
        }
    }

    // ── Text records ──
    let text: Vec<u8> = markup.flows.concat();
    let mut records: Vec<Vec<u8>> = vec![Vec::new()]; // record 0 comes last
    let mut uncompressed_lengths = Vec::new();
    let mut records_size = 0usize;
    for r in text_records(&text) {
        uncompressed_lengths.push(r.data.len());
        let mut rec = palmdoc_compress(r.data);
        rec.extend_from_slice(r.overlap);
        rec.push(r.overlap.len() as u8);
        records_size += rec.len();
        records.push(rec);
    }
    let last_text_record = records.len() - 1;

    // ── FDST: where each flow starts and ends in the text. ──
    let mut fdst = ByteWriter::new();
    fdst.ascii("FDST").u32(12).u32(u32_of(markup.flows.len()));
    let mut flow_start = 0usize;
    for f in &markup.flows {
        fdst.u32(u32_of(flow_start))
            .u32(u32_of(flow_start + f.len()));
        flow_start += f.len();
    }

    // ── NCX: the TOC, sorted by (depth, offset) as main.py does. ──
    let flow0_length = markup.flows[0].len();
    let first_chunk_pos = markup.chunk_table.first().map_or(0, |c| c.insert_pos);
    let flat = flatten(&book.toc);
    struct Placed {
        flat: usize,
        pos: usize,
        fid: usize,
        offset: usize,
    }
    let mut placed: Vec<Placed> = Vec::with_capacity(flat.len());
    for (i, t) in flat.iter().enumerate() {
        let (href, frag) = split_fragment(&t.entry.href);
        let at = markup
            .aid_for(href, frag.unwrap_or(""))
            .and_then(|aid| markup.aid_offsets.get(aid));
        let (pos, fid, offset) = at.copied().unwrap_or((0, 0, first_chunk_pos));
        placed.push(Placed {
            flat: i,
            pos,
            fid,
            offset,
        });
    }
    placed.sort_by(|a, b| {
        flat[a.flat]
            .depth
            .cmp(&flat[b.flat].depth)
            .then(a.offset.cmp(&b.offset))
    });
    let mut new_index: HashMap<usize, usize> = HashMap::new();
    for (i, p) in placed.iter().enumerate() {
        new_index.insert(flat[p.flat].index, i);
    }
    let mut ncx: Vec<NcxEntry> = Vec::with_capacity(placed.len());
    for (i, p) in placed.iter().enumerate() {
        let f = &flat[p.flat];
        let next = placed
            .iter()
            .filter(|e| flat[e.flat].depth <= f.depth && e.offset > p.offset)
            .map(|e| e.offset)
            .min()
            .unwrap_or(flow0_length);
        let label = if f.entry.title.is_empty() {
            "Unknown".to_owned()
        } else {
            f.entry.title.clone()
        };
        ncx.push(NcxEntry {
            index: i,
            offset: p.offset,
            length: next - p.offset,
            label,
            depth: f.depth,
            parent: f.parent.and_then(|q| new_index.get(&q).copied()),
            first_child: f.children.first().and_then(|q| new_index.get(q).copied()),
            last_child: f.children.last().and_then(|q| new_index.get(q).copied()),
            pos_fid: (p.pos, p.fid),
        });
    }

    // Trailing bytes: the UTF-8 overlap is already on; the TBS go after it,
    // only in a book with a TOC.
    let has_tbs = !ncx.is_empty();
    if has_tbs {
        let entries: Vec<TbsEntry> = ncx
            .iter()
            .map(|e| TbsEntry {
                index: e.index,
                start: e.offset,
                length: e.length,
                depth: e.depth,
                parent: e.parent,
            })
            .collect();
        for (i, bytes) in trailing_byte_sequences(&entries, &uncompressed_lengths)
            .into_iter()
            .enumerate()
        {
            records[i + 1].extend(encode_trailing_data(&bytes));
        }
    }
    // Calibre's pad record: (size % 4) zero bytes, from the text records'
    // size before their TBS, exactly as main.py computes it.
    let mut first_non_text = last_text_record + 1;
    if !records_size.is_multiple_of(4) {
        records.push(vec![0; records_size % 4]);
        first_non_text += 1;
    }

    // ── Guide: where reading starts. ──
    let mut start_offset = None;
    let mut guide: Vec<GuideEntry> = Vec::new();
    for mark in &book.landmarks {
        if mark.kind != "bodymatter" {
            continue;
        }
        let (href, frag) = split_fragment(&mark.href);
        let Some(&(pos, fid, offset)) = markup
            .aid_for(href, frag.unwrap_or(""))
            .and_then(|aid| markup.aid_offsets.get(aid))
        else {
            continue;
        };
        start_offset = Some(offset);
        guide.push(GuideEntry {
            kind: "text".to_owned(),
            title: if mark.title.is_empty() {
                "Begin Reading".to_owned()
            } else {
                mark.title.clone()
            },
            pos_fid: (pos, fid),
        });
    }
    guide.sort_by(|a, b| a.kind.encode_utf16().cmp(b.kind.encode_utf16()));

    // ── Indices, in Calibre's order. ──
    let chunk_at = records.len();
    records.extend(chunk_index(&markup.chunk_table)?);
    let skel_at = records.len();
    records.extend(skel_index(&markup.skel_table)?);
    let mut guide_at = NULL_INDEX;
    if !guide.is_empty() {
        guide_at = u32_of(records.len());
        records.extend(guide_index(&guide)?);
    }
    let mut ncx_at = NULL_INDEX;
    if !ncx.is_empty() {
        ncx_at = u32_of(records.len());
        records.extend(ncx_index(&ncx)?);
    }

    // ── Resources, FDST, FLIS, FCIS, EOF. ──
    let resource_count = resource_bytes.len();
    let first_resource = if resource_count > 0 {
        u32_of(records.len())
    } else {
        NULL_INDEX
    };
    records.extend(resource_bytes);
    let fdst_at = records.len();
    records.push(fdst.into_vec());
    let flis_at = records.len();
    records.push(FLIS.to_vec());
    let fcis_at = records.len();
    records.push(fcis(u32_of(text.len())));
    records.push(EOF.to_vec());

    // ── EXTH ──
    let id = book
        .identifier
        .as_deref()
        .unwrap_or("")
        .rsplit(':')
        .next()
        .filter(|s| !s.is_empty())
        .map_or_else(|| hash32(&book.title).to_string(), str::to_owned);
    let date = book
        .modified
        .clone()
        .unwrap_or_else(|| clock::iso_seconds(time));
    let mut exth: Vec<Vec<u8>> = Vec::new();
    for author in &book.authors {
        exth.push(exth_record(100, &utf8_text(author)));
    }
    exth.push(exth_record(503, &utf8_text(&book.title)));
    exth.push(exth_record(524, &utf8_text(&book.language)));
    exth.push(exth_record(113, id.as_bytes())); // ASIN: the book's own id, as Calibre writes its uuid
    exth.push(exth_record(501, b"EBOK"));
    exth.push(exth_record(106, date.as_bytes()));
    // Calibre's be_kindlegen2: kindlegen 2.9's creator fields, the Linux
    // build (201), so the file is the same whatever system wrote it.
    exth.push(exth_number(204, 201));
    exth.push(exth_number(205, 2));
    exth.push(exth_number(206, 9));
    exth.push(exth_number(207, 0));
    exth.push(exth_record(535, b"0730-890adc2"));
    if let Some(cover) = cover_offset {
        exth.push(exth_number(201, u32_of(cover)));
        exth.push(exth_number(203, 0));
    }
    if let Some(start) = start_offset {
        exth.push(exth_number(116, u32_of(start)));
    }
    exth.push(exth_number(125, u32_of(resource_count)));
    if resource_count > 0 {
        exth.push(exth_number(131, 0));
    }
    exth.push(exth_record(528, b"true"));
    // Which writer made this file (revision.rs): the staleness rule rebuilds
    // an .azw3 from an older one. No reader acts on the record.
    exth.push(exth_number(AZW3_REVISION_EXTH, AZW3_WRITER_REVISION));
    let exth_body = exth.concat();
    let exth_pad = 4 - (exth_body.len() % 4); // always at least one byte
    let mut exth_block = ByteWriter::new();
    exth_block
        .ascii("EXTH")
        .u32(u32_of(exth_body.len() + 12))
        .u32(u32_of(exth.len()))
        .bytes(&exth_body)
        .zeros(exth_pad);
    let exth_block = exth_block.into_vec();

    // ── Record 0 ──
    let full_title = utf8_text(&book.title);
    let uid = hash32(&format!(
        "{} {} {}",
        book.identifier.as_deref().unwrap_or(""),
        book.title,
        text.len()
    ));
    let mut r0 = ByteWriter::new();
    r0.u16(2) // PalmDOC compression
        .zeros(2)
        .u32(u32_of(text.len()))
        .u16(
            u16::try_from(last_text_record)
                .map_err(|_| Azw3Error::new("More than 65535 text records."))?,
        )
        .u16(RECORD_SIZE as u16)
        .u16(0) // no encryption
        .u16(0)
        .ascii("MOBI")
        .u32(264) // header length
        .u32(2) // a book
        .u32(65001) // UTF-8
        .u32(uid)
        .u32(8) // file version: KF8
        .u32(NULL_INDEX) // 40 meta orth
        .u32(NULL_INDEX) // 44 meta infl
        .fill(32, 0xff) // 48..80 extra indices
        .u32(u32_of(first_non_text)) // 80
        .u32(u32_of(16 + 264 + exth_block.len())) // 84 full title offset
        .u32(u32_of(full_title.len())) // 88
        .u32(language_code(&book.language)) // 92
        .u32(0) // 96 input language
        .u32(0) // 100 output language
        .u32(8) // 104 min version
        .u32(first_resource) // 108
        .zeros(16) // 112..128 huffman
        .u32(0b101_0000) // 128 EXTH flags, as Calibre writes them
        .zeros(32) // 132
        .u32(NULL_INDEX) // 164
        .u32(NULL_INDEX) // 168 DRM offset
        .u32(0)
        .u32(0)
        .u32(0) // 172..184 DRM count, size, flags
        .zeros(8) // 184
        .u32(u32_of(fdst_at)) // 192
        .u32(u32_of(markup.flows.len())) // 196
        .u32(u32_of(fcis_at)) // 200
        .u32(1)
        .u32(u32_of(flis_at)) // 208
        .u32(1)
        .zeros(8) // 216
        .u32(NULL_INDEX) // 224 SRCS
        .u32(0)
        .fill(8, 0xff) // 232
        .u32(if has_tbs { 0b11 } else { 0b1 }) // 240 extra data flags: UTF-8 overlap, and TBS
        .u32(ncx_at) // 244
        .u32(u32_of(chunk_at)) // 248
        .u32(u32_of(skel_at)) // 252
        .u32(NULL_INDEX) // 256 DATP
        .u32(guide_at) // 260
        .fill(4, 0xff)
        .zeros(4)
        .fill(4, 0xff)
        .zeros(4) // 264..280
        .bytes(&exth_block)
        .bytes(&full_title)
        .zeros(8192);
    records[0] = r0.into_vec();

    Ok(palm_database(
        &book.title,
        uid,
        &records,
        sp_mobi::writer::clock_u32(time),
    ))
}

/// Build the AZW3 for an EPUB's bytes, at the build time
/// [`clock::now_seconds`] reads (`SOURCE_DATE_EPOCH`, else now).
pub fn build_azw3(epub: &[u8]) -> Result<Vec<u8>, Azw3Error> {
    build_azw3_at(epub, clock::now_seconds())
}

/// Build the AZW3 for an EPUB's bytes at `time` (seconds since 1970).
pub fn build_azw3_at(epub: &[u8], time: u64) -> Result<Vec<u8>, Azw3Error> {
    build_azw3_from_book(&mut read_epub(epub)?, time)
}

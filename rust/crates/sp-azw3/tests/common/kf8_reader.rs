//! A structural reader for KF8 (AZW3) files, for tests: it proves a book is
//! well formed with no Calibre on the machine, which is CI. Ports
//! tests/helpers/kf8-reader.ts.
//!
//! It reads the way Calibre's reader does, and parts are ported from it
//! (src/calibre/ebooks/mobi/reader/index.py, reader/mobi8.py and utils.py's
//! decint and get_trailing_data; calibre 9.11.0, GPL-3, Kovid Goyal), kept
//! independent of sp-azw3 so a test of the writer is not the writer
//! checking itself. It is STRICTER than Calibre's reader: where that one
//! warns and carries on, this fails, because a Kindle may not be as
//! forgiving.

// The record loops follow the reader they port, index by index.
#![allow(clippy::needless_range_loop)]

use std::collections::BTreeMap;

use super::palmdoc_decode::palmdoc_decompress;

pub const NULL_INDEX: u32 = 0xffff_ffff;

fn u16_at(b: &[u8], o: usize) -> u32 {
    u32::from(u16::from_be_bytes([b[o], b[o + 1]]))
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn ascii(b: &[u8], o: usize, n: usize) -> String {
    b.get(o..o + n)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .unwrap_or_default()
}

fn utf8(b: &[u8]) -> Result<String, String> {
    String::from_utf8(b.to_vec()).map_err(|e| format!("not UTF-8: {e}"))
}

/// A variable-width integer and how many bytes it took (utils.py decint).
pub fn decint(raw: &[u8], forward: bool) -> (u64, usize) {
    let src: Vec<u8> = if forward {
        raw.to_vec()
    } else {
        raw.iter().rev().copied().collect()
    };
    let mut bytes = Vec::new();
    for b in src {
        bytes.push(b & 0x7f);
        if b & 0x80 != 0 {
            break;
        }
    }
    if !forward {
        bytes.reverse();
    }
    let val = bytes.iter().fold(0u64, |v, &b| v * 128 + u64::from(b));
    (val, bytes.len())
}

/// The PalmDB's records, each its own bytes, after checking the record list
/// is in order, inside the file, and covers it to the end.
pub fn pdb_records(file: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    if ascii(file, 60, 8) != "BOOKMOBI" {
        return Err(format!("not a BOOKMOBI database: {}", ascii(file, 60, 8)));
    }
    let count = u16_at(file, 76) as usize;
    let offsets: Vec<usize> = (0..count)
        .map(|i| u32_at(file, 78 + i * 8) as usize)
        .collect();
    let ids: Vec<u32> = (0..count)
        .map(|i| u32_at(file, 78 + i * 8 + 4) & 0xff_ffff)
        .collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() != ids.len() {
        return Err("two records share a unique id".into());
    }
    if offsets[0] < 78 + count * 8 {
        return Err("the first record overlaps the record list".into());
    }
    for i in 1..count {
        if offsets[i] < offsets[i - 1] {
            return Err(format!("record {i} starts before record {}", i - 1));
        }
    }
    if offsets[count - 1] > file.len() {
        return Err("the last record starts past the end of the file".into());
    }
    Ok(offsets
        .iter()
        .enumerate()
        .map(|(i, &o)| file[o..offsets.get(i + 1).copied().unwrap_or(file.len())].to_vec())
        .collect())
}

/// One index: name → tag number → values, in entry order, and its strings.
#[derive(Debug, Clone)]
pub struct IndexTable {
    pub entries: Vec<(String, BTreeMap<u8, Vec<u64>>)>,
    pub cncx: BTreeMap<u64, String>,
    pub record_count: usize,
    pub cncx_count: usize,
}

impl IndexTable {
    pub fn get(&self, name: &str) -> Option<&BTreeMap<u8, Vec<u64>>> {
        self.entries.iter().find(|(n, _)| n == name).map(|(_, t)| t)
    }
}

#[derive(Debug, Clone, Copy)]
struct TagX {
    tag: u8,
    values_per_entry: u8,
    mask: u8,
    eof: u8,
}

fn read_cncx(records: &[Vec<u8>]) -> Result<BTreeMap<u64, String>, String> {
    let mut out = BTreeMap::new();
    for (r, raw) in records.iter().enumerate() {
        let mut pos = 0;
        while pos < raw.len() {
            let (length, consumed) = decint(&raw[pos..], true);
            let length = length as usize;
            if length > 0 {
                let end = (pos + consumed + length).min(raw.len());
                out.insert(
                    r as u64 * 0x10000 + pos as u64,
                    utf8(&raw[pos + consumed..end])?,
                );
            }
            pos += consumed + length;
        }
    }
    Ok(out)
}

fn tag_map(
    control_byte_count: usize,
    tagx: &[TagX],
    data: &[u8],
) -> Result<BTreeMap<u8, Vec<u64>>, String> {
    let mut control: Vec<u8> = data[..control_byte_count].to_vec();
    let mut rest: &[u8] = &data[control_byte_count..];
    struct Parsed {
        tag: u8,
        count: Option<u64>,
        bytes: Option<u64>,
        per: u8,
    }
    let mut parsed = Vec::new();
    for x in tagx {
        if x.eof == 1 {
            if !control.is_empty() {
                control.remove(0);
            }
            continue;
        }
        let mut value = control.first().copied().unwrap_or(0) & x.mask;
        if value == 0 {
            continue;
        }
        if value == x.mask {
            if x.mask.count_ones() > 1 {
                let (bytes, consumed) = decint(rest, true);
                rest = &rest[consumed..];
                parsed.push(Parsed {
                    tag: x.tag,
                    count: None,
                    bytes: Some(bytes),
                    per: x.values_per_entry,
                });
            } else {
                parsed.push(Parsed {
                    tag: x.tag,
                    count: Some(1),
                    bytes: None,
                    per: x.values_per_entry,
                });
            }
        } else {
            let mut mask = x.mask;
            while mask & 1 == 0 {
                mask >>= 1;
                value >>= 1;
            }
            parsed.push(Parsed {
                tag: x.tag,
                count: Some(u64::from(value)),
                bytes: None,
                per: x.values_per_entry,
            });
        }
    }
    let mut out = BTreeMap::new();
    for p in parsed {
        let mut values = Vec::new();
        if let Some(count) = p.count {
            for _ in 0..count * u64::from(p.per) {
                let (v, consumed) = decint(rest, true);
                rest = &rest[consumed.min(rest.len())..];
                values.push(v);
            }
        } else {
            let want = p.bytes.unwrap_or(0) as usize;
            let mut total = 0;
            while total < want {
                let (v, consumed) = decint(rest, true);
                rest = &rest[consumed.min(rest.len())..];
                total += consumed;
                values.push(v);
                if consumed == 0 {
                    break;
                }
            }
            if total != want {
                return Err(format!("a tag should take {want} bytes but took {total}"));
            }
        }
        out.insert(p.tag, values);
    }
    if rest.iter().any(|&b| b != 0) {
        return Err("index bytes left over after the last tag".into());
    }
    Ok(out)
}

/// One index: its header record at `idx`, its entry records after it, then
/// its CNCX records (reader/index.py's read_index).
pub fn read_index(records: &[Vec<u8>], idx: usize) -> Result<IndexTable, String> {
    let head = &records[idx];
    if ascii(head, 0, 4) != "INDX" {
        return Err(format!("record {idx} is not an INDX header"));
    }
    let header_length = u32_at(head, 4);
    if header_length != 192 {
        return Err(format!("INDX header length {header_length}"));
    }
    let record_count = u32_at(head, 24) as usize;
    let total_entries = u32_at(head, 36) as usize;
    let cncx_count = u32_at(head, 52) as usize;
    let tagx_at = u32_at(head, 180) as usize;
    if ascii(head, tagx_at, 4) != "TAGX" {
        return Err("no TAGX where the header says".into());
    }
    let tagx_length = u32_at(head, tagx_at + 4) as usize;
    let control_byte_count = u32_at(head, tagx_at + 8) as usize;
    let mut tagx = Vec::new();
    let mut i = tagx_at + 12;
    while i < tagx_at + tagx_length {
        tagx.push(TagX {
            tag: head[i],
            values_per_entry: head[i + 1],
            mask: head[i + 2],
            eof: head[i + 3],
        });
        i += 4;
    }
    let cncx_from = (idx + record_count + 1).min(records.len());
    let cncx_to = (cncx_from + cncx_count).min(records.len());
    let cncx = read_cncx(&records[cncx_from..cncx_to])?;

    let header_idxt = u32_at(head, 20) as usize;
    if ascii(head, header_idxt, 4) != "IDXT" {
        return Err("no IDXT in the INDX header where it says".into());
    }

    let mut entries: Vec<(String, BTreeMap<u8, Vec<u64>>)> = Vec::new();
    let mut seen = 0;
    for r in idx + 1..=idx + record_count {
        let data = &records[r];
        if ascii(data, 0, 4) != "INDX" {
            return Err(format!("record {r} is not an INDX record"));
        }
        let idxt_pos = u32_at(data, 20) as usize;
        let count = u32_at(data, 24) as usize;
        if ascii(data, idxt_pos, 4) != "IDXT" {
            return Err(format!("record {r} has no IDXT where it says"));
        }
        let mut positions: Vec<usize> = (0..count)
            .map(|j| u16_at(data, idxt_pos + 4 + 2 * j) as usize)
            .collect();
        positions.push(idxt_pos);
        for j in 0..count {
            let rec = &data[positions[j]..positions[j + 1]];
            let name_length = rec[0] as usize;
            let name = utf8(&rec[1..1 + name_length])?;
            if entries.iter().any(|(n, _)| *n == name) {
                return Err(format!("two index entries are named {name}"));
            }
            let tags = tag_map(control_byte_count, &tagx, &rec[1 + name_length..])?;
            entries.push((name, tags));
        }
        let geom_at = u16_at(head, header_idxt + 4 + 2 * (r - idx - 1)) as usize;
        let len = head[geom_at] as usize;
        let last_name = utf8(&head[geom_at + 1..geom_at + 1 + len])?;
        let last_count = u16_at(head, geom_at + 1 + len) as usize;
        if last_count != count {
            return Err(format!(
                "the INDX header says record {r} holds {last_count}, it holds {count}"
            ));
        }
        if Some(&last_name) != entries.last().map(|(n, _)| n) {
            return Err(format!(
                "the INDX header names {last_name} last in record {r}"
            ));
        }
        seen += count;
    }
    if seen != total_entries {
        return Err(format!(
            "the INDX header says {total_entries} entries, the records hold {seen}"
        ));
    }
    Ok(IndexTable {
        entries,
        cncx,
        record_count,
        cncx_count,
    })
}

#[derive(Debug, Clone)]
pub struct Header {
    pub compression: u32,
    pub text_length: u32,
    pub text_record_count: u32,
    pub record_size: u32,
    pub header_length: u32,
    pub book_type: u32,
    pub encoding: u32,
    pub version: u32,
    pub first_non_text: u32,
    pub title: String,
    pub language: u32,
    pub first_resource: u32,
    pub exth_flags: u32,
    pub fdst: u32,
    pub fdst_count: u32,
    pub fcis: u32,
    pub flis: u32,
    pub extra_data_flags: u32,
    pub ncx: u32,
    pub chunk: u32,
    pub skel: u32,
    pub datp: u32,
    pub guide: u32,
}

#[derive(Debug, Clone)]
pub struct Skel {
    pub name: String,
    pub chunk_count: u64,
    pub start: u64,
    pub length: u64,
}

#[derive(Debug, Clone)]
pub struct ChunkRow {
    pub insert_pos: u64,
    pub selector: String,
    pub file: u64,
    pub seq: u64,
    pub start: u64,
    pub length: u64,
}

#[derive(Debug, Clone)]
pub struct NcxRow {
    pub name: String,
    pub offset: u64,
    pub length: u64,
    pub label: String,
    pub depth: u64,
    pub pos: u64,
    pub fid: u64,
}

#[derive(Debug, Clone)]
pub struct GuideRow {
    pub kind: String,
    pub title: String,
    pub pos: u64,
    pub fid: u64,
}

#[derive(Debug, Clone)]
pub struct Trailing {
    pub overlap: Vec<u8>,
    pub tbs: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct Kf8Book {
    pub records: Vec<Vec<u8>>,
    pub header: Header,
    pub exth: Vec<(u32, Vec<u8>)>,
    /// The whole decompressed text: flow 0 then the CSS flows.
    pub text: Vec<u8>,
    pub flows: Vec<Vec<u8>>,
    pub skel: Vec<Skel>,
    pub chunks: Vec<ChunkRow>,
    pub ncx: Vec<NcxRow>,
    pub guide: Vec<GuideRow>,
    /// Each file rebuilt from its skeleton and fragments, as a reader does.
    pub parts: Vec<String>,
    /// Resource records, from the first resource to the last before FDST.
    pub resources: Vec<Vec<u8>>,
    /// What each text record's trailing data held.
    pub trailing: Vec<Trailing>,
}

impl Kf8Book {
    pub fn exth_value(&self, kind: u32) -> Vec<Vec<u8>> {
        self.exth
            .iter()
            .filter(|(t, _)| *t == kind)
            .map(|(_, d)| d.clone())
            .collect()
    }

    pub fn exth_string(&self, kind: u32) -> Vec<String> {
        self.exth_value(kind)
            .iter()
            .map(|d| String::from_utf8(d.clone()).expect("UTF-8"))
            .collect()
    }

    pub fn exth_number(&self, kind: u32) -> Vec<u32> {
        self.exth_value(kind).iter().map(|d| u32_at(d, 0)).collect()
    }
}

/// Split a text record into its data and its trailing entries
/// (utils.py's get_trailing_data).
fn trailing_data(record: &[u8], flags: u32) -> Result<(Vec<u8>, Trailing), String> {
    let mut rec: &[u8] = record;
    let mut tbs = None;
    let mut f = flags >> 1;
    let mut num = 0;
    while f != 0 {
        num += 1;
        if f & 1 == 1 {
            let (size, consumed) = decint(rec, false);
            let size = size as usize;
            if size < consumed || size > rec.len() {
                return Err("a trailing entry larger than its record".into());
            }
            if num == 1 {
                tbs = Some(rec[rec.len() - size..rec.len() - consumed].to_vec());
            }
            rec = &rec[..rec.len() - size];
        }
        f >>= 1;
    }
    let mut overlap = Vec::new();
    if flags & 1 == 1 {
        let size = (rec[rec.len() - 1] & 0b11) as usize + 1;
        overlap = rec[rec.len() - size..rec.len() - 1].to_vec();
        rec = &rec[..rec.len() - size];
    }
    Ok((rec.to_vec(), Trailing { overlap, tbs }))
}

pub fn read_kf8(file: &[u8]) -> Result<Kf8Book, String> {
    let records = pdb_records(file)?;
    let r0 = &records[0];
    if ascii(r0, 16, 4) != "MOBI" {
        return Err("record 0 has no MOBI header".into());
    }
    let header_length = u32_at(r0, 20);
    let title_at = u32_at(r0, 84) as usize;
    let header = Header {
        compression: u16_at(r0, 0),
        text_length: u32_at(r0, 4),
        text_record_count: u16_at(r0, 8),
        record_size: u16_at(r0, 10),
        header_length,
        book_type: u32_at(r0, 24),
        encoding: u32_at(r0, 28),
        version: u32_at(r0, 36),
        first_non_text: u32_at(r0, 80),
        title: utf8(&r0[title_at..title_at + u32_at(r0, 88) as usize])?,
        language: u32_at(r0, 92),
        first_resource: u32_at(r0, 108),
        exth_flags: u32_at(r0, 128),
        fdst: u32_at(r0, 192),
        fdst_count: u32_at(r0, 196),
        fcis: u32_at(r0, 200),
        flis: u32_at(r0, 208),
        extra_data_flags: u32_at(r0, 240),
        ncx: u32_at(r0, 244),
        chunk: u32_at(r0, 248),
        skel: u32_at(r0, 252),
        datp: u32_at(r0, 256),
        guide: u32_at(r0, 260),
    };
    if header.version != 8 {
        return Err(format!("MOBI version {}, not 8", header.version));
    }
    if header_length != 264 {
        return Err(format!("MOBI header length {header_length}, not 264"));
    }
    if header.encoding != 65001 {
        return Err("not UTF-8".into());
    }

    // EXTH, straight after the MOBI header.
    if header.exth_flags & 0x40 == 0 {
        return Err("the header says there is no EXTH".into());
    }
    let exth_at = 16 + header_length as usize;
    if ascii(r0, exth_at, 4) != "EXTH" {
        return Err("no EXTH after the MOBI header".into());
    }
    let exth_length = u32_at(r0, exth_at + 4) as usize;
    let exth_count = u32_at(r0, exth_at + 8);
    let mut exth = Vec::new();
    let mut p = exth_at + 12;
    for _ in 0..exth_count {
        let size = u32_at(r0, p + 4) as usize;
        exth.push((u32_at(r0, p), r0[p + 8..p + size].to_vec()));
        p += size;
    }
    if p - exth_at != exth_length {
        return Err(format!(
            "EXTH says {exth_length} bytes, its records walk {}",
            p - exth_at
        ));
    }
    if title_at < p {
        return Err("the title overlaps the EXTH block".into());
    }

    // Text: decompress each record after cutting its trailing entries.
    let mut trailing = Vec::new();
    let mut text = Vec::new();
    for i in 1..=header.text_record_count as usize {
        let (data, t) = trailing_data(&records[i], header.extra_data_flags)?;
        trailing.push(t);
        let data = match header.compression {
            2 => palmdoc_decompress(&data),
            1 => data,
            c => return Err(format!("compression {c}")),
        };
        if i < header.text_record_count as usize && data.len() != header.record_size as usize {
            return Err(format!(
                "text record {i} decompresses to {} bytes, not {}",
                data.len(),
                header.record_size
            ));
        }
        text.extend(data);
    }
    if text.len() != header.text_length as usize {
        return Err(format!(
            "text is {} bytes, the header says {}",
            text.len(),
            header.text_length
        ));
    }

    // FDST: the flows.
    let fdst = &records[header.fdst as usize];
    if ascii(fdst, 0, 4) != "FDST" {
        return Err("no FDST where the header says".into());
    }
    let flow_count = u32_at(fdst, 8);
    if flow_count != header.fdst_count {
        return Err("FDST and the header disagree on the flow count".into());
    }
    let mut flows = Vec::new();
    let mut expect_start = 0usize;
    let table = u32_at(fdst, 4) as usize;
    for i in 0..flow_count as usize {
        let start = u32_at(fdst, table + i * 8) as usize;
        let end = u32_at(fdst, table + i * 8 + 4) as usize;
        if start != expect_start || end < start || end > text.len() {
            return Err(format!("flow {i} runs {start} to {end}"));
        }
        flows.push(text[start..end].to_vec());
        expect_start = end;
    }
    if expect_start != text.len() {
        return Err("the flows do not cover the text".into());
    }
    if ascii(&records[header.flis as usize], 0, 4) != "FLIS" {
        return Err("no FLIS where the header says".into());
    }
    if ascii(&records[header.fcis as usize], 0, 4) != "FCIS" {
        return Err("no FCIS where the header says".into());
    }
    let eof = records.last().expect("records");
    if eof.len() != 4 || u32_at(eof, 0) != 0xe98e_0d0a {
        return Err("no EOF record at the end".into());
    }

    // The indices.
    let skel_table = read_index(&records, header.skel as usize)?;
    let mut skel = Vec::new();
    for (name, tags) in &skel_table.entries {
        let c = &tags[&1];
        let g = &tags[&6];
        if c[0] != c[1] || g[0] != g[2] || g[1] != g[3] {
            return Err(format!("skeleton {name}'s repeated values differ"));
        }
        skel.push(Skel {
            name: name.clone(),
            chunk_count: c[0],
            start: g[0],
            length: g[1],
        });
    }
    let chunk_table = read_index(&records, header.chunk as usize)?;
    let chunks: Vec<ChunkRow> = chunk_table
        .entries
        .iter()
        .map(|(name, tags)| ChunkRow {
            insert_pos: name.parse().unwrap_or(u64::MAX),
            selector: chunk_table
                .cncx
                .get(&tags[&2][0])
                .cloned()
                .unwrap_or_default(),
            file: tags[&3][0],
            seq: tags[&4][0],
            start: tags[&6][0],
            length: tags[&6][1],
        })
        .collect();
    let mut ncx = Vec::new();
    if header.ncx != NULL_INDEX {
        let t = read_index(&records, header.ncx as usize)?;
        for (name, tags) in &t.entries {
            ncx.push(NcxRow {
                name: name.clone(),
                offset: tags[&1][0],
                length: tags[&2][0],
                label: t.cncx.get(&tags[&3][0]).cloned().unwrap_or_default(),
                depth: tags[&4][0],
                pos: tags[&6][0],
                fid: tags[&6][1],
            });
        }
    }
    let mut guide = Vec::new();
    if header.guide != NULL_INDEX {
        let t = read_index(&records, header.guide as usize)?;
        for (kind, tags) in &t.entries {
            guide.push(GuideRow {
                kind: kind.clone(),
                title: t.cncx.get(&tags[&1][0]).cloned().unwrap_or_default(),
                pos: tags[&6][0],
                fid: tags[&6][1],
            });
        }
    }

    // Rebuild each file the way reader/mobi8.py's build_parts does, but
    // refuse a fragment whose insert point lands inside a tag.
    let flow0 = &flows[0];
    let mut parts = Vec::new();
    let mut chunk_at = 0usize;
    for (file_index, s) in skel.iter().enumerate() {
        let (start, length) = (s.start as usize, s.length as usize);
        let mut skeleton = flow0[start..start + length].to_vec();
        let mut base = start + length;
        for _ in 0..s.chunk_count {
            let c = chunks.get(chunk_at).ok_or_else(|| {
                format!(
                    "skeleton {} counts more fragments than the table has",
                    s.name
                )
            })?;
            chunk_at += 1;
            if c.seq as usize != chunk_at - 1 {
                return Err(format!(
                    "fragment {} has sequence number {}",
                    chunk_at - 1,
                    c.seq
                ));
            }
            if c.file as usize != file_index {
                return Err(format!("fragment {} names file {}", c.seq, c.file));
            }
            let part = &flow0[base..base + c.length as usize];
            let insert = c.insert_pos as usize - start;
            let head = utf8(&skeleton[..insert])?;
            let tail = utf8(&skeleton[insert..])?;
            let inside = head.rfind('>').map_or(-1, |i| i as i64)
                < head.rfind('<').map_or(-1, |i| i as i64)
                || tail
                    .find('>')
                    .is_some_and(|gt| tail.find('<').is_some_and(|lt| gt < lt));
            if inside {
                return Err(format!("fragment {} would be inserted inside a tag", c.seq));
            }
            skeleton.splice(insert..insert, part.iter().copied());
            base += c.length as usize;
        }
        parts.push(utf8(&skeleton)?);
    }
    if chunk_at != chunks.len() {
        return Err(format!(
            "{} fragments belong to no skeleton",
            chunks.len() - chunk_at
        ));
    }
    let last_end = skel.last().map_or(0, |l| {
        (l.start + l.length) as usize
            + chunks
                .iter()
                .filter(|c| c.file as usize == skel.len() - 1)
                .map(|c| c.length as usize)
                .sum::<usize>()
    });
    if last_end != flow0.len() {
        return Err(format!(
            "the files end at {last_end}, flow 0 at {}",
            flow0.len()
        ));
    }

    let resources = if header.first_resource == NULL_INDEX {
        Vec::new()
    } else {
        records[header.first_resource as usize..header.fdst as usize].to_vec()
    };

    Ok(Kf8Book {
        records,
        header,
        exth,
        text,
        flows,
        skel,
        chunks,
        ncx,
        guide,
        parts,
        resources,
        trailing,
    })
}

//! The little of an embedded TrueType program that changes what pdf.js
//! extracts as text.
//!
//! pdf.js rebuilds every embedded TrueType font for the browser
//! (`Font.checkAndRepair`), and nearly all of that is about drawing. Two
//! outcomes reach getTextContent: whether the rebuild throws (the font is
//! then treated as missing), and the `name` table's records, which decide
//! whether a symbolic font with no encoding gets WinAnsi Unicode values
//! (`adjustTrueTypeToUnicode`). Ported from pdf.js `src/core/fonts.js`
//! (Mozilla Foundation, Apache License 2.0).

pub struct TtInfo {
    /// `readNameTable`'s records, Mac (1,0,0) and Windows (3,1,0x409) only.
    pub name_records: Vec<(u16, u16, u16)>,
    /// True when checkAndRepair would throw on a missing required table.
    pub would_fail: bool,
}

fn u16_at(d: &[u8], p: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(p)?, *d.get(p + 1)?]))
}

fn u32_at(d: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *d.get(p)?,
        *d.get(p + 1)?,
        *d.get(p + 2)?,
        *d.get(p + 3)?,
    ]))
}

/// Reads the table directory of a single (non-collection) sfnt.
pub fn read(data: &[u8]) -> TtInfo {
    let mut info = TtInfo {
        name_records: Vec::new(),
        would_fail: false,
    };
    let Some(num_tables) = u16_at(data, 4) else {
        info.would_fail = true;
        return info;
    };
    let is_otto = data.get(0..4) == Some(b"OTTO");
    let has = |tag: &[u8]| -> Option<(usize, usize)> {
        for i in 0..num_tables as usize {
            let e = 12 + i * 16;
            let t = data.get(e..e + 4)?;
            let length = u32_at(data, e + 12)? as usize;
            if t == tag && length != 0 {
                let offset = u32_at(data, e + 8)? as usize;
                return Some((offset, length));
            }
        }
        None
    };
    let head = has(b"head");
    let hhea = has(b"hhea");
    let maxp = has(b"maxp");
    let loca = has(b"loca");
    let cff = has(b"CFF ");
    let name = has(b"name");
    if maxp.is_none() || head.is_none() || hhea.is_none() {
        info.would_fail = true;
    }
    if !(is_otto && cff.is_some()) && cff.is_none() && loca.is_none() {
        info.would_fail = true;
    }
    if let Some((start, length)) = name {
        let end = start + length;
        if u16_at(data, start) == Some(0) && length >= 6 {
            let num_records = u16_at(data, start + 2).unwrap_or(0) as usize;
            let mut pos = start + 6;
            for _ in 0..num_records {
                if pos + 12 > end {
                    break;
                }
                let (Some(p), Some(e), Some(l)) = (
                    u16_at(data, pos),
                    u16_at(data, pos + 2),
                    u16_at(data, pos + 4),
                ) else {
                    break;
                };
                if (p == 1 && e == 0 && l == 0) || (p == 3 && e == 1 && l == 0x409) {
                    info.name_records.push((p, e, l));
                }
                pos += 12;
            }
        }
    }
    info
}

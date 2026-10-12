//! The PalmDB ("BOOKMOBI") container and the small byte tools both Kindle
//! writers share: the engine's MOBI 6 ([`crate::writer`]) and its AZW3, KF8
//! (`sp-azw3`). One copy, so the two books are laid out by the same code: a
//! 78-byte database header, a record list of offsets and ids, a 2-byte gap,
//! then the records back to back.
//!
//! Ports src/mobi/palmdb.ts.

/// A growing big-endian byte buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ByteWriter {
    buf: Vec<u8>,
}

impl ByteWriter {
    pub fn new() -> Self {
        ByteWriter::default()
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// Each character's low byte, as the TypeScript's `charCodeAt(0) & 0xff`
    /// wrote it. Only ever handed ASCII.
    pub fn ascii(&mut self, s: &str) -> &mut Self {
        self.buf.extend(s.bytes());
        self
    }

    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(b);
        self
    }

    pub fn zeros(&mut self, n: usize) -> &mut Self {
        self.fill(n, 0)
    }

    pub fn fill(&mut self, n: usize, v: u8) -> &mut Self {
        self.buf.resize(self.buf.len() + n, v);
        self
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.buf.clone()
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }
}

/// One EXTH record: type, length including this 8-byte head, data.
pub fn exth_record(kind: u32, data: &[u8]) -> Vec<u8> {
    let mut w = ByteWriter::new();
    w.u32(kind).u32(len32(data.len() + 8)).bytes(data);
    w.into_vec()
}

/// An EXTH record holding one big-endian 32-bit number.
pub fn exth_number(kind: u32, value: u32) -> Vec<u8> {
    exth_record(kind, &value.to_be_bytes())
}

/// Deterministic 32-bit hash (FNV-1a over UTF-16 code units, as the
/// TypeScript's `charCodeAt` walk) for the unique-id fields.
pub fn hash32(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for unit in s.encode_utf16() {
        h ^= u32::from(unit);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Concatenate byte slices.
pub fn concat_bytes<T: AsRef<[u8]>>(parts: &[T]) -> Vec<u8> {
    let mut out = Vec::with_capacity(parts.iter().map(|p| p.as_ref().len()).sum());
    for p in parts {
        out.extend_from_slice(p.as_ref());
    }
    out
}

/// A length the format stores in 32 bits. No book comes near 4 GiB; one that
/// did would be a bug to hear about rather than a header to wrap.
pub(crate) fn len32(n: usize) -> u32 {
    u32::try_from(n).expect("a length past 4 GiB has no MOBI spelling")
}

/// The PalmDB name: printable ASCII kept, every other UTF-16 code unit an
/// underscore, then the first 31 (`title.replace(/[^\x20-\x7e]/g, '_')`).
fn db_name(title: &str) -> String {
    title
        .encode_utf16()
        .map(|u| {
            if (0x20..=0x7e).contains(&u) {
                char::from(u8::try_from(u).unwrap_or(b'_'))
            } else {
                '_'
            }
        })
        .take(31)
        .collect()
}

/// The whole file: the PalmDB header for `title`, then `records` in order.
/// `uid` seeds the database's unique-id field; `time` (seconds since 1970)
/// is written as both the creation and the modification date.
pub fn palm_database<T: AsRef<[u8]>>(title: &str, uid: u32, records: &[T], time: u32) -> Vec<u8> {
    let header_size = 78 + records.len() * 8 + 2; // header + info list + 2-byte pad
    let mut pdb = ByteWriter::new();

    let name = db_name(title);
    pdb.ascii(&name).zeros(32 - name.len());
    pdb.u16(0); // attributes
    pdb.u16(0); // version
    pdb.u32(time).u32(time).u32(0); // created, modified, backup
    pdb.u32(0); // modification number
    pdb.u32(0); // app info
    pdb.u32(0); // sort info
    pdb.ascii("BOOK").ascii("MOBI");
    pdb.u32(uid); // unique id seed
    pdb.u32(0); // next record list
    pdb.u16(u16::try_from(records.len()).expect("a PalmDB holds at most 65535 records"));
    let mut offset = header_size;
    for (i, record) in records.iter().enumerate() {
        pdb.u32(len32(offset));
        // attributes + 3-byte unique id: the full index, not its low byte;
        // a long script tops 128 text records and single-byte ids collide.
        let i = len32(i);
        pdb.u8(0)
            .u8(((i >> 16) & 0xff) as u8)
            .u8(((i >> 8) & 0xff) as u8)
            .u8((i & 0xff) as u8);
        offset += record.as_ref().len();
    }
    pdb.zeros(2); // traditional gap before record data

    let mut file = pdb.into_vec();
    file.reserve(offset - file.len());
    for record in records {
        file.extend_from_slice(record.as_ref());
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash32_reads_utf16_units() {
        assert_eq!(hash32(""), 0x811c_9dc5);
        // "a": (0x811c9dc5 ^ 0x61) * 0x01000193
        assert_eq!(hash32("a"), 0xe40c_292c);
        // An astral character is two code units, as in JavaScript.
        assert_ne!(hash32("\u{1F600}"), hash32("\u{F600}"));
    }

    #[test]
    fn db_name_counts_code_units() {
        assert_eq!(db_name("Caf\u{e9}"), "Caf_");
        assert_eq!(db_name("\u{1F600}"), "__");
        assert_eq!(db_name(&"x".repeat(40)).len(), 31);
    }
}

//! Just enough of the SQLite file format (https://sqlite.org/fileformat2.html)
//! to read every row of one small table, WAL included, without SQLite.
//!
//! It exists for one job: the Tauri window kept the update consent in
//! WebKit's localStorage, which is an SQLite file, and the Rust app reads
//! those four values once, on its first run after the handover
//! (`super::webkit`). Read-only, never locks, never writes: the file is the
//! old window's, and it may still be open while this runs, so the newest
//! rows can sit in the `-wal` file beside it, which is read too, up to its
//! last valid commit.

use std::collections::HashMap;
use std::path::Path;

/// One column of one row.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

pub struct Database {
    main: Vec<u8>,
    page_size: usize,
    usable: usize,
    encoding: u32,
    wal: HashMap<u32, Vec<u8>>,
}

fn be16(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
        .ok_or_else(|| "truncated".to_string())
}

fn be32(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| "truncated".to_string())
}

/// A big-endian varint of one to nine bytes, and how many bytes it took.
fn varint(b: &[u8], at: usize) -> Result<(u64, usize), String> {
    let mut v: u64 = 0;
    for i in 0..9 {
        let byte = *b.get(at + i).ok_or("truncated varint")?;
        if i == 8 {
            return Ok(((v << 8) | byte as u64, 9));
        }
        v = (v << 7) | (byte & 0x7f) as u64;
        if byte & 0x80 == 0 {
            return Ok((v, i + 1));
        }
    }
    unreachable!()
}

impl Database {
    /// Open `path`, and `path-wal` beside it when there is one.
    pub fn open(path: &Path) -> Result<Self, String> {
        let main = std::fs::read(path).map_err(|e| e.to_string())?;
        let mut wal_path = path.as_os_str().to_owned();
        wal_path.push("-wal");
        let wal = std::fs::read(Path::new(&wal_path)).ok();
        Self::from_bytes(main, wal.as_deref())
    }

    pub fn from_bytes(main: Vec<u8>, wal: Option<&[u8]>) -> Result<Self, String> {
        let mut db = Database {
            main,
            page_size: 0,
            usable: 0,
            encoding: 1,
            wal: HashMap::new(),
        };
        if let Some(w) = wal {
            db.wal = read_wal(w)?;
        }
        let first = match db.wal.get(&1) {
            Some(p) => p.clone(),
            None => db.main.get(..100).ok_or("not an SQLite file")?.to_vec(),
        };
        if first.get(..16) != Some(b"SQLite format 3\0".as_slice()) {
            return Err("not an SQLite file".into());
        }
        let raw = be16(&first, 16)?;
        db.page_size = if raw == 1 { 65536 } else { raw as usize };
        if db.page_size < 512 || !db.page_size.is_power_of_two() {
            return Err("bad page size".into());
        }
        let reserved = *first.get(20).ok_or("truncated")? as usize;
        db.usable = db.page_size - reserved;
        db.encoding = be32(&first, 56)?;
        Ok(db)
    }

    fn page(&self, n: u32) -> Result<std::borrow::Cow<'_, [u8]>, String> {
        if n == 0 {
            return Err("page 0".into());
        }
        if let Some(p) = self.wal.get(&n) {
            return Ok(std::borrow::Cow::Borrowed(p));
        }
        let start = (n as usize - 1) * self.page_size;
        self.main
            .get(start..start + self.page_size)
            .map(std::borrow::Cow::Borrowed)
            .ok_or_else(|| format!("page {n} is past the end"))
    }

    fn text(&self, bytes: &[u8]) -> String {
        match self.encoding {
            2 => utf16(bytes, false),
            3 => utf16(bytes, true),
            _ => String::from_utf8_lossy(bytes).into_owned(),
        }
    }

    /// Every row of the table named `name`, in rowid order.
    pub fn rows(&self, name: &str) -> Result<Vec<Vec<Value>>, String> {
        let schema = self.table_rows(1)?;
        let root = schema
            .iter()
            .find(|r| {
                r.first() == Some(&Value::Text("table".into()))
                    && r.get(1) == Some(&Value::Text(name.into()))
            })
            .and_then(|r| match r.get(3) {
                Some(Value::Integer(n)) => u32::try_from(*n).ok(),
                _ => None,
            })
            .ok_or_else(|| format!("no table {name}"))?;
        self.table_rows(root)
    }

    fn table_rows(&self, root: u32) -> Result<Vec<Vec<Value>>, String> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        let mut seen = 0usize;
        while let Some(n) = stack.pop() {
            seen += 1;
            if seen > 100_000 {
                return Err("the table is too deep".into());
            }
            let page = self.page(n)?;
            let base = if n == 1 { 100 } else { 0 };
            let kind = *page.get(base).ok_or("truncated")?;
            let cells = be16(&page, base + 3)? as usize;
            let header = if kind == 0x05 { 12 } else { 8 };
            let pointers: Vec<usize> = (0..cells)
                .map(|i| be16(&page, base + header + 2 * i).map(|p| p as usize))
                .collect::<Result<_, _>>()?;
            match kind {
                0x0d => {
                    for p in pointers {
                        out.push(self.leaf_cell(&page, p)?);
                    }
                }
                0x05 => {
                    // Children in key order: the right-most last, so it is
                    // pushed first.
                    stack.push(be32(&page, base + 8)?);
                    for p in pointers.iter().rev() {
                        stack.push(be32(&page, *p)?);
                    }
                }
                other => return Err(format!("page {n} is not a table page ({other:#x})")),
            }
        }
        Ok(out)
    }

    fn leaf_cell(&self, page: &[u8], at: usize) -> Result<Vec<Value>, String> {
        let (size, a) = varint(page, at)?;
        let (_rowid, b) = varint(page, at + a)?;
        let start = at + a + b;
        let size = usize::try_from(size).map_err(|_| "cell too large")?;
        let u = self.usable;
        let max_local = u - 35;
        let payload = if size <= max_local {
            page.get(start..start + size)
                .ok_or("truncated cell")?
                .to_vec()
        } else {
            let min_local = ((u - 12) * 32 / 255) - 23;
            let k = min_local + ((size - min_local) % (u - 4));
            let local = if k <= max_local { k } else { min_local };
            let mut p = page
                .get(start..start + local)
                .ok_or("truncated cell")?
                .to_vec();
            let mut next = be32(page, start + local)?;
            let mut hops = 0;
            while p.len() < size {
                hops += 1;
                if next == 0 || hops > 100_000 {
                    return Err("overflow chain ends early".into());
                }
                let o = self.page(next)?;
                let want = (size - p.len()).min(u - 4);
                p.extend_from_slice(o.get(4..4 + want).ok_or("truncated overflow")?);
                next = be32(&o, 0)?;
            }
            p
        };
        self.record(&payload)
    }

    fn record(&self, p: &[u8]) -> Result<Vec<Value>, String> {
        let (header_len, mut h) = varint(p, 0)?;
        let header_len = header_len as usize;
        let mut body = header_len;
        let mut out = Vec::new();
        while h < header_len {
            let (t, n) = varint(p, h)?;
            h += n;
            let int = |len: usize, body: usize| -> Result<i64, String> {
                let s = p.get(body..body + len).ok_or("truncated value")?;
                let mut v: i64 = if s[0] & 0x80 != 0 { -1 } else { 0 };
                for byte in s {
                    v = (v << 8) | *byte as i64;
                }
                Ok(v)
            };
            let (value, len) = match t {
                0 => (Value::Null, 0),
                1..=6 => {
                    let len = [0, 1, 2, 3, 4, 6, 8][t as usize];
                    (Value::Integer(int(len, body)?), len)
                }
                7 => {
                    let s = p.get(body..body + 8).ok_or("truncated value")?;
                    let mut a = [0u8; 8];
                    a.copy_from_slice(s);
                    (Value::Real(f64::from_be_bytes(a)), 8)
                }
                8 => (Value::Integer(0), 0),
                9 => (Value::Integer(1), 0),
                10 | 11 => return Err("reserved serial type".into()),
                n if n % 2 == 0 => {
                    let len = ((n - 12) / 2) as usize;
                    let s = p.get(body..body + len).ok_or("truncated blob")?;
                    (Value::Blob(s.to_vec()), len)
                }
                n => {
                    let len = ((n - 13) / 2) as usize;
                    let s = p.get(body..body + len).ok_or("truncated text")?;
                    (Value::Text(self.text(s)), len)
                }
            };
            body += len;
            out.push(value);
        }
        Ok(out)
    }
}

/// UTF-16 bytes, little- or big-endian, lossily.
pub fn utf16(bytes: &[u8], big_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| {
            if big_endian {
                u16::from_be_bytes([c[0], c[1]])
            } else {
                u16::from_le_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// The pages a WAL file holds, as of its last valid commit: frames whose
/// salts match the header and whose running checksum holds, later frames
/// replacing earlier ones, nothing after the last commit frame.
fn read_wal(w: &[u8]) -> Result<HashMap<u32, Vec<u8>>, String> {
    let mut pages = HashMap::new();
    if w.len() < 32 {
        return Ok(pages);
    }
    let magic = be32(w, 0)?;
    if magic & !1 != 0x377f_0682 {
        return Ok(pages);
    }
    let big = magic & 1 == 1;
    let page_size = be32(w, 8)? as usize;
    if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() {
        return Ok(pages);
    }
    let salt = (be32(w, 16)?, be32(w, 20)?);
    let mut sum = checksum((0, 0), &w[..24], big);
    if sum != (be32(w, 24)?, be32(w, 28)?) {
        return Ok(pages);
    }
    let mut pending: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut at = 32;
    while at + 24 + page_size <= w.len() {
        let frame = &w[at..at + 24];
        if (be32(frame, 8)?, be32(frame, 12)?) != salt {
            break;
        }
        let data = &w[at + 24..at + 24 + page_size];
        sum = checksum(sum, &frame[..8], big);
        sum = checksum(sum, data, big);
        if sum != (be32(frame, 16)?, be32(frame, 20)?) {
            break;
        }
        pending.push((be32(frame, 0)?, data.to_vec()));
        if be32(frame, 4)? != 0 {
            for (n, d) in pending.drain(..) {
                pages.insert(n, d);
            }
        }
        at += 24 + page_size;
    }
    Ok(pages)
}

fn checksum(mut s: (u32, u32), data: &[u8], big: bool) -> (u32, u32) {
    for c in data.chunks_exact(8) {
        let word = |b: &[u8]| {
            if big {
                u32::from_be_bytes([b[0], b[1], b[2], b[3]])
            } else {
                u32::from_le_bytes([b[0], b[1], b[2], b[3]])
            }
        };
        s.0 = s.0.wrapping_add(word(&c[..4]).wrapping_add(s.1));
        s.1 = s.1.wrapping_add(word(&c[4..]).wrapping_add(s.0));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_of_every_length() {
        assert_eq!(varint(&[0x05], 0).unwrap(), (5, 1));
        assert_eq!(varint(&[0x81, 0x00], 0).unwrap(), (128, 2));
        assert_eq!(varint(&[0xff; 9], 0).unwrap(), (u64::MAX, 9));
        assert!(varint(&[0x81], 0).is_err());
    }

    #[test]
    fn junk_is_an_error_not_a_panic() {
        assert!(Database::from_bytes(b"hello".to_vec(), None).is_err());
        let mut fake = b"SQLite format 3\0".to_vec();
        fake.resize(100, 0);
        assert!(Database::from_bytes(fake, None).is_err());
    }
}

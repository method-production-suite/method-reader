//! The zip container, written the way JSZip 3.10.1 writes it
//! (lib/generate/ZipFileWorker.js) with the options src/epub/build.ts
//! passes: DOS platform, no streaming (sizes in the local header, no data
//! descriptor), no comments, a folder entry ahead of the first file under
//! each folder, folders stored and empty, and DEFLATE at level 9.
//!
//! Every header field is JSZip's (versions, flags, method, dates, CRC,
//! sizes, attributes). The deflated bytes are zlib-rs's, which descends from
//! zlib-ng and so does not pick pako's (zlib 1.2.8's) matches: a deflated
//! entry inflates to the same bytes but may be a few bytes longer or
//! shorter, so compressed sizes and the offsets after them can differ.
//! Parity is on the inflated entries (rust/PARITY.md, Decisions).
//!
//! Dates are JSZip's: the UTC fields of one instant, written as DOS time.

use std::io::Write;

use flate2::Compression;
use flate2::write::DeflateEncoder;

/// How one entry is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Stored,
    Deflated,
}

struct Entry {
    name: String,
    dir: bool,
    method: Method,
    data: Vec<u8>,
}

/// An ordered zip in the making, JSZip's way.
pub struct ZipWriter {
    entries: Vec<Entry>,
    /// Seconds since the Unix epoch, UTC.
    epoch_seconds: i64,
}

impl ZipWriter {
    pub fn new(epoch_seconds: i64) -> Self {
        ZipWriter {
            entries: Vec::new(),
            epoch_seconds,
        }
    }

    /// `zip.file(name, data, { compression })`: the folders the path implies
    /// first (outermost first), each once; a name added twice keeps its
    /// first position and takes the new data, as a JavaScript object key
    /// does.
    pub fn file(&mut self, name: &str, data: &[u8], method: Method) {
        let mut folders = Vec::new();
        let mut at = name.len();
        while let Some(slash) = name[..at].rfind('/') {
            folders.push(&name[..=slash]);
            at = slash;
        }
        for folder in folders.into_iter().rev() {
            if !self.entries.iter().any(|e| e.name == folder) {
                self.entries.push(Entry {
                    name: folder.to_owned(),
                    dir: true,
                    method: Method::Stored,
                    data: Vec::new(),
                });
            }
        }
        let entry = Entry {
            name: name.to_owned(),
            dir: false,
            method,
            data: data.to_vec(),
        };
        match self.entries.iter_mut().find(|e| e.name == name) {
            Some(e) => *e = entry,
            None => self.entries.push(entry),
        }
    }

    /// `generateAsync({ type: 'uint8array' })`.
    pub fn finish(self) -> Vec<u8> {
        let (time, date) = dos_time_date(self.epoch_seconds);
        let mut out = Vec::new();
        let mut central = Vec::new();
        for e in &self.entries {
            let compressed = match e.method {
                Method::Stored => e.data.clone(),
                Method::Deflated => deflate(&e.data),
            };
            let name = e.name.as_bytes();
            // A name whose UTF-8 is longer than its UTF-16 length gets the
            // UTF-8 flag and an Info-ZIP Unicode Path field, as JSZip
            // writes them. Every name build_epub writes is ASCII.
            let utf8_name = name.len() != e.name.chars().map(char::len_utf16).sum::<usize>();
            let mut extra = Vec::new();
            if utf8_name {
                let mut field = vec![1u8];
                field.extend_from_slice(&crc32(name).to_le_bytes());
                field.extend_from_slice(name);
                extra.extend_from_slice(b"up");
                extra.extend_from_slice(&u16_len(field.len()).to_le_bytes());
                extra.extend_from_slice(&field);
            }
            let mut header = Vec::with_capacity(26);
            header.extend_from_slice(&[0x0A, 0x00]);
            let flags: u16 = if utf8_name { 0x0800 } else { 0 };
            header.extend_from_slice(&flags.to_le_bytes());
            let magic: u16 = match e.method {
                Method::Stored => 0,
                Method::Deflated => 8,
            };
            header.extend_from_slice(&magic.to_le_bytes());
            header.extend_from_slice(&time.to_le_bytes());
            header.extend_from_slice(&date.to_le_bytes());
            header.extend_from_slice(&crc32(&e.data).to_le_bytes());
            header.extend_from_slice(&u32_len(compressed.len()).to_le_bytes());
            header.extend_from_slice(&u32_len(e.data.len()).to_le_bytes());
            header.extend_from_slice(&u16_len(name.len()).to_le_bytes());
            header.extend_from_slice(&u16_len(extra.len()).to_le_bytes());

            let offset = u32_len(out.len());
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&header);
            out.extend_from_slice(name);
            out.extend_from_slice(&extra);
            out.extend_from_slice(&compressed);

            central.extend_from_slice(b"PK\x01\x02");
            // Version made by: DOS, 2.0.
            central.extend_from_slice(&0x0014u16.to_le_bytes());
            central.extend_from_slice(&header);
            central.extend_from_slice(&0u16.to_le_bytes()); // comment length
            central.extend_from_slice(&0u16.to_le_bytes()); // disk number
            central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
            let external: u32 = if e.dir { 0x10 } else { 0 };
            central.extend_from_slice(&external.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name);
            central.extend_from_slice(&extra);
        }
        let count = u16_len(self.entries.len());
        let central_at = u32_len(out.len());
        let central_len = u32_len(central.len());
        out.extend_from_slice(&central);
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&central_len.to_le_bytes());
        out.extend_from_slice(&central_at.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}

/// `decToHex(n, 4)` keeps the low 32 bits; a book never nears them.
#[allow(clippy::cast_possible_truncation)]
fn u32_len(n: usize) -> u32 {
    n as u32
}

/// `decToHex(n, 2)` keeps the low 16 bits.
#[allow(clippy::cast_possible_truncation)]
fn u16_len(n: usize) -> u16 {
    n as u16
}

/// Raw DEFLATE at level 9, as JSZip asks pako for (`{ raw: true, level: 9 }`).
fn deflate(data: &[u8]) -> Vec<u8> {
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::new(9));
    // Writing into a Vec cannot fail.
    enc.write_all(data).expect("deflate into memory");
    enc.finish().expect("deflate into memory")
}

/// CRC-32 (IEEE), as JSZip's crc32.js computes it.
fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in (0u32..).zip(table.iter_mut()) {
        let mut c = n;
        for _ in 0..8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// Days since 1970-01-01 to (year, month 1..=12, day 1..=31), proleptic
/// Gregorian (Howard Hinnant's civil_from_days).
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The UTC fields of an instant: (year, month, day, hour, minute, second).
pub fn utc_fields(epoch_seconds: i64) -> (i64, i64, i64, i64, i64, i64) {
    let days = epoch_seconds.div_euclid(86_400);
    let secs = epoch_seconds.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    (y, mo, d, secs / 3600, secs / 60 % 60, secs % 60)
}

/// JSZip's DOS time and date, with JavaScript's int32 bit arithmetic.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn dos_time_date(epoch_seconds: i64) -> (u16, u16) {
    let (y, mo, d, h, mi, s) = utc_fields(epoch_seconds);
    let (y, mo, d, h, mi, s) = (y as i32, mo as i32, d as i32, h as i32, mi as i32, s as i32);
    let time = (((h << 6) | mi) << 5) | (s / 2);
    let date = ((((y.wrapping_sub(1980)) << 4) | mo) << 5) | d;
    (time as u16, date as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_of_the_mimetype() {
        // The oracle's book.epub records this for `application/epub+zip`.
        assert_eq!(crc32(b"application/epub+zip"), 0x2CAB_616F);
    }

    #[test]
    fn dos_dates() {
        // 2026-01-01T00:00:00Z: 0x5C21 in the oracle's headers.
        assert_eq!(dos_time_date(1_767_225_600), (0, 0x5C21));
        assert_eq!(utc_fields(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(utc_fields(951_782_400), (2000, 2, 29, 0, 0, 0));
    }

    #[test]
    fn folders_come_first_and_once() {
        let mut z = ZipWriter::new(0);
        z.file("mimetype", b"x", Method::Stored);
        z.file("A/b/c.txt", b"1", Method::Deflated);
        z.file("A/d.txt", b"2", Method::Deflated);
        let names: Vec<&str> = z.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["mimetype", "A/", "A/b/", "A/b/c.txt", "A/d.txt"]);
    }
}

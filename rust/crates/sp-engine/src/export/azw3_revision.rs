//! The read half of `src/azw3/revision.ts`: which AZW3 writer revision made
//! a file, read from its first record's EXTH, so the Kindle ladder's
//! staleness rule can tell a current `.azw3` from one an older writer made.
//! The write half (the constant and the stamp) is sp-azw3's; this reader
//! moves there, or re-exports from it, when that crate lands.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

/// `AZW3_REVISION_EXTH`: "SP", far from every number Amazon and Calibre use.
pub const AZW3_REVISION_EXTH: u32 = 0x5350;

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// `azw3Revision(file)`: the writer revision in an AZW3's bytes (its head
/// is enough), or `None` when it has none or is not a MOBI-family file.
pub fn azw3_revision(file: &[u8]) -> Option<u32> {
    if file.len() < 86 {
        return None;
    }
    let has = |at: usize, n: usize| at.checked_add(n).is_some_and(|end| end <= file.len());
    let r0 = u32_at(file, 78) as usize;
    if !has(r0, 132) {
        return None;
    }
    if &file[r0 + 16..r0 + 20] != b"MOBI" {
        return None;
    }
    if u32_at(file, r0 + 128) & 0x40 == 0 {
        return None;
    }
    let exth = (r0 + 16).checked_add(u32_at(file, r0 + 20) as usize)?;
    if !has(exth, 12) || &file[exth..exth + 4] != b"EXTH" {
        return None;
    }
    let count = u32_at(file, exth + 8);
    let mut at = exth + 12;
    let mut found = None;
    for _ in 0..count {
        if !has(at, 8) {
            return None;
        }
        let kind = u32_at(file, at);
        let size = u32_at(file, at + 4) as usize;
        if size < 8 || !has(at, size) {
            return None;
        }
        if kind == AZW3_REVISION_EXTH && size == 12 {
            found = Some(u32_at(file, at + 8));
        }
        at += size;
    }
    found
}

/// `azw3RevisionOfFile(path)`: the revision of the `.azw3` at `path`,
/// reading the PalmDB header and record 0 only.
pub fn azw3_revision_of_file(path: &str) -> Option<u32> {
    let mut f = File::open(path).ok()?;
    let mut at = [0u8; 4];
    f.seek(SeekFrom::Start(78)).ok()?;
    f.read_exact(&mut at).ok()?;
    let r0 = u32::from_be_bytes(at) as usize;
    if r0 > 1024 * 1024 {
        return None;
    }
    let mut head = vec![0u8; r0 + 32 * 1024];
    f.seek(SeekFrom::Start(0)).ok()?;
    let mut n = 0;
    while n < head.len() {
        match f.read(&mut head[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(_) => return None,
        }
    }
    azw3_revision(&head[..n])
}

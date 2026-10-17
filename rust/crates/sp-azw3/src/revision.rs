//! Which version of the AZW3 writer made a file. Every AZW3 we write carries
//! [`AZW3_WRITER_REVISION`] in an EXTH record of its own, and the Kindle
//! ladder's staleness rule counts an .azw3 as current only when it is newer
//! than its EPUB AND carries this revision or a later one. So a fixed writer
//! reaches the files already beside the books: the next export rebuilds
//! them. A file with no revision at all is Calibre's AZW3 or ours from
//! before the stamp, and is rebuilt too.
//!
//! Ports src/azw3/revision.ts.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// BUMP THIS whenever the writer's output changes in a way a device could
/// notice: what the book shows, where it opens, how it navigates, whether
/// it opens at all. Every .azw3 written by an earlier revision is then
/// rebuilt on its next export. A change that leaves the bytes as they were
/// does not bump it.
pub const AZW3_WRITER_REVISION: u32 = 1;

/// The EXTH record that holds it, as one big-endian 32-bit number. No
/// reader acts on it: Amazon's and Calibre's numbers run 1-4, 100-209, 300,
/// 401-453 and 501-544, and Calibre's reader and KindleUnpack both skip a
/// number they do not know. 0x5350 is "SP", for Screepub, the app's earlier name.
pub const AZW3_REVISION_EXTH: u32 = 0x5350;

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// The writer revision in an AZW3's bytes (only the first record is read,
/// so the head of the file is enough), or None when it has none or is not
/// a MOBI-family file at all.
pub fn azw3_revision(file: &[u8]) -> Option<u32> {
    if file.len() < 86 {
        return None;
    }
    let has = |at: usize, n: usize| at.checked_add(n).is_some_and(|end| end <= file.len());
    let r0 = u32_at(file, 78) as usize;
    if !has(r0, 132) || &file[r0 + 16..r0 + 20] != b"MOBI" || u32_at(file, r0 + 128) & 0x40 == 0 {
        return None;
    }
    let exth = r0 + 16 + u32_at(file, r0 + 20) as usize;
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

/// The writer revision of the .azw3 at `path`, or None when it has none or
/// cannot be read. Reads the PalmDB header to find record 0, then record 0
/// and no further: never the text or the pictures.
pub fn azw3_revision_of_file(path: &Path) -> Option<u32> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut at = [0u8; 4];
    f.seek(SeekFrom::Start(78)).ok()?;
    f.read_exact(&mut at).ok()?;
    // A record list long enough to push record 0 past 1 MiB is no book of
    // ours, and a garbage offset must not decide how much memory this asks
    // for. Record 0 itself fits in 32 KiB past its start.
    let r0 = u32::from_be_bytes(at) as usize;
    if r0 > 1024 * 1024 {
        return None;
    }
    f.seek(SeekFrom::Start(0)).ok()?;
    let mut head = Vec::new();
    f.take((r0 + 32 * 1024) as u64)
        .read_to_end(&mut head)
        .ok()?;
    azw3_revision(&head)
}

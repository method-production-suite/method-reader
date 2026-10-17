//! PalmDOC compression for the AZW3's text records.
//!
//! Ported from cpalmdoc_do_compress in Calibre's
//! src/calibre/ebooks/compression/palmdoc.c (calibre 9.11.0, GPL-3,
//! Copyright Kovid Goyal; this port is licensed GPL-3.0-only (LICENSE-GPL-3.0),
//! see THIRD-PARTY-NOTICES.md), by way of src/azw3/palmdoc.ts. The output
//! format is the classic one every MOBI and KF8 reader decodes: a byte
//! 0x01-0x08 is a run of that many literal bytes; 0x00 and 0x09-0x7F stand
//! for themselves; 0xC0-0xFF is a space followed by the byte XOR 0x80; and
//! 0x80-0xBF starts a two-byte back-reference of 3 to 10 bytes from at most
//! 2047 bytes back.
//!
//! What differs from the C is only HOW the back-reference is found. Calibre
//! tries each length from 10 down to 3 and, for each, scans backward for the
//! nearest copy within 2047 bytes. That is the longest length with any copy
//! in the window, taken from the nearest copy of that length, and it is
//! what this finds too, through a hash chain over the window.

const WINDOW: usize = 2047;
const MAX_LEN: usize = 10;
const MIN_LEN: usize = 3;
const HASH_BITS: u32 = 12;

fn hash3(d: &[u8], p: usize) -> usize {
    ((usize::from(d[p]) << 7) ^ (usize::from(d[p + 1]) << 4) ^ usize::from(d[p + 2]))
        & ((1 << HASH_BITS) - 1)
}

const fn stands_for_itself(c: u8) -> bool {
    c == 0 || (c > 8 && c < 0x80)
}

/// Compress one text record (Calibre compresses each record on its own).
pub fn palmdoc_compress(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut out = Vec::with_capacity(n + n / 4 + 16);
    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; n.max(1)];
    let mut inserted = 0usize;

    let mut i = 0usize;
    while i < n {
        let c = data[i];
        // Back-references, under the C's own guard: not in the first 11
        // bytes, and not within 10 of the end.
        if i > 10 && n - i > 10 {
            // Every position that could start a copy for this one: a copy
            // must end before i begins, so it starts at i - 3 or earlier.
            while inserted + MIN_LEN <= i {
                let h = hash3(data, inserted);
                prev[inserted] = head[h];
                head[h] = inserted;
                inserted += 1;
            }
            let mut best_len = 0;
            let mut best_dist = 0;
            let mut j = head[hash3(data, i)];
            while j != usize::MAX && i - j <= WINDOW {
                let room = MAX_LEN.min(i - j);
                let mut m = 0;
                while m < room && data[j + m] == data[i + m] {
                    m += 1;
                }
                // Strictly longer only, so a tie keeps the nearer copy, as
                // the C's backward scan does.
                if m >= MIN_LEN && m > best_len {
                    best_len = m;
                    best_dist = i - j;
                    if m == MAX_LEN {
                        break;
                    }
                }
                j = prev[j];
            }
            if best_len > 0 {
                let compound = (best_dist << 3) + best_len - MIN_LEN;
                out.push((0x80 + (compound >> 8)) as u8);
                out.push((compound & 0xff) as u8);
                i += best_len;
                continue;
            }
        }

        // A single character.
        i += 1;
        if c == 0x20 && i < n {
            let next = data[i];
            if (0x40..=0x7f).contains(&next) {
                out.push(next ^ 0x80);
                i += 1;
                continue;
            }
        }
        if stands_for_itself(c) {
            out.push(c);
        } else {
            // A run of up to 8 bytes that cannot stand for themselves.
            let at = out.len();
            out.push(0); // the count, filled in below
            out.push(c);
            let mut len = 1usize;
            let mut j = i;
            while j < n && len < 8 {
                let d = data[j];
                if stands_for_itself(d) {
                    break;
                }
                out.push(d);
                len += 1;
                j += 1;
            }
            i += len - 1;
            out[at] = len as u8;
        }
    }
    out
}

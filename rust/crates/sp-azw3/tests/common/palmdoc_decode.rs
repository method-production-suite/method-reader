//! PalmDOC decompression, for tests only: the engine writes compressed text
//! and never reads it back. A line-for-line port of cpalmdoc_decompress in
//! Calibre's src/calibre/ebooks/compression/palmdoc.c (GPL-3, Kovid Goyal),
//! by way of tests/helpers/palmdoc-decode.ts, kept independent of
//! sp-azw3's compressor so a test of one is not the other checking itself.

pub fn palmdoc_decompress(input: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let c = input[i];
        i += 1;
        if (1..=8).contains(&c) {
            let mut n = c;
            while n > 0 && i < input.len() {
                out.push(input[i]);
                i += 1;
                n -= 1;
            }
        } else if c <= 0x7f {
            out.push(c);
        } else if c >= 0xc0 {
            out.push(0x20);
            out.push(c ^ 0x80);
        } else if i < input.len() {
            let c = (usize::from(c) << 8) | usize::from(input[i]);
            i += 1;
            let di = (c & 0x3fff) >> 3;
            if di <= out.len() {
                for _ in 0..(c & 7) + 3 {
                    // di = 0 reads past the end: JavaScript's undefined, a 0
                    // once in a Uint8Array.
                    out.push(if di == 0 { 0 } else { out[out.len() - di] });
                }
            }
        }
    }
    out
}

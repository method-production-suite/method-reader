// PalmDOC decompression, for tests only: the engine writes compressed text
// and never reads it back. A line-for-line port of cpalmdoc_decompress in
// Calibre's src/calibre/ebooks/compression/palmdoc.c (GPL-3, Kovid Goyal),
// kept independent of src/azw3/palmdoc.ts so a test of one is not the other
// checking itself.
export function palmdocDecompress(input: Uint8Array): Uint8Array {
  const out: number[] = [];
  let i = 0;
  while (i < input.length) {
    let c = input[i++];
    if (c >= 1 && c <= 8) {
      while (c-- && i < input.length) out.push(input[i++]);
    } else if (c <= 0x7f) {
      out.push(c);
    } else if (c >= 0xc0) {
      out.push(0x20, c ^ 0x80);
    } else if (i < input.length) {
      c = (c << 8) | input[i++];
      const di = (c & 0x3fff) >> 3;
      if (di <= out.length) {
        for (let n = (c & 7) + 3; n > 0; n--) out.push(out[out.length - di]);
      }
    }
  }
  return new Uint8Array(out);
}

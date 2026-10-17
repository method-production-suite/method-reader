//! pdf.js's baseline/progressive JPEG decoder, ported.
//!
//! Ported from pdf.js `src/core/jpg.js` (Mozilla Foundation, Apache License
//! 2.0; itself derived from the Mozilla jpgjs project). A different JPEG
//! decoder would round its inverse DCT and colour conversion differently and
//! land a level or two off on many pixels; this one reproduces pdf.js's
//! integer IDCT, its Int16 and Uint8Clamped storage, and its error recovery,
//! so the pixels come out exactly as the TypeScript engine saw them.

// Index loops are kept where they mirror the JavaScript line for line.
#![allow(clippy::needless_range_loop)]

use crate::images::{clamp_u8, cmyk_to_rgb_buffer};

#[derive(Debug)]
pub struct JpegError;

const DCT_ZIG_ZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];
const DCT_COS1: i64 = 4017;
const DCT_SIN1: i64 = 799;
const DCT_COS3: i64 = 3406;
const DCT_SIN3: i64 = 2276;
const DCT_COS6: i64 = 1567;
const DCT_SIN6: i64 = 3784;
const DCT_SQRT2: i64 = 5793;
const DCT_SQRT1D2: i64 = 2896;

/// JavaScript `x >> n` on an integer-valued double: ToInt32, then shift.
#[inline]
fn sar(x: i64, n: u32) -> i64 {
    ((x as i32) >> n) as i64
}

#[inline]
fn i32w(x: i64) -> i64 {
    (x as i32) as i64
}

/// A Huffman tree as pdf.js builds it: nested arrays whose entries are a
/// symbol, a child array, or nothing.
#[derive(Clone, Debug)]
enum Entry {
    Empty,
    Leaf(u8),
    Child(usize),
}

#[derive(Clone, Debug, Default)]
struct Huffman {
    nodes: Vec<Vec<Entry>>,
}

impl Huffman {
    fn set(&mut self, node: usize, index: usize, e: Entry) {
        let v = &mut self.nodes[node];
        if v.len() <= index {
            v.resize(index + 1, Entry::Empty);
        }
        v[index] = e;
    }
}

fn build_huffman_table(code_lengths: &[u8; 16], values: &[u8]) -> Result<Huffman, JpegError> {
    // pdf.js keeps a stack of {children, index} objects; here a stack of
    // (node, index) pairs over an arena of child arrays. Every mutation goes
    // through a pop and a push, so the stack always holds current state, and
    // the `p` pdf.js keeps in a variable is always the top of the stack.
    let mut h = Huffman {
        nodes: vec![Vec::new()],
    };
    let mut length = 16;
    while length > 0 && code_lengths[length - 1] == 0 {
        length -= 1;
    }
    let mut code: Vec<(usize, usize)> = vec![(0, 0)];
    let mut k = 0;
    for i in 0..length {
        for _ in 0..code_lengths[i] {
            // `code.pop()` on an empty stack is undefined, and the next
            // property access throws: the whole image fails.
            let mut p = code.pop().ok_or(JpegError)?;
            let v = values.get(k).copied();
            h.set(p.0, p.1, v.map(Entry::Leaf).unwrap_or(Entry::Empty));
            while p.1 > 0 {
                p = code.pop().ok_or(JpegError)?;
            }
            p.1 += 1;
            code.push(p);
            while code.len() <= i {
                h.nodes.push(Vec::new());
                let q = (h.nodes.len() - 1, 0);
                let top = *code.last().ok_or(JpegError)?;
                code.push(q);
                h.set(top.0, top.1, Entry::Child(q.0));
            }
            k += 1;
        }
        if i + 1 < length {
            h.nodes.push(Vec::new());
            let q = (h.nodes.len() - 1, 0);
            let top = *code.last().ok_or(JpegError)?;
            code.push(q);
            h.set(top.0, top.1, Entry::Child(q.0));
        }
    }
    Ok(h)
}

#[derive(Clone, Debug, Default)]
struct Component {
    h: usize,
    v: usize,
    quantization_id: usize,
    quantization_table: Option<[u16; 64]>,
    block_data: Vec<i16>,
    blocks_per_line: usize,
    blocks_per_column: usize,
    huffman_dc: Option<usize>,
    huffman_ac: Option<usize>,
    pred: i64,
    index: i32,
}

#[derive(Clone, Debug, Default)]
struct Frame {
    progressive: bool,
    precision: u8,
    scan_lines: usize,
    samples_per_line: usize,
    components: Vec<Component>,
    component_ids: Vec<(u8, usize)>,
    max_h: usize,
    max_v: usize,
    mcus_per_line: usize,
    mcus_per_column: usize,
}

fn get_u16(data: &[u8], offset: usize) -> Result<u32, JpegError> {
    if offset + 1 >= data.len() {
        // DataView.getUint16 past the end throws a RangeError.
        return Err(JpegError);
    }
    Ok(((data[offset] as u32) << 8) | data[offset + 1] as u32)
}

fn block_offset(c: &Component, row: usize, col: usize) -> usize {
    64 * ((c.blocks_per_line + 1) * row + col)
}

struct FileMarker {
    invalid: bool,
    marker: u32,
    offset: usize,
}

fn find_next_file_marker(
    data: &[u8],
    current: usize,
    start: usize,
) -> Result<Option<FileMarker>, JpegError> {
    let max_pos = data.len() as isize - 1;
    let mut new_pos = start.min(current);
    if current as isize >= max_pos {
        return Ok(None);
    }
    let current_marker = get_u16(data, current)?;
    if (0xffc0..=0xfffe).contains(&current_marker) {
        return Ok(Some(FileMarker {
            invalid: false,
            marker: current_marker,
            offset: current,
        }));
    }
    let mut new_marker = get_u16(data, new_pos)?;
    while !(0xffc0..=0xfffe).contains(&new_marker) {
        new_pos += 1;
        if new_pos as isize >= max_pos {
            return Ok(None);
        }
        new_marker = get_u16(data, new_pos)?;
    }
    Ok(Some(FileMarker {
        invalid: true,
        marker: new_marker,
        offset: new_pos,
    }))
}

enum ScanError {
    Jpeg,
    Dnl(usize),
    Eoi,
}

impl From<JpegError> for ScanError {
    fn from(_: JpegError) -> Self {
        ScanError::Jpeg
    }
}

struct Scan<'a> {
    data: &'a [u8],
    offset: usize,
    bits_data: Option<u8>,
    bits_count: u32,
    parse_dnl: bool,
    scan_lines: usize,
    precision: u8,
    block_row: usize,
    eobrun: i64,
    spectral_start: usize,
    spectral_end: usize,
    successive: u32,
    successive_ac_state: u8,
    successive_ac_next_value: i64,
}

impl Scan<'_> {
    fn read_bit(&mut self) -> Result<i64, ScanError> {
        if self.bits_count > 0 {
            self.bits_count -= 1;
            return Ok(self
                .bits_data
                .map(|b| ((b >> self.bits_count) & 1) as i64)
                .unwrap_or(0));
        }
        self.bits_data = self.data.get(self.offset).copied();
        self.offset += 1;
        if self.bits_data == Some(0xff) {
            let next = self.data.get(self.offset).copied();
            self.offset += 1;
            if let Some(nb) = next
                && nb != 0
            {
                if nb == 0xdc && self.parse_dnl {
                    self.offset += 2;
                    let scan_lines = get_u16(self.data, self.offset)? as usize;
                    self.offset += 2;
                    if scan_lines > 0 && scan_lines != self.scan_lines {
                        return Err(ScanError::Dnl(scan_lines));
                    }
                } else if nb == 0xd9 {
                    if self.parse_dnl {
                        let maybe = self.block_row * if self.precision == 8 { 8 } else { 0 };
                        if maybe > 0 && ((self.scan_lines as f64) / (maybe as f64)).round() >= 5.0 {
                            return Err(ScanError::Dnl(maybe));
                        }
                    }
                    return Err(ScanError::Eoi);
                }
                if nb != 0xdc || !self.parse_dnl {
                    return Err(ScanError::Jpeg);
                }
                // A DNL marker with a matching line count falls through to
                // the "unexpected marker" throw in pdf.js as well.
                return Err(ScanError::Jpeg);
            }
        }
        self.bits_count = 7;
        Ok(self.bits_data.map(|b| (b >> 7) as i64).unwrap_or(0))
    }

    fn decode_huffman(&mut self, tree: &Huffman) -> Result<i64, ScanError> {
        let mut node = 0usize;
        loop {
            let bit = self.read_bit()? as usize;
            match tree.nodes[node].get(bit) {
                Some(Entry::Leaf(v)) => return Ok(*v as i64),
                Some(Entry::Child(c)) => node = *c,
                _ => return Err(ScanError::Jpeg),
            }
        }
    }

    fn receive(&mut self, mut length: i64) -> Result<i64, ScanError> {
        let mut n: i64 = 0;
        while length > 0 {
            n = i32w((n << 1) | self.read_bit()?);
            length -= 1;
        }
        Ok(n)
    }

    fn receive_and_extend(&mut self, length: i64) -> Result<i64, ScanError> {
        if length == 1 {
            return Ok(if self.read_bit()? == 1 { 1 } else { -1 });
        }
        let n = self.receive(length)?;
        if n >= i32w(1i64 << ((length - 1) & 31)) {
            return Ok(n);
        }
        Ok(n + i32w(-1i64 << (length & 31)) + 1)
    }
}

#[derive(Clone, Copy)]
enum DecodeFn {
    Baseline,
    DcFirst,
    DcSuccessive,
    AcFirst,
    AcSuccessive,
}

fn decode_block_fn(
    s: &mut Scan<'_>,
    f: DecodeFn,
    c: &mut Component,
    dc: &[Huffman],
    ac: &[Huffman],
    off: usize,
) -> Result<(), ScanError> {
    let dc_t = c.huffman_dc.and_then(|i| dc.get(i));
    let ac_t = c.huffman_ac.and_then(|i| ac.get(i));
    let store = |c: &mut Component, idx: usize, v: i64| {
        if let Some(x) = c.block_data.get_mut(idx) {
            *x = v as i16;
        }
    };
    let load =
        |c: &Component, idx: usize| -> i64 { c.block_data.get(idx).copied().unwrap_or(0) as i64 };
    match f {
        DecodeFn::Baseline => {
            let t = s.decode_huffman(dc_t.ok_or(ScanError::Jpeg)?)?;
            let diff = if t == 0 { 0 } else { s.receive_and_extend(t)? };
            c.pred += diff;
            store(c, off, c.pred);
            let mut k = 1usize;
            while k < 64 {
                let rs = s.decode_huffman(ac_t.ok_or(ScanError::Jpeg)?)?;
                let ss = rs & 15;
                let r = rs >> 4;
                if ss == 0 {
                    if r < 15 {
                        break;
                    }
                    k += 16;
                    continue;
                }
                k += r as usize;
                let Some(&z) = DCT_ZIG_ZAG.get(k) else {
                    // dctZigZag[k] past the end is undefined: the store goes
                    // to blockData[NaN], which is no store at all.
                    let _ = s.receive_and_extend(ss)?;
                    k += 1;
                    continue;
                };
                let v = s.receive_and_extend(ss)?;
                store(c, off + z, v);
                k += 1;
            }
        }
        DecodeFn::DcFirst => {
            let t = s.decode_huffman(dc_t.ok_or(ScanError::Jpeg)?)?;
            let diff = if t == 0 {
                0
            } else {
                i32w(s.receive_and_extend(t)? << s.successive)
            };
            c.pred += diff;
            store(c, off, c.pred);
        }
        DecodeFn::DcSuccessive => {
            let b = s.read_bit()?;
            let v = i32w(load(c, off) | i32w(b << s.successive));
            store(c, off, v);
        }
        DecodeFn::AcFirst => {
            if s.eobrun > 0 {
                s.eobrun -= 1;
                return Ok(());
            }
            let mut k = s.spectral_start;
            let e = s.spectral_end;
            while k <= e {
                let rs = s.decode_huffman(ac_t.ok_or(ScanError::Jpeg)?)?;
                let ss = rs & 15;
                let r = rs >> 4;
                if ss == 0 {
                    if r < 15 {
                        s.eobrun = s.receive(r)? + (1i64 << r) - 1;
                        break;
                    }
                    k += 16;
                    continue;
                }
                k += r as usize;
                let v = s.receive_and_extend(ss)? * (1i64 << s.successive);
                if let Some(&z) = DCT_ZIG_ZAG.get(k) {
                    store(c, off + z, v);
                }
                k += 1;
            }
        }
        DecodeFn::AcSuccessive => {
            let mut k = s.spectral_start;
            let e = s.spectral_end;
            let mut r: i64 = 0;
            while k <= e {
                // dctZigZag[k] past 63 is undefined: blockData[NaN] reads
                // undefined (falsy) and a store there is no store.
                let offset_z = DCT_ZIG_ZAG.get(k).map(|z| off + z);
                let cur = offset_z.map(|o| load(c, o)).unwrap_or(0);
                let sign: i64 = if cur < 0 { -1 } else { 1 };
                let put = |c: &mut Component, v: i64| {
                    if let Some(o) = offset_z {
                        store(c, o, v);
                    }
                };
                match s.successive_ac_state {
                    0 => {
                        let rs = s.decode_huffman(ac_t.ok_or(ScanError::Jpeg)?)?;
                        let ss = rs & 15;
                        r = rs >> 4;
                        if ss == 0 {
                            if r < 15 {
                                s.eobrun = s.receive(r)? + (1i64 << r);
                                s.successive_ac_state = 4;
                            } else {
                                r = 16;
                                s.successive_ac_state = 1;
                            }
                        } else {
                            if ss != 1 {
                                return Err(ScanError::Jpeg);
                            }
                            s.successive_ac_next_value = s.receive_and_extend(ss)?;
                            s.successive_ac_state = if r != 0 { 2 } else { 3 };
                        }
                        continue;
                    }
                    1 | 2 => {
                        if cur != 0 {
                            let b = s.read_bit()?;
                            put(c, cur + sign * i32w(b << s.successive));
                        } else {
                            r -= 1;
                            if r == 0 {
                                s.successive_ac_state =
                                    if s.successive_ac_state == 2 { 3 } else { 0 };
                            }
                        }
                    }
                    3 => {
                        if cur != 0 {
                            let b = s.read_bit()?;
                            put(c, cur + sign * i32w(b << s.successive));
                        } else {
                            put(c, i32w(s.successive_ac_next_value << s.successive));
                            s.successive_ac_state = 0;
                        }
                    }
                    4 if cur != 0 => {
                        let b = s.read_bit()?;
                        put(c, cur + sign * i32w(b << s.successive));
                    }
                    _ => {}
                }
                k += 1;
            }
            if s.successive_ac_state == 4 {
                s.eobrun -= 1;
                if s.eobrun == 0 {
                    s.successive_ac_state = 0;
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn decode_scan(
    data: &[u8],
    mut offset: usize,
    frame: &mut Frame,
    comp_idx: &[usize],
    reset_interval: Option<usize>,
    spectral_start: usize,
    spectral_end: usize,
    successive_prev: u32,
    successive: u32,
    parse_dnl: bool,
    dc: &[Huffman],
    ac: &[Huffman],
) -> Result<usize, ScanError> {
    let mcus_per_line = frame.mcus_per_line;
    let progressive = frame.progressive;
    let start_offset = offset;
    let mut s = Scan {
        data,
        offset,
        bits_data: None,
        bits_count: 0,
        parse_dnl,
        scan_lines: frame.scan_lines,
        precision: frame.precision,
        block_row: 0,
        eobrun: 0,
        spectral_start,
        spectral_end,
        successive,
        successive_ac_state: 0,
        successive_ac_next_value: 0,
    };
    let f = if progressive {
        if spectral_start == 0 {
            if successive_prev == 0 {
                DecodeFn::DcFirst
            } else {
                DecodeFn::DcSuccessive
            }
        } else if successive_prev == 0 {
            DecodeFn::AcFirst
        } else {
            DecodeFn::AcSuccessive
        }
    } else {
        DecodeFn::Baseline
    };
    let n_comp = comp_idx.len();
    let mut mcu = 0usize;
    let mcu_expected = if n_comp == 1 {
        let c = &frame.components[comp_idx[0]];
        c.blocks_per_line * c.blocks_per_column
    } else {
        mcus_per_line * frame.mcus_per_column
    };
    while mcu <= mcu_expected {
        let mcu_to_read = match reset_interval {
            Some(ri) if ri != 0 => (mcu_expected - mcu).min(ri),
            _ => mcu_expected,
        };
        if mcu_to_read > 0 {
            for &ci in comp_idx {
                frame.components[ci].pred = 0;
            }
            s.eobrun = 0;
            if n_comp == 1 {
                let ci = comp_idx[0];
                for _ in 0..mcu_to_read {
                    let c = &mut frame.components[ci];
                    let bpl = c.blocks_per_line.max(1);
                    s.block_row = mcu / bpl;
                    let block_col = mcu % bpl;
                    let off = block_offset(c, s.block_row, block_col);
                    decode_block_fn(&mut s, f, c, dc, ac, off)?;
                    mcu += 1;
                }
            } else {
                for _ in 0..mcu_to_read {
                    for &ci in comp_idx {
                        let c = &mut frame.components[ci];
                        let (h, v) = (c.h, c.v);
                        for j in 0..v {
                            for k in 0..h {
                                let mpl = mcus_per_line.max(1);
                                let mcu_row = mcu / mpl;
                                let mcu_col = mcu % mpl;
                                s.block_row = mcu_row * c.v + j;
                                let block_col = mcu_col * c.h + k;
                                let off = block_offset(c, s.block_row, block_col);
                                decode_block_fn(&mut s, f, c, dc, ac, off)?;
                            }
                        }
                    }
                    mcu += 1;
                }
            }
        }
        s.bits_count = 0;
        let fm = find_next_file_marker(data, s.offset, s.offset)?;
        let Some(fm) = fm else { break };
        if fm.invalid {
            s.offset = fm.offset;
        }
        if (0xffd0..=0xffd7).contains(&fm.marker) {
            s.offset += 2;
        } else {
            break;
        }
    }
    offset = s.offset;
    Ok(offset - start_offset)
}

fn quantize_and_inverse(c: &mut Component, off: usize, p: &mut [i16; 64]) -> Result<(), JpegError> {
    let qt = c.quantization_table.ok_or(JpegError)?;
    let bd = &mut c.block_data;
    if off + 64 > bd.len() {
        return Ok(());
    }
    let mut t: i64;
    for row in (0..64).step_by(8) {
        let mut p0 = bd[off + row] as i64;
        let mut p1 = bd[off + row + 1] as i64;
        let mut p2 = bd[off + row + 2] as i64;
        let mut p3 = bd[off + row + 3] as i64;
        let mut p4 = bd[off + row + 4] as i64;
        let mut p5 = bd[off + row + 5] as i64;
        let mut p6 = bd[off + row + 6] as i64;
        let mut p7 = bd[off + row + 7] as i64;
        p0 *= qt[row] as i64;
        if (i32w(p1) | i32w(p2) | i32w(p3) | i32w(p4) | i32w(p5) | i32w(p6) | i32w(p7)) == 0 {
            t = sar(DCT_SQRT2 * p0 + 512, 10);
            for i in 0..8 {
                p[row + i] = t as i16;
            }
            continue;
        }
        p1 *= qt[row + 1] as i64;
        p2 *= qt[row + 2] as i64;
        p3 *= qt[row + 3] as i64;
        p4 *= qt[row + 4] as i64;
        p5 *= qt[row + 5] as i64;
        p6 *= qt[row + 6] as i64;
        p7 *= qt[row + 7] as i64;
        let mut v0 = sar(DCT_SQRT2 * p0 + 128, 8);
        let mut v1 = sar(DCT_SQRT2 * p4 + 128, 8);
        let mut v2 = p2;
        let mut v3 = p6;
        let mut v4 = sar(DCT_SQRT1D2 * (p1 - p7) + 128, 8);
        let mut v7 = sar(DCT_SQRT1D2 * (p1 + p7) + 128, 8);
        let mut v5 = i32w(i32w(p3) << 4);
        let mut v6 = i32w(i32w(p5) << 4);
        v0 = sar(v0 + v1 + 1, 1);
        v1 = v0 - v1;
        t = sar(v2 * DCT_SIN6 + v3 * DCT_COS6 + 128, 8);
        v2 = sar(v2 * DCT_COS6 - v3 * DCT_SIN6 + 128, 8);
        v3 = t;
        v4 = sar(v4 + v6 + 1, 1);
        v6 = v4 - v6;
        v7 = sar(v7 + v5 + 1, 1);
        v5 = v7 - v5;
        v0 = sar(v0 + v3 + 1, 1);
        v3 = v0 - v3;
        v1 = sar(v1 + v2 + 1, 1);
        v2 = v1 - v2;
        t = sar(v4 * DCT_SIN3 + v7 * DCT_COS3 + 2048, 12);
        v4 = sar(v4 * DCT_COS3 - v7 * DCT_SIN3 + 2048, 12);
        v7 = t;
        t = sar(v5 * DCT_SIN1 + v6 * DCT_COS1 + 2048, 12);
        v5 = sar(v5 * DCT_COS1 - v6 * DCT_SIN1 + 2048, 12);
        v6 = t;
        p[row] = (v0 + v7) as i16;
        p[row + 7] = (v0 - v7) as i16;
        p[row + 1] = (v1 + v6) as i16;
        p[row + 6] = (v1 - v6) as i16;
        p[row + 2] = (v2 + v5) as i16;
        p[row + 5] = (v2 - v5) as i16;
        p[row + 3] = (v3 + v4) as i16;
        p[row + 4] = (v3 - v4) as i16;
    }
    let fin = |x: i64| -> i16 {
        if x < 16 {
            0
        } else if x >= 4080 {
            255
        } else {
            sar(x, 4) as i16
        }
    };
    for col in 0..8 {
        let p0 = p[col] as i64;
        let p1 = p[col + 8] as i64;
        let p2 = p[col + 16] as i64;
        let p3 = p[col + 24] as i64;
        let p4 = p[col + 32] as i64;
        let p5 = p[col + 40] as i64;
        let p6 = p[col + 48] as i64;
        let p7 = p[col + 56] as i64;
        if (p1 | p2 | p3 | p4 | p5 | p6 | p7) == 0 {
            let mut t = sar(DCT_SQRT2 * p0 + 8192, 14);
            if t < -2040 {
                t = 0;
            } else if t >= 2024 {
                t = 255;
            } else {
                t = sar(t + 2056, 4);
            }
            for i in 0..8 {
                bd[off + col + 8 * i] = t as i16;
            }
            continue;
        }
        let mut v0 = sar(DCT_SQRT2 * p0 + 2048, 12);
        let mut v1 = sar(DCT_SQRT2 * p4 + 2048, 12);
        let mut v2 = p2;
        let mut v3 = p6;
        let mut v4 = sar(DCT_SQRT1D2 * (p1 - p7) + 2048, 12);
        let mut v7 = sar(DCT_SQRT1D2 * (p1 + p7) + 2048, 12);
        let mut v5 = p3;
        let mut v6 = p5;
        v0 = sar(v0 + v1 + 1, 1) + 4112;
        v1 = v0 - v1;
        let mut t = sar(v2 * DCT_SIN6 + v3 * DCT_COS6 + 2048, 12);
        v2 = sar(v2 * DCT_COS6 - v3 * DCT_SIN6 + 2048, 12);
        v3 = t;
        v4 = sar(v4 + v6 + 1, 1);
        v6 = v4 - v6;
        v7 = sar(v7 + v5 + 1, 1);
        v5 = v7 - v5;
        v0 = sar(v0 + v3 + 1, 1);
        v3 = v0 - v3;
        v1 = sar(v1 + v2 + 1, 1);
        v2 = v1 - v2;
        t = sar(v4 * DCT_SIN3 + v7 * DCT_COS3 + 2048, 12);
        v4 = sar(v4 * DCT_COS3 - v7 * DCT_SIN3 + 2048, 12);
        v7 = t;
        t = sar(v5 * DCT_SIN1 + v6 * DCT_COS1 + 2048, 12);
        v5 = sar(v5 * DCT_COS1 - v6 * DCT_SIN1 + 2048, 12);
        v6 = t;
        bd[off + col] = fin(v0 + v7);
        bd[off + col + 8] = fin(v1 + v6);
        bd[off + col + 16] = fin(v2 + v5);
        bd[off + col + 24] = fin(v3 + v4);
        bd[off + col + 32] = fin(v3 - v4);
        bd[off + col + 40] = fin(v2 - v5);
        bd[off + col + 48] = fin(v1 - v6);
        bd[off + col + 56] = fin(v0 - v7);
    }
    Ok(())
}

fn prepare_components(frame: &mut Frame) -> Result<(), JpegError> {
    if frame.max_h == 0 || frame.max_v == 0 {
        return Err(JpegError);
    }
    let mcus_per_line = (frame.samples_per_line as f64 / 8.0 / frame.max_h as f64).ceil() as usize;
    let mcus_per_column = (frame.scan_lines as f64 / 8.0 / frame.max_v as f64).ceil() as usize;
    for c in frame.components.iter_mut() {
        let bpl = (((frame.samples_per_line as f64 / 8.0).ceil() * c.h as f64) / frame.max_h as f64)
            .ceil() as usize;
        let bpc = (((frame.scan_lines as f64 / 8.0).ceil() * c.v as f64) / frame.max_v as f64)
            .ceil() as usize;
        let bpl_mcu = mcus_per_line * c.h;
        let bpc_mcu = mcus_per_column * c.v;
        let size = 64usize
            .checked_mul(bpc_mcu)
            .and_then(|x| x.checked_mul(bpl_mcu + 1))
            .ok_or(JpegError)?;
        if size > 1 << 31 {
            return Err(JpegError);
        }
        c.block_data = vec![0i16; size];
        c.blocks_per_line = bpl;
        c.blocks_per_column = bpc;
    }
    frame.mcus_per_line = mcus_per_line;
    frame.mcus_per_column = mcus_per_column;
    Ok(())
}

pub struct JpegImage {
    pub width: usize,
    pub height: usize,
    adobe_transform: Option<u8>,
    color_transform: i64,
    components: Vec<OutComponent>,
}

struct OutComponent {
    index: i32,
    output: Vec<i16>,
    scale_x: f64,
    scale_y: f64,
    blocks_per_line: usize,
}

fn read_data_block(data: &[u8], offset: usize) -> Result<(usize, usize), JpegError> {
    let length = get_u16(data, offset)? as usize;
    let off = offset + 2;
    let mut end = (off + length).saturating_sub(2);
    if let Some(fm) = find_next_file_marker(data, end, off)?
        && fm.invalid
    {
        end = fm.offset;
    }
    let end = end.min(data.len()).max(off.min(data.len()));
    Ok((off.min(data.len()), end))
}

impl JpegImage {
    pub fn parse(data: &[u8], color_transform: Option<i64>) -> Result<Self, JpegError> {
        Self::parse_inner(data, color_transform.unwrap_or(-1), None, 0)
    }

    fn parse_inner(
        data: &[u8],
        color_transform: i64,
        dnl_scan_lines: Option<usize>,
        depth: u32,
    ) -> Result<Self, JpegError> {
        if depth > 4 {
            return Err(JpegError);
        }
        let max_offset = data.len() as isize - 1;
        let mut offset = 0usize;
        let mut adobe: Option<u8> = None;
        let mut frame: Option<Frame> = None;
        let mut reset_interval: Option<usize> = None;
        let mut num_sos = 0;
        let mut qtables: Vec<Option<[u16; 64]>> = vec![None; 16];
        let mut huff_ac: Vec<Huffman> = vec![Huffman::default(); 16];
        let mut huff_dc: Vec<Huffman> = vec![Huffman::default(); 16];
        let mut has_ac = [false; 16];
        let mut has_dc = [false; 16];
        let mut file_marker = get_u16(data, offset)?;
        offset += 2;
        if file_marker != 0xffd8 {
            return Err(JpegError);
        }
        file_marker = get_u16(data, offset)?;
        offset += 2;
        'marker: while file_marker != 0xffd9 {
            match file_marker {
                0xffe0..=0xffef | 0xfffe => {
                    let (s, e) = read_data_block(data, offset)?;
                    let app = &data[s..e];
                    offset = e;
                    if file_marker == 0xffee && app.len() >= 12 && &app[0..5] == b"Adobe" {
                        adobe = Some(app[11]);
                    } else if file_marker == 0xffee && app.len() >= 5 && &app[0..5] == b"Adobe" {
                        // appData[11] past the end is undefined; !!undefined is false.
                        adobe = Some(0);
                    }
                }
                0xffdb => {
                    let len = get_u16(data, offset)? as usize;
                    offset += 2;
                    let end = len + offset - 2;
                    while offset < end {
                        // data[offset++] past the end is undefined, and
                        // undefined >> 4 and undefined & 15 are both 0.
                        let spec = data.get(offset).copied().unwrap_or(0);
                        offset += 1;
                        let mut table = [0u16; 64];
                        if spec >> 4 == 0 {
                            for z in DCT_ZIG_ZAG {
                                table[z] = data.get(offset).copied().unwrap_or(0) as u16;
                                offset += 1;
                            }
                        } else if spec >> 4 == 1 {
                            for z in DCT_ZIG_ZAG {
                                table[z] = get_u16(data, offset)? as u16;
                                offset += 2;
                            }
                        } else {
                            return Err(JpegError);
                        }
                        qtables[(spec & 15) as usize] = Some(table);
                    }
                }
                0xffc0..=0xffc2 => {
                    if frame.is_some() {
                        return Err(JpegError);
                    }
                    offset += 2;
                    let mut f = Frame {
                        progressive: file_marker == 0xffc2,
                        ..Default::default()
                    };
                    f.precision = *data.get(offset).ok_or(JpegError)?;
                    offset += 1;
                    let sof_scan_lines = get_u16(data, offset)? as usize;
                    offset += 2;
                    f.scan_lines = dnl_scan_lines.unwrap_or(sof_scan_lines);
                    f.samples_per_line = get_u16(data, offset)? as usize;
                    offset += 2;
                    // The coefficient buffers are sized from the frame, not
                    // from the scan data: the same cap as the picture's own.
                    if f.samples_per_line * f.scan_lines > crate::images::MAX_PIXELS {
                        return Err(JpegError);
                    }
                    let count = *data.get(offset).ok_or(JpegError)?;
                    offset += 1;
                    let (mut max_h, mut max_v) = (0, 0);
                    for _ in 0..count {
                        let id = *data.get(offset).ok_or(JpegError)?;
                        let hv = *data.get(offset + 1).ok_or(JpegError)?;
                        let h = (hv >> 4) as usize;
                        let v = (hv & 15) as usize;
                        max_h = max_h.max(h);
                        max_v = max_v.max(v);
                        let q = *data.get(offset + 2).ok_or(JpegError)? as usize;
                        f.components.push(Component {
                            h,
                            v,
                            quantization_id: q,
                            index: -1,
                            ..Default::default()
                        });
                        // componentIds[id] = l - 1: the last component with an id wins.
                        let idx = f.components.len() - 1;
                        if let Some(e) = f.component_ids.iter_mut().find(|(k, _)| *k == id) {
                            e.1 = idx;
                        } else {
                            f.component_ids.push((id, idx));
                        }
                        offset += 3;
                    }
                    f.max_h = max_h;
                    f.max_v = max_v;
                    prepare_components(&mut f)?;
                    frame = Some(f);
                }
                0xffc4 => {
                    let len = get_u16(data, offset)? as usize;
                    offset += 2;
                    let mut i = 2;
                    while i < len {
                        let spec = *data.get(offset).ok_or(JpegError)?;
                        offset += 1;
                        let mut lengths = [0u8; 16];
                        let mut sum = 0usize;
                        for l in lengths.iter_mut() {
                            *l = data.get(offset).copied().unwrap_or(0);
                            sum += *l as usize;
                            offset += 1;
                        }
                        let mut values = vec![0u8; sum];
                        for v in values.iter_mut() {
                            *v = data.get(offset).copied().unwrap_or(0);
                            offset += 1;
                        }
                        i += 17 + sum;
                        let t = build_huffman_table(&lengths, &values)?;
                        let slot = (spec & 15) as usize;
                        if spec >> 4 == 0 {
                            huff_dc[slot] = t;
                            has_dc[slot] = true;
                        } else {
                            huff_ac[slot] = t;
                            has_ac[slot] = true;
                        }
                    }
                }
                0xffdd => {
                    offset += 2;
                    reset_interval = Some(get_u16(data, offset)? as usize);
                    offset += 2;
                }
                0xffda => {
                    num_sos += 1;
                    let parse_dnl = num_sos == 1 && dnl_scan_lines.is_none();
                    offset += 2;
                    let f = frame.as_mut().ok_or(JpegError)?;
                    let count = *data.get(offset).ok_or(JpegError)?;
                    offset += 1;
                    let mut comps = Vec::new();
                    for _ in 0..count {
                        let index = *data.get(offset).ok_or(JpegError)?;
                        offset += 1;
                        let ci = f
                            .component_ids
                            .iter()
                            .find(|(k, _)| *k == index)
                            .map(|(_, v)| *v)
                            .ok_or(JpegError)?;
                        let spec = *data.get(offset).ok_or(JpegError)?;
                        offset += 1;
                        let c = &mut f.components[ci];
                        c.index = index as i32;
                        let dci = (spec >> 4) as usize;
                        let aci = (spec & 15) as usize;
                        c.huffman_dc = if has_dc[dci] { Some(dci) } else { None };
                        c.huffman_ac = if has_ac[aci] { Some(aci) } else { None };
                        comps.push(ci);
                    }
                    let ss = *data.get(offset).ok_or(JpegError)? as usize;
                    let se = *data.get(offset + 1).ok_or(JpegError)? as usize;
                    let sa = *data.get(offset + 2).ok_or(JpegError)? as u32;
                    offset += 3;
                    match decode_scan(
                        data,
                        offset,
                        f,
                        &comps,
                        reset_interval,
                        ss,
                        se,
                        sa >> 4,
                        sa & 15,
                        parse_dnl,
                        &huff_dc,
                        &huff_ac,
                    ) {
                        Ok(n) => offset += n,
                        Err(ScanError::Dnl(lines)) => {
                            return Self::parse_inner(
                                data,
                                color_transform,
                                Some(lines),
                                depth + 1,
                            );
                        }
                        Err(ScanError::Eoi) => break 'marker,
                        Err(ScanError::Jpeg) => return Err(JpegError),
                    }
                }
                0xffdc => offset += 4,
                0xffff => {
                    if data.get(offset) != Some(&0xff) {
                        offset -= 1;
                    }
                }
                _ => {
                    let next = find_next_file_marker(data, offset - 2, offset.saturating_sub(3))?;
                    if let Some(n) = &next
                        && n.invalid
                    {
                        offset = n.offset;
                    } else if next.is_none() || offset as isize >= max_offset {
                        break 'marker;
                    } else {
                        return Err(JpegError);
                    }
                }
            }
            if (offset as isize) < max_offset {
                file_marker = get_u16(data, offset)?;
                offset += 2;
            } else {
                file_marker = 0;
            }
        }
        let mut f = frame.ok_or(JpegError)?;
        let mut comps = Vec::new();
        let (max_h, max_v) = (f.max_h, f.max_v);
        for c in f.components.iter_mut() {
            if let Some(q) = qtables.get(c.quantization_id).copied().flatten() {
                c.quantization_table = Some(q);
            }
            let mut p = [0i16; 64];
            for row in 0..c.blocks_per_column {
                for col in 0..c.blocks_per_line {
                    let off = block_offset(c, row, col);
                    quantize_and_inverse(c, off, &mut p)?;
                }
            }
            comps.push(OutComponent {
                index: c.index,
                output: std::mem::take(&mut c.block_data),
                scale_x: c.h as f64 / max_h as f64,
                scale_y: c.v as f64 / max_v as f64,
                blocks_per_line: c.blocks_per_line,
            });
        }
        Ok(JpegImage {
            width: f.samples_per_line,
            height: f.scan_lines,
            adobe_transform: adobe,
            color_transform,
            components: comps,
        })
    }

    pub fn num_components(&self) -> usize {
        self.components.len()
    }

    fn linearized(&self, width: usize, height: usize) -> Vec<u8> {
        let scale_x = self.width as f64 / width as f64;
        let scale_y = self.height as f64 / height as f64;
        let n = self.components.len();
        let mut data = vec![0u8; width * height * n];
        let mut x_offsets = vec![0u32; width];
        let mut last_scale_x: Option<f64> = None;
        for (i, c) in self.components.iter().enumerate() {
            let csx = c.scale_x * scale_x;
            let csy = c.scale_y * scale_y;
            let mut offset = i;
            let bps = (c.blocks_per_line + 1) << 3;
            if last_scale_x != Some(csx) {
                for (x, o) in x_offsets.iter_mut().enumerate() {
                    let j = ((x as f64) * csx) as i64 as u32;
                    *o = ((j & 0xfffffff8) << 3) | (j & 7);
                }
                last_scale_x = Some(csx);
            }
            for y in 0..height {
                let j = ((y as f64) * csy) as i64 as u32;
                let index = (bps as u32).wrapping_mul(j & 0xfffffff8) | ((j & 7) << 3);
                for &xo in x_offsets.iter() {
                    let v = c
                        .output
                        .get((index as usize) + xo as usize)
                        .copied()
                        .unwrap_or(0);
                    data[offset] = v.clamp(0, 255) as u8;
                    offset += n;
                }
            }
        }
        data
    }

    fn color_conversion_needed(&self) -> bool {
        if let Some(t) = self.adobe_transform {
            return t != 0;
        }
        if self.components.len() == 3 {
            if self.color_transform == 0 {
                return false;
            }
            if self.components[0].index == 0x52
                && self.components[1].index == 0x47
                && self.components[2].index == 0x42
            {
                return false;
            }
            return true;
        }
        self.color_transform == 1
    }

    /// `getData({ width, height, forceRGB })` (forceRGBA is never asked for
    /// under Method Reader's settings).
    pub fn get_data(
        &self,
        width: usize,
        height: usize,
        force_rgb: bool,
    ) -> Result<Vec<u8>, JpegError> {
        let n = self.components.len();
        if n > 4 {
            return Err(JpegError);
        }
        let mut data = self.linearized(width, height);
        if n == 1 && force_rgb {
            let mut out = Vec::with_capacity(data.len() * 3);
            for g in data {
                out.extend_from_slice(&[g, g, g]);
            }
            return Ok(out);
        } else if n == 3 && self.color_conversion_needed() {
            for px in data.chunks_exact_mut(3) {
                let y = px[0] as f64;
                let cb = px[1] as f64;
                let cr = px[2] as f64;
                px[0] = clamp_u8(y - 179.456 + 1.402 * cr);
                px[1] = clamp_u8(y + 135.459 - 0.344 * cb - 0.714 * cr);
                px[2] = clamp_u8(y - 226.816 + 1.772 * cb);
            }
            return Ok(data);
        } else if n == 4 {
            if self.color_conversion_needed() {
                for px in data.chunks_exact_mut(4) {
                    let y = px[0] as f64;
                    let cb = px[1] as f64;
                    let cr = px[2] as f64;
                    px[0] = clamp_u8(434.456 - y - 1.402 * cr);
                    px[1] = clamp_u8(119.541 - y + 0.344 * cb + 0.714 * cr);
                    px[2] = clamp_u8(481.816 - y - 1.772 * cb);
                }
                if force_rgb {
                    return Ok(cmyk_in_place_to_rgb(data));
                }
                return Ok(data);
            } else if force_rgb {
                return Ok(cmyk_in_place_to_rgb(data));
            }
        }
        Ok(data)
    }
}

/// `_convertCmykToRgb`: the conversion writes RGB over the CMYK buffer in
/// place (3 bytes out per 4 in) and keeps the first `count * 3` bytes.
fn cmyk_in_place_to_rgb(mut data: Vec<u8>) -> Vec<u8> {
    let count = data.len() / 4;
    cmyk_to_rgb_buffer(&mut data, count, 8, 0);
    data.truncate(count * 3);
    data
}

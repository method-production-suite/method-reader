//! The general-purpose stream filters, for the two places hayro-syntax
//! cannot run them for us: an inline image (which is not an object hayro
//! can fetch) and the filters in front of an image filter (where pdf.js's
//! own JPEG decoder has to take over from hayro's). Ported in behaviour from
//! pdf.js `src/core/{flate,predictor,lzw,ascii_hex,ascii_85,run_length}
//! _stream.js` (Mozilla Foundation, Apache License 2.0); inflate itself is
//! miniz_oxide.

use crate::images::ImageError;
use crate::obj::{Dict, Obj, XRef};

fn params_for(xref: &XRef<'_>, dict: &Dict, i: usize, single: bool) -> Option<std::rc::Rc<Dict>> {
    match xref.get2(dict, "DP", "DecodeParms") {
        Obj::Dict(d) if single => Some(d),
        Obj::Array(a) => a
            .get(i)
            .map(|o| xref.resolve(o))
            .and_then(|o| o.dict().cloned()),
        _ => None,
    }
}

/// Runs `names` over `data`, in order.
pub fn decode_chain(
    xref: &XRef<'_>,
    mut data: Vec<u8>,
    dict: &Dict,
    names: &[String],
    single: bool,
) -> Result<Vec<u8>, ImageError> {
    for (i, name) in names.iter().enumerate() {
        let params = params_for(xref, dict, i, single);
        data = match name.as_str() {
            "FlateDecode" | "Fl" => predict(xref, inflate(&data)?, params.as_deref())?,
            "LZWDecode" | "LZW" => {
                let early = params
                    .as_ref()
                    .map(|p| xref.get(p, "EarlyChange").num().unwrap_or(1.0))
                    .unwrap_or(1.0);
                predict(xref, lzw(&data, early != 0.0), params.as_deref())?
            }
            "ASCIIHexDecode" | "AHx" => ascii_hex(&data),
            "ASCII85Decode" | "A85" => ascii85(&data),
            "RunLengthDecode" | "RL" => run_length(&data),
            // An unsupported filter is passed through by pdf.js with a
            // warning.
            _ => data,
        };
    }
    Ok(data)
}

fn inflate(data: &[u8]) -> Result<Vec<u8>, ImageError> {
    match miniz_oxide::inflate::decompress_to_vec_zlib(data) {
        Ok(v) => Ok(v),
        Err(e) => {
            // A stream that simply ends early still yields what it had.
            if matches!(
                e.status,
                miniz_oxide::inflate::TINFLStatus::FailedCannotMakeProgress
            ) {
                Ok(e.output)
            } else {
                Err(ImageError)
            }
        }
    }
}

fn predict(xref: &XRef<'_>, data: Vec<u8>, params: Option<&Dict>) -> Result<Vec<u8>, ImageError> {
    let Some(p) = params else { return Ok(data) };
    let predictor = xref.get(p, "Predictor").num().unwrap_or(1.0) as i64;
    if predictor <= 1 {
        return Ok(data);
    }
    if predictor != 2 && !(10..=15).contains(&predictor) {
        return Err(ImageError);
    }
    let colors = xref.get(p, "Colors").num().unwrap_or(1.0) as usize;
    let bits = xref.get(p, "BitsPerComponent").num().unwrap_or(8.0) as usize;
    let columns = xref.get(p, "Columns").num().unwrap_or(1.0) as usize;
    let pix_bytes = (colors * bits).div_ceil(8);
    let row_bytes = (columns * colors * bits).div_ceil(8);
    if row_bytes == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(data.len());
    if predictor == 2 {
        for row in data.chunks(row_bytes) {
            let mut r = row.to_vec();
            if bits == 8 {
                for i in pix_bytes..r.len() {
                    r[i] = r[i].wrapping_add(r[i - pix_bytes]);
                }
            } else if bits == 16 {
                let mut i = pix_bytes;
                while i + 1 < r.len() {
                    let prev = u16::from_be_bytes([r[i - pix_bytes], r[i + 1 - pix_bytes]]);
                    let cur = u16::from_be_bytes([r[i], r[i + 1]]).wrapping_add(prev);
                    r[i] = (cur >> 8) as u8;
                    r[i + 1] = cur as u8;
                    i += 2;
                }
            }
            out.extend_from_slice(&r);
        }
        return Ok(out);
    }
    let mut prev = vec![0u8; row_bytes];
    for chunk in data.chunks(row_bytes + 1) {
        if chunk.is_empty() {
            break;
        }
        let ty = chunk[0];
        let mut cur = vec![0u8; row_bytes];
        let raw = &chunk[1..];
        for (i, c) in cur.iter_mut().enumerate() {
            *c = raw.get(i).copied().unwrap_or(0);
        }
        match ty {
            0 => {}
            1 => {
                for i in pix_bytes..row_bytes {
                    cur[i] = cur[i].wrapping_add(cur[i - pix_bytes]);
                }
            }
            2 => {
                for i in 0..row_bytes {
                    cur[i] = cur[i].wrapping_add(prev[i]);
                }
            }
            3 => {
                for i in 0..row_bytes {
                    let left = if i >= pix_bytes {
                        cur[i - pix_bytes] as u16
                    } else {
                        0
                    };
                    cur[i] = cur[i].wrapping_add(((left + prev[i] as u16) >> 1) as u8);
                }
            }
            4 => {
                for i in 0..row_bytes {
                    let a = if i >= pix_bytes {
                        cur[i - pix_bytes] as i16
                    } else {
                        0
                    };
                    let b = prev[i] as i16;
                    let c = if i >= pix_bytes {
                        prev[i - pix_bytes] as i16
                    } else {
                        0
                    };
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    let pred = if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    };
                    cur[i] = cur[i].wrapping_add(pred as u8);
                }
            }
            _ => return Err(ImageError),
        }
        let n = raw.len().min(row_bytes);
        out.extend_from_slice(&cur[..n]);
        prev = cur;
    }
    Ok(out)
}

fn lzw(data: &[u8], early_change: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut dict: Vec<Vec<u8>> = (0..256u16).map(|i| vec![i as u8]).collect();
    dict.push(Vec::new());
    dict.push(Vec::new());
    let mut code_len = 9u32;
    let mut bitbuf: u32 = 0;
    let mut bits = 0u32;
    let mut pos = 0;
    let mut prev: Option<Vec<u8>> = None;
    loop {
        while bits < code_len {
            if pos >= data.len() {
                return out;
            }
            bitbuf = (bitbuf << 8) | data[pos] as u32;
            pos += 1;
            bits += 8;
        }
        let code = ((bitbuf >> (bits - code_len)) & ((1 << code_len) - 1)) as usize;
        bits -= code_len;
        if code == 256 {
            dict.truncate(258);
            code_len = 9;
            prev = None;
            continue;
        }
        if code == 257 {
            return out;
        }
        let entry = if code < dict.len() {
            dict[code].clone()
        } else if let Some(p) = &prev {
            let mut e = p.clone();
            e.push(p[0]);
            e
        } else {
            return out;
        };
        out.extend_from_slice(&entry);
        if let Some(p) = prev
            && dict.len() < 4096
        {
            let mut e = p;
            e.push(entry[0]);
            dict.push(e);
        }
        let n = dict.len() + usize::from(early_change);
        code_len = if n >= 2048 {
            12
        } else if n >= 1024 {
            11
        } else if n >= 512 {
            10
        } else {
            9
        };
        prev = Some(entry);
    }
}

fn ascii_hex(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut first: Option<u8> = None;
    for &c in data {
        if c == b'>' {
            break;
        }
        let d = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => continue,
        };
        match first.take() {
            None => first = Some(d),
            Some(f) => out.push((f << 4) | d),
        }
    }
    if let Some(f) = first {
        out.push(f << 4);
    }
    out
}

fn ascii85(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut group: Vec<u32> = Vec::with_capacity(5);
    for &c in data {
        if c == b'~' {
            break;
        }
        if c.is_ascii_whitespace() || c == 0 {
            continue;
        }
        if c == b'z' && group.is_empty() {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        if !(b'!'..=b'u').contains(&c) {
            continue;
        }
        group.push((c - b'!') as u32);
        if group.len() == 5 {
            let v = group
                .iter()
                .fold(0u32, |a, &d| a.wrapping_mul(85).wrapping_add(d));
            out.extend_from_slice(&v.to_be_bytes());
            group.clear();
        }
    }
    if !group.is_empty() {
        let n = group.len();
        while group.len() < 5 {
            group.push(84);
        }
        let v = group
            .iter()
            .fold(0u32, |a, &d| a.wrapping_mul(85).wrapping_add(d));
        out.extend_from_slice(&v.to_be_bytes()[..n - 1]);
    }
    out
}

fn run_length(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let n = data[i];
        i += 1;
        if n == 128 {
            break;
        }
        if n < 128 {
            let len = n as usize + 1;
            let end = (i + len).min(data.len());
            out.extend_from_slice(&data[i..end]);
            i = end;
        } else {
            let len = 257 - n as usize;
            if let Some(&b) = data.get(i) {
                out.extend(std::iter::repeat_n(b, len));
            }
            i += 1;
        }
    }
    out
}

//! Image pixels, as pdf.js hands them to the engine.
//!
//! Ported from pdf.js `src/core/image.js` (`PDFImage`), `colorspace.js`,
//! `image_resizer.js` and `jpx.js` (Mozilla Foundation, Apache License 2.0),
//! under the engine's settings: no OffscreenCanvas, no ImageDecoder, no ICC
//! (an ICCBased space falls back to its /Alternate or its component count).
//! The result is pdf.js's `{ width, height, kind, data }`: kind 1 is packed
//! 1-bit gray, 2 is RGB, 3 is RGBA, and the data is exactly the bytes pdf.js
//! would have put in the object store.
//!
//! JPEG goes through a port of pdf.js's own decoder (jpeg.rs), so its pixels
//! match. JPEG 2000 goes through hayro-jpeg2000 in place of the OpenJPEG
//! build pdf.js loads; lossless images decode identically, lossy ones can
//! differ by a level where the two decoders' floating point differs. JBIG2
//! and CCITT go through hayro-syntax.

// Index loops are kept where they mirror the JavaScript line for line.
#![allow(clippy::needless_range_loop)]

use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::obj::{Dict, Obj, Stream, StreamSource, XRef};
use crate::oplist::ImageSource;

/// The most pixels a picture (or the canvas of a picture and its masks) may
/// declare: 2^26, a 600 dpi scan of a whole US Letter page with room to
/// spare. Every buffer is sized from the declared size, not from the bytes
/// the stream holds, so without this a few hundred bytes of PDF could ask
/// for gigabytes (pdf.js has no limit of its own; JavaScript's allocator
/// throws where Rust's aborts). An over-size picture is not decoded, as an
/// undecodable one is not, and the parser carries no picture for it.
pub const MAX_PIXELS: usize = 1 << 26;

/// What pdf.js puts in its object store for an image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PdfImage {
    pub width: u32,
    pub height: u32,
    /// pdf.js `ImageKind`: 1 GRAYSCALE_1BPP, 2 RGB_24BPP, 3 RGBA_32BPP.
    pub kind: u8,
    #[serde(skip)]
    pub data: Vec<u8>,
}

/// JavaScript's ToUint8Clamp: what storing a number into a
/// Uint8ClampedArray does (NaN to 0, clamp, round half to even).
#[inline]
pub fn clamp_u8(v: f64) -> u8 {
    if v.is_nan() || v <= 0.0 {
        0
    } else if v >= 255.0 {
        255
    } else {
        v.round_ties_even() as u8
    }
}

/// JavaScript's ToUint8: what storing a number into a Uint8Array does.
#[inline]
fn to_u8(v: f64) -> u8 {
    if !v.is_finite() {
        return 0;
    }
    (v.trunc() as i64).rem_euclid(256) as u8
}

/// `DeviceCmykCS.getRgbBuffer` writing into the same buffer it reads (the
/// writes trail the reads, as in pdf.js's in-place use of it).
pub fn cmyk_to_rgb_buffer(buf: &mut [u8], count: usize, bits: u32, alpha01: usize) {
    let scale = 1.0 / (((1u64 << bits) - 1) as f64);
    let mut src = 0;
    let mut dst = 0;
    for _ in 0..count {
        let rgb = cmyk_pixel(
            buf[src] as f64 * scale,
            buf[src + 1] as f64 * scale,
            buf[src + 2] as f64 * scale,
            buf[src + 3] as f64 * scale,
        );
        buf[dst] = clamp_u8(rgb[0]);
        buf[dst + 1] = clamp_u8(rgb[1]);
        buf[dst + 2] = clamp_u8(rgb[2]);
        src += 4;
        dst += 3 + alpha01;
    }
}

#[allow(clippy::excessive_precision)]
fn cmyk_pixel(c: f64, m: f64, y: f64, k: f64) -> [f64; 3] {
    [
        255.0
            + c * (-4.387332384609988 * c
                + 54.48615194189176 * m
                + 18.82290502165302 * y
                + 212.25662451639585 * k
                + -285.2331026137004)
            + m * (1.7149763477362134 * m - 5.6096736904047315 * y + -17.873870861415444 * k
                - 5.497006427196366)
            + y * (-2.5217340131683033 * y - 21.248923337353073 * k + 17.5119270841813)
            + k * (-21.86122147463605 * k - 189.48180835922747),
        255.0
            + c * (8.841041422036149 * c
                + 60.118027045597366 * m
                + 6.871425592049007 * y
                + 31.159100130055922 * k
                + -79.2970844816548)
            + m * (-15.310361306967817 * m + 17.575251261109482 * y + 131.35250912493976 * k
                - 190.9453302588951)
            + y * (4.444339102852739 * y + 9.8632861493405 * k - 24.86741582555878)
            + k * (-20.737325471181034 * k - 187.80453709719578),
        255.0
            + c * (0.8842522430003296 * c + 8.078677503112928 * m + 30.89978309703729 * y
                - 0.23883238689178934 * k
                + -14.183576799673286)
            + m * (10.49593273432072 * m + 63.02378494754052 * y + 50.606957656360734 * k
                - 112.23884253719248)
            + y * (0.03296041114873217 * y + 115.60384449646641 * k + -193.58209356861505)
            + k * (-22.33816807309886 * k - 180.12613974708367),
    ]
}

#[derive(Debug)]
pub struct ImageError;

/// A colour space, as far as images need one.
#[derive(Clone, Debug)]
enum Cs {
    Gray,
    Rgb,
    Rgba,
    Cmyk,
    Indexed { high_val: usize, lookup: Vec<u8> },
    CalGray { yw: f64, g: f64 },
    Lab(Box<Lab>),
}

#[derive(Clone, Debug)]
struct Lab {
    xw: f64,
    yw: f64,
    zw: f64,
    amin: f64,
    amax: f64,
    bmin: f64,
    bmax: f64,
}

impl Cs {
    fn num_comps(&self) -> usize {
        match self {
            Cs::Gray | Cs::Indexed { .. } | Cs::CalGray { .. } => 1,
            Cs::Rgb | Cs::Lab(_) => 3,
            Cs::Rgba | Cs::Cmyk => 4,
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Cs::Gray => "DeviceGray",
            Cs::Rgb => "DeviceRGB",
            Cs::Rgba => "DeviceRGBA",
            Cs::Cmyk => "DeviceCMYK",
            Cs::Indexed { .. } => "Indexed",
            Cs::CalGray { .. } => "CalGray",
            Cs::Lab(_) => "Lab",
        }
    }
    fn is_passthrough(&self, bits: u32) -> bool {
        matches!(self, Cs::Rgb | Cs::Rgba) && bits == 8
    }

    /// `isDefaultDecode(decode, bpc)`.
    fn is_default_decode(&self, decode: &mut Vec<f64>, bpc: u32) -> bool {
        match self {
            Cs::Lab(_) => true,
            Cs::Indexed { .. } => {
                if is_default_decode_helper(decode, 2) {
                    return true;
                }
                if bpc < 1 {
                    return true;
                }
                decode[0] == 0.0 && decode[1] == ((1u64 << bpc) - 1) as f64
            }
            _ => is_default_decode_static(decode, self.num_comps()),
        }
    }

    /// `getRgbBuffer(src, 0, count, dest, destOffset, bits, alpha01)`, with
    /// `dest` a Uint8ClampedArray.
    fn get_rgb_buffer(
        &self,
        src: &[f64],
        count: usize,
        dest: &mut [u8],
        dest_offset: usize,
        bits: u32,
        alpha01: usize,
    ) {
        let mut q = dest_offset;
        match self {
            Cs::Gray => {
                let scale = 255.0 / (((1u64 << bits) - 1) as f64);
                for &s in src.iter().take(count) {
                    let c = clamp_u8(scale * s);
                    if q + 2 >= dest.len() {
                        break;
                    }
                    dest[q] = c;
                    dest[q + 1] = c;
                    dest[q + 2] = c;
                    q += 3 + alpha01;
                }
            }
            Cs::Rgb => {
                if bits == 8 && alpha01 == 0 {
                    for (i, s) in src.iter().take(count * 3).enumerate() {
                        if let Some(d) = dest.get_mut(dest_offset + i) {
                            *d = clamp_u8(*s);
                        }
                    }
                    return;
                }
                let scale = 255.0 / (((1u64 << bits) - 1) as f64);
                let mut j = 0;
                for _ in 0..count {
                    if q + 2 >= dest.len() || j + 2 >= src.len() {
                        break;
                    }
                    dest[q] = clamp_u8(scale * src[j]);
                    dest[q + 1] = clamp_u8(scale * src[j + 1]);
                    dest[q + 2] = clamp_u8(scale * src[j + 2]);
                    j += 3;
                    q += 3 + alpha01;
                }
            }
            Cs::Rgba => {}
            Cs::Cmyk => {
                let scale = 1.0 / (((1u64 << bits) - 1) as f64);
                let mut j = 0;
                for _ in 0..count {
                    if q + 2 >= dest.len() || j + 3 >= src.len() {
                        break;
                    }
                    let rgb = cmyk_pixel(
                        src[j] * scale,
                        src[j + 1] * scale,
                        src[j + 2] * scale,
                        src[j + 3] * scale,
                    );
                    dest[q] = clamp_u8(rgb[0]);
                    dest[q + 1] = clamp_u8(rgb[1]);
                    dest[q + 2] = clamp_u8(rgb[2]);
                    j += 4;
                    q += 3 + alpha01;
                }
            }
            Cs::Indexed { high_val, lookup } => {
                for &s in src.iter().take(count) {
                    let pos = (s.round().clamp(0.0, *high_val as f64) as usize) * 3;
                    if q + 2 >= dest.len() {
                        break;
                    }
                    dest[q] = lookup.get(pos).copied().unwrap_or(0);
                    dest[q + 1] = lookup.get(pos + 1).copied().unwrap_or(0);
                    dest[q + 2] = lookup.get(pos + 2).copied().unwrap_or(0);
                    q += 3 + alpha01;
                }
            }
            Cs::CalGray { yw, g } => {
                let scale = 1.0 / (((1u64 << bits) - 1) as f64);
                for &s in src.iter().take(count) {
                    let a = s * scale;
                    let ag = a.powf(*g);
                    let l = yw * ag;
                    let val = (295.8 * l.powf(0.3333333333333333) - 40.8).max(0.0);
                    if q + 2 >= dest.len() {
                        break;
                    }
                    let v = clamp_u8(val);
                    dest[q] = v;
                    dest[q + 1] = v;
                    dest[q + 2] = v;
                    q += 3 + alpha01;
                }
            }
            Cs::Lab(lab) => {
                let max_val = ((1u64 << bits) - 1) as f64;
                let mut j = 0;
                for _ in 0..count {
                    if q + 2 >= dest.len() || j + 2 >= src.len() {
                        break;
                    }
                    let rgb = lab_to_rgb(lab, src[j], src[j + 1], src[j + 2], Some(max_val));
                    dest[q] = clamp_u8(rgb[0]);
                    dest[q + 1] = clamp_u8(rgb[1]);
                    dest[q + 2] = clamp_u8(rgb[2]);
                    j += 3;
                    q += 3 + alpha01;
                }
            }
        }
    }
}

fn lab_to_rgb(l: &Lab, mut ls: f64, mut a: f64, mut b: f64, max_val: Option<f64>) -> [f64; 3] {
    let decode = |v: f64, high1: f64, low2: f64, high2: f64| low2 + v * (high2 - low2) / high1;
    if let Some(m) = max_val {
        ls = decode(ls, m, 0.0, 100.0);
        a = decode(a, m, l.amin, l.amax);
        b = decode(b, m, l.bmin, l.bmax);
    }
    if a > l.amax {
        a = l.amax;
    } else if a < l.amin {
        a = l.amin;
    }
    if b > l.bmax {
        b = l.bmax;
    } else if b < l.bmin {
        b = l.bmin;
    }
    let g = |x: f64| {
        if x >= 6.0 / 29.0 {
            x * x * x
        } else {
            108.0 / 841.0 * (x - 4.0 / 29.0)
        }
    };
    let m = (ls + 16.0) / 116.0;
    let ll = m + a / 500.0;
    let n = m - b / 200.0;
    let x = l.xw * g(ll);
    let y = l.yw * g(m);
    let z = l.zw * g(n);
    let (r, gg, bb) = if l.zw < 1.0 {
        (
            x * 3.1339 + y * -1.617 + z * -0.4906,
            x * -0.9785 + y * 1.916 + z * 0.0333,
            x * 0.072 + y * -0.229 + z * 1.4057,
        )
    } else {
        (
            x * 3.2406 + y * -1.5372 + z * -0.4986,
            x * -0.9689 + y * 1.8758 + z * 0.0415,
            x * 0.0557 + y * -0.204 + z * 1.057,
        )
    };
    [r.sqrt() * 255.0, gg.sqrt() * 255.0, bb.sqrt() * 255.0]
}

fn is_default_decode_helper(decode: &mut Vec<f64>, expected: usize) -> bool {
    if decode.len() < expected {
        return true;
    }
    if decode.len() > expected {
        decode.truncate(expected);
    }
    false
}

fn is_default_decode_static(decode: &mut Vec<f64>, num_comps: usize) -> bool {
    if is_default_decode_helper(decode, num_comps * 2) {
        return true;
    }
    let mut i = 0;
    while i + 1 < decode.len() {
        if decode[i] != 0.0 || decode[i + 1] != 1.0 {
            return false;
        }
        i += 2;
    }
    true
}

fn num_array(xref: &XRef<'_>, o: &Obj) -> Option<Vec<f64>> {
    let a = o.array()?;
    a.iter().map(|x| xref.resolve(x).num()).collect()
}

/// `ColorSpaceUtils.parse`.
fn parse_cs(
    xref: &XRef<'_>,
    cs: &Obj,
    resources: Option<&Dict>,
    depth: u32,
) -> Result<Cs, ImageError> {
    if depth > 8 {
        return Err(ImageError);
    }
    let cs = xref.resolve(cs);
    let mut cs = cs;
    if let Obj::Name(n) = &cs {
        match &**n {
            "G" | "DeviceGray" => return Ok(Cs::Gray),
            "RGB" | "DeviceRGB" => return Ok(Cs::Rgb),
            "DeviceRGBA" => return Ok(Cs::Rgba),
            "CMYK" | "DeviceCMYK" => return Ok(Cs::Cmyk),
            "Pattern" => return Err(ImageError),
            other => {
                let mut found = None;
                if let Some(res) = resources
                    && let Some(css) = xref.get(res, "ColorSpace").dict()
                {
                    let r = xref.get(css, other);
                    if r.truthy() {
                        if let Obj::Name(_) = &r {
                            return parse_cs(xref, &r, resources, depth + 1);
                        }
                        found = Some(r);
                    }
                }
                match found {
                    Some(r) => cs = r,
                    // "Unrecognized ColorSpace": gray.
                    None => return Ok(Cs::Gray),
                }
            }
        }
    }
    let Obj::Array(a) = &cs else {
        return Err(ImageError);
    };
    let mode = a
        .first()
        .map(|o| xref.resolve(o))
        .and_then(|o| o.name().map(str::to_string));
    let arg = |i: usize| a.get(i).map(|o| xref.resolve(o)).unwrap_or(Obj::Null);
    match mode.as_deref() {
        Some("G" | "DeviceGray") => Ok(Cs::Gray),
        Some("RGB" | "DeviceRGB") => Ok(Cs::Rgb),
        Some("CMYK" | "DeviceCMYK") => Ok(Cs::Cmyk),
        Some("CalGray") => {
            let p = arg(1);
            let p = p.dict().ok_or(ImageError)?;
            let wp = xref.get_array(p, "WhitePoint").ok_or(ImageError)?;
            let wp: Vec<f64> = wp.iter().map(|o| o.num().unwrap_or(f64::NAN)).collect();
            let g = xref
                .get(p, "Gamma")
                .num()
                .filter(|g| *g != 0.0)
                .unwrap_or(1.0);
            let (xw, yw, zw) = (
                wp.first().copied().unwrap_or(f64::NAN),
                wp.get(1).copied().unwrap_or(f64::NAN),
                wp.get(2).copied().unwrap_or(f64::NAN),
            );
            if xw < 0.0 || zw < 0.0 || yw != 1.0 {
                return Err(ImageError);
            }
            Ok(Cs::CalGray {
                yw,
                g: if g < 1.0 { 1.0 } else { g },
            })
        }
        Some("ICCBased") => {
            let s = arg(1);
            let s = s.stream().ok_or(ImageError)?;
            let n = xref.get(&s.dict, "N").num();
            if let Some(alt) = s.dict.get_raw("Alternate").cloned() {
                let alt_cs = parse_cs(xref, &alt, resources, depth + 1)?;
                if Some(alt_cs.num_comps() as f64) == n {
                    return Ok(alt_cs);
                }
            }
            match n {
                Some(1.0) => Ok(Cs::Gray),
                Some(3.0) => Ok(Cs::Rgb),
                Some(4.0) => Ok(Cs::Cmyk),
                _ => Err(ImageError),
            }
        }
        Some("I" | "Indexed") => {
            let base = parse_cs(xref, a.get(1).unwrap_or(&Obj::Null), resources, depth + 1)?;
            let hi = arg(2).num().unwrap_or(f64::NAN);
            let hi = if hi.is_nan() {
                0.0
            } else {
                hi.clamp(0.0, 255.0)
            };
            let high_val = hi as usize;
            let count = high_val + 1;
            let length = base.num_comps() * count;
            let mut palette = vec![0u8; length];
            match arg(3) {
                Obj::Stream(st) => {
                    let b = xref.stream_bytes(&st).unwrap_or_default();
                    for (i, v) in b.iter().take(length).enumerate() {
                        palette[i] = *v;
                    }
                }
                Obj::Str(s) => {
                    for (i, v) in palette.iter_mut().enumerate() {
                        *v = s.get(i).copied().unwrap_or(0);
                    }
                }
                _ => return Err(ImageError),
            }
            let mut lookup = vec![0u8; count * 3];
            let pf: Vec<f64> = palette.iter().map(|&b| b as f64).collect();
            base.get_rgb_buffer(&pf, count, &mut lookup, 0, 8, 0);
            Ok(Cs::Indexed { high_val, lookup })
        }
        Some("Lab") => {
            let p = arg(1);
            let p = p.dict().ok_or(ImageError)?;
            let wp = xref.get_array(p, "WhitePoint").ok_or(ImageError)?;
            let wp: Vec<f64> = wp.iter().map(|o| o.num().unwrap_or(f64::NAN)).collect();
            let r = xref
                .get_array(p, "Range")
                .map(|r| {
                    r.iter()
                        .map(|o| o.num().unwrap_or(f64::NAN))
                        .collect::<Vec<f64>>()
                })
                .unwrap_or(vec![-100.0, 100.0, -100.0, 100.0]);
            let mut lab = Lab {
                xw: wp.first().copied().unwrap_or(f64::NAN),
                yw: wp.get(1).copied().unwrap_or(f64::NAN),
                zw: wp.get(2).copied().unwrap_or(f64::NAN),
                amin: r.first().copied().unwrap_or(f64::NAN),
                amax: r.get(1).copied().unwrap_or(f64::NAN),
                bmin: r.get(2).copied().unwrap_or(f64::NAN),
                bmax: r.get(3).copied().unwrap_or(f64::NAN),
            };
            if lab.xw < 0.0 || lab.zw < 0.0 || lab.yw != 1.0 {
                return Err(ImageError);
            }
            if lab.amin > lab.amax || lab.bmin > lab.bmax {
                lab.amin = -100.0;
                lab.amax = 100.0;
                lab.bmin = -100.0;
                lab.bmax = 100.0;
            }
            Ok(Cs::Lab(Box::new(lab)))
        }
        // CalRGB, Separation, DeviceN and Pattern need machinery (the CalRGB
        // matrix chain, PDF functions) not ported yet: the image fails, which
        // is what the engine already counts as a picture it could not read.
        _ => Err(ImageError),
    }
}

/// An image's bytes after every non-image filter, and the image filter
/// still to run.
enum Raw {
    Bytes(Vec<u8>),
    Jpeg {
        bytes: Vec<u8>,
        color_transform: Option<i64>,
    },
    Jpx(Vec<u8>),
}

fn filter_names(xref: &XRef<'_>, dict: &Dict) -> Vec<String> {
    match xref.get2(dict, "F", "Filter") {
        Obj::Name(n) => vec![n.to_string()],
        Obj::Array(a) => a
            .iter()
            .filter_map(|o| xref.resolve(o).name().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn filter_params(xref: &XRef<'_>, dict: &Dict, i: usize, single: bool) -> Option<Rc<Dict>> {
    match xref.get2(dict, "DP", "DecodeParms") {
        Obj::Dict(d) if single => Some(d),
        Obj::Array(a) => a
            .get(i)
            .map(|o| xref.resolve(o))
            .and_then(|o| o.dict().cloned()),
        _ => None,
    }
}

fn hayro_stream<'p>(xref: &XRef<'p>, s: &Stream) -> Option<hayro_syntax::object::Stream<'p>> {
    match &s.source {
        StreamSource::Indirect(r) => xref.pdf().xref().get::<hayro_syntax::object::Stream<'_>>(
            hayro_syntax::object::ObjectIdentifier::new(r.num, r.generation),
        ),
        StreamSource::Inline(_) => None,
    }
}

/// Decoded bytes of an image stream with its image filter split off.
fn raw(xref: &XRef<'_>, s: &Stream) -> Result<Raw, ImageError> {
    let names = filter_names(xref, &s.dict);
    let last = names.last().map(String::as_str);
    let n = names.len();
    let pre = |upto: usize| -> Result<Vec<u8>, ImageError> {
        let data = xref.stream_raw(s).ok_or(ImageError)?;
        crate::filters::decode_chain(xref, data, &s.dict, &names[..upto], n == 1)
    };
    match last {
        Some("DCTDecode" | "DCT") => {
            let ct = filter_params(xref, &s.dict, n - 1, n == 1)
                .and_then(|p| xref.get(&p, "ColorTransform").int())
                .map(|v| v as i64);
            Ok(Raw::Jpeg {
                bytes: pre(n - 1)?,
                color_transform: ct,
            })
        }
        Some("JPXDecode" | "JPX") => Ok(Raw::Jpx(pre(n - 1)?)),
        Some("JBIG2Decode" | "CCITTFaxDecode" | "CCF") => {
            let hs = hayro_stream(xref, s).ok_or(ImageError)?;
            let params = hayro_syntax::object::stream::ImageDecodeParams {
                is_indexed: true,
                bpc: Some(1),
                num_components: Some(1),
                target_dimension: None,
                width: 0,
                height: 0,
            };
            let r = hs.decoded_image(&params).map_err(|_| ImageError)?;
            Ok(Raw::Bytes(r.data.into_owned()))
        }
        _ => match &s.source {
            StreamSource::Indirect(_) => Ok(Raw::Bytes(xref.stream_bytes(s).ok_or(ImageError)?)),
            StreamSource::Inline(_) => Ok(Raw::Bytes(pre(n)?)),
        },
    }
}

/// A stream's fully decoded bytes (used for a stencil mask's one pixel).
pub fn stream_data(xref: &XRef<'_>, s: &Stream) -> Option<Vec<u8>> {
    match raw(xref, s).ok()? {
        Raw::Bytes(b) => Some(b),
        _ => None,
    }
}

/// `ImageResizer` under Bun: no canvas, so the "good" square is 2048.
fn reduce_power_for_jpx(width: u32, height: u32, comps: u32) -> u32 {
    if width == 0 || height == 0 {
        return 0;
    }
    let max_area = 2f64.powi(30) / (comps as f64 * 4.0);
    let area = width as f64 * height as f64;
    let needs_resize = !(width <= 2048 && height <= 2048);
    if !needs_resize {
        if area > max_area {
            return (area / max_area).log2().ceil() as u32;
        }
        return 0;
    }
    let max_area_canvas = 2048.0 * 2048.0;
    let min_factor = (width as f64 / 2048.0)
        .max(height as f64 / 2048.0)
        .max((area / max_area.min(max_area_canvas)).sqrt());
    min_factor.log2().ceil().max(0.0) as u32
}

/// `JpxImage.parseImageProperties`: the SIZ marker's image size and
/// component count.
fn jpx_properties(data: &[u8]) -> Result<(u32, u32, u32), ImageError> {
    let mut i = 0;
    while i + 1 < data.len() {
        if data[i] == 0xff && data[i + 1] == 0x51 {
            let p = i + 2 + 4;
            let rd = |o: usize| -> Option<u32> {
                Some(u32::from_be_bytes([
                    *data.get(o)?,
                    *data.get(o + 1)?,
                    *data.get(o + 2)?,
                    *data.get(o + 3)?,
                ]))
            };
            let xsiz = rd(p).ok_or(ImageError)?;
            let ysiz = rd(p + 4).ok_or(ImageError)?;
            let xosiz = rd(p + 8).ok_or(ImageError)?;
            let yosiz = rd(p + 12).ok_or(ImageError)?;
            let c = p + 16 + 16;
            let csiz = u16::from_be_bytes([
                *data.get(c).ok_or(ImageError)?,
                *data.get(c + 1).ok_or(ImageError)?,
            ]) as u32;
            return Ok((xsiz.wrapping_sub(xosiz), ysiz.wrapping_sub(yosiz), csiz));
        }
        i += 1;
    }
    Err(ImageError)
}

/// What OpenJPEG's pdf.js glue returns: 8-bit samples, interleaved, laid
/// out for `numComponents` (0 meaning RGBA).
fn decode_jpx(
    data: &[u8],
    num_components: usize,
    indexed: bool,
    reduce_power: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ImageError> {
    let settings = hayro_jpeg2000::DecodeSettings {
        resolve_palette_indices: !indexed,
        strict: false,
        target_resolution: if reduce_power > 0 {
            Some((
                (width >> reduce_power).max(1),
                (height >> reduce_power).max(1),
            ))
        } else {
            None
        },
    };
    let image = hayro_jpeg2000::Image::new(data, &settings).map_err(|_| ImageError)?;
    let mut ctx = hayro_jpeg2000::DecoderContext::default();
    let decoded = image.decode(&mut ctx).map_err(|_| ImageError)?;
    let comps = decoded.components();
    if comps.is_empty() {
        return Err(ImageError);
    }
    let len = comps.iter().map(|c| c.samples().len()).min().unwrap_or(0);
    let to8 = |c: &hayro_jpeg2000::ComponentData, i: usize| -> u8 {
        let v = c.samples()[i];
        let bd = c.bit_depth() as i32;
        let v = if bd > 8 {
            (v.round() as i64) >> (bd - 8)
        } else if bd < 8 {
            (v.round() as i64) << (8 - bd)
        } else {
            v.round() as i64
        };
        v.clamp(0, 255) as u8
    };
    let n = comps.len();
    let mut out;
    if num_components == 0 {
        out = Vec::with_capacity(len * 4);
        for i in 0..len {
            match n {
                1 => {
                    let g = to8(&comps[0], i);
                    out.extend_from_slice(&[g, g, g, 255]);
                }
                2 => {
                    let g = to8(&comps[0], i);
                    out.extend_from_slice(&[g, g, g, to8(&comps[1], i)]);
                }
                3 => out.extend_from_slice(&[
                    to8(&comps[0], i),
                    to8(&comps[1], i),
                    to8(&comps[2], i),
                    255,
                ]),
                _ => out.extend_from_slice(&[
                    to8(&comps[0], i),
                    to8(&comps[1], i),
                    to8(&comps[2], i),
                    to8(&comps[3], i),
                ]),
            }
        }
    } else {
        out = Vec::with_capacity(len * num_components);
        for i in 0..len {
            for k in 0..num_components {
                let c = &comps[k.min(n - 1)];
                out.push(if k < n { to8(c, i) } else { 255 });
            }
        }
    }
    Ok(out)
}

/// One PDFImage: the image, or its soft mask or stencil mask.
struct PdfImg {
    width: usize,
    height: usize,
    bpc: u32,
    cs: Option<Cs>,
    num_comps: usize,
    decode: Option<Vec<f64>>,
    needs_decode: bool,
    decode_coefficients: Vec<f64>,
    decode_addends: Vec<f64>,
    matte: Option<Vec<f64>>,
    raw: Raw,
    jpx_rgba_premultiplied: bool,
    smask: Option<Box<PdfImg>>,
    mask: Option<Mask>,
}

enum Mask {
    Image(Box<PdfImg>),
    ColorKey(Vec<f64>),
}

impl PdfImg {
    #[allow(clippy::too_many_arguments)]
    fn new(
        xref: &XRef<'_>,
        res: &Dict,
        s: &Stream,
        is_inline: bool,
        fallback: Option<(f64, f64)>,
        is_mask: bool,
        smask: Option<Rc<Stream>>,
        mask: Option<Obj>,
    ) -> Result<Self, ImageError> {
        let dict = &s.dict;
        let names = filter_names(xref, dict);
        let first = names.first().map(String::as_str);
        let mut raw = raw(xref, s)?;
        let mut image_w: Option<f64> = None;
        let mut image_h: Option<f64> = None;
        let mut image_bpc: Option<u32> = None;
        let mut image_comps: Option<u32> = None;
        let mut jpx: Option<(bool, u32)> = None; // (smaskInData, reducePower)
        let smask_in_data = xref
            .get(dict, "SMaskInData")
            .num()
            .is_some_and(|v| v >= 1.0);
        match first {
            Some("JPXDecode") => {
                let bytes = match &raw {
                    Raw::Jpx(b) => b,
                    _ => return Err(ImageError),
                };
                let (w, h, c) = jpx_properties(bytes)?;
                let rp = reduce_power_for_jpx(w, h, c);
                let (mut w, mut h) = (w as f64, h as f64);
                if rp > 0 {
                    let f = 2f64.powi(rp as i32);
                    w = (w / f).ceil();
                    h = (h / f).ceil();
                }
                image_w = Some(w);
                image_h = Some(h);
                image_bpc = Some(8);
                image_comps = Some(c);
                jpx = Some((smask_in_data, rp));
            }
            Some("JBIG2Decode") => {
                image_bpc = Some(1);
                image_comps = Some(1);
            }
            _ => {}
        }
        let dw = xref.get2(dict, "W", "Width");
        let dh = xref.get2(dict, "H", "Height");
        let (mut width, mut height) = (dw.num().unwrap_or(f64::NAN), dh.num().unwrap_or(f64::NAN));
        if let (Some(iw), Some(ih)) = (image_w, image_h)
            && iw.fract() == 0.0
            && iw > 0.0
            && ih.fract() == 0.0
            && ih > 0.0
            && (iw != width || ih != height)
        {
            width = iw;
            height = ih;
        } else {
            let valid_w = width > 0.0;
            let valid_h = height > 0.0;
            if !valid_w || !valid_h {
                let Some((fw, fh)) = fallback else {
                    return Err(ImageError);
                };
                if !valid_w {
                    width = fw;
                }
                if !valid_h {
                    height = fh;
                }
            }
        }
        // toRgb in the engine refuses a fractional size, and every buffer
        // below is sized from these.
        if width.fract() != 0.0 || height.fract() != 0.0 || width < 1.0 || height < 1.0 {
            return Err(ImageError);
        }
        if width * height > MAX_PIXELS as f64 {
            return Err(ImageError);
        }
        let image_mask = xref.get2(dict, "IM", "ImageMask").truthy();
        let matte = num_array(xref, &xref.get(dict, "Matte"));
        let bpc = match image_bpc {
            Some(b) => b,
            None => {
                let b = xref
                    .get2(dict, "BPC", "BitsPerComponent")
                    .num()
                    .unwrap_or(0.0);
                if b == 0.0 || b.is_nan() {
                    if image_mask {
                        1
                    } else {
                        return Err(ImageError);
                    }
                } else {
                    b as u32
                }
            }
        };
        if bpc == 0 || bpc > 16 {
            return Err(ImageError);
        }
        let mut cs = None;
        let mut num_comps = 1;
        let mut jpx_premul = false;
        if !image_mask {
            let raw_cs = dict
                .get_raw("CS")
                .or_else(|| dict.get_raw("ColorSpace"))
                .cloned();
            let has_cs = raw_cs.as_ref().is_some_and(|c| c.truthy());
            if let Some((sid, _)) = jpx
                && sid
                && xref.get(dict, "SMaskInData").num() == Some(2.0)
            {
                jpx_premul = true;
            }
            let cs_obj = if !has_cs {
                if jpx.is_some() {
                    Obj::Name(Rc::from("DeviceRGBA"))
                } else {
                    match image_comps {
                        Some(1) => Obj::Name(Rc::from("DeviceGray")),
                        Some(3) => Obj::Name(Rc::from("DeviceRGB")),
                        Some(4) => Obj::Name(Rc::from("DeviceCMYK")),
                        _ => return Err(ImageError),
                    }
                }
            } else if jpx.is_some_and(|(sid, _)| sid) {
                Obj::Name(Rc::from("DeviceRGBA"))
            } else {
                raw_cs.unwrap_or(Obj::Null)
            };
            let parsed = parse_cs(xref, &cs_obj, if is_inline { Some(res) } else { None }, 0)?;
            num_comps = parsed.num_comps();
            if let Some((_, rp)) = jpx {
                let nc = if has_cs { num_comps } else { 0 };
                let indexed = matches!(parsed, Cs::Indexed { .. });
                if let Raw::Jpx(bytes) = &raw {
                    let (w0, h0, _) = jpx_properties(bytes)?;
                    let decoded = decode_jpx(bytes, nc, indexed, rp, w0, h0)?;
                    raw = Raw::Bytes(decoded);
                }
            }
            cs = Some(parsed);
        } else if let Raw::Jpx(bytes) = &raw {
            let (w0, h0, _) = jpx_properties(bytes)?;
            raw = Raw::Bytes(decode_jpx(
                bytes,
                0,
                false,
                jpx.map(|j| j.1).unwrap_or(0),
                w0,
                h0,
            )?);
        }
        let decode = num_array(xref, &xref.get2(dict, "D", "Decode"));
        let mut img = PdfImg {
            width: width as usize,
            height: height as usize,
            bpc,
            cs,
            num_comps,
            decode: None,
            needs_decode: false,
            decode_coefficients: Vec::new(),
            decode_addends: Vec::new(),
            matte,
            raw,
            jpx_rgba_premultiplied: jpx_premul,
            smask: None,
            mask: None,
        };
        if let Some(mut d) = decode {
            let not_default = match &img.cs {
                Some(c) => !c.is_default_decode(&mut d, bpc),
                None => false,
            } || (is_mask && !is_default_decode_static(&mut d, 1));
            if not_default {
                img.needs_decode = true;
                let max = ((1u64 << bpc) - 1) as f64;
                let is_indexed = matches!(img.cs, Some(Cs::Indexed { .. }));
                let mut i = 0;
                while i < d.len() {
                    let dmin = d[i];
                    let dmax = d.get(i + 1).copied().unwrap_or(f64::NAN);
                    img.decode_coefficients.push(if is_indexed {
                        (dmax - dmin) / max
                    } else {
                        dmax - dmin
                    });
                    img.decode_addends
                        .push(if is_indexed { dmin } else { max * dmin });
                    i += 2;
                }
            }
            img.decode = Some(d);
        }
        if let Some(sm) = smask {
            img.smask = Some(Box::new(PdfImg::new(
                xref,
                res,
                &sm,
                is_inline,
                Some((width, height)),
                false,
                None,
                None,
            )?));
        } else if let Some(m) = mask {
            match m {
                Obj::Stream(ms) => {
                    if xref.get2(&ms.dict, "IM", "ImageMask").truthy() {
                        img.mask = Some(Mask::Image(Box::new(PdfImg::new(
                            xref,
                            res,
                            &ms,
                            is_inline,
                            Some((width, height)),
                            true,
                            None,
                            None,
                        )?)));
                    }
                }
                Obj::Array(a) => {
                    img.mask = Some(Mask::ColorKey(
                        a.iter()
                            .map(|o| xref.resolve(o).num().unwrap_or(f64::NAN))
                            .collect(),
                    ));
                }
                _ => {}
            }
        }
        Ok(img)
    }

    fn draw_width(&self) -> usize {
        let mut w = self.width;
        if let Some(s) = &self.smask {
            w = w.max(s.width);
        }
        if let Some(Mask::Image(m)) = &self.mask {
            w = w.max(m.width);
        }
        w
    }

    fn draw_height(&self) -> usize {
        let mut h = self.height;
        if let Some(s) = &self.smask {
            h = h.max(s.height);
        }
        if let Some(Mask::Image(m)) = &self.mask {
            h = h.max(m.height);
        }
        h
    }

    /// `getImageBytes(length, { drawWidth, drawHeight, forceRGB })`.
    fn image_bytes(
        &self,
        length: usize,
        draw: Option<(usize, usize)>,
        force_rgb: bool,
    ) -> Result<Vec<u8>, ImageError> {
        let mut b = match &self.raw {
            Raw::Bytes(b) => b.clone(),
            Raw::Jpeg {
                bytes,
                color_transform,
            } => {
                // #skipUselessBytes: start at the first FFD8.
                let mut start = 0;
                for i in 0..bytes.len().saturating_sub(1) {
                    if bytes[i] == 0xff && bytes[i + 1] == 0xd8 {
                        start = i;
                        break;
                    }
                }
                let jpeg = crate::jpeg::JpegImage::parse(&bytes[start..], *color_transform)
                    .map_err(|_| ImageError)?;
                let (w, h) = draw.unwrap_or((self.width, self.height));
                jpeg.get_data(w, h, force_rgb).map_err(|_| ImageError)?
            }
            Raw::Jpx(_) => return Err(ImageError),
        };
        if length > 0 {
            b.truncate(length);
        }
        Ok(b)
    }

    /// `getComponents(buffer)`, as numbers.
    fn components(&self, buffer: &[u8]) -> Vec<f64> {
        let bpc = self.bpc;
        if bpc == 8 {
            return buffer.iter().map(|&b| b as f64).collect();
        }
        let length = self.width * self.height * self.num_comps;
        let mut out = vec![0f64; length];
        let row_comps = self.width * self.num_comps;
        let max = ((1u64 << bpc) - 1) as i64;
        let mut pos = 0usize;
        let mut i = 0usize;
        let get = |p: usize| -> i64 { buffer.get(p).copied().map(|b| b as i64).unwrap_or(0) };
        if bpc == 1 {
            for _ in 0..self.height {
                let loop1_end = i + (row_comps & !7);
                let loop2_end = i + row_comps;
                while i < loop1_end {
                    let buf = get(pos);
                    pos += 1;
                    for b in 0..8 {
                        out[i + b] = ((buf >> (7 - b)) & 1) as f64;
                    }
                    i += 8;
                }
                if i < loop2_end {
                    let buf = get(pos);
                    pos += 1;
                    let mut mask = 128;
                    while i < loop2_end {
                        out[i] = if buf & mask != 0 { 1.0 } else { 0.0 };
                        i += 1;
                        mask >>= 1;
                    }
                }
            }
        } else {
            let mut bits = 0u32;
            let mut buf: i64 = 0;
            for (i, o) in out.iter_mut().enumerate() {
                if row_comps != 0 && i % row_comps == 0 {
                    buf = 0;
                    bits = 0;
                }
                while bits < bpc {
                    buf = ((buf << 8) | get(pos)) & 0xffff_ffff;
                    pos += 1;
                    bits += 8;
                }
                let remaining = bits - bpc;
                *o = ((buf >> remaining).clamp(0, max)) as f64;
                buf &= (1i64 << remaining) - 1;
                bits = remaining;
            }
        }
        out
    }

    /// `decodeBuffer`, storing back as the component array's type stores.
    fn decode_buffer(&self, comps: &mut [f64]) {
        let bpc = self.bpc;
        if bpc == 1 {
            for c in comps.iter_mut() {
                *c = if *c == 0.0 { 1.0 } else { 0.0 };
            }
            return;
        }
        let max = ((1u64 << bpc) - 1) as f64;
        let n = self.num_comps;
        let mut index = 0;
        for _ in 0..self.width * self.height {
            for j in 0..n {
                if index >= comps.len() {
                    return;
                }
                let v = self.decode_addends.get(j).copied().unwrap_or(f64::NAN)
                    + comps[index] * self.decode_coefficients.get(j).copied().unwrap_or(f64::NAN);
                let v = if v.is_nan() {
                    f64::NAN
                } else {
                    v.clamp(0.0, max)
                };
                // Stored into a Uint8Array (bpc <= 8) or Uint16Array: truncation.
                comps[index] = if bpc <= 8 {
                    to_u8(v) as f64
                } else if v.is_finite() {
                    ((v.trunc() as i64).rem_euclid(65536)) as f64
                } else {
                    0.0
                };
                index += 1;
            }
        }
    }

    /// `fillGrayBuffer` into the alpha channel of an RGBA buffer.
    fn fill_gray_alpha(
        &self,
        buffer: &mut [u8],
        dest_w: usize,
        dest_h: usize,
        invert: bool,
        max_rows: usize,
    ) -> Result<(), ImageError> {
        if self.num_comps != 1 {
            return Err(ImageError);
        }
        let (sw, sh) = (self.width, self.height);
        let row_bytes = (sw * self.bpc as usize).div_ceil(8);
        let img = self.image_bytes(sh * row_bytes, None, false)?;
        let mut comps = self.components(&img);
        let resample = dest_w != sw || dest_h != sh;
        let rows = dest_h.min(max_rows);
        let output_w = if resample { dest_w } else { sw };
        let y_ratio = sh as f64 / dest_h as f64;
        let x_map: Vec<usize> = if resample {
            let xr = sw as f64 / dest_w as f64;
            (0..dest_w)
                .map(|i| ((i as f64) * xr).floor() as u32 as usize)
                .collect()
        } else {
            Vec::new()
        };
        let mask: i64 = if invert { 0xff } else { 0 };
        let at = |comps: &[f64], i: usize| -> f64 { comps.get(i).copied().unwrap_or(f64::NAN) };
        let mut put = |i: usize, v: i64| {
            if let Some(d) = buffer.get_mut(i * 4 + 3) {
                *d = (v & 0xff) as u8;
            }
        };
        let js_int = |v: f64| -> i64 {
            if v.is_finite() {
                (v.trunc() as i64) as i32 as i64
            } else {
                0
            }
        };
        if self.bpc == 1 {
            let f = |c: f64| -> i64 {
                if self.needs_decode {
                    ((js_int(c) - 1) & 255) ^ mask
                } else {
                    ((-js_int(c)) & 255) ^ mask
                }
            };
            if resample {
                let mut di = 0;
                for row in 0..rows {
                    let py = ((row as f64) * y_ratio).floor() as usize * sw;
                    for col in 0..output_w {
                        put(di, f(at(&comps, py + x_map[col])));
                        di += 1;
                    }
                }
            } else {
                for i in 0..output_w * rows {
                    put(i, f(at(&comps, i)));
                }
            }
            return Ok(());
        }
        if self.needs_decode {
            self.decode_buffer(&mut comps);
        }
        let scale = 255.0 / (((1u64 << self.bpc) - 1) as f64);
        let f = |c: f64| -> i64 { js_int(scale * c) ^ mask };
        if resample {
            let mut di = 0;
            for row in 0..rows {
                let py = ((row as f64) * y_ratio).floor() as usize * sw;
                for col in 0..output_w {
                    put(di, f(at(&comps, py + x_map[col])));
                    di += 1;
                }
            }
        } else {
            for i in 0..output_w * rows {
                put(i, f(at(&comps, i)));
            }
        }
        Ok(())
    }

    /// `fillRgb` into `dest` (RGB with alpha01 = 0, RGBA with 1).
    #[allow(clippy::too_many_arguments)]
    fn fill_rgb(
        &self,
        dest: &mut [u8],
        ow: usize,
        oh: usize,
        w: usize,
        h: usize,
        actual_height: usize,
        comps: &[f64],
        alpha01: usize,
    ) {
        let Some(cs) = &self.cs else { return };
        let bpc = self.bpc;
        if let Cs::Rgba = cs {
            if oh != h || ow != w {
                resize_rgba(comps, dest, ow, oh, w, h, alpha01);
            } else if alpha01 == 1 {
                for (i, c) in comps.chunks(4).enumerate() {
                    for k in 0..3 {
                        if let (Some(d), Some(s)) = (dest.get_mut(i * 4 + k), c.get(k)) {
                            *d = clamp_u8(*s);
                        }
                    }
                }
            } else {
                let mut j = 0;
                for c in comps.chunks(4) {
                    for k in 0..3 {
                        if let (Some(d), Some(s)) = (dest.get_mut(j), c.get(k)) {
                            *d = clamp_u8(*s);
                        }
                        j += 1;
                    }
                }
            }
            return;
        }
        let count = ow * oh;
        let num_colors = 1usize << bpc;
        let needs_resizing = oh != h || ow != w;
        let mut rgb_buf: Option<Vec<u8>> = None;
        if cs.is_passthrough(bpc) {
            rgb_buf = Some(comps.iter().map(|&v| clamp_u8(v)).collect());
        } else if cs.num_comps() == 1
            && count > num_colors
            && cs.name() != "DeviceGray"
            && cs.name() != "DeviceRGB"
        {
            let all: Vec<f64> = (0..num_colors).map(|i| i as f64).collect();
            let mut color_map = vec![0u8; num_colors * 3];
            cs.get_rgb_buffer(&all, num_colors, &mut color_map, 0, bpc, 0);
            if !needs_resizing {
                let mut dp = 0;
                for &c in comps.iter().take(count) {
                    let key = (c as usize) * 3;
                    for k in 0..3 {
                        if let Some(d) = dest.get_mut(dp) {
                            *d = color_map.get(key + k).copied().unwrap_or(0);
                        }
                        dp += 1;
                    }
                    dp += alpha01;
                }
            } else {
                let mut buf = vec![0u8; count * 3];
                for (i, &c) in comps.iter().take(count).enumerate() {
                    let key = (c as usize) * 3;
                    for k in 0..3 {
                        buf[i * 3 + k] = color_map.get(key + k).copied().unwrap_or(0);
                    }
                }
                rgb_buf = Some(buf);
            }
        } else if !needs_resizing {
            cs.get_rgb_buffer(comps, w * actual_height, dest, 0, bpc, alpha01);
        } else {
            let mut buf = vec![0u8; count * 3];
            cs.get_rgb_buffer(comps, count, &mut buf, 0, bpc, 0);
            rgb_buf = Some(buf);
        }
        if let Some(buf) = rgb_buf {
            if needs_resizing {
                resize_rgb(&buf, dest, ow, oh, w, h, alpha01);
            } else {
                let mut dp = 0;
                let mut rp = 0;
                for _ in 0..w * actual_height {
                    for _ in 0..3 {
                        if let Some(d) = dest.get_mut(dp) {
                            *d = buf.get(rp).copied().unwrap_or(0);
                        }
                        dp += 1;
                        rp += 1;
                    }
                    dp += alpha01;
                }
            }
        }
    }

    /// `createImageData(false, false)`.
    fn create_image_data(&self) -> Result<PdfImage, ImageError> {
        let dw = self.draw_width();
        let dh = self.draw_height();
        // The canvas is the widest of the picture and its masks by the
        // tallest, so two pictures under the cap can still multiply past it.
        if dw.checked_mul(dh).is_none_or(|area| area > MAX_PIXELS) {
            return Err(ImageError);
        }
        let ow = self.width;
        let oh = self.height;
        let bpc = self.bpc as usize;
        let row_bytes = (ow * self.num_comps * bpc).div_ceil(8);
        let cs_name = self.cs.as_ref().map(Cs::name).unwrap_or("");
        if self.smask.is_none() && self.mask.is_none() && cs_name == "DeviceRGBA" {
            let mut data = self.image_bytes(oh * ow * 4, None, false)?;
            if self.jpx_rgba_premultiplied {
                undo_preblend(&mut data, [0.0, 0.0, 0.0]);
            }
            return Ok(PdfImage {
                width: dw as u32,
                height: dh as u32,
                kind: 3,
                data,
            });
        }
        if self.smask.is_none() && self.mask.is_none() {
            let kind = if cs_name == "DeviceGray" && bpc == 1 {
                Some(1)
            } else if cs_name == "DeviceRGB" && bpc == 8 && !self.needs_decode {
                Some(2)
            } else {
                None
            };
            if let Some(kind) = kind
                && dw == ow
                && dh == oh
            {
                let mut data = self.image_bytes(oh * row_bytes, None, false)?;
                if self.needs_decode {
                    for b in data.iter_mut() {
                        *b ^= 0xff;
                    }
                }
                return Ok(PdfImage {
                    width: dw as u32,
                    height: dh as u32,
                    kind,
                    data,
                });
            }
            if matches!(self.raw, Raw::Jpeg { .. })
                && !self.needs_decode
                && matches!(cs_name, "DeviceGray" | "DeviceRGB" | "DeviceCMYK")
            {
                let mut len = oh * row_bytes;
                if cs_name == "DeviceGray" {
                    len *= 3;
                }
                let data = self.image_bytes(len, Some((dw, dh)), true)?;
                return Ok(PdfImage {
                    width: dw as u32,
                    height: dh as u32,
                    kind: 2,
                    data,
                });
            }
        }
        let img = self.image_bytes(oh * row_bytes, None, false)?;
        let actual_height = if row_bytes == 0 || oh == 0 {
            0
        } else {
            ((img.len() as f64 / row_bytes as f64) * dh as f64 / oh as f64) as i32 as usize
        };
        let mut comps = self.components(&img);
        let (kind, mut data, alpha01) = if self.smask.is_none() && self.mask.is_none() {
            (2u8, vec![0u8; dw * dh * 3], 0usize)
        } else {
            let mut d = vec![0u8; dw * dh * 4];
            self.fill_opacity(&mut d, dw, dh, actual_height, &comps)?;
            (3u8, d, 1usize)
        };
        if self.needs_decode {
            self.decode_buffer(&mut comps);
        }
        self.fill_rgb(&mut data, ow, oh, dw, dh, actual_height, &comps, alpha01);
        if kind == 3
            && let Some(sm) = &self.smask
            && let Some(m) = &sm.matte
            && let Some(cs) = &self.cs
        {
            let mut rgb = [0u8; 3];
            cs.get_rgb_buffer(
                &m.iter().map(|v| v * 255.0).collect::<Vec<_>>(),
                1,
                &mut rgb,
                0,
                8,
                0,
            );
            let len = (dw * actual_height * 4).min(data.len());
            undo_preblend(
                &mut data[..len],
                [rgb[0] as f64, rgb[1] as f64, rgb[2] as f64],
            );
        }
        Ok(PdfImage {
            width: dw as u32,
            height: dh as u32,
            kind,
            data,
        })
    }

    fn fill_opacity(
        &self,
        buf: &mut [u8],
        w: usize,
        h: usize,
        actual_height: usize,
        image: &[f64],
    ) -> Result<(), ImageError> {
        if let Some(sm) = &self.smask {
            return sm.fill_gray_alpha(buf, w, h, false, actual_height);
        }
        match &self.mask {
            Some(Mask::Image(m)) => m.fill_gray_alpha(buf, w, h, true, actual_height),
            Some(Mask::ColorKey(range)) => {
                for i in 0..w * actual_height {
                    let mut opacity = 0u8;
                    let off = i * self.num_comps;
                    for j in 0..self.num_comps {
                        let color = image.get(off + j).copied().unwrap_or(f64::NAN);
                        let lo = range.get(j * 2).copied().unwrap_or(f64::NAN);
                        let hi = range.get(j * 2 + 1).copied().unwrap_or(f64::NAN);
                        if color < lo || color > hi {
                            opacity = 255;
                            break;
                        }
                    }
                    if let Some(d) = buf.get_mut(i * 4 + 3) {
                        *d = opacity;
                    }
                }
                Ok(())
            }
            None => {
                for i in 0..w * actual_height {
                    if let Some(d) = buf.get_mut(i * 4 + 3) {
                        *d = 255;
                    }
                }
                Ok(())
            }
        }
    }
}

/// `#undoPreblend` over an RGBA buffer (values stored back clamped).
fn undo_preblend(buf: &mut [u8], matte: [f64; 3]) {
    let mut i = 0;
    while i + 3 < buf.len() {
        let alpha = buf[i + 3];
        if alpha == 0 {
            buf[i] = 255;
            buf[i + 1] = 255;
            buf[i + 2] = 255;
        } else {
            let k = 255.0 / alpha as f64;
            for c in 0..3 {
                buf[i + c] = clamp_u8((buf[i + c] as f64 - matte[c]) * k + matte[c]);
            }
        }
        i += 4;
    }
}

fn resize_rgb(
    src: &[u8],
    dest: &mut [u8],
    w1: usize,
    h1: usize,
    w2: usize,
    h2: usize,
    alpha01: usize,
) {
    let x_ratio = w1 as f64 / w2 as f64;
    let y_ratio = h1 as f64 / h2 as f64;
    let x_scaled: Vec<usize> = (0..w2)
        .map(|i| (((i as f64) * x_ratio).floor() as usize * 3) & 0xffff)
        .collect();
    let w1_scanline = w1 * 3;
    let mut ni = 0;
    for i in 0..h2 {
        let py = ((i as f64) * y_ratio).floor() as usize * w1_scanline;
        for &xs in x_scaled.iter() {
            let oi = py + xs;
            for k in 0..3 {
                if let Some(d) = dest.get_mut(ni) {
                    *d = src.get(oi + k).copied().unwrap_or(0);
                }
                ni += 1;
            }
            ni += alpha01;
        }
    }
}

fn resize_rgba(
    src: &[f64],
    dest: &mut [u8],
    w1: usize,
    h1: usize,
    w2: usize,
    h2: usize,
    alpha01: usize,
) {
    let x_ratio = w1 as f64 / w2 as f64;
    let y_ratio = h1 as f64 / h2 as f64;
    let x_scaled: Vec<usize> = (0..w2)
        .map(|i| ((i as f64) * x_ratio).floor() as usize & 0xffff)
        .collect();
    let mut ni = 0;
    for i in 0..h2 {
        let base = ((i as f64) * y_ratio).floor() as usize * w1;
        for &xs in x_scaled.iter() {
            let o = (base + xs) * 4;
            for k in 0..3 {
                if let Some(d) = dest.get_mut(ni + k) {
                    *d = clamp_u8(src.get(o + k).copied().unwrap_or(0.0));
                }
            }
            ni += 3 + alpha01;
        }
    }
}

/// `PDFImage.buildImage(...).createImageData(false, false)` for one paint.
pub fn decode(xref: &XRef<'_>, src: &ImageSource) -> Option<PdfImage> {
    let s = &src.stream;
    let smask = xref.get(&s.dict, "SMask");
    let mask = xref.get(&s.dict, "Mask");
    let (smask, mask) = if smask.truthy() {
        (smask.stream().cloned(), None)
    } else if mask.truthy() {
        match mask {
            Obj::Stream(_) | Obj::Array(_) => (None, Some(mask)),
            _ => (None, None),
        }
    } else {
        (None, None)
    };
    let img = PdfImg::new(
        xref,
        &src.resources,
        s,
        src.is_inline,
        None,
        false,
        smask,
        mask,
    )
    .ok()?;
    img.create_image_data().ok()
}

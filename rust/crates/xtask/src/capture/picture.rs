//! Reading, writing and comparing PNGs, with the pure-Rust `png` crate.
//!
//! Chrome hands the capture finished PNG bytes, and the README pictures are
//! compared byte for byte ([`super::shots::write_if_changed`]), which is
//! what keeps an unchanged window from growing the repository. A backend
//! that draws pixels itself (the egui window at cutover) needs the other
//! half: [`encode_rgba`] writes the same bytes for the same pixels every
//! time. [`diff`] says how far apart two pictures are by their pixels, for
//! telling a real change from a re-encoding, and [`size`] is what
//! `cargo xtask make-og` prints of the card it drew.

/// A decoded picture: RGBA, 8 bits a channel, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A PNG's size in pixels, read from its header.
pub fn size(bytes: &[u8]) -> Result<(u32, u32), String> {
    let reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .map_err(|e| format!("not a PNG: {e}"))?;
    let info = reader.info();
    Ok((info.width, info.height))
}

/// Any PNG, as RGBA8: grey, palette and 16-bit pictures are widened.
pub fn decode(bytes: &[u8]) -> Result<Rgba, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| format!("not a PNG: {e}"))?;
    let size = reader
        .output_buffer_size()
        .ok_or("a PNG too big to decode")?;
    let mut buf = vec![0; size];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("a PNG that would not decode: {e}"))?;
    buf.truncate(frame.buffer_size());
    let (w, h) = (frame.width, frame.height);
    let pixels = match frame.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("a palette PNG the decoder did not expand".into()),
    };
    Ok(Rgba {
        width: w,
        height: h,
        pixels,
    })
}

/// RGBA8 pixels as PNG bytes. Same pixels, same bytes: no time, no text
/// chunk, one fixed compression and filter.
pub fn encode_rgba(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, String> {
    let want = (width as usize) * (height as usize) * 4;
    if pixels.len() != want {
        return Err(format!(
            "{} bytes of pixels for a {width} by {height} picture, which needs {want}",
            pixels.len()
        ));
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::High);
        enc.set_filter(png::Filter::Adaptive);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(pixels).map_err(|e| e.to_string())?;
        w.finish().map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// How two pictures differ, by their pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diff {
    /// Every pixel the same (the bytes may still differ).
    Same,
    /// Not the same size, so not comparable pixel by pixel.
    Size {
        before: (u32, u32),
        after: (u32, u32),
    },
    /// The same size, with this many pixels of `total` different.
    Pixels { changed: u64, total: u64 },
}

pub fn diff(before: &[u8], after: &[u8]) -> Result<Diff, String> {
    let (a, b) = (decode(before)?, decode(after)?);
    if (a.width, a.height) != (b.width, b.height) {
        return Ok(Diff::Size {
            before: (a.width, a.height),
            after: (b.width, b.height),
        });
    }
    let changed = a
        .pixels
        .chunks_exact(4)
        .zip(b.pixels.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count() as u64;
    if changed == 0 {
        return Ok(Diff::Same);
    }
    Ok(Diff::Pixels {
        changed,
        total: u64::from(a.width) * u64::from(a.height),
    })
}

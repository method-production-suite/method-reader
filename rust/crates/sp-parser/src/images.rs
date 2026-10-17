//! Port of `src/parser/images.ts`: images carried out of a PDF, where they
//! sit and what pixels they hold.
//!
//! This is the PDF-FACING half of image extraction, and the ORCHESTRATION:
//! placements out of an operator list, pixels out of the page's facts, PNGs
//! out of png.rs, and finally (`finish_images`) the whole document's pile
//! handed to artwork.rs and returned as `RawImage`s. Whether a given picture
//! is art or furniture is a judgement, not a fact about the file, and that
//! judgement lives in artwork.rs with nothing of the PDF in it — the same
//! line png.rs draws around itself. The two halves change for different
//! reasons: this file changes when the PDF layer does, artwork.rs changes
//! when a real script proves a threshold wrong.
//!
//! extract.rs holds a single edge to this file and none to artwork.rs. It
//! reads pages; it should not also be where the editorial policy is invoked.
//!
//! The placement walk mirrors `collect_underline_marks` in extract.rs —
//! same save/restore/transform bookkeeping over the same operator list. The
//! 6-element `concat` is deliberately a second copy rather than an import:
//! it is a matrix multiply with no judgement in it to drift, which is what
//! CLAUDE.md's one-shared-copy rule (registry 5b, 18) is actually about.
//! Each copy is pinned by the suite of the file it lives in.
//!
//! What the TypeScript did and this does not: it waited on pdf.js's object
//! stores for pixels that arrive after the operator list (a per-image
//! timeout, and the page store raced against `commonObjs`). The facts here
//! arrive with their pixels already resolved, so a picture that never
//! arrived is simply `None` in [`PageFacts::images`] and counts as failed.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sp_model::RawImage;

use crate::artwork::{DroppedBy, ImageCandidate, keep_artwork};
use crate::facts::{ImageFact, Op, PageFacts};
use crate::js;
use crate::png::{downscale_rgb, encode_png};

/// One `Do` of an image XObject, in PDF user space (y up).
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePlacement {
    pub obj_id: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    /// SOURCE pixels, as the paint op declares them. Named `src` because the
    /// same subsystem also carries OUTPUT pixels — what the PNG ends up being
    /// after the downscale — and one object holding both under
    /// near-identical names is a mix-up nothing would catch.
    pub src_px_width: f64,
    pub src_px_height: f64,
}

/// Handed out on an unbalanced `Q`, and pushed on the stack like any other
/// matrix. Every matrix in this file is replaced, never mutated.
const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `m ∘ t` — PDF's `cm` post-multiplies onto the current matrix. The same
/// multiply as extract.rs, operands in the same order: `t` applies FIRST,
/// inside `m`, so a translate under a 2x scale moves twice as far. A missing
/// entry reads as `undefined` did, which is NaN.
fn concat(m: &[f64; 6], t: &[f64]) -> [f64; 6] {
    let t = |i: usize| t.get(i).copied().unwrap_or(f64::NAN);
    [
        m[0] * t(0) + m[2] * t(1),
        m[1] * t(0) + m[3] * t(1),
        m[0] * t(2) + m[2] * t(3),
        m[1] * t(2) + m[3] * t(3),
        m[0] * t(4) + m[2] * t(5) + m[4],
        m[1] * t(4) + m[3] * t(5) + m[5],
    ]
}

fn min_of(v: &[f64]) -> f64 {
    // Math.min: any NaN makes the answer NaN.
    if v.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    v.iter().copied().fold(f64::INFINITY, f64::min)
}

fn max_of(v: &[f64]) -> f64 {
    if v.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

/// Every image paint on a page, with the box it actually covers.
///
/// Two operator cases beyond `q`/`cm`/`Q` decide whether this is right on
/// real files, and each was read out of pdfjs-dist rather than assumed
/// (6.2.108, and re-checked on 6.3.289):
///
///   - A form XObject does NOT expand into save/transform ops. Its /Matrix
///     rides on `paintFormXObjectBegin` and the renderer saves, applies it,
///     and restores on End — so placed art (Illustrator, InDesign, a stamped
///     appearance) would land at the wrong offset, and the form's own
///     transforms would leak into the rest of the page.
///   - Images inside an annotation are skipped on WHAT they are, not on any
///     difficulty placing them: a stamp, a signature or a form-field
///     appearance is markup ON the script, never the script's art. What a
///     future change must NOT do is simply delete the skip: an appearance
///     stream does not compose onto the CTM tracked here, so a paint that
///     fell through would be placed by whatever the page happened to leave
///     behind. pdf.js appends annotations AFTER the page's own operators, so
///     the skip can never swallow page art.
///
/// The form and annotation branches are DEFENSIVE, not load-bearing, and a
/// corpus scan is not grounds for deleting them. 2026-08-06: 549 pages
/// across seven generators carry no form XObjects, no annotations and no
/// masks at all. One PDF from a generator nobody here owns is all it takes
/// to need them, and the failure they prevent is silent: art placed hundreds
/// of points from where it was drawn.
///
/// An image XObject always occupies the unit square, so the CTM in force at
/// the paint IS the placement. Unlike the underline walk this one does not
/// skip rotated content: a bbox stops describing a rule on a slant, but it
/// still describes the region a turned storyboard panel covers, and that
/// panel is worth keeping.
pub fn collect_image_placements(ops: &[Op]) -> Vec<ImagePlacement> {
    let mut out = Vec::new();
    let mut m = IDENTITY;
    let mut stack: Vec<[f64; 6]> = Vec::new();
    let mut in_annotation = false;

    for op in ops {
        match op {
            Op::Save => stack.push(m),
            Op::Restore | Op::PaintFormXObjectEnd => m = stack.pop().unwrap_or(IDENTITY),
            Op::Transform { matrix } => m = concat(&m, matrix),
            Op::PaintFormXObjectBegin { matrix } => {
                stack.push(m);
                // A form with no /Matrix still saves and restores; only the
                // concat is conditional.
                if let Some(matrix) = matrix {
                    m = concat(&m, matrix);
                }
            }
            Op::BeginAnnotation => in_annotation = true,
            Op::EndAnnotation => in_annotation = false,
            // Everything else falls through, and image MASKS are ignored on
            // what they are: a 1-bit stencil in a screenplay is a bullet or a
            // rule, never the art anyone wants kept.
            Op::PaintImageXObject {
                obj_id,
                width,
                height,
            } => {
                if in_annotation {
                    continue;
                }
                let Some(obj_id) = obj_id else { continue };
                // pdf.js refuses to emit a paint whose Width/Height are not
                // numbers, so this only fires on a hand-built list. Worth the
                // lines anyway: the pixel stage sizes buffers from these,
                // where a NaN becomes a corrupt image far from the operator
                // that caused it.
                let (Some(w), Some(h)) = (*width, *height) else {
                    continue;
                };
                if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
                    continue;
                }

                // BOTH corners was the underline walk's hard-won lesson; for
                // a box it is all FOUR, then min/max. Final Draft draws under
                // a y-flip, and mapping only the origin puts the image off
                // the page — the same defect, one dimension richer.
                let mut xs = [0.0; 4];
                let mut ys = [0.0; 4];
                for (k, (ux, uy)) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
                    .into_iter()
                    .enumerate()
                {
                    xs[k] = m[0] * ux + m[2] * uy + m[4];
                    ys[k] = m[1] * ux + m[3] * uy + m[5];
                }
                out.push(ImagePlacement {
                    obj_id: obj_id.clone(),
                    x0: min_of(&xs),
                    y0: min_of(&ys),
                    x1: max_of(&xs),
                    y1: max_of(&ys),
                    src_px_width: w,
                    src_px_height: h,
                });
            }
            Op::ConstructPath { .. } | Op::ShowText { .. } | Op::Other => {}
        }
    }
    out
}

/// Normalize any pdf.js image kind to packed 8-bit RGB, or `None` if we
/// can't.
///
/// `None` rather than an error on anything unexpected, because every input
/// here comes out of a file we did not write: a short buffer is what a
/// TRUNCATED image stream looks like from this side, the decoder having
/// capped the read at what actually decoded rather than reporting the
/// shortfall. png.rs's `assert_rgb` is the opposite end of the same contract
/// — it errors, because by then a mismatch could only be our own bug.
pub fn to_rgb(width: f64, height: f64, kind: u8, data: Option<&[u8]>) -> Option<Vec<u8>> {
    // Integer, not merely finite: a /Width of 2.5 would otherwise sail all
    // the way through as a well-formed buffer labelled 2.5x2, and png.rs
    // would fail a stage later with nothing pointing back here.
    if !js::is_integer(width) || !js::is_integer(height) || width < 1.0 || height < 1.0 {
        return None;
    }
    // `{ data: null, bitmap }` is the shape pdf.js returns when it holds an
    // ImageBitmap instead of pixels.
    let data = data?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (w, h) = (width as usize, height as usize);
    let px = w.checked_mul(h)?;

    // Commonest first, not in ImageKind order: RGB_24BPP is every picture in
    // the fixture and the general colour-space fallback lands on RGBA_32BPP,
    // which leaves 1bpp the rare one.
    match kind {
        2 => {
            if data.len() < px.checked_mul(3)? {
                return None;
            }
            Some(data[..px * 3].to_vec())
        }
        3 => {
            if data.len() < px.checked_mul(4)? {
                return None;
            }
            // FLATTENED onto white, not dropped. png.rs emits colour type 2,
            // which has no alpha channel to carry, so a transparent pixel has
            // to become SOMETHING — and taking the stored RGB makes it
            // whatever sat underneath, frequently black. A logo cut out over a
            // dark backing would ship as art in a black rectangle, on an e-ink
            // page. White is the page a screenplay is read on, and it is what
            // pdf.js itself writes for a fully transparent pixel when it
            // un-premultiplies a matted mask.
            //
            // The alpha is STRAIGHT, not premultiplied, so this is the
            // ordinary blend and not `c + 255 * (1 - a)`.
            //
            // The limitation, written down because a device pass is where it
            // will surface: a reader in dark mode gets a WHITE surround
            // wherever the source was transparent.
            let mut out = vec![0u8; px * 3];
            for i in 0..px {
                let a = f64::from(data[i * 4 + 3]);
                for c in 0..3 {
                    // Rounded for png.rs's reason: a byte store truncates, and
                    // half a level shaved off every pixel is a uniform
                    // darkening. a=255 comes back exactly the stored colour, so
                    // an opaque image is untouched.
                    let v = (f64::from(data[i * 4 + c]) * a + 255.0 * (255.0 - a)) / 255.0;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        out[i * 3 + c] = js::round(v) as u8;
                    }
                }
            }
            Some(out)
        }
        1 => {
            // Packed 1-bit, and every ROW starts on a byte boundary. Ignoring
            // that padding smears each row progressively further than the
            // last.
            let row_bytes = (w + 7) >> 3;
            if data.len() < row_bytes.checked_mul(h)? {
                return None;
            }
            let mut out = vec![0u8; px * 3];
            for y in 0..h {
                for x in 0..w {
                    // Bits run MSB-first, so pixel 0 is bit 7 of its byte.
                    // 1 is WHITE: zero maps to black and one to white. A
                    // /Decode that inverts has already been applied to these
                    // bytes, so honoring it again here would come out as a
                    // negative.
                    let bit = (data[y * row_bytes + (x >> 3)] >> (7 - (x & 7))) & 1;
                    let v = if bit == 1 { 255 } else { 0 };
                    let o = (y * w + x) * 3;
                    out[o] = v;
                    out[o + 1] = v;
                    out[o + 2] = v;
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Long edge, in pixels, of anything we carry.
///
/// A constant and NOT a format option, deliberately. An option here would
/// have to apply at render time to survive the app's `.fountain` cache
/// boundary, which would mean decoding a PNG as well as encoding one;
/// applied at parse time instead it would put a format option inside the
/// parser and break that invariant.
///
/// WHAT 1400 CLEARS: it is a size cap, not a panel measurement. It refuses
/// to carry a 4000px lookbook scan at full size, which is the point, and it
/// is comfortably over the width any figure is drawn at on the 6" class a
/// script is usually read on (a Paperwhite panel is 1236 across). It does
/// NOT clear every panel Method Reader ships to; the trade is deliberate rather
/// than overlooked, because raising the edge costs every book that carries
/// any picture at all, and past about 1670 the engine's own picture-folder
/// reader stops accepting its output.
pub const IMAGE_MAX_EDGE: usize = 1400;

/// Ceiling on the time ONE DOCUMENT may spend on pictures AT ALL: waiting
/// and work both. A document of scanned sheets is the expensive case (every
/// sheet a distinct full-page image, the encode cache never hitting), and
/// the budget covers convert, downscale and encode so that document stops
/// rather than running on. 60 images (MAX_IMAGES) at the measured worst case
/// of 241ms is 14.5s, inside this ceiling, and since both the cap and the
/// budget run in document order, exhausting the budget can never cost a
/// document a picture the cap would have kept. Placements skipped once it is
/// gone are counted in `ImageRun::failed`, so the loss is reported rather
/// than silent.
pub const IMAGE_BUDGET_MS: f64 = 20_000.0;

/// One picture's encoded pixels: everything a candidate needs that came from
/// the image itself rather than from where it was painted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedImage {
    /// The PNG. Empty when the picture was judged on its facts alone (an
    /// oracle dump that names its pixels by hash instead of carrying them).
    pub bytes: Vec<u8>,
    pub hash: String,
}

/// A candidate with its pixels attached: what artwork.rs judges, plus what
/// survives into the book if it clears.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodedCandidate {
    pub candidate: ImageCandidate,
    pub bytes: Vec<u8>,
}

impl Borrow<ImageCandidate> for EncodedCandidate {
    fn borrow(&self) -> &ImageCandidate {
        &self.candidate
    }
}

/// What one DOCUMENT's worth of image collection carries between pages.
///
/// All three fields are document-scoped, which is why they travel together:
/// the budget ceiling is per document (see IMAGE_BUDGET_MS), the encode
/// cache is what stops a logo on ninety sheets being encoded ninety times,
/// and the failure count is only meaningful as a total.
#[derive(Debug, Clone)]
pub struct ImageRun {
    /// Milliseconds left for the whole document to spend on pictures.
    pub remaining_ms: f64,
    /// Encoded pixels by objId, so a picture placed on many sheets is
    /// encoded once per id rather than once per sheet. pdf.js renames a
    /// picture when it promotes it to the document-wide store on the second
    /// sheet it appears on, so a repeated picture is two ids and two encodes.
    /// SUCCESSES only.
    pub encoded: HashMap<String, EncodedImage>,
    /// Placements that yielded no candidate: pixels that never arrived,
    /// pixels we could not convert, an encode that failed, or a placement
    /// skipped because the budget was gone. Counted rather than merely
    /// skipped, because the failures that matter are total, and this is the
    /// number that tells "no art and no error" apart from "never had
    /// pictures".
    pub failed: u32,
}

/// `newImageRun`.
pub fn new_image_run() -> ImageRun {
    ImageRun {
        remaining_ms: IMAGE_BUDGET_MS,
        encoded: HashMap::new(),
        failed: 0,
    }
}

/// FNV-1a over the encoded bytes: cheap, and only ever compared, never
/// stored. 32 bits is ample for the comparison population, which is one
/// document's pictures.
pub fn hash_bytes(b: &[u8]) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for &x in b {
        h ^= u32::from(x);
        h = h.wrapping_mul(0x0100_0193);
    }
    format!("{h:x}")
}

/// The identity of a picture known only by its facts: the same source
/// pixels at the same size and kind encode to the same PNG, so this keys
/// the furniture rule exactly as the PNG's hash would.
fn facts_hash(img: &ImageFact, sha: &str) -> String {
    format!("{}:{}x{}:{sha}", img.kind, img.width, img.height)
}

/// Whether `to_rgb` would accept a picture known only by its facts.
fn facts_convert(img: &ImageFact) -> bool {
    let (width, height) = (img.width, img.height);
    if !js::is_integer(width) || !js::is_integer(height) || width < 1.0 || height < 1.0 {
        return false;
    }
    let Some(len) = img.data_length else {
        return false;
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (w, h) = (width as usize, height as usize);
    let need = match img.kind {
        2 => w.checked_mul(h).and_then(|p| p.checked_mul(3)),
        3 => w.checked_mul(h).and_then(|p| p.checked_mul(4)),
        1 => ((w + 7) >> 3).checked_mul(h),
        _ => None,
    };
    need.is_some_and(|n| len >= n)
}

/// Resolve one placement's pixels and encode them, or `None` if anything
/// refused. The blast radius of any step has to be ONE image rather than
/// the whole book.
fn encode_placement(
    page: &PageFacts,
    obj_id: &str,
    run: &mut ImageRun,
    fetch: Option<&FetchImage<'_>>,
) -> Option<EncodedImage> {
    let started = Instant::now();
    let result = (|| {
        // Pixels not read with the page are fetched now, inside the timed
        // region: decoding is part of what the budget charges, as waiting
        // for pdf.js's pixels was.
        let fetched;
        let img = match page.images.get(obj_id) {
            Some(img) => img.as_ref()?,
            None => {
                fetched = fetch?(obj_id)?;
                &fetched
            }
        };
        if let Some(data) = img.data.as_deref() {
            let rgb = to_rgb(img.width, img.height, img.kind, Some(data))?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let small =
                downscale_rgb(rgb, img.width as usize, img.height as usize, IMAGE_MAX_EDGE).ok()?;
            let bytes = encode_png(&small.rgb, small.width, small.height).ok()?;
            let hash = hash_bytes(&bytes);
            return Some(EncodedImage { bytes, hash });
        }
        // No pixels in hand: an oracle dump, which names them by hash.
        let sha = img.data_sha256.as_deref()?;
        if !facts_convert(img) {
            return None;
        }
        Some(EncodedImage {
            bytes: Vec::new(),
            hash: facts_hash(img, sha),
        })
    })();
    // Charged on every exit: the conversion, the downscale and the deflate,
    // whether this placement produced a picture, gave up on one, or failed.
    // A document's ceiling is meaningless if the expensive half of the work
    // is free.
    run.remaining_ms -= started.elapsed().as_secs_f64() * 1000.0;
    result
}

/// Every image on one page, encoded and ready for the document-wide
/// filters.
///
/// Encoding happens here rather than after filtering because the furniture
/// rule compares ENCODED bytes: the same logo is frequently a different
/// XObject on every sheet, and identical pixels are the fact worth acting
/// on.
///
/// `text_lines` is the entire discriminator between a designed title card
/// and a watermark behind a full page, and `page_num` is what the furniture
/// rule counts distinct values of.
pub fn collect_page_images(
    page: &PageFacts,
    ops: Option<&[Op]>,
    page_num: u32,
    text_lines: usize,
    run: &mut ImageRun,
) -> Vec<EncodedCandidate> {
    collect_page_images_with(page, ops, page_num, text_lines, run, None)
}

/// Where a picture's pixels come from when the page's facts do not carry
/// them: the PDF layer, asked by objId only once a picture is wanted.
pub type FetchImage<'a> = dyn Fn(&str) -> Option<ImageFact> + 'a;

/// [`collect_page_images`], fetching through `fetch` the pixels of any
/// placement `page.images` has no entry for (absent `fetch`, such a
/// placement has no pixels and counts as failed).
pub fn collect_page_images_with(
    page: &PageFacts,
    ops: Option<&[Op]>,
    page_num: u32,
    text_lines: usize,
    run: &mut ImageRun,
    fetch: Option<&FetchImage<'_>>,
) -> Vec<EncodedCandidate> {
    let mut candidates = Vec::new();
    let Some(ops) = ops else { return candidates };

    for p in collect_image_placements(ops) {
        let encoded = if let Some(hit) = run.encoded.get(&p.obj_id) {
            hit.clone()
        } else {
            // Out of budget means stop WAITING, not stop working, and the
            // difference is the cache: a picture whose bytes are already in
            // hand costs nothing to place, so an exhausted budget must not
            // throw away a logo this document has already paid for. Only an
            // uncached placement is skipped, and it is counted, because it is
            // a picture we failed to carry. `continue`, not `break`.
            if run.remaining_ms <= 0.0 {
                run.failed += 1;
                continue;
            }
            let Some(encoded) = encode_placement(page, &p.obj_id, run, fetch) else {
                run.failed += 1;
                continue;
            };
            run.encoded.insert(p.obj_id.clone(), encoded.clone());
            encoded
        };

        candidates.push(EncodedCandidate {
            candidate: ImageCandidate {
                page_num,
                x0: p.x0,
                y0: p.y0,
                x1: p.x1,
                y1: p.y1,
                src_px_width: p.src_px_width,
                src_px_height: p.src_px_height,
                hash: encoded.hash,
                page_text_lines: text_lines,
            },
            bytes: encoded.bytes,
        });
    }
    candidates
}

/// Deterministic filename inside the picture folder (`<stem>.images/`), so
/// re-converting reproduces byte for byte.
///
/// NOT public, and that is the whole design: `RawImage::file` is the one
/// filename anything downstream ever needs, and a second caller would mean a
/// second per-page counter to keep in step with this one.
fn image_file_name(page_num: u32, index_on_page: u32) -> String {
    format!("p{page_num:03}-{index_on_page:02}.png")
}

/// True for a name the line above could have produced, and nothing else.
///
/// Public where `image_file_name` is not, because this asks a different
/// question and a caller outside the parser genuinely needs it: whoever
/// REWRITES the picture folder has to know which files in it are its own,
/// and "any name a note could carry" is not that set. Pruning against the
/// wide filename allowlist in notes would delete the user's pictures out of a
/// folder they filled themselves.
///
/// `\d{3,}` and `\d{2,}`: padding never truncates, so a 1200-page shooting
/// script produces `p1200-01.png`. Anchored at both ends, with no separator
/// in the class, so it can only ever match a bare filename.
pub fn is_generated_image_file(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('p') else {
        return false;
    };
    let Some(rest) = rest.strip_suffix(".png") else {
        return false;
    };
    let Some((page, index)) = rest.split_once('-') else {
        return false;
    };
    page.len() >= 3
        && index.len() >= 2
        && page.bytes().all(|b| b.is_ascii_digit())
        && index.bytes().all(|b| b.is_ascii_digit())
}

/// Kept candidates as `RawImage`, in reading order and named for where they
/// sit.
///
/// SORTED here (page ascending, y descending — PDF user space puts the top
/// of a sheet at the high y), and not left in paint order: `insert_images`
/// merges this array against the element stream on exactly that ordering.
/// The sort is after `keep_artwork`'s cap and cannot change WHICH images
/// survive, only their order; equal-y placements keep their paint order,
/// since the sort is stable.
pub fn to_raw_images(
    kept: &[EncodedCandidate],
    page_width: f64,
    page_height: f64,
) -> Vec<RawImage> {
    let mut sorted: Vec<&EncodedCandidate> = kept.iter().collect();
    js::sort_by(&mut sorted, |a, b| {
        a.candidate
            .page_num
            .cmp(&b.candidate.page_num)
            .then_with(|| js::cmp_num(b.candidate.y1, a.candidate.y1))
    });
    let mut per_page: HashMap<u32, u32> = HashMap::new();
    sorted
        .into_iter()
        .map(|c| {
            let n = per_page.entry(c.candidate.page_num).or_insert(0);
            *n += 1;
            RawImage {
                file: image_file_name(c.candidate.page_num, *n),
                page_num: c.candidate.page_num,
                // TOP edge, so a plain descending sort interleaves images with
                // lines.
                y: c.candidate.y1,
                page_width_pct: ((c.candidate.x1 - c.candidate.x0) / page_width) * 100.0,
                page_height_pct: ((c.candidate.y1 - c.candidate.y0) / page_height) * 100.0,
                bytes: c.bytes.clone(),
            }
        })
        .collect()
}

/// What became of the pictures, for a caller that wants to say so.
///
/// Three of the four fields are USER-FACING (the engine turns `over_cap` and
/// `failed` into a warning each). `dropped_by` is the per-rule histogram a
/// retune needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStats {
    /// Placements that yielded no candidate at all — see `ImageRun::failed`.
    pub failed: u32,
    /// True only when the PDF layer's image decoders could not be loaded,
    /// which makes every JPEG2000, JBIG2 and CCITT picture fail at once.
    /// Absent on the happy path rather than false: it is a fact about the
    /// machine, not the document. The Rust decoders are compiled in, so
    /// nothing sets it today; it stays so the `--json` answer keeps its shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decoders_unavailable: Option<bool>,
    /// Cleared every filter and lost to the document cap alone.
    pub over_cap: u32,
    /// Which rule refused each dropped candidate, keyed by the first to fire.
    pub dropped_by: DroppedBy,
}

/// A page's size, for `dominant_page_size`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f64,
    pub height: f64,
}

/// The geometry the document is mostly made of, for the rules that need ONE.
///
/// `keep_artwork` and `to_raw_images` measure every image against a single
/// page size. Given that, the question is only which page to take it from,
/// and the most common one is the least bad answer: on a uniform script —
/// nearly all of them — it is exact, and on a production draft carrying one
/// landscape storyboard insert it confines the error to that sheet instead
/// of measuring the whole document against whichever page happened to be
/// last. Ties go to the first seen, so this is deterministic on a document
/// with no majority at all.
///
/// Zeros for an empty list, which `keep_artwork` refuses to judge on rather
/// than dividing by.
pub fn dominant_page_size(pages: &[PageSize]) -> PageSize {
    // Keyed as `${width}x${height}` was: by value, with -0 and 0 one key.
    let key = |p: &PageSize| ((p.width + 0.0).to_bits(), (p.height + 0.0).to_bits());
    let mut tally: Vec<(PageSize, usize)> = Vec::new();
    for p in pages {
        match tally.iter_mut().find(|(s, _)| key(s) == key(p)) {
            Some((_, n)) => *n += 1,
            None => tally.push((*p, 1)),
        }
    }
    let mut best = (
        PageSize {
            width: 0.0,
            height: 0.0,
        },
        0,
    );
    for &(size, n) in &tally {
        if n > best.1 {
            best = (size, n);
        }
    }
    best.0
}

/// Everything about the pictures that cannot be decided until every page
/// has been read: which geometry the whole document is measured against,
/// which page count the furniture share divides by, and how the counts add
/// up. The join between the two halves of this feature.
pub fn finish_images(
    candidates: &[EncodedCandidate],
    page_sizes: &[PageSize],
    page_count: u32,
    run: &ImageRun,
) -> (Vec<RawImage>, ImageStats) {
    let PageSize { width, height } = dominant_page_size(page_sizes);
    let verdict = keep_artwork(candidates, width, height, page_count);
    let images = to_raw_images(&verdict.kept, width, height);
    let stats = ImageStats {
        failed: run.failed,
        decoders_unavailable: None,
        over_cap: u32::try_from(verdict.over_cap).unwrap_or(u32::MAX),
        dropped_by: verdict.dropped_by,
    };
    (images, stats)
}

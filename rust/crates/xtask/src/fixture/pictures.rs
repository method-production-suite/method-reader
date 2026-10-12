//! The picture-bearing kinds: `images`, `jpx`, and torture's pictures.
//! Re-exported from the fixture module, so `xtask::fixture::IMAGES` and the
//! rest keep the names make-fixture.py gave them.

use std::io::Write;

use super::{
    Fail, LINES_PER_PAGE, PAGE_H, PAGE_W, PT, Row, content_stream, f2, textwrap, title_stream,
};

// --- the images kind -------------------------------------------------------
// Six pictures, one per rule in the six-rules bullet of
// docs/superpowers/specs/2026-10-01-images-in-the-book-design.md, section
// "What the August branch already settled". Two are art and survive; four
// are furniture of one sort or another and must be dropped. Everything a
// rule keys on is geometry, so the numbers below are the fixture: change one
// and you change which rule fires.
//
// The thresholds, from the same spec (and registry #19), against a 612x792
// page: area 3%, width 10%, height 3%, 32 pixels on either edge; furniture
// at 3 pages, or at 25% of them once a picture is on at least two;
// background at 90% of a page carrying 20+ text lines; full-page at 70%.
// Every row below states its own margin to the ONE threshold that governs
// it, because a retune bites hardest on the KEPT images and a kept image
// quietly becoming dropped fails nothing here: the generator would still
// emit a perfectly good PDF.
//
//   imCard   sheet 1  522x702pt, 261x351px
//            KEPT, full page, cover candidate. 75.6% of the page: 5.6
//            points over the 70% that makes it full-page, and 14 under the
//            90% the background rule needs, on a sheet of 5 text lines, 15
//            under that rule's 20. Clears every size floor by an order of
//            magnitude.
//   imPanel  sheet 2  180x135pt, 240x180px
//            KEPT, inline. 5.0% of the page against the 3% area floor, 29%
//            wide against 10%, 17% high against 3%. The plan's original
//            144x108pt cleared area by 0.21 points, which is a knife edge
//            for a KEPT image; tests/artwork.test.ts owns the boundary case
//            with hand-built candidates, this one wants headroom.
//   imLogo   all 5 sheets  200x100pt, 100x50px
//            DROPPED: page furniture, on 5 sheets against a 3-sheet
//            threshold and 100% of them against 25%. It must clear every
//            SIZE floor so furniture is the only thing that can drop it,
//            and area is where it is closest: 4.1% against 3%.
//   imTiny   sheet 3  130x130pt, 16x16px
//            DROPPED: pixel floor, 16 against 32. Rendered big on purpose
//            so it clears all three size floors: 3.5% area against 3%, 21%
//            width against 10%, 16% height against 3%. That area margin is
//            0.5 points: shrink this box and the area floor starts catching
//            it instead, and a loosened pixel floor sails through unnoticed.
//            The thinnest margin in this table, so it is the first to check
//            if the area floor is ever retuned.
//   imSmall  sheet 3  20x20pt, 200x200px
//            DROPPED: rendered-size floor, and it fails all three at once:
//            0.08% area against 3%, 3.3% width against 10%, 2.5% height
//            against 3%. 200x200px so the pixel floor is demonstrably not
//            what caught it.
//   imMark   sheet 4  full bleed, 120x160px
//            DROPPED: background. 100% of a page against 90%, printed
//            beneath 29 text lines against the rule's 20. Clears the pixel
//            floor at 120x160 so that is not what caught it, and sits on one
//            sheet of five (20%, under the 25% furniture share) so that is
//            not either.
//
// One shared fragility that USED to be worth knowing, recorded because the
// arithmetic still looks alarming: imCard, imPanel and imMark are each on a
// single sheet of five, so each is at 20% against the 25% furniture share,
// and on four sheets each would be at exactly 25%. The share alone would
// make all three furniture at once. It does not, because the share needs a
// picture on at least two sheets before it can fire at all (artwork.ts,
// FURNITURE_SHARE_MIN_PAGES): repetition needs something to repeat against,
// and without that floor a lone picture in a four-page short would be read
// as letterhead. So the sheet count is free to change here, down to two:
// imLogo is on EVERY sheet, so it is caught by the 3-sheet rule at five and
// by the share at two, and only a one-sheet fixture would leave it as art.

/// name -> (px width, px height, gradient seed).
///
/// The seeds only have to DIFFER. Two images of identical dimensions and
/// identical pixels would encode to identical PNGs, hash alike, and be read
/// as one repeated picture, which is to say, as furniture.
pub const IMAGES: &[(&str, (usize, usize, usize))] = &[
    ("imCard", (261, 351, 17)),
    ("imPanel", (240, 180, 53)),
    ("imLogo", (100, 50, 101)),
    ("imTiny", (16, 16, 149)),
    ("imSmall", (200, 200, 197)),
    ("imMark", (120, 160, 233)),
];

/// The same corner of every sheet, in the bottom margin where no line of
/// the script reaches: a letterhead logo that the furniture rule must catch
/// on repetition alone, not on being small or badly placed.
pub const LOGO_W: f64 = 200.0;
pub const LOGO_H: f64 = 100.0;

/// (name, x, y, w, h) in PDF points, origin bottom-left.
pub type Placement = (&'static str, f64, f64, f64, f64);

pub const LOGO: Placement = (
    "imLogo",
    PAGE_W - LOGO_W - 0.5 * PT,
    0.5 * PT,
    LOGO_W,
    LOGO_H,
);

/// (name, x, y, w, h) per sheet, drawn in this order and therefore UNDER
/// the text, which is where a watermark and a title card both live in a
/// real PDF.
pub const SHEET_PLACEMENTS: &[&[Placement]] = &[
    &[("imCard", 45.0, 45.0, 522.0, 702.0), LOGO],
    &[("imPanel", 108.0, 561.0, 180.0, 135.0), LOGO],
    &[
        ("imTiny", 108.0, 400.0, 130.0, 130.0),
        ("imSmall", 300.0, 440.0, 20.0, 20.0),
        LOGO,
    ],
    &[("imMark", 0.0, 0.0, PAGE_W, PAGE_H), LOGO],
    &[LOGO],
];

pub const IMAGE_TITLE: &[(f64, &str)] = &[
    (4.0, "THE PAPER LANTERN"),
    (4.5, "Written by"),
    (5.0, "J. Q. Nobody"),
];

/// A row of a picture sheet: a script element, or n blank lines reserved
/// for a picture. The text and the image have to be positioned in one
/// coordinate system, or the sheet looks right to a rule and wrong to a
/// reader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SheetItem {
    El(&'static str, &'static str),
    Gap(usize),
}

use SheetItem::{El, Gap};

/// Sheets 2-5 of the images kind.
pub const IMAGE_SHEETS: &[&[SheetItem]] = &[
    // sheet 2: the panel sits in the gap, under the scene heading
    &[
        El("scene", "INT. ARCHIVE ROOM - DAY"),
        // The panel spans y 561..696, so its bottom edge falls between the
        // baselines of rows 13 and 14. Reserving 13 rows puts the action at
        // row 15, clearing the picture by a full line rather than grazing it.
        Gap(13),
        El(
            "action",
            "WREN, 30s, holds a storyboard panel up to the window \
             and squints at it.",
        ),
        El("character", "WREN"),
        El(
            "dialogue",
            "Somebody drew this in 1974 and nobody has looked at \
             it since.",
        ),
        El("action", "HOLT sets down a box of film cans."),
        El("character", "HOLT"),
        El("dialogue", "That is the job."),
    ],
    // sheet 3: text stops well above the two undersized pictures
    &[
        El("scene", "INT. ARCHIVE ROOM - PROJECTION BOOTH - LATER"),
        El(
            "action",
            "A splice reel turns. The room smells of vinegar and \
             dust.",
        ),
        El("character", "HOLT"),
        El(
            "dialogue",
            "Two frames are missing from the head of the roll.",
        ),
        El("character", "WREN"),
        El("dialogue", "Then we cut around them."),
        El(
            "action",
            "She threads the leader by feel, the way she was \
             taught.",
        ),
    ],
    // sheet 4: a full page of text over the watermark. The line COUNT is the
    // point: the background rule needs 20 or more, and this sheet lands at
    // 29 as the parser groups them (28 rows plus the page number). Cut TEN
    // lines here and the watermark becomes art: at 20 it is still
    // background, because the rule fires at 20 or more.
    &[
        El("scene", "EXT. LOADING DOCK - NIGHT"),
        El(
            "action",
            "Rain on corrugated steel. WREN wheels a cart of film \
             cans out under a sodium lamp and counts them twice.",
        ),
        El(
            "action",
            "HOLT holds an umbrella over the cart and not over \
             himself.",
        ),
        El("character", "WREN"),
        El(
            "dialogue",
            "Forty-one. There were forty-three when we started.",
        ),
        El("character", "HOLT"),
        El(
            "dialogue",
            "Two of them went out for repair in March and nobody \
             wrote it down.",
        ),
        El("character", "WREN"),
        El("dialogue", "Nobody ever writes it down."),
        El(
            "action",
            "She pulls a tarp over the cart and ties it at the \
             corners.",
        ),
        El("character", "HOLT"),
        El(
            "dialogue",
            "The truck comes at six. Everything on that manifest \
             has to be on the dock by then, in order, and dry.",
        ),
        El("character", "WREN"),
        El("dialogue", "Then we work through it."),
        El(
            "action",
            "The lamp buzzes and cuts out. Neither of them stops \
             moving.",
        ),
        El(
            "action",
            "In the dark the cart is a long shape and the rain is \
             louder than it was.",
        ),
        El("character", "WREN"),
        El(
            "dialogue",
            "Bring the second cart. And the good tarp, not the \
             one with the hole in it.",
        ),
        El(
            "action",
            "Holt goes. Wren stands in the rain with a clipboard \
             and does not hurry.",
        ),
    ],
    // sheet 5: text only, so the logo is furniture on a sheet with no art
    &[
        El("scene", "INT. ARCHIVE ROOM - DAWN"),
        El(
            "action",
            "The cart is gone. A single storyboard panel is still \
             pinned to the window.",
        ),
        El("character", "WREN"),
        El("dialogue", "Leave that one."),
        El("trans", "FADE OUT."),
    ],
];

/// Deterministic synthetic pixels, RGB. Never a real photograph: fixtures
/// are public and a real storyboard panel is somebody's confidential art.
///
/// Blocky rather than smooth, and deliberately so. Eight-pixel runs give
/// deflate something to match, which is what keeps this fixture tens of KB
/// instead of hundreds; varying in BOTH axes is what keeps it able to catch
/// a transposed or reversed row downstream, which a striped image could
/// not.
pub fn rgb_gradient(w: usize, h: usize, seed: usize) -> Vec<u8> {
    let mut px = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            px.push((((x / 8) * 37 + seed) % 256) as u8);
            px.push((((y / 8) * 53 + seed) % 256) as u8);
            px.push(((seed * 3) % 256) as u8);
        }
    }
    px
}

/// Python's `zlib.compress(data, 9)`.
///
/// zlib-rs at level 9 writes the same stream byte for byte as zlib 1.2.12
/// (the Mac the fixture was authored on) and 1.2.13 (Linux, checked
/// 2026-10-01), for every picture here. The deflate format pins only what
/// decompresses back out, not how, so if images.pdf ALONE ever stops
/// matching, suspect a zlib-rs bump before the emitter: inflate both sides
/// and compare the PIXELS, which are what this file actually promises.
pub fn zlib_compress_9(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(9));
    encoder
        .write_all(data)
        .expect("deflating into memory cannot fail");
    encoder.finish().expect("deflating into memory cannot fail")
}

/// A FlateDecode RGB image XObject body, ready for the object table.
///
/// FlateDecode rather than DCTDecode so the generator stays dependency-light
/// and the bytes are reproducible. pdf.js hands both back as the same
/// decoded RGB pixels, so the code path under test is identical either way.
pub fn image_xobject(w: usize, h: usize, seed: usize) -> Vec<u8> {
    let data = zlib_compress_9(&rgb_gradient(w, h, seed));
    let mut out = format!(
        "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode \
         /Length {} >>",
        data.len()
    )
    .into_bytes();
    out.extend_from_slice(b"\nstream\n");
    out.extend_from_slice(&data);
    out.extend_from_slice(b"\nendstream");
    out
}

/// Content-stream operators drawing `name` at (x,y) with size (w,h), in PDF
/// points with the origin bottom-left.
///
/// An image XObject always occupies the unit square, so this `cm` matrix IS
/// the placement the parser reads back out of the operator list.
pub fn place(p: &Placement) -> String {
    let (name, x, y, w, h) = *p;
    format!(
        "q {} 0 0 {} {} {} cm /{name} Do Q\n",
        f2(w),
        f2(h),
        f2(x),
        f2(y)
    )
}

fn spec_of(specs: &[(&'static str, (usize, usize, usize))], name: &str) -> (usize, usize, usize) {
    specs
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("KeyError: {}", super::py::repr(name)))
        .1
}

/// name -> XObject body, for every picture the images kind can draw.
pub fn xobject_bodies() -> Vec<(String, Vec<u8>)> {
    IMAGES
        .iter()
        .map(|(name, (w, h, seed))| ((*name).to_string(), image_xobject(*w, *h, *seed)))
        .collect()
}

/// Sheet items -> content_stream rows, with Gap(n) reserving space.
///
/// A cue or a parenthetical prints directly above what it introduces, as in
/// flow_torture; everything else gets its blank line. `sheet` is carried
/// only so the overflow message can say which one is too long.
pub fn image_rows(items: &[SheetItem], sheet: usize) -> Result<Vec<Row>, Fail> {
    let mut rows: Vec<Row> = Vec::new();
    for item in items {
        match *item {
            Gap(n) => rows.extend(std::iter::repeat_n(None, n)),
            El(kind, text) => {
                let mut chunks = textwrap::wrap(text, super::wrap_of(kind));
                if chunks.is_empty() {
                    chunks.push(String::new());
                }
                for chunk in chunks {
                    rows.push(Some((super::x_of(kind), chunk)));
                }
                if !matches!(kind, "character" | "paren") {
                    rows.push(None);
                }
            }
        }
    }
    if rows.len() > LINES_PER_PAGE {
        // Failing rather than overflowing, for flow_torture's reason: rows
        // past the last one just draw at negative y, off the sheet, and the
        // fixture would still generate, still open, and still pass byte
        // stability while quietly testing less than it claims to.
        return Err(format!(
            "images sheet {sheet} needs {} lines and a page holds {LINES_PER_PAGE}. \
             Text would run through the bottom margin and off the sheet: shorten it, \
             or move content to a new sheet.",
            rows.len()
        ));
    }
    Ok(rows)
}

/// Sheet 1 is the title page and carries no scripted rows; sheets 2-5 are
/// IMAGE_SHEETS, numbered the way a script is (first page unnumbered).
pub fn image_streams() -> Result<Vec<String>, Fail> {
    let mut pages = vec![title_stream(IMAGE_TITLE)];
    for (i, items) in IMAGE_SHEETS.iter().enumerate() {
        let rows = image_rows(items, i + 2)?;
        pages.push(content_stream(
            &rows,
            if i > 0 { Some(i + 1) } else { None },
        ));
    }
    Ok(pages
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut out: String = SHEET_PLACEMENTS[i].iter().map(place).collect();
            out.push_str(s);
            out
        })
        .collect())
}

/// What each sheet of the images kind draws, from the placement table the
/// content streams are drawn from.
pub fn image_page_xobjects() -> Vec<Vec<&'static str>> {
    SHEET_PLACEMENTS
        .iter()
        .map(|sheet| sheet.iter().map(|p| p.0).collect())
        .collect()
}

// --- the jpx kind ----------------------------------------------------------
// One picture, in the one image format pdf.js cannot decode out of its own
// bundle. JPEG2000 (/JPXDecode) goes to a separate decoder module that pdf.js
// loads AT RUNTIME from the directory given by its `wasmUrl` option, and
// with no such directory every JPX image in a document fails at once,
// silently, since a picture that will not decode is simply a picture the
// book does not have. A real 110-page script-plus-lookbook lost all 34 of
// its pictures that way. src/parser/pdfjs-wasm.ts is the fix; this fixture
// is what notices if it ever stops working.
//
// Deliberately its OWN kind rather than a seventh picture in `images`. The
// blast radius is the point: JPX either decodes or it does not, and when it
// does not, one small fixture should fail and say so, rather than a dozen
// assertions about rules that never came into it.
//
// The picture is 64x48 source pixels drawn at 180x135pt on a 612x792 sheet,
// which is imPanel's geometry from the images kind and clears every filter
// the same way: 5.0% of the page against the 3% area floor, 29% wide against
// 10%, 17% high against 3%, 64 and 48 pixels against the floor of 32. It
// sits on one sheet of two, and the furniture share needs a picture on at
// least two sheets before it can fire, so repetition cannot catch it either.
// It must come out KEPT, at 64x48, or the fixture is testing nothing.

/// w, h, seed: the pixels JPX_CODESTREAM decodes to.
pub const JPX_GRADIENT: (usize, usize, usize) = (64, 48, 71);

/// A JPEG2000 codestream, base64. NOT generated at run time, unlike every
/// other picture here: a J2K encoder is a wavelet transform and an
/// arithmetic coder, far past what this generator is allowed to be, and
/// shelling out to one would make the fixture unbuildable on a machine that
/// lacks it. So the bytes are committed, and here is exactly how they were
/// made, from this module's own rgb_gradient, so the pixels are the same
/// invented blocks as every other fixture picture and can be regenerated by
/// anyone who has openjpeg:
///
/// ```text
/// # P6 header (64 48 255) followed by rgb_gradient(64, 48, 71) > grad.ppm
/// opj_compress -i grad.ppm -o grad.j2k     # lossless, its default
/// base64 grad.j2k
/// ```
///
/// LOSSLESS matters and is not the only option opj_compress has: the test
/// asserts the decoded pixels equal rgb_gradient(64, 48, 71) byte for byte,
/// which is what proves the picture came through CORRECT rather than merely
/// present. A lossy codestream would still decode, still be kept, and still
/// look like a passing fixture while quietly checking nothing.
pub const JPX_CODESTREAM: &str = concat!(
    "/0//UQAvAAAAAABAAAAAMAAAAAAAAAAAAAAAQAAAADAAAAAAAAAAAAADBwEBBwEB",
    "BwEB/1IADAAAAAEBBQQEAAH/XAATQEBISFBISFBISFBISFBISFD/ZAAlAAFDcmVh",
    "dGVkIGJ5IE9wZW5KUEVHIHZlcnNpb24gMi41LjT/kAAKAAAAAALjAAH/k8fUCg9l",
    "5RR/z7QQA5c568+0FAxdcMxpw+oEh9QHACEOBmssAccfB6PtAwAIhX/PwBY+0DAO",
    "BvBcLwiFf8D5A8PqCoA6IBZ+fTtHPb8Lbb+7ZsUn9v9/EBq5c6PtCwAMYHkFM8xy",
    "lJzJK8PqCY+0LBZ+fVF7vgo5vwxgeQUzzHKUnMkrwfOYh9Q3ACInOmS3/F9+AiwA",
    "lNHZlZOYWgJwKrblbPkDhLcbwoYxD1AW+Dw48rCigLbffEFpTFREyYciX6PtHgAA",
    "1K8bwoYxD1AW+Dw48rCip2ZmMWrxDAPEdI6yAL/H2jsfaPAnOmS4YBt69JKZQmod",
    "pn0e8SO/I+hrFAK5jWJTNwDUrxvChjEPUBb4PDjysKKnZmYxavEMA8R0jrIAv8Hz",
    "m4fUWwAnQOBB4a9g1KwNnsgkkMMPCylfE/A5sCOEoBalZq5EcevvBVE2aj3l57pP",
    "xAyxaUMuGs+0yUnugo4RA4c3oghQjAujOtjexI2vJAsCPZh9k6PtKwBnaqq49feC",
    "qJs1HvLz3SfwxqBM3r8lSlKuJs0g7JZ9DZGGbPzJXZYRGMy/x9pTH2lYQOBB4a9g",
    "1KwNnsgkkLoSEv4q+cB8ICs/ti8NgAAYKdv8sX6omhL7/39naqq49feCqJs1HvLz",
    "3SfwxqBM3r8lSlKuJs0g7JZ9DZGGbPzJXZYRGMy/wfOph9Q6dCrHPc4VEu5q1PvB",
    "EOQlYBgNTD8qoPi+iXnKc+MLrJGtvjC6yRrb4x/ENkqeb9YOxzS5O477fMgAAAAw",
    "jWH2QWPTEhL/f6PtJQDEMOqOb9YOxzS5O477fMgAAAAwjWH2QWPTEhLYpcAGEd8p",
    "hIS/x9plH2kodCrHPdDCim5q1PvBEOQlYBgNTD8qoPi9ShnNZ4Li2o/tR/aj+1Hw",
    "FvkJfjvoAABhIS/EMOqOb9YOxzS5O477fMgAAAAwjWH2QWPTEhLYpcAGEd8phIS/",
    "/9k=",
);

/// Standard base64 (with padding) to bytes.
pub fn base64_decode(text: &str) -> Vec<u8> {
    fn value(c: u8) -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("not base64: {}", c as char),
        }
    }
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in text.as_bytes() {
        if c == b'=' {
            break;
        }
        acc = (acc << 6) | value(c);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// The JPEG2000 image XObject body.
///
/// No /ColorSpace and no /BitsPerComponent, which is not an omission: for
/// /JPXDecode the PDF spec makes both optional because the codestream
/// carries them, and leaving them out is what a real JPX-bearing PDF from a
/// layout app looks like. It also means the fixture exercises the branch
/// where pdf.js has to ASK the decoder how many components there are, which
/// it can only do if the decoder loaded, the whole point here.
pub fn jpx_xobject() -> Vec<u8> {
    let data = base64_decode(JPX_CODESTREAM);
    let (w, h, _) = JPX_GRADIENT;
    let mut out = format!(
        "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} \
         /Filter /JPXDecode /Length {} >>",
        data.len()
    )
    .into_bytes();
    out.extend_from_slice(b"\nstream\n");
    out.extend_from_slice(&data);
    out.extend_from_slice(b"\nendstream");
    out
}

pub const JPX_TITLE: &[(f64, &str)] = &[
    (4.0, "THE SECOND UNIT"),
    (4.5, "Written by"),
    (5.0, "J. Q. Nobody"),
];

/// One scripted sheet. Thirteen reserved rows put the action a full line
/// below the picture's bottom edge, exactly as sheet 2 of the images kind
/// does.
pub const JPX_SHEET: &[SheetItem] = &[
    El("scene", "INT. COLOUR TIMING BAY - NIGHT"),
    Gap(13),
    El(
        "action",
        "A reference print hangs under the loupe, one frame of it \
         circled in grease pencil.",
    ),
    El("character", "WREN"),
    El(
        "dialogue",
        "The blue is two points cold and it has been all week.",
    ),
    El("action", "HOLT writes the number down without looking up."),
    El("character", "HOLT"),
    El("dialogue", "Then we time it again."),
];

pub const JPX_PLACEMENT: Placement = ("imJpx", 108.0, 561.0, 180.0, 135.0);

pub fn jpx_streams() -> Result<Vec<String>, Fail> {
    let title = title_stream(JPX_TITLE);
    let sheet = content_stream(&image_rows(JPX_SHEET, 2)?, Some(1));
    Ok(vec![title, place(&JPX_PLACEMENT) + &sheet])
}

// --- torture's pictures ----------------------------------------------------
// Five pictures, so a device pass can ask its picture questions of the same
// book as every other row (registry #19 for which are kept, #19a for how the
// book shows them). Every fraction below is against the 612x792 sheet, and
// `columnWidthPct` remaps a page share to a COLUMN share by dividing by
// 0.706 (artwork.ts), which is where each `w=` comes from.
//
// The four that are not the letterhead deliberately do NOT repeat: one sheet
// each, so the furniture rule cannot reach them and every one must arrive as
// art. None has transparency (every picture here is plain RGB), so this
// fixture cannot show how a reader composites one onto a dark page.

/// Pictures only torture draws, kept out of IMAGES so images.pdf does not
/// gain unused objects and its bytes stay what tests/fixture.rs pins.
/// name -> (px width, px height, gradient seed).
pub const TORTURE_IMAGES: &[(&str, (usize, usize, usize))] = &[
    // The full-page plate under a scene heading. Its seed differs from
    // every other picture's so no two encode to the same PNG.
    ("imPlate", (279, 333, 241)),
    // The tall plate. Its SOURCE is 1:2.75, which alone decides how tall it
    // renders: the book sets a width and lets the height follow the aspect.
    ("imTall", (200, 550, 211)),
];

/// The title card, behind the title rows exactly as images.pdf draws it:
/// 522x702pt is 75.6% of the sheet, 5.6 points over the 70% that makes it
/// full-page and 14 under the background rule's 90%. It stands ahead of
/// every scene heading, so the book opens on it in a section of its own.
pub const TORTURE_CARD: Placement = ("imCard", 45.0, 45.0, 522.0, 702.0);

/// 180pt wide is 29.4% of the sheet and 41.6% of the text column, so the
/// note reads w=42. Area 5.0% against the 3% floor, both edges far over the
/// 32px minimum. Spans y 570..705, under the heading's row and inside the
/// rows torture_content.rs reserves with an AtLine.
pub const TORTURE_PANEL: Placement = ("imPanel", 108.0, 570.0, 180.0, 135.0);

/// 558x666pt is 91.2% x 84.1% of the sheet: 76.7% of its area, 6.7 points
/// over the full-page mark and 13.3 under the background rule's 90%, on a
/// sheet of two text lines against that rule's 20. 91.2% of the sheet is
/// 129% of the column, so the note reads w=100 full. Its top edge at 702
/// sits below the heading's baseline at 720, which is what puts the heading
/// first.
pub const TORTURE_PLATE: Placement = ("imPlate", 27.0, 36.0, 558.0, 666.0);

/// 240x660pt is 39.2% x 83.3% of the sheet: 32.7% of its area, far UNDER
/// the 70% full-page mark, so it stays in the flow with no forced break to
/// hide behind. w=56, and at 1:2.75 that renders taller than the page on a
/// 3:4 e-reader screen. Its top edge at 672 clears the action's baseline at
/// 696.
pub const TORTURE_TALL: Placement = ("imTall", 186.0, 12.0, 240.0, 660.0);

/// (name, x, y, w, h) per sheet, drawn UNDER the text like a real PDF.
///
/// Built from the flowed page count rather than written out, so a sheet
/// added earlier in torture_content.rs cannot leave the last three pictures
/// on the wrong pages: the panel, the plate and the tall plate are always
/// the last three sheets, which is where that file puts them.
pub fn torture_placements() -> Result<Vec<Vec<Placement>>, Fail> {
    let n_pages = 1 + super::flow_torture(&super::torture_content::content())?.len();
    if n_pages < 4 {
        return Err("torture_placements: need a title sheet and the three picture sheets".into());
    }
    // The letterhead the furniture rule has to catch on REPETITION alone: on
    // every sheet, in the bottom margin, and large enough (200x100pt) to
    // clear every size floor. Seventeen sheets is far past the 3-sheet rule.
    let mut sheets: Vec<Vec<Placement>> = (0..n_pages).map(|_| vec![LOGO]).collect();
    sheets[0].insert(0, TORTURE_CARD);
    sheets[n_pages - 3].insert(0, TORTURE_PANEL);
    sheets[n_pages - 2].insert(0, TORTURE_PLATE);
    sheets[n_pages - 1].insert(0, TORTURE_TALL);
    Ok(sheets)
}

/// Only the names torture actually draws, from IMAGES and TORTURE_IMAGES
/// both, sorted by name. Apart from `xobject_bodies()` so images.pdf stays
/// as it is.
pub fn torture_xobject_bodies() -> Result<Vec<(String, Vec<u8>)>, Fail> {
    let specs: Vec<(&'static str, (usize, usize, usize))> =
        IMAGES.iter().chain(TORTURE_IMAGES).copied().collect();
    let drawn: std::collections::BTreeSet<&str> = torture_placements()?
        .iter()
        .flatten()
        .map(|p| p.0)
        .collect();
    Ok(drawn
        .into_iter()
        .map(|name| {
            let (w, h, seed) = spec_of(&specs, name);
            (name.to_string(), image_xobject(w, h, seed))
        })
        .collect())
}

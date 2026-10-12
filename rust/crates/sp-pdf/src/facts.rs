//! The parser's input, `sp_parser::facts::DocumentFacts`: what this crate
//! reads, in the exact shape of the oracle's `pages.json`, with each
//! picture's pixels attached in memory.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use sha2::{Digest, Sha256};
use sp_parser::facts::{DocumentFacts, ImageFact, Op, PageFacts, PaintOp, TextItemFact};

use crate::{Document, LoadError, PageData, PageOp, Reader};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Reads a whole PDF into the parser's facts.
///
/// A page whose reading panics inside hayro-syntax (none known: the one
/// found by `examples/mutate.rs` is fixed) comes back as a page with no text
/// and no operator list, the same as a page pdf.js could not read, rather
/// than taking the conversion down.
pub fn read(bytes: Vec<u8>) -> Result<DocumentFacts, LoadError> {
    let doc = catch_unwind(|| Document::load(bytes)).map_err(|_| LoadError::Invalid)??;
    Ok(read_document(&doc))
}

/// [`read`] over an open document: every page, then every picture's pixels.
pub fn read_document(doc: &Document) -> DocumentFacts {
    let mut reader = doc.reader();
    let mut facts = read_pages(doc, &mut reader);
    for page in &mut facts.pages {
        let ids: Vec<String> = page
            .ops
            .iter()
            .flatten()
            .filter_map(|op| match op {
                Op::PaintImageXObject {
                    obj_id: Some(id), ..
                } => Some(id.clone()),
                _ => None,
            })
            .collect();
        for id in ids {
            page.images
                .entry(id)
                .or_insert_with_key(|id| image_fact(&reader, id));
        }
    }
    facts
}

/// The text half of [`read_document`]: every page's items, fonts and
/// operators, with NO picture decoded (each page's `images` is empty). The
/// pixels stay in `reader` for [`image_fact`], so a caller can decode them
/// only once it knows it wants them: the engine refuses a scan or a
/// document that is not a screenplay on the text alone, and a refused
/// document must not pay for its pictures.
pub fn read_pages(doc: &Document, reader: &mut Reader<'_>) -> DocumentFacts {
    let mut pages = Vec::new();
    for i in 0..doc.num_pages() {
        let page = catch_unwind(AssertUnwindSafe(|| reader.page(i)));
        pages.push(match page {
            Ok(p) => page_facts(p, BTreeMap::new()),
            Err(_) => PageFacts {
                page_num: i as u32 + 1,
                width: 0.0,
                height: 0.0,
                items: Vec::new(),
                fonts: BTreeMap::new(),
                ops: None,
                images: BTreeMap::new(),
            },
        });
    }
    DocumentFacts {
        num_pages: doc.num_pages() as u32,
        pages,
    }
}

/// One painted picture's pixels, decoded the way pdf.js decodes them, as
/// the parser's [`ImageFact`]; `None` where pdf.js's would be missing (or
/// the decoder panicked: one picture, never the book).
pub fn image_fact(reader: &Reader<'_>, obj_id: &str) -> Option<ImageFact> {
    let img = catch_unwind(AssertUnwindSafe(|| reader.image(obj_id)))
        .ok()
        .flatten()?;
    Some(ImageFact {
        width: img.width as f64,
        height: img.height as f64,
        kind: img.kind,
        data_length: Some(img.data.len()),
        data_sha256: Some(hex(&Sha256::digest(&img.data))),
        data: Some(img.data),
    })
}

fn page_facts(p: PageData, images: BTreeMap<String, Option<ImageFact>>) -> PageFacts {
    PageFacts {
        page_num: p.page_num as u32,
        width: p.width,
        height: p.height,
        items: p
            .items
            .into_iter()
            .map(|it| TextItemFact {
                str: it.str,
                dir: Some(it.dir),
                width: Some(it.width),
                height: Some(it.height),
                transform: it.transform.to_vec(),
                font_name: Some(it.font_name),
                has_eol: Some(it.has_eol),
            })
            .collect(),
        fonts: p.fonts,
        ops: p.ops.map(|ops| ops.into_iter().filter_map(op).collect()),
        images,
    }
}

fn op(o: PageOp) -> Option<Op> {
    Some(match o {
        PageOp::Save => Op::Save,
        PageOp::Restore => Op::Restore,
        PageOp::Transform { m } => Op::Transform { matrix: m.to_vec() },
        PageOp::FormBegin { m } => Op::PaintFormXObjectBegin {
            matrix: m.map(|m| m.to_vec()),
        },
        PageOp::FormEnd => Op::PaintFormXObjectEnd,
        PageOp::BeginAnnotation => Op::BeginAnnotation,
        PageOp::EndAnnotation => Op::EndAnnotation,
        PageOp::ConstructPath { paint, bbox } => Op::ConstructPath {
            paint: Some(PaintOp::Name(paint)),
            bbox: bbox.map(|b| {
                // An empty-but-present path keeps pdf.js's ±Infinity box.
                [
                    b[0].unwrap_or(f64::INFINITY),
                    b[1].unwrap_or(f64::INFINITY),
                    b[2].unwrap_or(f64::NEG_INFINITY),
                    b[3].unwrap_or(f64::NEG_INFINITY),
                ]
                .to_vec()
            }),
        },
        PageOp::PaintImage { obj_id, w, h } => Op::PaintImageXObject {
            obj_id: Some(obj_id),
            width: Some(w),
            height: Some(h),
        },
        PageOp::ShowText { text } => Op::ShowText { text: Some(text) },
        // The parser reads neither inline images nor stencil masks.
        PageOp::PaintInlineImage | PageOp::PaintImageMask | PageOp::PaintSolidColorImageMask => {
            return None;
        }
    })
}

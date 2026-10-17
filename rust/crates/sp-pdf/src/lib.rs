//! sp-pdf: what pdf.js gives Method Reader's parser, in pure Rust.
//!
//! The TypeScript engine reads a PDF through pdf.js: per page, the text
//! items of `getTextContent`, the font names behind them, a handful of
//! operators out of `getOperatorList` (the transforms, the drawn rules, the
//! painted images and the glyph runs), and each painted image's pixels.
//! This crate produces the same things from the same file, using
//! hayro-syntax for the file structure and a port of pdf.js's own evaluator
//! for everything whose exact behavior the parser depends on.
//!
//! Large parts are ported from pdf.js (Mozilla Foundation, Apache License
//! 2.0); each such module says so, and the crate's NOTICE carries the
//! license notice.

pub mod bidi;
pub mod cmap;
pub mod facts;
pub mod filters;
pub mod fonts;
pub mod images;
pub mod jpeg;
pub mod js;
pub mod lexer;
pub mod obj;
pub mod oplist;
pub mod ops;
pub mod page;
#[allow(clippy::all, clippy::pedantic)]
pub mod tables;
pub mod text;
pub mod truetype;
pub mod unicode;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

pub use facts::{image_fact, read, read_document, read_pages};
pub use images::PdfImage;
pub use oplist::PageOp;
pub use text::TextItem;

use crate::fonts::FontLoader;
use crate::obj::{Obj, Ref, XRef};
use crate::oplist::{DocState, GlobalImages};

/// Why a document could not be opened at all.
#[derive(Debug)]
pub enum LoadError {
    /// Not a PDF hayro-syntax can read (no usable xref, no pages).
    Invalid,
    /// Encrypted with a password we do not have.
    Encrypted,
}

/// One page, as the parser sees it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PageData {
    #[serde(rename = "pageNum")]
    pub page_num: usize,
    /// `getViewport({ scale: 1 })`.
    pub width: f64,
    pub height: f64,
    pub view: [f64; 4],
    pub rotate: i64,
    pub items: Vec<TextItem>,
    /// Each item font's PostScript name (`commonObjs.get(fontName).name`),
    /// null where pdf.js has none (an unusable font).
    pub fonts: BTreeMap<String, Option<String>>,
    /// The operators the parser walks, in order.
    pub ops: Option<Vec<PageOp>>,
}

/// An open document.
pub struct Document {
    pdf: hayro_syntax::Pdf,
}

impl Document {
    pub fn load(bytes: Vec<u8>) -> Result<Self, LoadError> {
        match hayro_syntax::Pdf::new(bytes) {
            Ok(pdf) => Ok(Self { pdf }),
            Err(hayro_syntax::LoadPdfError::Decryption(_)) => Err(LoadError::Encrypted),
            Err(_) => Err(LoadError::Invalid),
        }
    }

    pub fn num_pages(&self) -> usize {
        self.pdf.pages().len()
    }

    /// A reader over the document. Pages must be read in order, as pdf.js's
    /// engine reads them, because fonts and images are shared across pages
    /// the way pdf.js shares them (and named by the page that first used
    /// them).
    pub fn reader(&self) -> Reader<'_> {
        Reader {
            pdf: &self.pdf,
            xref: XRef::new(&self.pdf),
            state: DocState {
                fonts: FontLoader::new(),
                global_images: GlobalImages::default(),
                images: HashMap::new(),
            },
        }
    }

    /// Every page in order.
    pub fn pages(&self) -> Vec<PageData> {
        let mut r = self.reader();
        (0..self.num_pages()).map(|i| r.page(i)).collect()
    }
}

/// Reads pages, and later the pixels of the images they painted.
pub struct Reader<'d> {
    pdf: &'d hayro_syntax::Pdf,
    xref: XRef<'d>,
    state: DocState,
}

impl Reader<'_> {
    /// One page: its text items, their font names, and its operators.
    pub fn page(&mut self, index: usize) -> PageData {
        let dict = match self.pdf.pages().get(index).and_then(|p| p.raw().obj_id()) {
            Some(id) => self.xref.fetch(Ref {
                num: id.obj_number,
                generation: id.gen_number,
            }),
            None => Obj::Null,
        };
        let dict = match dict {
            Obj::Dict(d) => d,
            _ => Rc::new(obj::Dict::default()),
        };
        read_page(&self.xref, &mut self.state, dict, index)
    }

    /// The pixels behind a `paintImageXObject` id, decoded the way pdf.js
    /// decodes them; `None` where pdf.js's would be missing or null.
    pub fn image(&self, obj_id: &str) -> Option<PdfImage> {
        let src = self.state.images.get(obj_id)?;
        images::decode(&self.xref, src)
    }
}

fn read_page(xref: &XRef<'_>, state: &mut DocState, dict: Rc<obj::Dict>, index: usize) -> PageData {
    let info = page::page_info(xref, dict);
    let (content, content_dict) = page::content_bytes(xref, &info.dict);
    let resources = page::merged_resources(xref, content_dict.as_ref(), &info.resources);
    let mut ex = text::TextExtractor {
        xref,
        fonts: &mut state.fonts,
        view_box: info.view,
        items: Vec::new(),
    };
    ex.run(&content, &resources, None, &HashSet::new());
    let items = ex.items;
    let annots: Vec<Rc<obj::Dict>> = match xref.get(&info.dict, "Annots") {
        Obj::Array(a) => a
            .iter()
            .filter_map(|o| xref.resolve(o).dict().cloned())
            .collect(),
        _ => Vec::new(),
    };
    let mut b = oplist::OpListBuilder::new(xref, state, index);
    b.run_page(&content, &resources, &annots);
    let ops = b.ops;
    let (width, height) = page::viewport_size(&info.view, info.rotate);
    let mut font_names = BTreeMap::new();
    for it in &items {
        if !font_names.contains_key(&it.font_name) {
            font_names.insert(it.font_name.clone(), state.fonts.name_of(&it.font_name));
        }
    }
    PageData {
        page_num: index + 1,
        width,
        height,
        view: info.view,
        rotate: info.rotate,
        items,
        fonts: font_names,
        ops: Some(ops),
    }
}

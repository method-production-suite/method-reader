//! What the PDF layer hands the parser: the shape of the oracle's
//! `pages.json` (tools/oracle/dump.ts, `pdfFacts`), which is exactly the set
//! of questions extract.ts asked pdf.js and nothing more.
//!
//! sp-pdf produces these; the parser reads nothing else about a PDF. Every
//! field is named as the dump writes it, so a dump reads straight in.
//!
//! | Field | pdf.js source | Read by |
//! |---|---|---|
//! | `width`, `height` | `page.getViewport({ scale: 1 })` | indent, picture geometry |
//! | `items` | `page.getTextContent().items`, untouched | every text pass |
//! | `fonts` | `commonObjs.get(fontName).name` after `getOperatorList` | bold, italic, family |
//! | `ops` | `page.getOperatorList()`, only the operators listed in [`Op`] | underlines, phantom spaces, pictures |
//! | `images` | the decoded pixels each `paintImageXObject` names | pictures |
//!
//! `ops` is `None` for a page pdf.js could not build an operator list for:
//! that page then has no underlines, no repairs and no pictures, and its
//! fonts resolve to nothing.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// `pages.json`: the whole document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentFacts {
    /// The DOCUMENT's page count.
    pub num_pages: u32,
    pub pages: Vec<PageFacts>,
}

/// One page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageFacts {
    pub page_num: u32,
    /// Viewport size at scale 1, in points.
    #[serde(with = "sp_model::js::number")]
    pub width: f64,
    #[serde(with = "sp_model::js::number")]
    pub height: f64,
    pub items: Vec<TextItemFact>,
    /// Font id (as `items[].fontName` names it) to its resolved PostScript
    /// name, or `None` when the font never resolved.
    pub fonts: BTreeMap<String, Option<String>>,
    pub ops: Option<Vec<Op>>,
    /// Object id (as a `paintImageXObject` names it) to its pixels, or `None`
    /// when they never arrived.
    pub images: BTreeMap<String, Option<ImageFact>>,
}

/// One pdf.js text item, as `getTextContent` returned it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextItemFact {
    pub str: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Rendered width in points; `None` falls back to six points a character.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "sp_model::js::opt_number"
    )]
    pub width: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "sp_model::js::opt_number"
    )]
    pub height: Option<f64>,
    /// `[a, b, c, d, e, f]`: `e` is the x of the item's origin, `f` its
    /// baseline, `d` its vertical scale (the size the font pass reads).
    #[serde(with = "sp_model::js::numbers")]
    pub transform: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_name: Option<String>,
    #[serde(rename = "hasEOL", default, skip_serializing_if = "Option::is_none")]
    pub has_eol: Option<bool>,
}

/// The operators the parser walks, in order. Everything else pdf.js emits
/// is left out by the producer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Op {
    #[serde(rename = "save")]
    Save,
    #[serde(rename = "restore")]
    Restore,
    #[serde(rename = "transform")]
    Transform {
        #[serde(with = "sp_model::js::numbers")]
        matrix: Vec<f64>,
    },
    /// A form XObject opens: its /Matrix, when it has one.
    #[serde(rename = "paintFormXObjectBegin")]
    PaintFormXObjectBegin {
        #[serde(default, serialize_with = "opt_numbers")]
        matrix: Option<Vec<f64>>,
    },
    #[serde(rename = "paintFormXObjectEnd")]
    PaintFormXObjectEnd,
    #[serde(rename = "beginAnnotation")]
    BeginAnnotation,
    #[serde(rename = "endAnnotation")]
    EndAnnotation,
    /// A path: the paint operator that ends it (`"endPath"` is a clip and
    /// paints nothing) and its bounding box `[x0, y0, x1, y1]` before the
    /// current matrix is applied.
    #[serde(rename = "constructPath")]
    ConstructPath {
        #[serde(default)]
        paint: Option<PaintOp>,
        #[serde(default, serialize_with = "opt_numbers")]
        bbox: Option<Vec<f64>>,
    },
    /// A glyph run: each glyph's unicode joined, positioning numbers dropped.
    #[serde(rename = "showText")]
    ShowText {
        #[serde(default)]
        text: Option<String>,
    },
    /// An image XObject painted into the unit square under the current
    /// matrix, with the pixel size it declares.
    #[serde(rename = "paintImageXObject")]
    PaintImageXObject {
        #[serde(rename = "objId", default, deserialize_with = "string_or_none")]
        obj_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "number_or_none",
            serialize_with = "sp_model::js::opt_number::serialize"
        )]
        width: Option<f64>,
        #[serde(
            default,
            deserialize_with = "number_or_none",
            serialize_with = "sp_model::js::opt_number::serialize"
        )]
        height: Option<f64>,
    },
    /// Any operator the parser does not read.
    #[serde(other)]
    Other,
}

/// The paint operator of a `constructPath`: pdf.js's operator name, or its
/// number when it had none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PaintOp {
    Name(String),
    Code(#[serde(with = "sp_model::js::number")] f64),
}

impl PaintOp {
    /// `endPath`: the clip-path spelling, which builds geometry and paints
    /// nothing.
    pub fn is_end_path(&self) -> bool {
        matches!(self, Self::Name(n) if n == "endPath")
    }
}

/// One picture's pixels, as pdf.js handed them over (before any conversion).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageFact {
    #[serde(with = "sp_model::js::number")]
    pub width: f64,
    #[serde(with = "sp_model::js::number")]
    pub height: f64,
    /// pdf.js ImageKind: 1 GRAYSCALE_1BPP, 2 RGB_24BPP, 3 RGBA_32BPP.
    pub kind: u8,
    /// `data.length`, or `None` for the bitmap path (no pixels).
    pub data_length: Option<usize>,
    /// SHA-256 of `data`, hex: how the oracle names pixels it does not
    /// carry, and how a producer's pixels are checked against it.
    pub data_sha256: Option<String>,
    /// The pixels themselves. Never in JSON: sp-pdf fills this in memory.
    /// When it is `None` but `data_sha256` is set (an oracle dump), the
    /// picture is judged on its facts alone and carries no PNG.
    #[serde(skip)]
    pub data: Option<Vec<u8>>,
}

fn string_or_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::String(s) => Some(s),
        _ => None,
    })
}

fn number_or_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(v.as_f64())
}

#[allow(clippy::ref_option)]
fn opt_numbers<S: serde::Serializer>(v: &Option<Vec<f64>>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(v) => sp_model::js::numbers::serialize(v, s),
        None => s.serialize_none(),
    }
}

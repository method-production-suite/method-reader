//! getOperatorList, cut down to the operators the parser walks.
//!
//! Ported from pdf.js `src/core/evaluator.js` (`PartialEvaluator
//! .getOperatorList`, `buildFormXObject`, `buildPaintImageXObject`,
//! `buildPath`, `setGState`, `handleSMask`), `src/core/document.js`
//! (`Page.getOperatorList`) and `src/core/annotation.js` (the appearance
//! path), Mozilla Foundation, Apache License 2.0.
//!
//! The parser reads save/restore/transform (to track the CTM), path paints
//! with their bounding boxes (underlines), form begin/end (art placed by a
//! form), annotation begin/end (to skip stamps), image paints, and the glyph
//! runs of each show-text operator (phantom-space repair). Those come out in
//! pdf.js's order with pdf.js's values, Float32 rounding included; nothing
//! else does.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::fonts::{Font, FontLoader};
use crate::js::{js_max, js_min, js_to_number, transform};
use crate::obj::{Dict, Obj, Ref, Stream, XRef};
use crate::ops::{Op, Preprocessor, StateHooks};

/// One operator the parser reads, in the oracle's JSON shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum PageOp {
    #[serde(rename = "save")]
    Save,
    #[serde(rename = "restore")]
    Restore,
    /// `cm`, its six operands as pdf.js passes them on.
    #[serde(rename = "transform")]
    Transform { m: [f64; 6] },
    /// A form XObject's /Matrix, rounded to Float32 as pdf.js does.
    #[serde(rename = "paintFormXObjectBegin")]
    FormBegin { m: Option<[f64; 6]> },
    #[serde(rename = "paintFormXObjectEnd")]
    FormEnd,
    #[serde(rename = "beginAnnotation")]
    BeginAnnotation,
    #[serde(rename = "endAnnotation")]
    EndAnnotation,
    /// A path paint: the paint operator's pdf.js name and the path's
    /// bounding box in the coordinates it was drawn in (Float32, as pdf.js
    /// keeps it; null for an empty path, and ±Infinity, which JSON writes
    /// as null, for a path with no points).
    #[serde(rename = "constructPath")]
    ConstructPath {
        paint: String,
        bbox: Option<[Option<f64>; 4]>,
    },
    /// An image XObject (or large inline image) paint: its id and the
    /// /Width and /Height its dictionary declares.
    #[serde(rename = "paintImageXObject")]
    PaintImage {
        #[serde(rename = "objId")]
        obj_id: String,
        w: f64,
        h: f64,
    },
    #[serde(rename = "paintInlineImageXObject")]
    PaintInlineImage,
    #[serde(rename = "paintImageMaskXObject")]
    PaintImageMask,
    #[serde(rename = "paintSolidColorImageMask")]
    PaintSolidColorImageMask,
    /// A show-text operator's glyphs, as their Unicode values joined.
    #[serde(rename = "showText")]
    ShowText { text: String },
}

/// Where an image's pixels come from, kept until they are asked for.
#[derive(Clone)]
pub struct ImageSource {
    pub stream: Rc<Stream>,
    pub resources: Rc<Dict>,
    pub is_inline: bool,
}

/// pdf.js's GlobalImageCache, as far as it decides ids: an image painted
/// on a second page is promoted to a document-wide `g_d0_` id.
#[derive(Default)]
pub struct GlobalImages {
    pages_by_ref: HashMap<Ref, HashSet<usize>>,
    data: HashMap<Ref, CachedImage>,
}

#[derive(Clone)]
struct CachedImage {
    op: PageOp,
    has_mask: bool,
}

impl GlobalImages {
    fn should_cache(&mut self, r: Ref, page: usize) -> bool {
        let set = self.pages_by_ref.entry(r).or_default();
        set.insert(page);
        // The byte-size ceiling (ten images AND 50 MB) is not modelled; it
        // only ever changes an id's prefix.
        set.len() >= 2
    }
    fn get(&mut self, r: Ref, page: usize) -> Option<CachedImage> {
        let set = self.pages_by_ref.get_mut(&r)?;
        if set.len() < 2 {
            return None;
        }
        let d = self.data.get(&r)?.clone();
        set.insert(page);
        Some(d)
    }
}

/// Everything a page's operator list shares with the document.
pub struct DocState {
    pub fonts: FontLoader,
    pub global_images: GlobalImages,
    pub images: HashMap<String, ImageSource>,
}

/// One page's run.
pub struct OpListBuilder<'x, 'p> {
    pub xref: &'x XRef<'p>,
    pub doc: &'x mut DocState,
    pub page_index: usize,
    obj_counter: u32,
    regional: HashMap<Ref, CachedImage>,
    pub ops: Vec<PageOp>,
    pub failed: bool,
}

#[derive(Clone)]
struct EvalState {
    font: Option<Rc<Font>>,
    cur_x: f64,
    cur_y: f64,
    fill_pattern: bool,
}

struct Frame {
    state: EvalState,
    stack: Vec<EvalState>,
}

impl StateHooks for Frame {
    fn save(&mut self) {
        self.stack.push(self.state.clone());
    }
    fn restore(&mut self) {
        if let Some(p) = self.stack.pop() {
            self.state = p;
        }
    }
    fn transform(&mut self, _args: &[Obj]) {}
}

/// The path being built, shared by every q/Q level of one content stream
/// (pdf.js's clones share these arrays) and fresh for a form.
struct Path {
    entries: usize,
    min_max: [f32; 4],
}

const F32_BBOX_INIT: [f32; 4] = [
    f32::INFINITY,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::NEG_INFINITY,
];

fn upd(mm: &mut [f32; 4], i: usize, v: f64, max: bool) {
    let cur = mm[i] as f64;
    mm[i] = (if max { js_max(cur, v) } else { js_min(cur, v) }) as f32;
}

fn point_bbox(x: f64, y: f64, mm: &mut [f32; 4]) {
    upd(mm, 0, x, false);
    upd(mm, 1, y, false);
    upd(mm, 2, x, true);
    upd(mm, 3, y, true);
}

fn rect_bbox(x0: f64, y0: f64, x1: f64, y1: f64, mm: &mut [f32; 4]) {
    upd(mm, 0, js_min(x0, x1), false);
    upd(mm, 1, js_min(y0, y1), false);
    upd(mm, 2, js_max(x0, x1), true);
    upd(mm, 3, js_max(y0, y1), true);
}

#[allow(clippy::too_many_arguments)]
fn extremum_on_curve(
    x0: f64,
    x1: f64,
    x2: f64,
    x3: f64,
    y0: f64,
    y1: f64,
    y2: f64,
    y3: f64,
    t: f64,
    mm: &mut [f32; 4],
) {
    if t <= 0.0 || t >= 1.0 || t.is_nan() {
        return;
    }
    let mt = 1.0 - t;
    let tt = t * t;
    let ttt = tt * t;
    let x = mt * (mt * (mt * x0 + 3.0 * t * x1) + 3.0 * tt * x2) + ttt * x3;
    let y = mt * (mt * (mt * y0 + 3.0 * t * y1) + 3.0 * tt * y2) + ttt * y3;
    point_bbox(x, y, mm);
}

#[allow(clippy::too_many_arguments)]
fn extremum(
    x0: f64,
    x1: f64,
    x2: f64,
    x3: f64,
    y0: f64,
    y1: f64,
    y2: f64,
    y3: f64,
    a: f64,
    b: f64,
    c: f64,
    mm: &mut [f32; 4],
) {
    if a.abs() < 1e-12 {
        if b.abs() >= 1e-12 {
            extremum_on_curve(x0, x1, x2, x3, y0, y1, y2, y3, -c / b, mm);
        }
        return;
    }
    let delta = b * b - 4.0 * c * a;
    if delta < 0.0 {
        return;
    }
    let sqrt_delta = delta.sqrt();
    let a2 = 2.0 * a;
    extremum_on_curve(x0, x1, x2, x3, y0, y1, y2, y3, (-b + sqrt_delta) / a2, mm);
    extremum_on_curve(x0, x1, x2, x3, y0, y1, y2, y3, (-b - sqrt_delta) / a2, mm);
}

#[allow(clippy::too_many_arguments)]
fn bezier_bbox(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x3: f64,
    y3: f64,
    mm: &mut [f32; 4],
) {
    upd(mm, 0, js_min(x0, x3), false);
    upd(mm, 1, js_min(y0, y3), false);
    upd(mm, 2, js_max(x0, x3), true);
    upd(mm, 3, js_max(y0, y3), true);
    extremum(
        x0,
        x1,
        x2,
        x3,
        y0,
        y1,
        y2,
        y3,
        3.0 * (-x0 + 3.0 * (x1 - x2) + x3),
        6.0 * (x0 - 2.0 * x1 + x2),
        3.0 * (x1 - x0),
        mm,
    );
    extremum(
        x0,
        x1,
        x2,
        x3,
        y0,
        y1,
        y2,
        y3,
        3.0 * (-y0 + 3.0 * (y1 - y2) + y3),
        6.0 * (y0 - 2.0 * y1 + y2),
        3.0 * (y1 - y0),
        mm,
    );
}

fn f32_matrix(a: &[Obj]) -> Option<[f64; 6]> {
    if a.len() != 6 || !a.iter().all(|o| matches!(o, Obj::Num(_))) {
        return None;
    }
    let mut m = [0.0; 6];
    for (i, o) in a.iter().enumerate() {
        m[i] = js_to_number(o) as f32 as f64;
    }
    Some(m)
}

struct Stop;

impl<'x, 'p> OpListBuilder<'x, 'p> {
    pub fn new(xref: &'x XRef<'p>, doc: &'x mut DocState, page_index: usize) -> Self {
        Self {
            xref,
            doc,
            page_index,
            obj_counter: 0,
            regional: HashMap::new(),
            ops: Vec::new(),
            failed: false,
        }
    }

    fn create_obj_id(&mut self) -> String {
        self.obj_counter += 1;
        format!("p{}_{}", self.page_index, self.obj_counter)
    }

    /// The page: its content, then each annotation's appearance.
    pub fn run_page(&mut self, content: &[u8], resources: &Rc<Dict>, annots: &[Rc<Dict>]) {
        let initial = EvalState {
            font: None,
            cur_x: 0.0,
            cur_y: 0.0,
            fill_pattern: false,
        };
        self.run(content, resources, initial, &HashSet::new());
        for a in annots {
            self.annotation(a);
        }
    }

    fn annotation(&mut self, dict: &Rc<Dict>) {
        let xref = self.xref;
        let subtype = xref.get(dict, "Subtype").name().map(str::to_string);
        if subtype.as_deref() == Some("Widget") {
            // Under ENABLE_FORMS every widget but a signature is drawn by
            // the HTML layer and contributes no operators.
            let ft = inheritable_name(xref, dict, "FT");
            if ft.as_deref() != Some("Sig") {
                return;
            }
        }
        let mut flags = match xref.get(dict, "F") {
            Obj::Num(n) if n.fract() == 0.0 && n > 0.0 => n as i64,
            _ => 0,
        };
        let known = matches!(
            subtype.as_deref(),
            Some(
                "Link"
                    | "Text"
                    | "Widget"
                    | "Popup"
                    | "FreeText"
                    | "Line"
                    | "Square"
                    | "Circle"
                    | "PolyLine"
                    | "Polygon"
                    | "Caret"
                    | "Ink"
                    | "Highlight"
                    | "Underline"
                    | "Squiggly"
                    | "StrikeOut"
                    | "Stamp"
                    | "FileAttachment"
                    | "RichMedia"
                    | "Screen"
                    | "Sound"
            )
        );
        if flags & 1 != 0 && known {
            flags ^= 1;
        }
        let viewable = flags == 0 || (flags & 1 == 0 && flags & 32 == 0);
        if !viewable || flags & 2 != 0 {
            return;
        }
        let ap = xref.get(dict, "AP");
        let Some(ap) = ap.dict() else { return };
        let appearance = match xref.get(ap, "N") {
            Obj::Stream(s) => Some(s),
            Obj::Dict(n) => match xref.get(dict, "AS") {
                Obj::Name(a) => xref.get(&n, &a).stream().cloned(),
                _ => None,
            },
            _ => None,
        };
        let Some(appearance) = appearance else { return };
        let resources = match xref.get(&appearance.dict, "Resources") {
            Obj::Dict(d) => d,
            _ => Rc::new(Dict::default()),
        };
        let Some(bytes) = xref.stream_bytes(&appearance) else {
            return;
        };
        self.ops.push(PageOp::BeginAnnotation);
        let initial = EvalState {
            font: None,
            cur_x: 0.0,
            cur_y: 0.0,
            fill_pattern: false,
        };
        let mut seen = HashSet::new();
        if let Some(id) = appearance.dict.obj_id {
            seen.insert(id);
        }
        self.run(&bytes, &resources, initial, &seen);
        self.ops.push(PageOp::EndAnnotation);
    }

    /// One getOperatorList call over a content stream. Returns after
    /// closing what it left open, as pdf.js does on success and on error.
    fn run(
        &mut self,
        content: &[u8],
        resources: &Rc<Dict>,
        initial: EvalState,
        seen: &HashSet<Ref>,
    ) {
        let mut frame = Frame {
            state: initial,
            stack: Vec::new(),
        };
        let mut path = Path {
            entries: 0,
            min_max: F32_BBOX_INIT,
        };
        let mut local_images: HashMap<String, CachedImage> = HashMap::new();
        let mut local_images_by_ref: HashMap<Ref, CachedImage> = HashMap::new();
        let mut shading_ids: HashMap<usize, String> = HashMap::new();
        let r = self.run_inner(
            content,
            resources,
            &mut frame,
            &mut path,
            &mut local_images,
            &mut local_images_by_ref,
            &mut shading_ids,
            seen,
        );
        if r.is_err() {
            self.failed = true;
        }
        // closePendingRestoreOPS: one restore per q still open.
        for _ in 0..frame.stack.len() {
            self.ops.push(PageOp::Restore);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_inner(
        &mut self,
        content: &[u8],
        resources: &Rc<Dict>,
        frame: &mut Frame,
        path: &mut Path,
        local_images: &mut HashMap<String, CachedImage>,
        local_images_by_ref: &mut HashMap<Ref, CachedImage>,
        shading_ids: &mut HashMap<usize, String>,
        seen: &HashSet<Ref>,
    ) -> Result<(), Stop> {
        let xref = self.xref;
        let mut pre = Preprocessor::new(content).map_err(|_| Stop)?;
        loop {
            let operation = match pre.read(frame) {
                Ok(Some(o)) => o,
                Ok(None) => break,
                Err(_) => return Err(Stop),
            };
            let args = operation.args;
            match operation.op {
                Op::PaintXObject => {
                    let name = match args.first() {
                        Some(Obj::Name(n)) => n.to_string(),
                        // `args[0].name` on a non-Name: the lookup fails and
                        // the XObject is ignored with a warning.
                        _ => continue,
                    };
                    if let Some(c) = local_images.get(&name).cloned() {
                        self.add_image_ops(&c);
                        continue;
                    }
                    let xobjs = xref.get(resources, "XObject");
                    let Some(xobjs) = xobjs.dict() else { continue };
                    let raw = xobjs.get_raw(&name).cloned();
                    if let Some(Obj::Ref(r)) = &raw {
                        let cached = local_images_by_ref
                            .get(r)
                            .cloned()
                            .or_else(|| self.regional.get(r).cloned())
                            .or_else(|| self.doc.global_images.get(*r, self.page_index));
                        if let Some(c) = cached {
                            self.add_image_ops(&c);
                            continue;
                        }
                    }
                    let xobj = match &raw {
                        Some(o) => xref.resolve(o),
                        None => continue,
                    };
                    let Obj::Stream(s) = xobj else { continue };
                    let ty = xref.get(&s.dict, "Subtype");
                    match ty.name() {
                        Some("Form") => {
                            frame.save();
                            let st = EvalState {
                                font: frame.state.font.clone(),
                                cur_x: frame.state.cur_x,
                                cur_y: frame.state.cur_y,
                                fill_pattern: frame.state.fill_pattern,
                            };
                            self.build_form(resources, &s, st, seen);
                            frame.restore();
                        }
                        Some("Image") => {
                            let image_ref = match &raw {
                                Some(Obj::Ref(r)) => Some(*r),
                                _ => s.dict.obj_id,
                            };
                            self.build_image(
                                resources,
                                &s,
                                false,
                                Some(name.clone()),
                                image_ref,
                                local_images,
                                local_images_by_ref,
                            );
                        }
                        _ => {}
                    }
                }
                Op::SetFont => {
                    let name = match args.first() {
                        Some(Obj::Name(n)) => Some(n.to_string()),
                        _ => None,
                    };
                    let t = self
                        .doc
                        .fonts
                        .load_font(xref, name.as_deref(), None, resources);
                    frame.state.font = Some(t.font.clone());
                }
                Op::EndInlineImage => {
                    let Some(Obj::Stream(s)) = args.first() else {
                        continue;
                    };
                    let key = pre
                        .parser
                        .inline_keys
                        .iter()
                        .find(|(p, _)| *p == Rc::as_ptr(s))
                        .map(|(_, k)| k.clone());
                    if let Some(k) = &key
                        && let Some(c) = local_images.get(k).cloned()
                    {
                        self.add_image_ops(&c);
                        continue;
                    }
                    let s = s.clone();
                    self.build_image(
                        resources,
                        &s,
                        true,
                        key,
                        None,
                        local_images,
                        local_images_by_ref,
                    );
                }
                Op::ShowText => {
                    let Some(font) = frame.state.font.clone() else {
                        continue;
                    };
                    let chars = str_chars(args.first());
                    let text: String = font
                        .chars_to_glyphs(&chars)
                        .into_iter()
                        .map(|g| g.unicode)
                        .collect();
                    self.ops.push(PageOp::ShowText { text });
                }
                Op::ShowSpacedText => {
                    let Some(font) = frame.state.font.clone() else {
                        continue;
                    };
                    let mut text = String::new();
                    match args.first() {
                        Some(Obj::Array(a)) => {
                            for item in a.iter() {
                                if let Obj::Str(s) = item {
                                    let chars: Vec<u16> = s.iter().map(|&b| b as u16).collect();
                                    for g in font.chars_to_glyphs(&chars) {
                                        text.push_str(&g.unicode);
                                    }
                                }
                            }
                        }
                        Some(Obj::Str(s)) => {
                            // Iterating a string yields one-char strings.
                            for &b in s.iter() {
                                for g in font.chars_to_glyphs(&[b as u16]) {
                                    text.push_str(&g.unicode);
                                }
                            }
                        }
                        _ => return Err(Stop),
                    }
                    self.ops.push(PageOp::ShowText { text });
                }
                Op::NextLineShowText => {
                    let Some(font) = frame.state.font.clone() else {
                        continue;
                    };
                    let chars = str_chars(args.first());
                    let text: String = font
                        .chars_to_glyphs(&chars)
                        .into_iter()
                        .map(|g| g.unicode)
                        .collect();
                    self.ops.push(PageOp::ShowText { text });
                }
                Op::NextLineSetSpacingShowText => {
                    let Some(font) = frame.state.font.clone() else {
                        continue;
                    };
                    // args.shift() twice, then args[0]: the third operand.
                    let chars = str_chars(args.get(2));
                    let text: String = font
                        .chars_to_glyphs(&chars)
                        .into_iter()
                        .map(|g| g.unicode)
                        .collect();
                    self.ops.push(PageOp::ShowText { text });
                }
                Op::SetGState => {
                    self.set_gstate(&args, resources, frame, seen);
                }
                Op::MoveTo | Op::LineTo => {
                    let x = arg(&args, 0);
                    let y = arg(&args, 1);
                    frame.state.cur_x = x;
                    frame.state.cur_y = y;
                    path.entries += 3;
                    point_bbox(x, y, &mut path.min_max);
                }
                Op::Rectangle => {
                    let x = arg(&args, 0);
                    let y = arg(&args, 1);
                    frame.state.cur_x = x;
                    frame.state.cur_y = y;
                    let width = arg(&args, 2);
                    let height = arg(&args, 3);
                    let xw = x + width;
                    let yh = y + height;
                    path.entries += if width == 0.0 || height == 0.0 { 7 } else { 13 };
                    rect_bbox(x, y, xw, yh, &mut path.min_max);
                }
                Op::CurveTo => {
                    let (sx, sy) = (frame.state.cur_x, frame.state.cur_y);
                    let v: Vec<f64> = (0..6).map(|i| arg(&args, i)).collect();
                    frame.state.cur_x = v[4];
                    frame.state.cur_y = v[5];
                    path.entries += 7;
                    bezier_bbox(
                        sx,
                        sy,
                        v[0],
                        v[1],
                        v[2],
                        v[3],
                        v[4],
                        v[5],
                        &mut path.min_max,
                    );
                }
                Op::CurveTo2 => {
                    let (sx, sy) = (frame.state.cur_x, frame.state.cur_y);
                    let v: Vec<f64> = (0..4).map(|i| arg(&args, i)).collect();
                    frame.state.cur_x = v[2];
                    frame.state.cur_y = v[3];
                    path.entries += 7;
                    bezier_bbox(sx, sy, sx, sy, v[0], v[1], v[2], v[3], &mut path.min_max);
                }
                Op::CurveTo3 => {
                    let (sx, sy) = (frame.state.cur_x, frame.state.cur_y);
                    let v: Vec<f64> = (0..4).map(|i| arg(&args, i)).collect();
                    frame.state.cur_x = v[2];
                    frame.state.cur_y = v[3];
                    path.entries += 7;
                    bezier_bbox(
                        sx,
                        sy,
                        v[0],
                        v[1],
                        v[2],
                        v[3],
                        v[2],
                        v[3],
                        &mut path.min_max,
                    );
                }
                Op::ClosePath => path.entries += 1,
                Op::Stroke
                | Op::CloseStroke
                | Op::Fill
                | Op::EoFill
                | Op::FillStroke
                | Op::EoFillStroke
                | Op::CloseFillStroke
                | Op::CloseEoFillStroke
                | Op::EndPath => {
                    let fn_ = operation.op;
                    if matches!(
                        fn_,
                        Op::CloseStroke | Op::CloseFillStroke | Op::CloseEoFillStroke
                    ) {
                        path.entries += 1;
                    }
                    if path.entries == 0 {
                        self.ops.push(PageOp::ConstructPath {
                            paint: fn_.js_name().to_string(),
                            bbox: None,
                        });
                    } else {
                        let mm = path.min_max;
                        let f = |v: f32| if v.is_finite() { Some(v as f64) } else { None };
                        self.ops.push(PageOp::ConstructPath {
                            paint: fn_.js_name().to_string(),
                            bbox: Some([f(mm[0]), f(mm[1]), f(mm[2]), f(mm[3])]),
                        });
                        path.entries = 0;
                        path.min_max = F32_BBOX_INIT;
                    }
                }
                Op::Save => self.ops.push(PageOp::Save),
                Op::Restore => self.ops.push(PageOp::Restore),
                Op::Transform => {
                    let m = [
                        arg(&args, 0),
                        arg(&args, 1),
                        arg(&args, 2),
                        arg(&args, 3),
                        arg(&args, 4),
                        arg(&args, 5),
                    ];
                    self.ops.push(PageOp::Transform { m });
                }
                Op::ShadingFill => {
                    let Some(Obj::Name(n)) = args.first() else {
                        continue;
                    };
                    let sh = xref.get(resources, "Shading");
                    let Some(sh) = sh.dict() else { continue };
                    let shading = xref.get(sh, n);
                    self.parse_shading(&shading, shading_ids);
                }
                Op::SetFillColorSpace => {
                    let cs = args.first().cloned().unwrap_or(Obj::Null);
                    frame.state.fill_pattern = is_pattern_cs(xref, &cs, resources);
                }
                Op::SetFillColorN => {
                    if frame.state.fill_pattern
                        && let Some(Obj::Name(pn)) = args.last()
                    {
                        self.pattern(pn, resources, shading_ids, seen);
                    }
                }
                Op::SetFillRgbColor | Op::SetFillGray | Op::SetFillCmykColor => {
                    frame.state.fill_pattern = false;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn add_image_ops(&mut self, c: &CachedImage) {
        if c.has_mask {
            self.ops.push(PageOp::Save);
        }
        self.ops.push(c.op.clone());
        if c.has_mask {
            self.ops.push(PageOp::Restore);
        }
    }

    fn parse_shading(&mut self, shading: &Obj, ids: &mut HashMap<usize, String>) {
        let key = match shading {
            Obj::Dict(d) => Rc::as_ptr(d) as usize,
            Obj::Stream(s) => Rc::as_ptr(s) as usize,
            _ => return,
        };
        if ids.contains_key(&key) {
            return;
        }
        let id = format!("pattern_{}", self.create_obj_id());
        ids.insert(key, id);
    }

    fn pattern(
        &mut self,
        name: &str,
        resources: &Rc<Dict>,
        ids: &mut HashMap<usize, String>,
        seen: &HashSet<Ref>,
    ) {
        let xref = self.xref;
        let patterns = xref.get(resources, "Pattern");
        let Some(patterns) = patterns.dict() else {
            return;
        };
        let pattern = xref.get(patterns, name);
        let (dict, stream) = match &pattern {
            Obj::Dict(d) => (d.clone(), None),
            Obj::Stream(s) => (s.dict.clone(), Some(s.clone())),
            _ => return,
        };
        match xref.get(&dict, "PatternType").num() {
            Some(1.0) => {
                // A tiling pattern's operators go to a list of its own; run
                // it for the ids, fonts and images it registers, and drop
                // the operators.
                let Some(s) = stream else { return };
                let Some(bytes) = xref.stream_bytes(&s) else {
                    return;
                };
                let res = merge_resources(xref, &dict, resources);
                let saved = std::mem::take(&mut self.ops);
                let st = EvalState {
                    font: None,
                    cur_x: 0.0,
                    cur_y: 0.0,
                    fill_pattern: false,
                };
                self.run(&bytes, &res, st, seen);
                self.ops = saved;
            }
            Some(2.0) => {
                let shading = xref.get(&dict, "Shading");
                self.parse_shading(&shading, ids);
            }
            _ => {}
        }
    }

    fn set_gstate(
        &mut self,
        args: &[Obj],
        resources: &Rc<Dict>,
        frame: &mut Frame,
        seen: &HashSet<Ref>,
    ) {
        let xref = self.xref;
        let Some(Obj::Name(name)) = args.first() else {
            return;
        };
        let ext = xref.get(resources, "ExtGState");
        let Some(ext) = ext.dict() else { return };
        let gs = xref.get(ext, name);
        let Some(gs) = gs.dict() else { return };
        for (key, raw) in &gs.entries {
            let value = xref.resolve(raw);
            match &**key {
                "Font" => {
                    let f0 = value
                        .array()
                        .and_then(|a| a.first().cloned())
                        .unwrap_or(Obj::Null);
                    let t = self.doc.fonts.load_font(xref, None, Some(&f0), resources);
                    frame.state.font = Some(t.font.clone());
                }
                "SMask" => {
                    if let Obj::Dict(sm) = &value {
                        let g = xref.get(sm, "G");
                        if let Obj::Stream(s) = g {
                            let st = EvalState {
                                font: frame.state.font.clone(),
                                cur_x: frame.state.cur_x,
                                cur_y: frame.state.cur_y,
                                fill_pattern: frame.state.fill_pattern,
                            };
                            self.build_form(resources, &s, st, seen);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn build_form(
        &mut self,
        resources: &Rc<Dict>,
        s: &Rc<Stream>,
        initial: EvalState,
        seen: &HashSet<Ref>,
    ) {
        let xref = self.xref;
        let dict = &s.dict;
        let matrix = xref.get_array(dict, "Matrix").and_then(|m| f32_matrix(&m));
        self.ops.push(PageOp::FormBegin { m: matrix });
        if let Some(id) = dict.obj_id
            && seen.contains(&id)
        {
            // "ignoring circular reference": thrown before the form's
            // operators and before its End.
            return;
        }
        if seen.len() > 64 {
            return;
        }
        let local = xref.get(dict, "Resources");
        let res = match local {
            Obj::Dict(d) => d,
            _ => resources.clone(),
        };
        let bytes = xref.stream_bytes(s).unwrap_or_default();
        let mut seen2 = seen.clone();
        if let Some(id) = dict.obj_id {
            seen2.insert(id);
        }
        let st = EvalState {
            font: initial.font,
            cur_x: initial.cur_x,
            cur_y: initial.cur_y,
            fill_pattern: initial.fill_pattern,
        };
        self.run(&bytes, &res, st, &seen2);
        self.ops.push(PageOp::FormEnd);
    }

    #[allow(clippy::too_many_arguments)]
    fn build_image(
        &mut self,
        resources: &Rc<Dict>,
        s: &Rc<Stream>,
        is_inline: bool,
        cache_key: Option<String>,
        image_ref: Option<Ref>,
        local_images: &mut HashMap<String, CachedImage>,
        local_images_by_ref: &mut HashMap<Ref, CachedImage>,
    ) {
        let xref = self.xref;
        let dict = &s.dict;
        let w = xref.get2(dict, "W", "Width");
        let h = xref.get2(dict, "H", "Height");
        let (Obj::Num(w), Obj::Num(h)) = (w, h) else {
            return;
        };
        if w == 0.0 || h == 0.0 || w.is_nan() || h.is_nan() {
            return;
        }
        let image_mask = xref.get2(dict, "IM", "ImageMask").truthy();
        let mut cache = |this: &mut Self, c: CachedImage| {
            if let Some(k) = &cache_key {
                match image_ref {
                    Some(r) => {
                        local_images.entry(k.clone()).or_insert(c.clone());
                        local_images_by_ref.entry(r).or_insert(c.clone());
                    }
                    None => {
                        local_images.entry(k.clone()).or_insert(c.clone());
                    }
                }
                if let Some(r) = image_ref {
                    this.regional.insert(r, c);
                }
            }
        };
        if image_mask {
            let op = if w == 1.0 && h == 1.0 && single_opaque_pixel(xref, s) {
                PageOp::PaintSolidColorImageMask
            } else {
                let _ = self.create_obj_id();
                PageOp::PaintImageMask
            };
            self.ops.push(op.clone());
            cache(
                self,
                CachedImage {
                    op,
                    has_mask: false,
                },
            );
            return;
        }
        let has_mask = dict.has("SMask") || dict.has("Mask");
        if is_inline && w + h < 200.0 && !has_mask {
            self.ops.push(PageOp::PaintInlineImage);
            return;
        }
        let mut obj_id = format!("img_{}", self.create_obj_id());
        let mut cache_globally = false;
        if cache_key.is_some()
            && let Some(r) = image_ref
        {
            cache_globally = self.doc.global_images.should_cache(r, self.page_index);
            if cache_globally {
                obj_id = format!("g_d0_{obj_id}");
            }
        }
        let op = PageOp::PaintImage {
            obj_id: obj_id.clone(),
            w,
            h,
        };
        self.add_image_ops(&CachedImage {
            op: op.clone(),
            has_mask,
        });
        self.doc.images.entry(obj_id).or_insert(ImageSource {
            stream: s.clone(),
            resources: resources.clone(),
            is_inline,
        });
        let c = CachedImage { op, has_mask };
        if cache_key.is_some() {
            cache(self, c.clone());
            if cache_globally && let Some(r) = image_ref {
                self.doc.global_images.data.insert(r, c);
            }
        }
    }
}

/// `PDFImage.createMask`'s `isSingleOpaquePixel` for a 1x1 stencil.
fn single_opaque_pixel(xref: &XRef<'_>, s: &Stream) -> bool {
    let decode = xref.get(&s.dict, "Decode");
    let inverse = match decode.array() {
        Some(a) => a.first().and_then(|o| xref.resolve(o).num()).unwrap_or(0.0) > 0.0,
        None => false,
    };
    let data = crate::images::stream_data(xref, s).unwrap_or_default();
    // imgArray[0] & 1 compared with inverseDecode: pdf.js tests the low bit
    // after reading one byte of the 1-bit row.
    let bit = data.first().map(|b| b & 128 != 0).unwrap_or(false);
    bit == inverse
}

fn is_pattern_cs(xref: &XRef<'_>, cs: &Obj, resources: &Dict) -> bool {
    match cs {
        Obj::Name(n) if &**n == "Pattern" => true,
        Obj::Name(n) => {
            let r = xref.get(resources, "ColorSpace");
            match r.dict().map(|d| xref.get(d, n)) {
                Some(Obj::Name(m)) => &*m == "Pattern",
                Some(Obj::Array(a)) => a
                    .first()
                    .map(|o| xref.resolve(o))
                    .is_some_and(|o| o.is_name("Pattern")),
                _ => false,
            }
        }
        Obj::Array(a) => a.first().is_some_and(|o| o.is_name("Pattern")),
        _ => false,
    }
}

fn merge_resources(xref: &XRef<'_>, pattern_dict: &Dict, resources: &Rc<Dict>) -> Rc<Dict> {
    let own = xref.get(pattern_dict, "Resources");
    let Some(own) = own.dict() else {
        return resources.clone();
    };
    let mut out = Dict::default();
    for d in [own.as_ref(), resources.as_ref()] {
        for (k, v) in &d.entries {
            if !out.has(k) {
                out.set(k, v.clone());
            }
        }
    }
    Rc::new(out)
}

fn inheritable_name(xref: &XRef<'_>, dict: &Rc<Dict>, key: &str) -> Option<String> {
    let mut d = Some(dict.clone());
    let mut guard = 0;
    while let Some(cur) = d {
        guard += 1;
        if guard > 100 {
            break;
        }
        if cur.has(key) {
            return xref.get(&cur, key).name().map(str::to_string);
        }
        d = xref.get(&cur, "Parent").dict().cloned();
    }
    None
}

fn arg(args: &[Obj], i: usize) -> f64 {
    args.get(i).map(js_to_number).unwrap_or(f64::NAN)
}

fn str_chars(o: Option<&Obj>) -> Vec<u16> {
    match o {
        Some(Obj::Str(s)) => s.iter().map(|&b| b as u16).collect(),
        _ => Vec::new(),
    }
}

/// The transform helper is shared with the text pass; re-exported for tests.
pub fn util_transform(m1: &[f64; 6], m2: &[f64]) -> [f64; 6] {
    transform(m1, m2)
}

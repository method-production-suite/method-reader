//! getTextContent: pdf.js's grouping of glyphs into text items.
//!
//! Ported from pdf.js `src/core/evaluator.js`, `PartialEvaluator
//! .getTextContent` (Mozilla Foundation, Apache License 2.0), with the
//! options Method Reader's engine uses: no marked content, normalization on,
//! whitespace not kept. Every arithmetic step keeps pdf.js's order so the
//! doubles come out bit for bit the same.

use std::collections::HashSet;
use std::rc::Rc;

use crate::bidi::bidi;
use crate::fonts::{FONT_IDENTITY_MATRIX, Font, FontLoader};
use crate::js::{js_to_number, transform};
use crate::obj::{Dict, Obj, Ref, XRef};
use crate::ops::{Op, Preprocessor, StateHooks};
use crate::unicode::normalize_unicode;

/// One text item, in the shape of pdf.js's `TextItem`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextItem {
    pub str: String,
    pub dir: String,
    pub width: f64,
    pub height: f64,
    pub transform: [f64; 6],
    #[serde(rename = "fontName")]
    pub font_name: String,
    #[serde(rename = "hasEOL")]
    pub has_eol: bool,
}

const TRACKING_SPACE_FACTOR: f64 = 0.102;
const NOT_A_SPACE_FACTOR: f64 = 0.03;
const NEGATIVE_SPACE_FACTOR: f64 = -0.2;
const SPACE_IN_FLOW_MIN_FACTOR: f64 = 0.102;
const SPACE_IN_FLOW_MAX_FACTOR: f64 = 0.6;
const VERTICAL_SHIFT_RATIO: f64 = 0.25;
const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// A font-name slot: pdf.js distinguishes `null` (initial, gstate font)
/// from `undefined` (a `Tf` whose operand is not a name).
#[derive(Clone, PartialEq, Debug)]
enum FName {
    Null,
    Undefined,
    Name(String),
}

#[derive(Clone)]
struct TextState {
    ctm: [f64; 6],
    font_name: FName,
    font_size: f64,
    loaded_name: Option<String>,
    font: Option<Rc<Font>>,
    font_matrix: Vec<f64>,
    text_matrix: [f64; 6],
    text_line_matrix: [f64; 6],
    char_spacing: f64,
    word_spacing: f64,
    leading: f64,
    text_h_scale: f64,
    text_rise: f64,
}

impl TextState {
    fn new() -> Self {
        Self {
            ctm: IDENTITY,
            font_name: FName::Null,
            font_size: 0.0,
            loaded_name: None,
            font: None,
            font_matrix: FONT_IDENTITY_MATRIX.to_vec(),
            text_matrix: IDENTITY,
            text_line_matrix: IDENTITY,
            char_spacing: 0.0,
            word_spacing: 0.0,
            leading: 0.0,
            text_h_scale: 1.0,
            text_rise: 0.0,
        }
    }
    // Written as pdf.js writes it; `+=` would be the same sum (IEEE
    // addition commutes), but the shape is kept for reading side by side.
    #[allow(clippy::assign_op_pattern)]
    fn translate_text_matrix(&mut self, x: f64, y: f64) {
        let m = &mut self.text_matrix;
        m[4] = m[0] * x + m[2] * y + m[4];
        m[5] = m[1] * x + m[3] * y + m[5];
    }
    #[allow(clippy::assign_op_pattern)]
    fn translate_text_line_matrix(&mut self, x: f64, y: f64) {
        let m = &mut self.text_line_matrix;
        m[4] = m[0] * x + m[2] * y + m[4];
        m[5] = m[1] * x + m[3] * y + m[5];
    }
    fn carriage_return(&mut self) {
        self.translate_text_line_matrix(0.0, -self.leading);
        self.text_matrix = self.text_line_matrix;
    }
}

struct StateManager {
    state: TextState,
    stack: Vec<TextState>,
}

impl StateHooks for StateManager {
    fn save(&mut self) {
        self.stack.push(self.state.clone());
    }
    fn restore(&mut self) {
        if let Some(p) = self.stack.pop() {
            self.state = p;
        }
    }
    fn transform(&mut self, args: &[Obj]) {
        let m: Vec<f64> = args.iter().map(js_to_number).collect();
        self.state.ctm = transform(&self.state.ctm, &m);
    }
}

/// The in-progress item.
struct Chunk {
    initialized: bool,
    str: Vec<String>,
    total_width: f64,
    total_height: f64,
    width: f64,
    height: f64,
    vertical: bool,
    prev_transform: Option<[f64; 6]>,
    prev_text_rise: f64,
    text_advance_scale: f64,
    space_in_flow_min: f64,
    space_in_flow_max: f64,
    tracking_space_min: f64,
    negative_space_max: f64,
    not_a_space: f64,
    transform: Option<[f64; 6]>,
    font_name: Option<String>,
    has_eol: bool,
}

impl Chunk {
    fn new() -> Self {
        Self {
            initialized: false,
            str: Vec::new(),
            total_width: 0.0,
            total_height: 0.0,
            width: 0.0,
            height: 0.0,
            vertical: false,
            prev_transform: None,
            prev_text_rise: 0.0,
            text_advance_scale: 0.0,
            space_in_flow_min: 0.0,
            space_in_flow_max: 0.0,
            tracking_space_min: f64::INFINITY,
            negative_space_max: f64::NEG_INFINITY,
            not_a_space: f64::NEG_INFINITY,
            transform: None,
            font_name: None,
            has_eol: false,
        }
    }
}

/// Where a throw inside pdf.js's text loop lands: the stream stops.
struct Stop;

pub struct TextExtractor<'x, 'p> {
    pub xref: &'x XRef<'p>,
    pub fonts: &'x mut FontLoader,
    pub view_box: [f64; 4],
    pub items: Vec<TextItem>,
}

/// JavaScript's `Math.sign`.
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x // ±0 and NaN pass through
    }
}

impl<'x, 'p> TextExtractor<'x, 'p> {
    /// One call of getTextContent over a content stream.
    pub fn run(
        &mut self,
        content: &[u8],
        resources: &Rc<Dict>,
        initial: Option<TextStateInit>,
        seen: &HashSet<Ref>,
    ) {
        let state = match initial {
            Some(i) => i.0,
            None => TextState::new(),
        };
        let mut sm = StateManager {
            state,
            stack: Vec::new(),
        };
        let mut run = Run {
            chunk: Chunk::new(),
            two_last_chars: [" ".to_string(), " ".to_string()],
            two_last_chars_pos: 0,
            current: None,
        };
        let _ = self.run_inner(content, resources, &mut sm, &mut run, seen);
        // Whether it finished or threw, pdf.js flushes what it has.
        self.flush(&mut run.chunk);
    }

    fn run_inner(
        &mut self,
        content: &[u8],
        resources: &Rc<Dict>,
        sm: &mut StateManager,
        run: &mut Run,
        seen: &HashSet<Ref>,
    ) -> Result<(), Stop> {
        let mut pre = Preprocessor::new(content).map_err(|_| Stop)?;
        let mut show_spaced_text_buffer: Vec<u16> = Vec::new();
        loop {
            let operation = match pre.read(sm) {
                Ok(Some(op)) => op,
                Ok(None) => break,
                Err(_) => return Err(Stop),
            };
            if run.current.is_none() {
                run.current = Some(sm.state.clone());
            }
            let args = operation.args;
            match operation.op {
                Op::SetFont => {
                    let font_name_arg = match args.first() {
                        Some(Obj::Name(n)) => FName::Name(n.to_string()),
                        _ => FName::Undefined,
                    };
                    let font_size_arg = args.get(1).map(js_to_number).unwrap_or(f64::NAN);
                    let ts = &sm.state;
                    if ts.font.is_some()
                        && font_name_arg == ts.font_name
                        && font_size_arg == ts.font_size
                    {
                        continue;
                    }
                    sm.state.font_name = font_name_arg.clone();
                    sm.state.font_size = font_size_arg;
                    let name = match &font_name_arg {
                        FName::Name(n) => Some(n.as_str()),
                        _ => None,
                    };
                    self.handle_set_font(&mut sm.state, name, None, resources);
                }
                Op::SetTextRise => sm.state.text_rise = arg(&args, 0),
                Op::SetHScale => sm.state.text_h_scale = arg(&args, 0) / 100.0,
                Op::SetLeading => sm.state.leading = arg(&args, 0),
                Op::MoveText => {
                    sm.state
                        .translate_text_line_matrix(arg(&args, 0), arg(&args, 1));
                    sm.state.text_matrix = sm.state.text_line_matrix;
                }
                Op::SetLeadingMoveText => {
                    sm.state.leading = -arg(&args, 1);
                    sm.state
                        .translate_text_line_matrix(arg(&args, 0), arg(&args, 1));
                    sm.state.text_matrix = sm.state.text_line_matrix;
                }
                Op::NextLine => sm.state.carriage_return(),
                Op::SetTextMatrix => {
                    let m = [
                        arg(&args, 0),
                        arg(&args, 1),
                        arg(&args, 2),
                        arg(&args, 3),
                        arg(&args, 4),
                        arg(&args, 5),
                    ];
                    sm.state.text_matrix = m;
                    sm.state.text_line_matrix = m;
                    self.update_advance_scale(&sm.state, &mut run.chunk);
                }
                Op::SetCharSpacing => sm.state.char_spacing = arg(&args, 0),
                Op::SetWordSpacing => sm.state.word_spacing = arg(&args, 0),
                Op::BeginText => {
                    sm.state.text_matrix = IDENTITY;
                    sm.state.text_line_matrix = IDENTITY;
                }
                Op::EndText => {}
                Op::ShowSpacedText => {
                    if sm.state.font.is_none() {
                        continue;
                    }
                    let font = sm.state.font.clone();
                    let vertical = font.as_ref().is_some_and(|f| f.vertical);
                    let space_factor =
                        (if vertical { 1.0 } else { -1.0 }) * sm.state.font_size / 1000.0;
                    let items = match args.first() {
                        Some(Obj::Array(a)) => a.clone(),
                        // `for (const item of args[0])` throws on a non-iterable;
                        // a string is iterable by char, each one a 1-char string.
                        Some(Obj::Str(s)) => {
                            Rc::new(s.iter().map(|b| Obj::Str(Rc::from(vec![*b]))).collect())
                        }
                        _ => return Err(Stop),
                    };
                    for item in items.iter() {
                        match item {
                            Obj::Str(s) => {
                                show_spaced_text_buffer.extend(s.iter().map(|&b| b as u16))
                            }
                            Obj::Num(n) if *n != 0.0 => {
                                let chars = std::mem::take(&mut show_spaced_text_buffer);
                                self.build_text_content_item(sm, run, &chars, n * space_factor)?;
                            }
                            _ => {}
                        }
                    }
                    if !show_spaced_text_buffer.is_empty() {
                        let chars = std::mem::take(&mut show_spaced_text_buffer);
                        self.build_text_content_item(sm, run, &chars, 0.0)?;
                    }
                }
                Op::ShowText => {
                    if sm.state.font.is_none() {
                        continue;
                    }
                    let chars = str_chars(args.first());
                    self.build_text_content_item(sm, run, &chars, 0.0)?;
                }
                Op::NextLineShowText => {
                    if sm.state.font.is_none() {
                        continue;
                    }
                    sm.state.carriage_return();
                    let chars = str_chars(args.first());
                    self.build_text_content_item(sm, run, &chars, 0.0)?;
                }
                Op::NextLineSetSpacingShowText => {
                    if sm.state.font.is_none() {
                        continue;
                    }
                    sm.state.word_spacing = arg(&args, 0);
                    sm.state.char_spacing = arg(&args, 1);
                    sm.state.carriage_return();
                    let chars = str_chars(args.get(2));
                    self.build_text_content_item(sm, run, &chars, 0.0)?;
                }
                Op::PaintXObject => {
                    self.flush(&mut run.chunk);
                    self.paint_xobject(&args, resources, sm, seen);
                }
                Op::SetGState => {
                    self.set_gstate(&args, resources, sm, run);
                }
                Op::BeginMarkedContent | Op::BeginMarkedContentProps | Op::EndMarkedContent => {
                    self.flush(&mut run.chunk);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_set_font(
        &mut self,
        ts: &mut TextState,
        name: Option<&str>,
        font_ref: Option<&Obj>,
        resources: &Dict,
    ) {
        let t = self.fonts.load_font(self.xref, name, font_ref, resources);
        ts.loaded_name = Some(t.loaded_name.clone());
        ts.font_matrix = t
            .font
            .font_matrix
            .clone()
            .unwrap_or_else(|| FONT_IDENTITY_MATRIX.to_vec());
        ts.font = Some(t.font.clone());
    }

    fn set_gstate(
        &mut self,
        args: &[Obj],
        resources: &Rc<Dict>,
        sm: &mut StateManager,
        run: &mut Run,
    ) {
        // Any failure here is a warning in pdf.js (ignoreErrors) and the
        // stream goes on.
        let Some(Obj::Name(name)) = args.first() else {
            return;
        };
        let ext = self.xref.get(resources, "ExtGState");
        let Some(ext) = ext.dict() else { return };
        let gs = self.xref.get(ext, name);
        let Some(gs) = gs.dict() else { return };
        let font = self.xref.get(gs, "Font");
        if !font.truthy() {
            return;
        }
        self.flush(&mut run.chunk);
        let Some(arr) = font.array() else {
            // gStateFont[1] on a non-array is undefined; gStateFont[0] too.
            sm.state.font_name = FName::Null;
            sm.state.font_size = f64::NAN;
            let ts = &mut sm.state;
            self.handle_set_font(ts, None, Some(&Obj::Null), resources);
            return;
        };
        sm.state.font_name = FName::Null;
        sm.state.font_size = arr.get(1).map(js_to_number).unwrap_or(f64::NAN);
        let f0 = arr.first().cloned().unwrap_or(Obj::Null);
        let ts = &mut sm.state;
        self.handle_set_font(ts, None, Some(&f0), resources);
    }

    fn paint_xobject(
        &mut self,
        args: &[Obj],
        resources: &Rc<Dict>,
        sm: &StateManager,
        seen: &HashSet<Ref>,
    ) {
        let Some(Obj::Name(name)) = args.first() else {
            return;
        };
        let xobjs = self.xref.get(resources, "XObject");
        let Some(xobjs) = xobjs.dict() else { return };
        let raw = xobjs.get_raw(name).cloned();
        let xobj = match &raw {
            Some(o) => self.xref.resolve(o),
            None => return,
        };
        let Obj::Stream(s) = xobj else { return };
        let dict = &s.dict;
        let ty = self.xref.get(dict, "Subtype");
        let Some(ty) = ty.name() else { return };
        if ty != "Form" {
            return;
        }
        let obj_id = dict.obj_id;
        if let Some(id) = obj_id
            && seen.contains(&id)
        {
            // "getTextContent - ignoring circular reference"
            return;
        }
        if seen.len() > 64 {
            return;
        }
        let mut state = sm.state.clone();
        if let Some(m) = self.xref.get_array(dict, "Matrix")
            && m.len() == 6
            && m.iter().all(|o| matches!(o, Obj::Num(_)))
        {
            let mv: Vec<f64> = m.iter().map(js_to_number).collect();
            state.ctm = transform(&state.ctm, &mv);
        }
        let local = self.xref.get(dict, "Resources");
        let res = match local {
            Obj::Dict(d) => d,
            _ => resources.clone(),
        };
        let Some(bytes) = self.xref.stream_bytes(&s) else {
            return;
        };
        let mut seen2 = seen.clone();
        if let Some(id) = obj_id {
            seen2.insert(id);
        }
        self.run(&bytes, &res, Some(TextStateInit(state)), &seen2);
    }

    fn current_text_transform(ts: &TextState) -> [f64; 6] {
        let font = ts.font.as_ref();
        let mut tsm = [
            ts.font_size * ts.text_h_scale,
            0.0,
            0.0,
            ts.font_size,
            0.0,
            ts.text_rise,
        ];
        if let Some(f) = font
            && f.is_type3
            && (ts.font_size <= 1.0 || f.is_char_bbox)
            && ts.font_matrix != FONT_IDENTITY_MATRIX
            && let Some(bbox) = f.bbox
        {
            let glyph_height = bbox[3] - bbox[1];
            if glyph_height > 0.0 {
                tsm[3] *= glyph_height * ts.font_matrix[3];
            }
        }
        let a = transform(&ts.text_matrix, &tsm);
        transform(&ts.ctm, &a)
    }

    fn ensure_text_content_item(&mut self, ts: &TextState, c: &mut Chunk) {
        if c.initialized {
            return;
        }
        c.font_name = ts.loaded_name.clone();
        let trm = Self::current_text_transform(ts);
        c.transform = Some(trm);
        let vertical = ts.font.as_ref().is_some_and(|f| f.vertical);
        if !vertical {
            c.width = 0.0;
            c.total_width = 0.0;
            c.height = trm[2].hypot(trm[3]);
            c.total_height = c.height;
            c.vertical = false;
        } else {
            c.width = trm[0].hypot(trm[1]);
            c.total_width = c.width;
            c.height = 0.0;
            c.total_height = 0.0;
            c.vertical = true;
        }
        let scale_line_x = ts.text_line_matrix[0].hypot(ts.text_line_matrix[1]);
        let scale_ctm_x = ts.ctm[0].hypot(ts.ctm[1]);
        c.text_advance_scale = scale_ctm_x * scale_line_x;
        let font_size = ts.font_size;
        c.tracking_space_min = font_size * TRACKING_SPACE_FACTOR;
        c.not_a_space = font_size * NOT_A_SPACE_FACTOR;
        c.negative_space_max = font_size * NEGATIVE_SPACE_FACTOR;
        c.space_in_flow_min = font_size * SPACE_IN_FLOW_MIN_FACTOR;
        c.space_in_flow_max = font_size * SPACE_IN_FLOW_MAX_FACTOR;
        c.has_eol = false;
        c.initialized = true;
    }

    fn update_advance_scale(&mut self, ts: &TextState, c: &mut Chunk) {
        if !c.initialized {
            return;
        }
        let scale_line_x = ts.text_line_matrix[0].hypot(ts.text_line_matrix[1]);
        let scale_ctm_x = ts.ctm[0].hypot(ts.ctm[1]);
        let scale_factor = scale_ctm_x * scale_line_x;
        if scale_factor == c.text_advance_scale {
            return;
        }
        if !c.vertical {
            c.total_width += c.width * c.text_advance_scale;
            c.width = 0.0;
        } else {
            c.total_height += c.height * c.text_advance_scale;
            c.height = 0.0;
        }
        c.text_advance_scale = scale_factor;
    }

    fn flush(&mut self, c: &mut Chunk) {
        if !c.initialized {
            return;
        }
        if !c.vertical {
            c.total_width += c.width * c.text_advance_scale;
        } else {
            c.total_height += c.height * c.text_advance_scale;
        }
        let text = normalize_unicode(&c.str.concat());
        let b = bidi(&text, c.vertical);
        self.items.push(TextItem {
            str: b.str,
            dir: b.dir.to_string(),
            width: c.total_width.abs(),
            height: c.total_height.abs(),
            transform: c.transform.unwrap_or(IDENTITY),
            font_name: c.font_name.clone().unwrap_or_default(),
            has_eol: c.has_eol,
        });
        c.initialized = false;
        c.str.clear();
    }

    fn push_whitespace(
        &mut self,
        width: f64,
        height: f64,
        transform: Option<[f64; 6]>,
        font_name: Option<String>,
    ) {
        self.items.push(TextItem {
            str: " ".to_string(),
            dir: "ltr".to_string(),
            width,
            height,
            transform: transform.unwrap_or(IDENTITY),
            font_name: font_name.unwrap_or_default(),
            has_eol: false,
        });
    }

    fn append_eol(&mut self, ts: &TextState, run: &mut Run) {
        run.reset_last_chars();
        if run.chunk.initialized {
            run.chunk.has_eol = true;
            self.flush(&mut run.chunk);
        } else {
            self.items.push(TextItem {
                str: String::new(),
                dir: "ltr".to_string(),
                width: 0.0,
                height: 0.0,
                transform: Self::current_text_transform(ts),
                font_name: ts.loaded_name.clone().unwrap_or_default(),
                has_eol: true,
            });
        }
    }

    fn add_fake_spaces(
        &mut self,
        ts: &TextState,
        run: &mut Run,
        mut width: f64,
        transf: Option<[f64; 6]>,
        orientation: f64,
    ) -> bool {
        let c = &mut run.chunk;
        if orientation * c.space_in_flow_min <= width && width <= orientation * c.space_in_flow_max
        {
            if c.initialized {
                run.reset_last_chars();
                run.chunk.str.push(" ".to_string());
            }
            return false;
        }
        let font_name = c.font_name.clone();
        let mut height = 0.0;
        if c.vertical {
            height = width;
            width = 0.0;
        }
        self.flush(&mut run.chunk);
        run.reset_last_chars();
        let t = transf.unwrap_or_else(|| Self::current_text_transform(ts));
        self.push_whitespace(width.abs(), height.abs(), Some(t), font_name);
        true
    }

    fn compare_with_last_position(
        &mut self,
        ts: &TextState,
        run: &mut Run,
        glyph_width: f64,
    ) -> bool {
        let current = Self::current_text_transform(ts);
        let mut pos_x = current[4];
        let mut pos_y = current[5];
        let vb = self.view_box;
        let vertical = ts.font.as_ref().is_some_and(|f| f.vertical);
        if vertical {
            if pos_x < vb[0] || pos_x > vb[2] || pos_y + glyph_width < vb[1] || pos_y > vb[3] {
                return false;
            }
        } else if pos_x + glyph_width < vb[0] || pos_x > vb[2] || pos_y < vb[1] || pos_y > vb[3] {
            return false;
        }
        let Some(prev) = run.chunk.prev_transform else {
            return true;
        };
        if ts.font.is_none() {
            return true;
        }
        let mut last_x = prev[4];
        let mut last_y = prev[5];
        if last_x == pos_x && last_y == pos_y {
            return true;
        }
        let mut rotate = -1;
        if current[0] != 0.0 && !current[0].is_nan() && current[1] == 0.0 && current[2] == 0.0 {
            rotate = if current[0] > 0.0 { 0 } else { 180 };
        } else if current[1] != 0.0
            && !current[1].is_nan()
            && current[0] == 0.0
            && current[3] == 0.0
        {
            rotate = if current[1] > 0.0 { 90 } else { 270 };
        }
        match rotate {
            0 => {}
            90 => {
                std::mem::swap(&mut pos_x, &mut pos_y);
                std::mem::swap(&mut last_x, &mut last_y);
            }
            180 => {
                pos_x = -pos_x;
                pos_y = -pos_y;
                last_x = -last_x;
                last_y = -last_y;
            }
            270 => {
                let (px, py) = (pos_x, pos_y);
                pos_x = -py;
                pos_y = -px;
                let (lx, ly) = (last_x, last_y);
                last_x = -ly;
                last_y = -lx;
            }
            _ => {
                let inv = |x: f64, y: f64, m: &[f64; 6]| {
                    let scale = m[0].hypot(m[1]);
                    ((m[0] * x + m[1] * y) / scale, (m[2] * x + m[3] * y) / scale)
                };
                (pos_x, pos_y) = inv(pos_x, pos_y, &current);
                (last_x, last_y) = inv(last_x, last_y, &prev);
            }
        }
        if vertical {
            let c = &run.chunk;
            let advance_y = (last_y - pos_y) / c.text_advance_scale;
            let advance_x = pos_x - last_x;
            let orientation = sign(if c.height != 0.0 && !c.height.is_nan() {
                c.height
            } else {
                c.total_height
            });
            if advance_y < orientation * c.negative_space_max {
                if advance_x.abs() > 0.5 * c.width {
                    self.append_eol(ts, run);
                    return true;
                }
                run.reset_last_chars();
                self.flush(&mut run.chunk);
                return true;
            }
            if advance_x.abs() > c.width {
                self.append_eol(ts, run);
                return true;
            }
            if advance_y <= orientation * c.not_a_space {
                run.reset_last_chars();
            }
            if advance_y <= orientation * run.chunk.tracking_space_min {
                if run.should_add_whitespace() {
                    run.reset_last_chars();
                    self.flush(&mut run.chunk);
                    let (t, f) = (run.chunk.prev_transform, run.chunk.font_name.clone());
                    self.push_whitespace(0.0, advance_y.abs(), t, f);
                } else {
                    run.chunk.height += advance_y;
                }
            } else if !self.add_fake_spaces(ts, run, advance_y, Some(prev), orientation) {
                if run.chunk.str.is_empty() {
                    run.reset_last_chars();
                    let (t, f) = (run.chunk.prev_transform, run.chunk.font_name.clone());
                    self.push_whitespace(0.0, advance_y.abs(), t, f);
                } else {
                    run.chunk.height += advance_y;
                }
            }
            if advance_x.abs() > run.chunk.width * VERTICAL_SHIFT_RATIO {
                self.flush(&mut run.chunk);
            }
            return true;
        }
        let c = &run.chunk;
        let advance_x = (pos_x - last_x) / c.text_advance_scale;
        let advance_y = pos_y - last_y;
        let orientation = sign(if c.width != 0.0 && !c.width.is_nan() {
            c.width
        } else {
            c.total_width
        });
        if advance_x < orientation * c.negative_space_max {
            if advance_y.abs() > 0.5 * c.height {
                self.append_eol(ts, run);
                return true;
            }
            run.reset_last_chars();
            self.flush(&mut run.chunk);
            return true;
        }
        let text_rise_delta = ts.text_rise - c.prev_text_rise;
        let advance_y_corrected = if text_rise_delta == 0.0 {
            advance_y
        } else {
            advance_y - current[3] / ts.font_size * text_rise_delta
        };
        if advance_y_corrected.abs() > c.height {
            self.append_eol(ts, run);
            return true;
        }
        if advance_x <= orientation * c.not_a_space {
            run.reset_last_chars();
        }
        if advance_x <= orientation * run.chunk.tracking_space_min {
            if run.should_add_whitespace() {
                run.reset_last_chars();
                self.flush(&mut run.chunk);
                let (t, f) = (run.chunk.prev_transform, run.chunk.font_name.clone());
                self.push_whitespace(advance_x.abs(), 0.0, t, f);
            } else {
                run.chunk.width += advance_x;
            }
        } else if !self.add_fake_spaces(ts, run, advance_x, Some(prev), orientation) {
            if run.chunk.str.is_empty() {
                run.reset_last_chars();
                let (t, f) = (run.chunk.prev_transform, run.chunk.font_name.clone());
                self.push_whitespace(advance_x.abs(), 0.0, t, f);
            } else {
                run.chunk.width += advance_x;
            }
        }
        if advance_y.abs() > run.chunk.height * VERTICAL_SHIFT_RATIO {
            self.flush(&mut run.chunk);
        }
        true
    }

    fn build_text_content_item(
        &mut self,
        sm: &mut StateManager,
        run: &mut Run,
        chars: &[u16],
        extra_spacing: f64,
    ) -> Result<(), Stop> {
        // `currentTextState !== textState` always holds: it is a clone.
        {
            let cur = run.current.as_ref().ok_or(Stop)?;
            let ts = &sm.state;
            let size_differs = cur.font_size != ts.font_size;
            let differs = size_differs || {
                cur.font_name != ts.font_name && {
                    // `currentTextState.font.name`: a TypeError on a null font.
                    let cf = cur.font.as_ref().ok_or(Stop)?;
                    let tf = ts.font.as_ref().ok_or(Stop)?;
                    cf.name != tf.name || cf.vertical != tf.vertical
                }
            };
            if differs {
                self.flush(&mut run.chunk);
                run.current = Some(sm.state.clone());
            }
        }
        let font = sm.state.font.clone().ok_or(Stop)?;
        let base_char_spacing = if font.vertical {
            -sm.state.char_spacing
        } else {
            sm.state.char_spacing
        };
        if chars.is_empty() {
            let char_spacing = base_char_spacing + extra_spacing;
            if char_spacing != 0.0 && !char_spacing.is_nan() {
                if !font.vertical {
                    let h = sm.state.text_h_scale;
                    sm.state.translate_text_matrix(char_spacing * h, 0.0);
                } else {
                    sm.state.translate_text_matrix(0.0, -char_spacing);
                }
            }
            return Ok(());
        }
        let glyphs = font.chars_to_glyphs(chars);
        let scale = sm.state.font_matrix.first().copied().unwrap_or(f64::NAN) * sm.state.font_size;
        let ii = glyphs.len();
        for (i, glyph) in glyphs.iter().enumerate() {
            if glyph.category.is_invisible_format_mark {
                continue;
            }
            let mut char_spacing =
                base_char_spacing + if i + 1 == ii { extra_spacing } else { 0.0 };
            let mut glyph_width = glyph.width;
            if font.vertical {
                glyph_width = match glyph.vmetric {
                    Some(v) => v[0],
                    None => -glyph_width,
                };
            }
            let mut scaled_dim = glyph_width * scale;
            if glyph.original_char_code == 0x20 {
                char_spacing += sm.state.word_spacing;
            }
            if glyph.category.is_whitespace {
                if !font.vertical {
                    char_spacing += scaled_dim;
                    let h = sm.state.text_h_scale;
                    sm.state.translate_text_matrix(char_spacing * h, 0.0);
                } else {
                    char_spacing += -scaled_dim;
                    sm.state.translate_text_matrix(0.0, -char_spacing);
                }
                run.save_last_char(" ");
                continue;
            }
            if !glyph.category.is_zero_width_diacritic
                && !self.compare_with_last_position(&sm.state, run, scaled_dim)
            {
                if !font.vertical {
                    let h = sm.state.text_h_scale;
                    sm.state.translate_text_matrix(scaled_dim * h, 0.0);
                } else {
                    sm.state.translate_text_matrix(0.0, scaled_dim);
                }
                continue;
            }
            self.ensure_text_content_item(&sm.state, &mut run.chunk);
            if glyph.category.is_zero_width_diacritic {
                scaled_dim = 0.0;
            }
            if !font.vertical {
                scaled_dim *= sm.state.text_h_scale;
                sm.state.translate_text_matrix(scaled_dim, 0.0);
                run.chunk.width += scaled_dim;
            } else {
                sm.state.translate_text_matrix(0.0, scaled_dim);
                scaled_dim = scaled_dim.abs();
                run.chunk.height += scaled_dim;
            }
            if scaled_dim != 0.0 && !scaled_dim.is_nan() {
                run.chunk.prev_transform = Some(Self::current_text_transform(&sm.state));
                run.chunk.prev_text_rise = sm.state.text_rise;
            }
            if run.save_last_char(&glyph.unicode) {
                run.chunk.str.push(" ".to_string());
            }
            run.chunk.str.push(glyph.unicode.clone());
            if char_spacing != 0.0 && !char_spacing.is_nan() {
                if !font.vertical {
                    let h = sm.state.text_h_scale;
                    sm.state.translate_text_matrix(char_spacing * h, 0.0);
                } else {
                    sm.state.translate_text_matrix(0.0, -char_spacing);
                }
            }
        }
        Ok(())
    }
}

/// The opaque initial state a form XObject's text pass starts from.
pub struct TextStateInit(TextState);

struct Run {
    chunk: Chunk,
    two_last_chars: [String; 2],
    two_last_chars_pos: usize,
    current: Option<TextState>,
}

impl Run {
    fn save_last_char(&mut self, ch: &str) -> bool {
        let next_pos = (self.two_last_chars_pos + 1) % 2;
        let ret = self.two_last_chars[self.two_last_chars_pos] != " "
            && self.two_last_chars[next_pos] == " ";
        self.two_last_chars[self.two_last_chars_pos] = ch.to_string();
        self.two_last_chars_pos = next_pos;
        ret
    }
    fn should_add_whitespace(&self) -> bool {
        self.two_last_chars[self.two_last_chars_pos] != " "
            && self.two_last_chars[(self.two_last_chars_pos + 1) % 2] == " "
    }
    fn reset_last_chars(&mut self) {
        self.two_last_chars[0] = " ".to_string();
        self.two_last_chars[1] = " ".to_string();
        self.two_last_chars_pos = 0;
    }
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

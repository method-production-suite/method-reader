//! Fonts, as far as text extraction sees them.
//!
//! Ported from pdf.js `src/core/evaluator.js` (loadFont, preEvaluateFont,
//! translateFont, extractDataStructures, buildToUnicode, readToUnicode,
//! extractWidths) and `src/core/fonts.js` (the Font constructor's
//! bookkeeping, fallbackToSystemFont, charToGlyph), Mozilla Foundation,
//! Apache License 2.0.
//!
//! What this does NOT port is the font-program rebuilding pdf.js does for
//! the browser. Its effects on text are reproduced where they exist (see
//! truetype.rs); embedded Type 1 and CFF programs are not parsed yet, which
//! matters only for a font with no ToUnicode map (see the crate's NOTES).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::cmap::{self, CMap, CVal};
use crate::obj::{Dict, Obj, Ref, XRef, latin1};
use crate::tables;
use crate::truetype;
use crate::unicode::{self, Category, Encoding};

pub const FONT_IDENTITY_MATRIX: [f64; 6] = [0.001, 0.0, 0.0, 0.001, 0.0, 0.0];

mod flags {
    pub const FIXED_PITCH: i64 = 1;
    pub const SERIF: i64 = 2;
    pub const SYMBOLIC: i64 = 4;
    pub const NONSYMBOLIC: i64 = 32;
}

/// A ToUnicode value: pdf.js stores strings, except in the one
/// fallbackToSystemFont branch that stores code points.
#[derive(Clone, Debug)]
pub enum UVal {
    Str(String),
    Num(u32),
}

#[derive(Clone, Debug)]
pub enum ToUnicode {
    Map { map: BTreeMap<u32, UVal>, len: u64 },
    Identity { first: i64, last: i64 },
}

impl ToUnicode {
    fn from_sparse(map: BTreeMap<u32, UVal>) -> Self {
        let len = map.keys().next_back().map(|k| *k as u64 + 1).unwrap_or(0);
        ToUnicode::Map { map, len }
    }
    pub fn length(&self) -> i64 {
        match self {
            ToUnicode::Map { len, .. } => *len as i64,
            ToUnicode::Identity { first, last } => last + 1 - first,
        }
    }
    pub fn has(&self, i: u32) -> bool {
        match self {
            ToUnicode::Map { map, .. } => map.contains_key(&i),
            ToUnicode::Identity { first, last } => *first <= i as i64 && i as i64 <= *last,
        }
    }
    pub fn get(&self, i: u32) -> Option<UVal> {
        match self {
            ToUnicode::Map { map, .. } => map.get(&i).cloned(),
            ToUnicode::Identity { first, last } => {
                if *first <= i as i64 && i as i64 <= *last {
                    Some(UVal::Str(unicode::from_char_code(i).to_string()))
                } else {
                    None
                }
            }
        }
    }
    /// `forEach((charCode, unicodeCodePoint) => ...)`.
    fn for_each(&self, mut f: impl FnMut(u32, u32)) {
        match self {
            ToUnicode::Map { map, .. } => {
                for (k, v) in map {
                    let cp = match v {
                        UVal::Str(s) => s.chars().next().map(|c| c as u32).unwrap_or(0),
                        UVal::Num(n) => *n,
                    };
                    f(*k, cp);
                }
            }
            ToUnicode::Identity { first, last } => {
                let mut i = *first;
                while i <= *last {
                    f(i as u32, i as u32);
                    i += 1;
                }
            }
        }
    }
    /// `charCodeOf(number)`: a string-valued map never matches a number.
    fn char_code_of(&self, v: u32) -> i64 {
        match self {
            ToUnicode::Map { map, .. } => map
                .iter()
                .find(|(_, x)| matches!(x, UVal::Num(n) if *n == v))
                .map(|(k, _)| *k as i64)
                .unwrap_or(-1),
            ToUnicode::Identity { first, last } => {
                if v as i64 >= *first && v as i64 <= *last {
                    v as i64
                } else {
                    -1
                }
            }
        }
    }
    fn amend(&mut self, m: &BTreeMap<u32, UVal>) {
        if let ToUnicode::Map { map, len } = self {
            for (k, v) in m {
                map.insert(*k, v.clone());
                if *k as u64 + 1 > *len {
                    *len = *k as u64 + 1;
                }
            }
        }
    }
    fn is_identity(&self) -> bool {
        matches!(self, ToUnicode::Identity { .. })
    }
}

/// One glyph as getTextContent uses it.
#[derive(Clone, Debug)]
pub struct Glyph {
    pub original_char_code: u32,
    pub unicode: String,
    pub width: f64,
    pub vmetric: Option<[f64; 3]>,
    pub category: Category,
}

/// A loaded font: pdf.js's `Font`, or its `ErrorFont`.
#[derive(Debug)]
pub struct Font {
    pub error: bool,
    pub name: Option<String>,
    pub font_type: String,
    pub is_type3: bool,
    pub missing_file: bool,
    pub vertical: bool,
    pub composite: bool,
    pub widths: HashMap<u32, f64>,
    pub default_width: f64,
    vmetrics: HashMap<u32, [f64; 3]>,
    default_vmetrics: Option<[f64; 3]>,
    cmap: Option<CMap>,
    pub to_unicode: ToUnicode,
    differences: BTreeMap<u32, String>,
    default_encoding: Vec<String>,
    to_font_char: BTreeMap<u32, u32>,
    pub font_matrix: Option<Vec<f64>>,
    pub bbox: Option<[f64; 4]>,
    pub is_char_bbox: bool,
    glyph_cache: RefCell<HashMap<u32, Glyph>>,
    space_width: RefCell<Option<f64>>,
}

impl Font {
    fn error_font() -> Self {
        Font {
            error: true,
            name: None,
            font_type: String::new(),
            is_type3: false,
            missing_file: true,
            vertical: false,
            composite: false,
            widths: HashMap::new(),
            default_width: 0.0,
            vmetrics: HashMap::new(),
            default_vmetrics: None,
            cmap: None,
            to_unicode: ToUnicode::from_sparse(BTreeMap::new()),
            differences: BTreeMap::new(),
            default_encoding: Vec::new(),
            to_font_char: BTreeMap::new(),
            font_matrix: None,
            bbox: None,
            is_char_bbox: false,
            glyph_cache: RefCell::new(HashMap::new()),
            space_width: RefCell::new(None),
        }
    }

    /// `charsToGlyphs`. `chars` holds the string's char codes.
    pub fn chars_to_glyphs(&self, chars: &[u16]) -> Vec<Glyph> {
        if self.error {
            return Vec::new();
        }
        let mut glyphs = Vec::new();
        if let Some(cmap) = &self.cmap {
            let mut i = 0;
            while i < chars.len() {
                let (charcode, length) = cmap.read_char_code(chars, i);
                i += length;
                glyphs.push(self.char_to_glyph(charcode));
            }
        } else {
            for &c in chars {
                glyphs.push(self.char_to_glyph(c as u32));
            }
        }
        glyphs
    }

    fn space_width(&self) -> f64 {
        if let Some(w) = *self.space_width.borrow() {
            return w;
        }
        let mut width: Option<f64> = None;
        for glyph_name in ["space", "minus", "one", "i", "I"] {
            // `glyphName in this.widths`: the widths are keyed by char code
            // by the time a Font exists, so no name is ever found.
            let glyph_unicode = unicode::glyphs_unicode()
                .get(glyph_name)
                .copied()
                .unwrap_or(0);
            let mut charcode: i64 = 0;
            if self.composite
                && let Some(cm) = &self.cmap
                && cm.contains(glyph_unicode)
            {
                charcode = match cm.lookup(glyph_unicode) {
                    Some(CVal::Int(i)) => i,
                    Some(CVal::Str(s)) => convert_cid_string(&s).unwrap_or(0),
                    None => 0,
                };
            }
            if charcode == 0 {
                charcode = self.to_unicode.char_code_of(glyph_unicode);
            }
            if charcode <= 0 {
                charcode = glyph_unicode as i64;
            }
            let w = self.widths.get(&(charcode as u32)).copied();
            width = w;
            if let Some(w) = w
                && w != 0.0
                && !w.is_nan()
            {
                break;
            }
        }
        let w = match width {
            Some(w) if w != 0.0 && !w.is_nan() => w,
            _ => self.default_width,
        };
        *self.space_width.borrow_mut() = Some(w);
        w
    }

    fn char_to_glyph(&self, charcode: u32) -> Glyph {
        if let Some(g) = self.glyph_cache.borrow().get(&charcode) {
            return g.clone();
        }
        let mut width_code = charcode;
        if let Some(cm) = &self.cmap
            && cm.contains(charcode)
        {
            match cm.lookup(charcode) {
                Some(CVal::Int(i)) => width_code = i as u32,
                Some(CVal::Str(s)) => {
                    width_code = convert_cid_string(&s).map(|x| x as u32).unwrap_or(u32::MAX)
                }
                None => {}
            }
        }
        let mut width = self
            .widths
            .get(&width_code)
            .copied()
            .unwrap_or(self.default_width);
        let vmetric = self
            .vmetrics
            .get(&width_code)
            .copied()
            .or(self.default_vmetrics);
        let mut unicode = match self.to_unicode.get(charcode) {
            Some(UVal::Str(s)) if !s.is_empty() => s,
            Some(UVal::Num(n)) if n != 0 => unicode::from_char_code(n).to_string(),
            _ => unicode::from_char_code(charcode).to_string(),
        };
        let mut font_char_code: u32 = match self.to_font_char.get(&charcode) {
            Some(&c) if c != 0 => c,
            _ => charcode,
        };
        if self.missing_file {
            let glyph_name = self
                .differences
                .get(&charcode)
                .filter(|s| !s.is_empty())
                .cloned()
                .or_else(|| self.default_encoding.get(charcode as usize).cloned());
            if let Some(gn) = &glyph_name
                && (gn == ".notdef" || gn.is_empty())
                && self.font_type == "Type1"
            {
                font_char_code = 0x20;
                if gn.is_empty() {
                    if width == 0.0 || width.is_nan() {
                        width = self.space_width();
                    }
                    unicode = " ".to_string();
                }
            }
            font_char_code = unicode::map_special_unicode_values(font_char_code);
        }
        if self.missing_file
            && self.vertical
            && font_char_code <= 0xffff
            && let Some(v) = unicode::vertical_presentation_form(font_char_code)
        {
            unicode = unicode::from_char_code(v).to_string();
        }
        let category = unicode::char_unicode_category(&unicode);
        let g = Glyph {
            original_char_code: charcode,
            unicode,
            width,
            vmetric,
            category,
        };
        self.glyph_cache.borrow_mut().insert(charcode, g.clone());
        g
    }
}

fn convert_cid_string(s: &[u16]) -> Option<i64> {
    match s.len() {
        1 => Some(s[0] as i64),
        2 => Some(((s[0] as i64) << 8) | s[1] as i64),
        _ => None,
    }
}

/// What a font load yields: pdf.js's TranslatedFont.
pub struct Translated {
    pub loaded_name: String,
    pub font: Rc<Font>,
    order: u32,
}

/// pdf.js `properties`, the bag translateFont fills.
struct Props {
    font_type: String,
    name: String,
    composite: bool,
    has_file: bool,
    file: Option<Vec<u8>>,
    flags: i64,
    is_simulated_flags: bool,
    first_char: i64,
    last_char: i64,
    widths: HashMap<u32, f64>,
    default_width: f64,
    vmetrics: HashMap<u32, [f64; 3]>,
    default_vmetrics: Option<[f64; 3]>,
    cmap: Option<CMap>,
    vertical: bool,
    cid_encoding: String,
    cid_system_info: Option<(String, String)>,
    cid_to_gid_map: Option<BTreeMap<u32, u32>>,
    differences: BTreeMap<u32, String>,
    default_encoding: Vec<String>,
    default_encoding_name: Option<&'static str>,
    base_encoding_name: Option<&'static str>,
    has_encoding: bool,
    to_unicode: Option<ToUnicode>,
    has_included_to_unicode_map: bool,
    fallback_to_unicode: Option<BTreeMap<u32, UVal>>,
    font_matrix: Option<Vec<f64>>,
    bbox: Option<[f64; 4]>,
    is_type3: bool,
}

impl Props {
    fn new(font_type: &str, name: &str) -> Self {
        Props {
            font_type: font_type.to_string(),
            name: name.to_string(),
            composite: false,
            has_file: false,
            file: None,
            flags: 0,
            is_simulated_flags: false,
            first_char: 0,
            last_char: 0,
            widths: HashMap::new(),
            default_width: 0.0,
            vmetrics: HashMap::new(),
            default_vmetrics: None,
            cmap: None,
            vertical: false,
            cid_encoding: String::new(),
            cid_system_info: None,
            cid_to_gid_map: None,
            differences: BTreeMap::new(),
            default_encoding: Vec::new(),
            default_encoding_name: None,
            base_encoding_name: None,
            has_encoding: false,
            to_unicode: None,
            has_included_to_unicode_map: false,
            fallback_to_unicode: None,
            font_matrix: None,
            bbox: None,
            is_type3: false,
        }
    }
}

fn enc_vec(e: Encoding) -> Vec<String> {
    e.iter().map(|s| s.to_string()).collect()
}

fn is_serif_font(base_font_name: &str) -> bool {
    let wo_style = base_font_name.split('-').next().unwrap_or("");
    unicode::is_serif_font_name(wo_style) || wo_style.to_ascii_lowercase().contains("serif")
}

/// `getBaseFontMetrics`: (defaultWidth, monospace, widths by glyph name).
fn base_font_metrics(name: &str) -> (f64, bool, HashMap<&'static str, f64>) {
    let mut font_name = unicode::normalize_font_name(name);
    if let Some(s) = unicode::std_font_map().get(font_name.as_str()) {
        font_name = s.to_string();
    }
    let m = unicode::std_metrics(&font_name).or_else(|| {
        unicode::std_metrics(if is_serif_font(name) {
            "Times-Roman"
        } else {
            "Helvetica"
        })
    });
    match m {
        Some(tables::StdMetrics::Fixed(w)) => (*w, true, HashMap::new()),
        Some(tables::StdMetrics::Table(t)) => (0.0, false, t.iter().copied().collect()),
        None => (0.0, false, HashMap::new()),
    }
}

/// `buildCharCodeToWidth`.
fn build_char_code_to_width(by_name: &HashMap<&'static str, f64>, p: &Props) -> HashMap<u32, f64> {
    let mut widths = HashMap::new();
    for code in 0..256u32 {
        if let Some(d) = p.differences.get(&code)
            && let Some(w) = by_name.get(d.as_str())
            && *w != 0.0
        {
            widths.insert(code, *w);
            continue;
        }
        if let Some(e) = p.default_encoding.get(code as usize)
            && let Some(w) = by_name.get(e.as_str())
            && *w != 0.0
        {
            widths.insert(code, *w);
        }
    }
    widths
}

/// A matrix entry the way pdf.js reads it: six numbers, or nothing.
fn lookup_matrix(a: Option<Vec<Obj>>) -> Option<Vec<f64>> {
    let a = a?;
    if a.len() != 6 {
        return None;
    }
    a.iter().map(|o| o.num()).collect()
}

fn lookup_normal_rect(a: Option<Vec<Obj>>) -> Option<[f64; 4]> {
    let a = a?;
    if a.len() != 4 {
        return None;
    }
    let v: Option<Vec<f64>> = a.iter().map(|o| o.num()).collect();
    let v = v?;
    let mut r = [v[0], v[1], v[2], v[3]];
    if v[0] > v[2] {
        r[0] = v[2];
        r[2] = v[0];
    }
    if v[1] > v[3] {
        r[1] = v[3];
        r[3] = v[1];
    }
    Some(r)
}

pub struct FontLoader {
    next_font_id: u32,
    loads: u32,
    cache: HashMap<FontKey, Rc<Translated>>,
    aliases: HashMap<(Ref, Vec<u8>), (u32, Option<Ref>)>,
    fallback_dict: Rc<Dict>,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
enum FontKey {
    Ref(Ref),
    Ptr(usize),
}

pub struct Failed;

impl Default for FontLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl FontLoader {
    pub fn new() -> Self {
        let mut d = Dict::default();
        d.set("BaseFont", Obj::Name(Rc::from("Helvetica")));
        d.set("Type", Obj::Name(Rc::from("FallbackType")));
        d.set("Subtype", Obj::Name(Rc::from("FallbackType")));
        d.set("Encoding", Obj::Name(Rc::from("WinAnsiEncoding")));
        Self {
            next_font_id: 0,
            loads: 0,
            cache: HashMap::new(),
            aliases: HashMap::new(),
            fallback_dict: Rc::new(d),
        }
    }

    /// `commonObjs.get(loadedName).name`: the first font loaded under that
    /// name, or none (an error font exports no name).
    pub fn name_of(&self, loaded_name: &str) -> Option<String> {
        if loaded_name == "g_font_error" {
            return None;
        }
        self.cache
            .values()
            .filter(|t| t.loaded_name == loaded_name)
            .min_by_key(|t| t.order)
            .and_then(|t| t.font.name.clone())
    }

    fn error_font() -> Rc<Translated> {
        Rc::new(Translated {
            loaded_name: "g_font_error".to_string(),
            font: Rc::new(Font::error_font()),
            order: u32::MAX,
        })
    }

    /// `loadFont(fontName, font, resources)`.
    pub fn load_font(
        &mut self,
        xref: &XRef<'_>,
        font_name: Option<&str>,
        font_arg: Option<&Obj>,
        resources: &Dict,
    ) -> Rc<Translated> {
        let mut font_ref: Option<Ref> = None;
        let mut font: Obj = Obj::Null;
        if let Some(f) = font_arg {
            if let Obj::Ref(r) = f {
                font_ref = Some(*r);
            } else {
                font = f.clone();
            }
        } else if let Some(fr) = xref.get(resources, "Font").dict() {
            match font_name.and_then(|n| fr.get_raw(n)) {
                Some(Obj::Ref(r)) => font_ref = Some(*r),
                Some(o) => font = o.clone(),
                None => {}
            }
        }
        if let Some(r) = font_ref {
            if let Some(t) = self.cache.get(&FontKey::Ref(r)) {
                return t.clone();
            }
            font = xref.fetch(r);
        }
        let dict: Rc<Dict> = match font {
            Obj::Dict(d) => d,
            // ignoreErrors: a missing font falls back to Helvetica.
            _ => self.fallback_dict.clone(),
        };
        let ptr_key = FontKey::Ptr(Rc::as_ptr(&dict) as usize);
        if font_ref.is_none()
            && let Some(t) = self.cache.get(&ptr_key)
        {
            return t.clone();
        }
        let pre = match pre_evaluate_font(xref, &dict) {
            Ok(p) => p,
            Err(_) => return Self::error_font(),
        };
        let font_id;
        if let (Some(hash), Some(desc_ref)) = (&pre.hash, pre.descriptor_ref) {
            let key = (desc_ref, hash.clone());
            match self.aliases.get(&key).copied() {
                Some((id, alias_ref)) => {
                    if let (Some(fr), Some(ar)) = (font_ref, alias_ref)
                        && let Some(t) = self.cache.get(&FontKey::Ref(ar)).cloned()
                    {
                        self.cache.insert(FontKey::Ref(fr), t.clone());
                        return t;
                    }
                    font_id = id;
                    if font_ref.is_some() {
                        self.aliases.insert(key, (id, font_ref));
                    }
                }
                None => {
                    self.next_font_id += 1;
                    font_id = self.next_font_id;
                    self.aliases.insert(key, (font_id, font_ref));
                }
            }
        } else {
            self.next_font_id += 1;
            font_id = self.next_font_id;
        }
        let loaded_name = format!("g_d0_f{font_id}");
        let font = match translate_font(xref, &pre) {
            Ok(f) => f,
            Err(_) => Font::error_font(),
        };
        self.loads += 1;
        let t = Rc::new(Translated {
            loaded_name,
            font: Rc::new(font),
            order: self.loads,
        });
        match font_ref {
            Some(r) => self.cache.insert(FontKey::Ref(r), t.clone()),
            None => self.cache.insert(ptr_key, t.clone()),
        };
        t
    }
}

struct PreEvaluated {
    descriptor: Option<Rc<Dict>>,
    descriptor_ref: Option<Ref>,
    dict: Rc<Dict>,
    base_dict: Rc<Dict>,
    composite: bool,
    font_type: String,
    first_char: i64,
    last_char: i64,
    to_unicode: Obj,
    hash: Option<Vec<u8>>,
}

fn num_to_js_string(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e21 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// `preEvaluateFont`. The hash is kept as the bytes pdf.js would feed its
/// MurmurHash, which only ever serves as an equality key.
fn pre_evaluate_font(xref: &XRef<'_>, base: &Rc<Dict>) -> Result<PreEvaluated, Failed> {
    let mut dict = base.clone();
    let mut font_type = xref.get(&dict, "Subtype").name().ok_or(Failed)?.to_string();
    let mut composite = false;
    if font_type == "Type0" {
        let df = xref.get(&dict, "DescendantFonts");
        let d = match &df {
            Obj::Array(a) => a.first().map(|o| xref.resolve(o)).unwrap_or(Obj::Null),
            Obj::Null => return Err(Failed),
            o => o.clone(),
        };
        dict = d.dict().ok_or(Failed)?.clone();
        font_type = xref.get(&dict, "Subtype").name().ok_or(Failed)?.to_string();
        composite = true;
    }
    let first_char = xref
        .get(&dict, "FirstChar")
        .int()
        .map(|v| v as i64)
        .unwrap_or(0);
    let last_char = xref
        .get(&dict, "LastChar")
        .int()
        .map(|v| v as i64)
        .unwrap_or(if composite { 0xffff } else { 0xff });
    let descriptor_raw = dict.get_raw("FontDescriptor").cloned();
    let descriptor_ref = descriptor_raw.as_ref().and_then(|o| o.as_ref());
    let descriptor = xref.get(&dict, "FontDescriptor").dict().cloned();
    let mut to_unicode = xref.get(&dict, "ToUnicode");
    if !to_unicode.truthy() {
        to_unicode = xref.get(base, "ToUnicode");
    }
    let mut hash = None;
    if let Some(desc) = &descriptor {
        let mut h: Vec<u8> = Vec::new();
        match base.get_raw("Encoding") {
            Some(Obj::Name(n)) => h.extend_from_slice(n.as_bytes()),
            Some(Obj::Ref(r)) => h.extend_from_slice(r.to_string().as_bytes()),
            Some(Obj::Dict(e)) => {
                for (_, entry) in &e.entries {
                    match entry {
                        Obj::Name(n) => h.extend_from_slice(n.as_bytes()),
                        Obj::Ref(r) => h.extend_from_slice(r.to_string().as_bytes()),
                        Obj::Array(a) => {
                            let parts: Vec<String> = a
                                .iter()
                                .map(|d| match d {
                                    Obj::Name(n) => n.to_string(),
                                    Obj::Num(n) => num_to_js_string(*n),
                                    Obj::Ref(r) => r.to_string(),
                                    _ => String::new(),
                                })
                                .collect();
                            h.extend_from_slice(parts.join(",").as_bytes());
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        h.extend_from_slice(format!("{first_char}-{last_char}").as_bytes());
        match &to_unicode {
            Obj::Stream(s) => {
                if let Some(b) = xref.stream_bytes(s) {
                    h.extend_from_slice(&b);
                }
            }
            Obj::Name(n) => h.extend_from_slice(n.as_bytes()),
            _ => {}
        }
        let mut widths = xref.get(&dict, "Widths");
        if !widths.truthy() {
            widths = xref.get(base, "Widths");
        }
        if let Obj::Array(a) = &widths {
            let parts: Vec<String> = a
                .iter()
                .filter_map(|e| match e {
                    Obj::Num(n) => Some(num_to_js_string(*n)),
                    Obj::Ref(r) => Some(r.to_string()),
                    _ => None,
                })
                .collect();
            h.extend_from_slice(parts.join(",").as_bytes());
        }
        if composite {
            h.extend_from_slice(b"compositeFont");
            let mut w = xref.get(&dict, "W");
            if !w.truthy() {
                w = xref.get(base, "W");
            }
            if let Obj::Array(a) = &w {
                let mut parts: Vec<String> = Vec::new();
                for e in a.iter() {
                    match e {
                        Obj::Num(n) => parts.push(num_to_js_string(*n)),
                        Obj::Ref(r) => parts.push(r.to_string()),
                        Obj::Array(sub) => {
                            let sp: Vec<String> = sub
                                .iter()
                                .filter_map(|x| match x {
                                    Obj::Num(n) => Some(num_to_js_string(*n)),
                                    Obj::Ref(r) => Some(r.to_string()),
                                    _ => None,
                                })
                                .collect();
                            parts.push(format!("[{}]", sp.join(",")));
                        }
                        _ => {}
                    }
                }
                h.extend_from_slice(parts.join(",").as_bytes());
            }
            let c2g = dict
                .get_raw("CIDToGIDMap")
                .or_else(|| base.get_raw("CIDToGIDMap"));
            match c2g {
                Some(Obj::Name(n)) => h.extend_from_slice(n.as_bytes()),
                Some(Obj::Ref(r)) => h.extend_from_slice(r.to_string().as_bytes()),
                _ => {}
            }
        }
        if font_type == "Type3"
            && let Some(cp) = xref.get(base, "CharProcs").dict()
        {
            for (k, e) in &cp.entries {
                match e {
                    Obj::Ref(r) => h.extend_from_slice(format!("{k}\0{r}").as_bytes()),
                    _ => h.extend_from_slice(k.as_bytes()),
                }
            }
        }
        let _ = desc;
        hash = Some(h);
    }
    Ok(PreEvaluated {
        descriptor,
        descriptor_ref,
        dict,
        base_dict: base.clone(),
        composite,
        font_type,
        first_char,
        last_char,
        to_unicode,
        hash,
    })
}

fn translate_font(xref: &XRef<'_>, pre: &PreEvaluated) -> Result<Font, Failed> {
    let is_type3 = pre.font_type == "Type3";
    let dict = &pre.dict;
    let base_dict = &pre.base_dict;
    let descriptor: Rc<Dict> = match &pre.descriptor {
        Some(d) => d.clone(),
        None if is_type3 || pre.composite => Rc::new(Dict::default()),
        None => {
            // A simple font with no descriptor: the standard 14 path.
            let base_font_name = xref.get(dict, "BaseFont");
            let base_font_name = unicode::normalize_font_name(base_font_name.name().ok_or(Failed)?);
            let (dw, monospace, by_name) = base_font_metrics(&base_font_name);
            let wo_style = base_font_name.split('-').next().unwrap_or("").to_string();
            let fl = (if is_serif_font(&wo_style) {
                flags::SERIF
            } else {
                0
            }) | (if monospace { flags::FIXED_PITCH } else { 0 })
                | (if unicode::is_symbols_font_name(&wo_style) {
                    flags::SYMBOLIC
                } else {
                    flags::NONSYMBOLIC
                });
            let mut p = Props::new(&pre.font_type, &base_font_name);
            p.default_width = dw;
            p.is_simulated_flags = true;
            p.flags = fl;
            p.first_char = pre.first_char;
            p.last_char = pre.last_char;
            p.is_type3 = is_type3;
            let widths = xref.get(dict, "Widths");
            extract_data_structures(xref, dict, &pre.to_unicode, &mut p)?;
            if let Obj::Array(a) = &widths {
                let mut gw = HashMap::new();
                for (j, w) in (pre.first_char..).zip(a.iter()) {
                    if let Some(n) = xref.resolve(w).num()
                        && j >= 0
                    {
                        gw.insert(j as u32, n);
                    }
                }
                p.widths = gw;
            } else {
                p.widths = build_char_code_to_width(&by_name, &p);
            }
            return Ok(build_font(&base_font_name, None, p));
        }
    };

    let mut font_name: Option<String> = match xref.get(&descriptor, "FontName") {
        Obj::Name(n) => Some(n.to_string()),
        Obj::Str(s) => Some(latin1(&s)),
        _ => None,
    };
    let base_font: Option<String> = match xref.get(dict, "BaseFont") {
        Obj::Name(n) => Some(n.to_string()),
        Obj::Str(s) => Some(latin1(&s)),
        _ => None,
    };
    if is_type3 {
        if font_name.as_deref().is_none_or(str::is_empty) {
            font_name = Some(pre.font_type.clone());
        }
    } else if font_name != base_font {
        if let (Some(f), Some(b)) = (&font_name, &base_font)
            && !f.is_empty()
            && !b.is_empty()
            && (b.starts_with(f.as_str())
                || (!unicode::is_known_font_name(f) && unicode::is_known_font_name(b)))
        {
            font_name = None;
        }
        // `fontName ||= baseFont`: a Name object is always truthy.
        if font_name.is_none() {
            font_name = base_font.clone();
        }
    }
    let font_name = font_name.ok_or(Failed)?;

    let mut font_file: Option<Rc<crate::obj::Stream>> = None;
    for n in ["FontFile", "FontFile2", "FontFile3"] {
        let f = xref.get(&descriptor, n);
        if f.truthy() {
            font_file = f.stream().cloned();
            break;
        }
    }
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut subtype: Option<String> = None;
    if let Some(ff) = &font_file {
        let b = xref.stream_bytes(ff).unwrap_or_default();
        if !b.is_empty() {
            subtype = xref.get(&ff.dict, "Subtype").name().map(str::to_string);
            file_bytes = Some(b);
        }
        // An empty font file throws in pdf.js and is then dropped: fontFile
        // becomes null and the font is treated as missing.
    }

    let font_matrix =
        lookup_matrix(xref.get_array(dict, "FontMatrix")).or(Some(FONT_IDENTITY_MATRIX.to_vec()));
    let bbox = lookup_normal_rect(
        xref.get_array(&descriptor, "FontBBox")
            .or_else(|| xref.get_array(dict, "FontBBox")),
    )
    .or(if is_type3 { Some([0.0; 4]) } else { None });
    let fl = xref
        .get(&descriptor, "Flags")
        .int()
        .map(|v| v as i64)
        .unwrap_or(0);

    let mut p = Props::new(&pre.font_type, &font_name);
    p.has_file = file_bytes.is_some();
    p.file = file_bytes;
    p.composite = pre.composite;
    p.font_matrix = font_matrix;
    p.first_char = pre.first_char;
    p.last_char = pre.last_char;
    p.bbox = bbox;
    p.flags = fl;
    p.is_type3 = is_type3;
    let _ = &subtype;

    if pre.composite {
        let cid_encoding = xref.get(base_dict, "Encoding");
        if let Obj::Name(n) = &cid_encoding {
            p.cid_encoding = n.to_string();
        }
        let cm = match &cid_encoding {
            Obj::Name(n) => cmap::create_built_in_cmap(n).map_err(|_| Failed)?,
            Obj::Stream(s) => {
                let bytes = xref.stream_bytes(s).unwrap_or_default();
                cmap::parse_cmap(&bytes).map_err(|_| Failed)?
            }
            _ => return Err(Failed),
        };
        p.vertical = cm.vertical;
        p.cmap = Some(cm);
    }
    extract_data_structures(xref, dict, &pre.to_unicode, &mut p)?;
    extract_widths(xref, dict, &descriptor, &mut p);
    Ok(build_font(&font_name, Some(&descriptor), p))
}

fn extract_data_structures(
    xref: &XRef<'_>,
    dict: &Dict,
    to_unicode_obj: &Obj,
    p: &mut Props,
) -> Result<(), Failed> {
    let to_unicode = read_to_unicode(xref, to_unicode_obj)?;
    if p.composite
        && let Some(csi) = xref.get(dict, "CIDSystemInfo").dict()
        && p.cid_system_info.is_none()
    {
        let s = |k: &str| match xref.get(csi, k) {
            Obj::Str(s) => string_to_pdf_string(&s),
            _ => String::new(),
        };
        p.cid_system_info = Some((s("Registry"), s("Ordering")));
    }
    let mut cid_to_gid_bytes: Option<Vec<u8>> = None;
    if p.composite
        && let Obj::Stream(s) = xref.get(dict, "CIDToGIDMap")
    {
        cid_to_gid_bytes = xref.stream_bytes(&s);
    }
    let mut differences: BTreeMap<u32, String> = BTreeMap::new();
    let mut base_encoding_name: Option<String> = None;
    if dict.has("Encoding") {
        match xref.get(dict, "Encoding") {
            Obj::Dict(enc) => {
                base_encoding_name = xref.get(&enc, "BaseEncoding").name().map(str::to_string);
                if enc.has("Differences") {
                    let diff = xref.get(&enc, "Differences");
                    if let Obj::Array(a) = &diff {
                        let mut index: f64 = 0.0;
                        for entry in a.iter() {
                            match xref.resolve(entry) {
                                Obj::Num(n) => index = n,
                                Obj::Name(n) => {
                                    if index >= 0.0 && index.fract() == 0.0 && index < 4294967295.0
                                    {
                                        differences.insert(index as u32, n.to_string());
                                    }
                                    index += 1.0;
                                }
                                _ => return Err(Failed),
                            }
                        }
                    } else if diff.truthy() {
                        // `for (const entry of diffEncoding)` on a non-array
                        // throws in pdf.js.
                        return Err(Failed);
                    }
                }
            }
            Obj::Name(n) => base_encoding_name = Some(n.to_string()),
            _ => {} // "Encoding is not a Name nor a Dict", ignored
        }
        if !matches!(
            base_encoding_name.as_deref(),
            Some("MacRomanEncoding") | Some("MacExpertEncoding") | Some("WinAnsiEncoding")
        ) {
            base_encoding_name = None;
        }
    }
    let non_embedded_font = !p.has_file;
    let is_symbols_font_name = unicode::is_symbols_font_name(&p.name);
    if base_encoding_name.is_some() && non_embedded_font && is_symbols_font_name {
        base_encoding_name = None;
    }
    if base_encoding_name.as_deref() == Some("WinAnsiEncoding")
        && non_embedded_font
        && p.name.chars().next().is_some_and(|c| c as u32 >= 0xb7)
    {
        const CHINESE: [&str; 10] = [
            "\u{CB}\u{CE}\u{CC}\u{E5}",
            "\u{BA}\u{DA}\u{CC}\u{E5}",
            "\u{BF}\u{AC}\u{CC}\u{E5}",
            "\u{B7}\u{C2}\u{CB}\u{CE}",
            "\u{BF}\u{AC}\u{CC}\u{E5}_GB2312",
            "\u{B7}\u{C2}\u{CB}\u{CE}_GB2312",
            "\u{C1}\u{A5}\u{CA}\u{E9}",
            "\u{D0}\u{C2}\u{CB}\u{CE}",
            "\u{B7}\u{C2}\u{CB}\u{CE}\u{CC}\u{E5}",
            "\u{D0}\u{A1}\u{B1}\u{EA}\u{CB}\u{CE}",
        ];
        if CHINESE.contains(&p.name.as_str()) {
            // pdf.js switches to the GBK-EUC-H CMap here, which it cannot
            // load without cMapUrl: the font fails.
            return Err(Failed);
        }
    }
    let base_enc_static: Option<&'static str> = match base_encoding_name.as_deref() {
        Some("MacRomanEncoding") => Some("MacRomanEncoding"),
        Some("MacExpertEncoding") => Some("MacExpertEncoding"),
        Some("WinAnsiEncoding") => Some("WinAnsiEncoding"),
        _ => None,
    };
    if let Some(b) = base_enc_static {
        p.default_encoding = enc_vec(unicode::get_encoding(b).ok_or(Failed)?);
        p.default_encoding_name = Some(b);
    } else {
        let mut is_symbolic_font = p.flags & flags::SYMBOLIC != 0;
        let is_nonsymbolic_font = p.flags & flags::NONSYMBOLIC != 0;
        if p.font_type == "TrueType"
            && is_symbolic_font
            && is_nonsymbolic_font
            && !differences.is_empty()
        {
            p.flags &= !flags::SYMBOLIC;
            is_symbolic_font = false;
        }
        let mut encoding = "StandardEncoding";
        if p.font_type == "TrueType" && !is_nonsymbolic_font {
            encoding = "WinAnsiEncoding";
        }
        if is_symbolic_font || is_symbols_font_name {
            encoding = "MacRomanEncoding";
            if non_embedded_font {
                let lower = p.name.to_ascii_lowercase();
                if lower.contains("symbol") {
                    encoding = "SymbolSetEncoding";
                } else if lower.contains("dingbats") {
                    encoding = "ZapfDingbatsEncoding";
                } else if lower.contains("wingdings") {
                    encoding = "WinAnsiEncoding";
                }
            }
        }
        p.default_encoding = enc_vec(unicode::get_encoding(encoding).ok_or(Failed)?);
        p.default_encoding_name = Some(encoding);
    }
    p.has_encoding = base_enc_static.is_some() || !differences.is_empty();
    p.differences = differences;
    p.base_encoding_name = base_enc_static;
    p.to_unicode = to_unicode;
    let built = build_to_unicode(p)?;
    p.to_unicode = Some(built);
    if let Some(bytes) = cid_to_gid_bytes {
        let tu = p.to_unicode.as_ref().ok_or(Failed)?;
        let mut result = BTreeMap::new();
        let mut j = 0;
        while j < bytes.len() {
            let hi = bytes[j] as u32;
            j += 1;
            let lo = bytes.get(j).copied().unwrap_or(0) as u32;
            let glyph_id = (hi << 8) | lo;
            let code = (j >> 1) as u32;
            j += 1;
            if glyph_id == 0 && !tu.has(code) {
                continue;
            }
            result.insert(code, glyph_id);
        }
        p.cid_to_gid_map = Some(result);
    }
    Ok(())
}

/// `stringToPDFString`: PDFDocEncoding, or UTF-16 with a BOM.
fn string_to_pdf_string(b: &[u8]) -> String {
    if b.len() >= 2 && b[0] == 0xfe && b[1] == 0xff {
        let u: Vec<u16> = b[2..]
            .chunks(2)
            .map(|c| ((c[0] as u16) << 8) | c.get(1).copied().unwrap_or(0) as u16)
            .collect();
        return String::from_utf16_lossy(&u);
    }
    latin1(b)
}

fn read_to_unicode(xref: &XRef<'_>, o: &Obj) -> Result<Option<ToUnicode>, Failed> {
    match o {
        Obj::Name(n) => {
            let cm = cmap::create_built_in_cmap(n).map_err(|_| Failed)?;
            if cm.identity {
                return Ok(Some(ToUnicode::Identity {
                    first: 0,
                    last: 0xffff,
                }));
            }
            Ok(None)
        }
        Obj::Stream(s) => {
            let bytes = xref.stream_bytes(s).unwrap_or_default();
            let cm = match cmap::parse_cmap(&bytes) {
                Ok(c) => c,
                Err(_) => return Ok(None),
            };
            if cm.identity {
                return Ok(Some(ToUnicode::Identity {
                    first: 0,
                    last: 0xffff,
                }));
            }
            let mut map = BTreeMap::new();
            let mut bad = false;
            cm.for_each(|code, token| match token {
                CVal::Int(i) => match u32::try_from(*i).ok().and_then(char::from_u32) {
                    Some(c) => {
                        map.insert(code, UVal::Str(c.to_string()));
                    }
                    // String.fromCodePoint throws a RangeError, which
                    // readToUnicode catches: no ToUnicode at all.
                    None => {
                        if *i < 0 || *i > 0x10ffff {
                            bad = true;
                        } else {
                            map.insert(code, UVal::Str("\u{fffd}".to_string()));
                        }
                    }
                },
                CVal::Str(t) => {
                    let mut t = t.clone();
                    if t.len() % 2 != 0 {
                        t.insert(0, 0);
                    }
                    let mut out = String::new();
                    let mut k = 0;
                    while k < t.len() {
                        let w1 = ((t[k] as u32) << 8) | t[k + 1] as u32;
                        if (w1 & 0xf800) != 0xd800 {
                            out.push(unicode::from_code_point(w1));
                            k += 2;
                            continue;
                        }
                        k += 2;
                        let w2 = if k + 1 < t.len() {
                            ((t[k] as u32) << 8) | t[k + 1] as u32
                        } else {
                            0 // charCodeAt past the end is NaN; NaN & 0x3ff is 0
                        };
                        out.push(unicode::from_code_point(
                            ((w1 & 0x3ff) << 10) + (w2 & 0x3ff) + 0x10000,
                        ));
                        k += 2;
                    }
                    map.insert(code, UVal::Str(out));
                }
            });
            if bad {
                return Ok(None);
            }
            let len = cm.length();
            Ok(Some(ToUnicode::Map { map, len }))
        }
        _ => Ok(None),
    }
}

fn simple_font_to_unicode(p: &Props, force_glyphs: bool) -> BTreeMap<u32, UVal> {
    let mut out = BTreeMap::new();
    let mut encoding: Vec<String> = p.default_encoding.clone();
    let mut extra: BTreeMap<u32, String> = BTreeMap::new();
    for (code, name) in &p.differences {
        if name == ".notdef" {
            continue;
        }
        if (*code as usize) < encoding.len() {
            encoding[*code as usize] = name.clone();
        } else {
            extra.insert(*code, name.clone());
        }
    }
    let glyphs = unicode::glyphs_unicode();
    let entries: Vec<(u32, String)> = encoding
        .iter()
        .enumerate()
        .map(|(i, s)| (i as u32, s.clone()))
        .chain(extra)
        .collect();
    for (charcode, glyph_name) in entries {
        if glyph_name.is_empty() {
            continue;
        }
        if let Some(u) = glyphs.get(glyph_name.as_str()) {
            out.insert(charcode, UVal::Str(unicode::from_char_code(*u).to_string()));
            continue;
        }
        let mut code: f64 = 0.0;
        let first = glyph_name.chars().next().unwrap_or(' ');
        let len = glyph_name.chars().count();
        match first {
            'G' if len == 3 => code = parse_int_hex(&glyph_name[1..]),
            'g' if len == 5 => code = parse_int_hex(&glyph_name[1..]),
            'C' | 'c' if (3..=4).contains(&len) => {
                let code_str = &glyph_name[1..];
                if force_glyphs {
                    code = parse_int_hex(code_str);
                } else {
                    code = js_unary_plus(code_str);
                    if code.is_nan() && !parse_int_hex(code_str).is_nan() {
                        return simple_font_to_unicode(p, true);
                    }
                }
            }
            'u' => {
                let u = unicode::get_unicode_for_glyph(&glyph_name, glyphs);
                if u != -1 {
                    code = u as f64;
                }
            }
            _ => {
                if matches!(glyph_name.as_str(), "f_h" | "f_t" | "T_h") {
                    out.insert(charcode, UVal::Str(glyph_name.replace('_', "")));
                    continue;
                }
            }
        }
        if code > 0.0 && code <= 1114111.0 && code.fract() == 0.0 {
            if let Some(be) = p.base_encoding_name
                && code == charcode as f64
                && let Some(enc) = unicode::get_encoding(be)
                && let Some(gn) = enc.get(charcode as usize)
                && !gn.is_empty()
            {
                // String.fromCharCode(glyphsUnicodeMap[name]): undefined
                // becomes NaN becomes "\0".
                let u = glyphs.get(gn).copied().unwrap_or(0);
                out.insert(charcode, UVal::Str(unicode::from_char_code(u).to_string()));
                continue;
            }
            out.insert(
                charcode,
                UVal::Str(unicode::from_code_point(code as u32).to_string()),
            );
        }
    }
    out
}

/// `parseInt(s, 16)`: the leading hex digits (after optional whitespace and
/// sign), NaN for none.
fn parse_int_hex(s: &str) -> f64 {
    let t = s.trim_start();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let t = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    let digits: String = t.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    if digits.is_empty() {
        return f64::NAN;
    }
    let v = digits
        .chars()
        .fold(0f64, |a, c| a * 16.0 + c.to_digit(16).unwrap_or(0) as f64);
    if neg { -v } else { v }
}

/// `+s` for a short string: a decimal number (whitespace allowed around it),
/// a hex/octal/binary literal, or NaN.
fn js_unary_plus(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    for (p, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(r) = t.strip_prefix(p) {
            return u64::from_str_radix(r, radix)
                .map(|v| v as f64)
                .unwrap_or(f64::NAN);
        }
    }
    if t.chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == '+' || c == '-' || c == 'e' || c == 'E')
    {
        return t.parse::<f64>().unwrap_or(f64::NAN);
    }
    if t == "Infinity" || t == "+Infinity" {
        return f64::INFINITY;
    }
    if t == "-Infinity" {
        return f64::NEG_INFINITY;
    }
    f64::NAN
}

fn build_to_unicode(p: &mut Props) -> Result<ToUnicode, Failed> {
    p.has_included_to_unicode_map = p.to_unicode.as_ref().is_some_and(|t| t.length() > 0);
    if p.has_included_to_unicode_map {
        if !p.composite && p.has_encoding {
            p.fallback_to_unicode = Some(simple_font_to_unicode(p, false));
        }
        return p.to_unicode.clone().ok_or(Failed);
    }
    if !p.composite {
        return Ok(ToUnicode::from_sparse(simple_font_to_unicode(p, false)));
    }
    let adobe_cjk = p.cid_system_info.as_ref().is_some_and(|(r, o)| {
        r == "Adobe" && matches!(o.as_str(), "GB1" | "CNS1" | "Japan1" | "Korea1")
    });
    let builtin_non_identity = p.cmap.as_ref().is_some_and(|c| c.built_in && !c.identity);
    if builtin_non_identity || adobe_cjk {
        // The UCS2 CMap is a built-in one; without cMapUrl it never loads.
        return Err(Failed);
    }
    Ok(ToUnicode::Identity {
        first: p.first_char,
        last: p.last_char,
    })
}

fn extract_widths(xref: &XRef<'_>, dict: &Dict, descriptor: &Dict, p: &mut Props) {
    let mut glyphs_widths: HashMap<u32, f64> = HashMap::new();
    let mut default_width = 0.0;
    let mut vmetrics: HashMap<u32, [f64; 3]> = HashMap::new();
    let mut default_vmetrics = None;
    if p.composite {
        let dw = xref.get(dict, "DW");
        default_width = match dw {
            Obj::Num(n) => n.ceil(),
            _ => 1000.0,
        };
        if let Obj::Array(widths) = xref.get(dict, "W") {
            let mut i = 0;
            while i < widths.len() {
                let start = xref.resolve(&widths[i]);
                i += 1;
                let Some(mut start) = start.int() else { break };
                let Some(code) = widths.get(i).map(|o| xref.resolve(o)) else {
                    break;
                };
                match &code {
                    Obj::Array(a) => {
                        for c in a.iter() {
                            if let Some(w) = xref.resolve(c).num()
                                && start >= 0.0
                            {
                                glyphs_widths.insert(start as u32, w);
                            }
                            start += 1.0;
                        }
                    }
                    o if o.int().is_some() => {
                        let end = o.int().unwrap_or(0.0);
                        i += 1;
                        let w = widths.get(i).map(|x| xref.resolve(x));
                        if let Some(w) = w.and_then(|w| w.num()) {
                            let mut j = start;
                            while j <= end {
                                if j >= 0.0 {
                                    glyphs_widths.insert(j as u32, w);
                                }
                                j += 1.0;
                            }
                        }
                    }
                    _ => break,
                }
                i += 1;
            }
        }
        if p.vertical {
            let dw2 = xref.get_array(dict, "DW2");
            let v: [f64; 2] = match dw2
                .as_ref()
                .map(|a| a.iter().map(|o| o.num()).collect::<Option<Vec<f64>>>())
            {
                Some(Some(v)) if v.len() == 2 => [v[0], v[1]],
                _ => [880.0, -1000.0],
            };
            default_vmetrics = Some([v[1], default_width * 0.5, v[0]]);
            if let Obj::Array(w2) = xref.get(dict, "W2") {
                let mut i = 0;
                while i < w2.len() {
                    let Some(mut start) = xref.resolve(&w2[i]).int() else {
                        break;
                    };
                    i += 1;
                    let Some(code) = w2.get(i).map(|o| xref.resolve(o)) else {
                        break;
                    };
                    match &code {
                        Obj::Array(a) => {
                            let mut j = 0;
                            while j < a.len() {
                                let vm = [
                                    a.get(j).map(|x| xref.resolve(x)),
                                    a.get(j + 1).map(|x| xref.resolve(x)),
                                    a.get(j + 2).map(|x| xref.resolve(x)),
                                ];
                                j += 3;
                                if let [Some(Obj::Num(a0)), Some(Obj::Num(a1)), Some(Obj::Num(a2))] =
                                    vm
                                    && start >= 0.0
                                {
                                    vmetrics.insert(start as u32, [a0, a1, a2]);
                                }
                                start += 1.0;
                            }
                        }
                        o if o.int().is_some() => {
                            let end = o.int().unwrap_or(0.0);
                            let vm = [
                                w2.get(i + 1).map(|x| xref.resolve(x)),
                                w2.get(i + 2).map(|x| xref.resolve(x)),
                                w2.get(i + 3).map(|x| xref.resolve(x)),
                            ];
                            i += 3;
                            if let [Some(Obj::Num(a0)), Some(Obj::Num(a1)), Some(Obj::Num(a2))] = vm
                            {
                                let mut j = start;
                                while j <= end {
                                    if j >= 0.0 {
                                        vmetrics.insert(j as u32, [a0, a1, a2]);
                                    }
                                    j += 1.0;
                                }
                            }
                        }
                        _ => break,
                    }
                    i += 1;
                }
            }
        }
    } else if let Obj::Array(widths) = xref.get(dict, "Widths") {
        for (j, w) in (p.first_char..).zip(widths.iter()) {
            if let Some(n) = xref.resolve(w).num()
                && j >= 0
            {
                glyphs_widths.insert(j as u32, n);
            }
        }
        default_width = xref.get(descriptor, "MissingWidth").num().unwrap_or(0.0);
    } else if let Obj::Name(base_font) = xref.get(dict, "BaseFont") {
        let (dw, _, by_name) = base_font_metrics(&base_font);
        glyphs_widths = build_char_code_to_width(&by_name, p);
        default_width = dw;
    }
    // FixedPitch is recomputed here and only feeds the fallback CSS family.
    p.default_width = default_width;
    p.widths = glyphs_widths;
    p.default_vmetrics = default_vmetrics;
    p.vmetrics = vmetrics;
}

/// `adjustWidths`.
fn adjust_widths(p: &mut Props) {
    let Some(fm) = &p.font_matrix else { return };
    if fm[0] == FONT_IDENTITY_MATRIX[0] {
        return;
    }
    let scale = 0.001 / fm[0];
    for w in p.widths.values_mut() {
        *w *= scale;
    }
    p.default_width *= scale;
}

/// `adjustTrueTypeToUnicode`.
fn adjust_true_type_to_unicode(p: &mut Props, is_symbolic_font: bool, records: &[(u16, u16, u16)]) {
    if p.has_included_to_unicode_map || p.has_encoding {
        return;
    }
    if p.to_unicode.as_ref().is_none_or(ToUnicode::is_identity) {
        return;
    }
    if !is_symbolic_font || records.is_empty() {
        return;
    }
    if p.default_encoding_name == Some("WinAnsiEncoding") {
        return;
    }
    if records.iter().any(|r| *r != (3, 1, 0x409)) {
        return;
    }
    let glyphs = unicode::glyphs_unicode();
    let mut m = BTreeMap::new();
    for (code, name) in tables::WIN_ANSI_ENCODING.iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        if let Some(u) = glyphs.get(name) {
            m.insert(
                code as u32,
                UVal::Str(unicode::from_char_code(*u).to_string()),
            );
        }
    }
    if !m.is_empty()
        && let Some(tu) = &mut p.to_unicode
    {
        tu.amend(&m);
    }
}

/// `amendFallbackToUnicode`.
fn amend_fallback_to_unicode(p: &mut Props) {
    let Some(fb) = p.fallback_to_unicode.clone() else {
        return;
    };
    let Some(tu) = &mut p.to_unicode else { return };
    if tu.is_identity() {
        return;
    }
    let m: BTreeMap<u32, UVal> = fb.into_iter().filter(|(k, _)| !tu.has(*k)).collect();
    if !m.is_empty() {
        tu.amend(&m);
    }
}

fn build_to_font_char(
    enc: &[String],
    glyphs: &HashMap<&'static str, u32>,
    diffs: &BTreeMap<u32, String>,
) -> BTreeMap<u32, u32> {
    let mut m = BTreeMap::new();
    for (i, name) in enc.iter().enumerate() {
        let u = unicode::get_unicode_for_glyph(name, glyphs);
        if u != -1 {
            m.insert(i as u32, u as u32);
        }
    }
    for (code, name) in diffs {
        let u = unicode::get_unicode_for_glyph(name, glyphs);
        if u != -1 {
            m.insert(*code, u as u32);
        }
    }
    m
}

/// The Font constructor, as far as text goes.
fn build_font(name: &str, _descriptor: Option<&Dict>, mut p: Props) -> Font {
    let mut font = Font::error_font();
    font.error = false;
    font.name = Some(name.to_string());
    font.font_type = p.font_type.clone();
    font.is_type3 = p.is_type3;
    font.missing_file = false;
    font.composite = p.composite;
    font.font_matrix = p.font_matrix.clone();
    font.bbox = p.bbox;
    font.differences = p.differences.clone();
    font.default_encoding = p.default_encoding.clone();
    if p.is_type3 {
        font.widths = std::mem::take(&mut p.widths);
        font.default_width = p.default_width;
        font.to_unicode = p
            .to_unicode
            .take()
            .unwrap_or(ToUnicode::from_sparse(BTreeMap::new()));
        return font;
    }
    font.vertical = p.vertical;
    let mut fallback = p.file.is_none();
    if let Some(file) = p.file.take() {
        let header = &file[..file.len().min(4)];
        let is_tt = header == b"\x00\x01\x00\x00" || header == b"true";
        let is_ttc = header == b"ttcf";
        let is_otf = header == b"OTTO";
        let is_t1 = header.len() >= 2
            && ((header[0] == 0x25 && header[1] == 0x21)
                || (header[0] == 0x80 && header[1] == 0x01));
        let is_cff = header.len() >= 4 && header[0] >= 1 && header[3] >= 1 && header[3] <= 4;
        let kind = if is_tt || is_ttc || is_otf {
            "TrueType"
        } else if is_t1 || is_cff {
            "Type1"
        } else {
            match p.font_type.as_str() {
                "TrueType" | "CIDFontType2" | "OpenType" => "TrueType",
                "Type1" | "MMType1" | "CIDFontType0" => "Type1",
                _ => "",
            }
        };
        match kind {
            "TrueType" => {
                let info = if is_ttc {
                    truetype::TtInfo {
                        name_records: Vec::new(),
                        would_fail: false,
                    }
                } else {
                    truetype::read(&file)
                };
                if info.would_fail {
                    fallback = true;
                } else {
                    if !p.composite {
                        let is_symbolic = p.flags & flags::SYMBOLIC != 0;
                        adjust_true_type_to_unicode(&mut p, is_symbolic, &info.name_records);
                    }
                    adjust_widths(&mut p);
                }
            }
            "Type1" => {
                // Embedded Type 1 / CFF programs are not parsed (see the
                // module note). The font is taken as loaded, with the
                // PDF's own widths and encoding.
                adjust_widths(&mut p);
            }
            _ => fallback = true,
        }
    }
    if fallback {
        font.widths = p.widths.clone();
        font.default_width = p.default_width;
        font.to_unicode = p
            .to_unicode
            .clone()
            .unwrap_or(ToUnicode::from_sparse(BTreeMap::new()));
        let replaced = fallback_to_system_font(&mut font, &p);
        amend_fallback_to_unicode(&mut p);
        // amendFallbackToUnicode amends properties.toUnicode, which is the
        // font's own map unless fallbackToSystemFont replaced it.
        if !replaced {
            font.to_unicode = p
                .to_unicode
                .clone()
                .unwrap_or(ToUnicode::from_sparse(BTreeMap::new()));
        }
    } else {
        amend_fallback_to_unicode(&mut p);
        font.widths = std::mem::take(&mut p.widths);
        font.default_width = p.default_width;
        font.to_unicode = p
            .to_unicode
            .take()
            .unwrap_or(ToUnicode::from_sparse(BTreeMap::new()));
    }
    font.vmetrics = p.vmetrics;
    font.default_vmetrics = p.default_vmetrics;
    font.cmap = p.cmap;
    font
}

/// `fallbackToSystemFont`.
fn fallback_to_system_font(font: &mut Font, p: &Props) -> bool {
    font.missing_file = true;
    let name = font.name.clone().unwrap_or_default();
    let mut font_name = unicode::normalize_font_name(&name);
    let std = unicode::std_font_map();
    let non_std = unicode::non_std_font_map();
    let is_standard_font = std.contains_key(font_name.as_str());
    let is_mapped_to_standard_font = non_std
        .get(font_name.as_str())
        .is_some_and(|m| std.contains_key(m));
    if let Some(s) = std.get(font_name.as_str()) {
        font_name = s.to_string();
    } else if let Some(s) = non_std.get(font_name.as_str()) {
        font_name = s.to_string();
    }
    let glyphs = unicode::glyphs_unicode();
    let cid_identity = p.cid_encoding.starts_with("Identity-");
    let cid_encoding_identity = cid_identity;
    if (is_standard_font || is_mapped_to_standard_font)
        && font.font_type == "CIDFontType2"
        && cid_encoding_identity
    {
        let mut map: BTreeMap<u32, u32> = BTreeMap::new();
        let apply = |map: &mut BTreeMap<u32, u32>, t: &[(u32, u32)]| {
            for (k, v) in t {
                map.insert(*k, *v);
            }
        };
        let lower = name.to_ascii_lowercase();
        if lower.contains("trebuchet") {
            apply(&mut map, &tables::GLYPH_MAP_FOR_MAC_ORDERED_FONTS);
            apply(&mut map, &tables::SUPPLEMENTAL_GLYPH_MAP_FOR_TREBUCHET_MS);
        } else {
            apply(&mut map, &tables::GLYPH_MAP_FOR_STANDARD_FONTS);
            let compact: String = lower.chars().filter(|c| *c != '-').collect();
            if compact.contains("arialblack") {
                apply(&mut map, &tables::SUPPLEMENTAL_GLYPH_MAP_FOR_ARIAL_BLACK);
            } else if lower.contains("calibri") {
                apply(&mut map, &tables::SUPPLEMENTAL_GLYPH_MAP_FOR_CALIBRI);
            }
        }
        if let Some(c2g) = &p.cid_to_gid_map {
            let keys: Vec<(u32, u32)> = map.iter().map(|(k, v)| (*k, *v)).collect();
            for (code, cid) in keys {
                if let Some(g) = c2g.get(&cid) {
                    map.insert(code, *g);
                }
            }
            let c2g_len = c2g.keys().next_back().map(|k| *k as i64 + 1).unwrap_or(0);
            if c2g_len != font.to_unicode.length()
                && p.has_included_to_unicode_map
                && font.to_unicode.is_identity()
            {
                let tu = font.to_unicode.clone();
                tu.for_each(|code, u| {
                    let cid = map.get(&code).copied();
                    if cid.is_none_or(|c| !c2g.contains_key(&c)) {
                        map.insert(code, u);
                    }
                });
            }
        }
        if !font.to_unicode.is_identity() {
            let tu = font.to_unicode.clone();
            tu.for_each(|code, u| {
                map.insert(code, u);
            });
        }
        font.to_font_char = map.clone();
        font.to_unicode =
            ToUnicode::from_sparse(map.into_iter().map(|(k, v)| (k, UVal::Num(v))).collect());
        return true;
    } else if font_name.to_ascii_lowercase().contains("symbol") {
        let is_cid_keyed = font.composite && cid_identity;
        if is_cid_keyed {
            let mut enc = vec![String::new(); 192];
            for (k, v) in tables::SYMBOL_GLYPH_ID_ENCODING.iter() {
                if (*k as usize) < enc.len() {
                    enc[*k as usize] = v.to_string();
                }
            }
            font.to_font_char = build_to_font_char(&enc, glyphs, &font.differences);
        } else {
            let enc = enc_vec(&tables::SYMBOL_SET_ENCODING);
            font.to_font_char = build_to_font_char(&enc, glyphs, &font.differences);
        }
    } else if font_name.to_ascii_lowercase().contains("dingbats") {
        let enc = enc_vec(&tables::ZAPF_DINGBATS_ENCODING);
        font.to_font_char =
            build_to_font_char(&enc, unicode::dingbats_glyphs_unicode(), &font.differences);
    } else if is_standard_font || is_mapped_to_standard_font {
        font.to_font_char = build_to_font_char(&font.default_encoding, glyphs, &font.differences);
    } else {
        let mut map = BTreeMap::new();
        let tu = font.to_unicode.clone();
        tu.for_each(|code, mut u| {
            if !font.composite {
                let gn = font
                    .differences
                    .get(&code)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .or_else(|| font.default_encoding.get(code as usize).cloned())
                    .unwrap_or_default();
                let x = unicode::get_unicode_for_glyph(&gn, glyphs);
                if x != -1 {
                    u = x as u32;
                }
            }
            map.insert(code, u);
        });
        font.to_font_char = map;
    }
    false
}

//! An owned PDF object model in pdf.js's shape, over hayro-syntax.
//!
//! hayro-syntax does the file-level work (xref tables and streams, repair,
//! decryption, object streams, filters). This module copies what it reads
//! into owned values shaped the way pdf.js's evaluator sees them: every
//! number is a double, names and strings are byte strings read as Latin-1
//! (pdf.js holds them as JavaScript strings of char codes 0-255), and an
//! indirect reference stays a reference until something asks for it with
//! `get`, exactly as pdf.js's `Dict.get` / `Dict.getRaw` split works.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use hayro_syntax::object::{MaybeRef, Object, ObjectIdentifier};

/// An indirect reference.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Ref {
    pub num: i32,
    pub generation: i32,
}

impl Ref {
    fn id(self) -> ObjectIdentifier {
        ObjectIdentifier::new(self.num, self.generation)
    }
}

impl std::fmt::Display for Ref {
    /// pdf.js's `Ref.toString()`: `12R`, or `12R3` for a non-zero generation.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.generation == 0 {
            write!(f, "{}R", self.num)
        } else {
            write!(f, "{}R{}", self.num, self.generation)
        }
    }
}

/// A PDF object. `Cmd` only ever comes out of a content stream.
#[derive(Clone, Debug)]
pub enum Obj {
    Null,
    Bool(bool),
    Num(f64),
    Str(Rc<[u8]>),
    Name(Rc<str>),
    Array(Rc<Vec<Obj>>),
    Dict(Rc<Dict>),
    Ref(Ref),
    Stream(Rc<Stream>),
    Cmd(Rc<str>),
}

impl Obj {
    pub fn num(&self) -> Option<f64> {
        match self {
            Obj::Num(n) => Some(*n),
            _ => None,
        }
    }
    /// `Number.isInteger(x)`.
    pub fn int(&self) -> Option<f64> {
        match self {
            Obj::Num(n) if n.is_finite() && n.fract() == 0.0 => Some(*n),
            _ => None,
        }
    }
    pub fn name(&self) -> Option<&str> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }
    pub fn is_name(&self, n: &str) -> bool {
        self.name() == Some(n)
    }
    pub fn dict(&self) -> Option<&Rc<Dict>> {
        match self {
            Obj::Dict(d) => Some(d),
            _ => None,
        }
    }
    /// The dictionary of a dict or of a stream.
    pub fn dict_or_stream_dict(&self) -> Option<&Rc<Dict>> {
        match self {
            Obj::Dict(d) => Some(d),
            Obj::Stream(s) => Some(&s.dict),
            _ => None,
        }
    }
    pub fn array(&self) -> Option<&Rc<Vec<Obj>>> {
        match self {
            Obj::Array(a) => Some(a),
            _ => None,
        }
    }
    pub fn stream(&self) -> Option<&Rc<Stream>> {
        match self {
            Obj::Stream(s) => Some(s),
            _ => None,
        }
    }
    pub fn str_bytes(&self) -> Option<&[u8]> {
        match self {
            Obj::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_ref(&self) -> Option<Ref> {
        match self {
            Obj::Ref(r) => Some(*r),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Obj::Null)
    }
    /// True for a value JavaScript would call truthy, for the handful of
    /// `x || y` defaults pdf.js writes over dictionary lookups.
    pub fn truthy(&self) -> bool {
        match self {
            Obj::Null => false,
            Obj::Bool(b) => *b,
            Obj::Num(n) => *n != 0.0 && !n.is_nan(),
            Obj::Str(s) => !s.is_empty(),
            _ => true,
        }
    }
}

/// A dictionary. Entries keep the order they were read in.
#[derive(Debug, Default)]
pub struct Dict {
    pub entries: Vec<(Rc<str>, Obj)>,
    /// The indirect object this dictionary is, when it is one.
    pub obj_id: Option<Ref>,
}

impl Dict {
    pub fn get_raw(&self, key: &str) -> Option<&Obj> {
        self.entries
            .iter()
            .find(|(k, _)| &**k == key)
            .map(|(_, v)| v)
    }
    pub fn has(&self, key: &str) -> bool {
        self.get_raw(key).is_some()
    }
    pub fn set(&mut self, key: &str, value: Obj) {
        if let Some(e) = self.entries.iter_mut().find(|(k, _)| &**k == key) {
            e.1 = value;
        } else {
            self.entries.push((Rc::from(key), value));
        }
    }
}

/// Where a stream's bytes come from.
#[derive(Debug)]
pub enum StreamSource {
    /// An indirect stream in the file, decoded on demand through hayro.
    Indirect(Ref),
    /// An inline image: the bytes between `ID` and `EI`, still encoded.
    Inline(Vec<u8>),
}

#[derive(Debug)]
pub struct Stream {
    pub dict: Rc<Dict>,
    pub source: StreamSource,
}

/// Deepest nesting copied out of a file. pdf.js has no explicit limit, but
/// its recursion would hit the JavaScript stack long before a real file
/// needs this; a hostile file must not hit Rust's.
const MAX_DEPTH: usize = 64;

/// Latin-1: one char per byte, as pdf.js reads names and strings.
pub fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// The document's objects, fetched and copied on demand, with a cache so the
/// same reference always yields the same `Rc` (pdf.js caches parsed objects
/// too, and a few of its caches key on object identity).
pub struct XRef<'p> {
    pdf: &'p hayro_syntax::Pdf,
    cache: RefCell<HashMap<Ref, Obj>>,
}

impl<'p> XRef<'p> {
    pub fn new(pdf: &'p hayro_syntax::Pdf) -> Self {
        Self {
            pdf,
            cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn pdf(&self) -> &'p hayro_syntax::Pdf {
        self.pdf
    }

    /// `xref.fetch(ref)`: the object, or null for one that is not there.
    pub fn fetch(&self, r: Ref) -> Obj {
        if let Some(o) = self.cache.borrow().get(&r) {
            return o.clone();
        }
        let obj = match self.pdf.xref().get::<Object<'_>>(r.id()) {
            Some(o) => convert(&o, 0, Some(r)),
            None => Obj::Null,
        };
        self.cache.borrow_mut().insert(r, obj.clone());
        obj
    }

    /// `xref.fetchIfRef(obj)`.
    pub fn resolve(&self, obj: &Obj) -> Obj {
        match obj {
            Obj::Ref(r) => self.fetch(*r),
            o => o.clone(),
        }
    }

    /// `dict.get(key)`: the value with one reference resolved.
    pub fn get(&self, dict: &Dict, key: &str) -> Obj {
        match dict.get_raw(key) {
            Some(o) => self.resolve(o),
            None => Obj::Null,
        }
    }

    /// `dict.get(key1, key2)`: the first key present.
    pub fn get2(&self, dict: &Dict, k1: &str, k2: &str) -> Obj {
        if dict.has(k1) {
            self.get(dict, k1)
        } else {
            self.get(dict, k2)
        }
    }

    /// `dict.getArray(key)`: the array with every element resolved.
    pub fn get_array(&self, dict: &Dict, key: &str) -> Option<Vec<Obj>> {
        let a = self.get(dict, key);
        a.array()
            .map(|a| a.iter().map(|o| self.resolve(o)).collect())
    }

    /// A stream's decoded bytes. `None` when hayro could not decode it,
    /// which is what pdf.js's best-effort streams also arrive at as empty.
    pub fn stream_bytes(&self, s: &Stream) -> Option<Vec<u8>> {
        match &s.source {
            StreamSource::Indirect(r) => {
                let st = self
                    .pdf
                    .xref()
                    .get::<hayro_syntax::object::Stream<'_>>(r.id())?;
                st.decoded().ok().map(|c| c.into_owned())
            }
            StreamSource::Inline(_) => None,
        }
    }

    /// A stream's raw (decrypted, undecoded) bytes.
    pub fn stream_raw(&self, s: &Stream) -> Option<Vec<u8>> {
        match &s.source {
            StreamSource::Indirect(r) => {
                let st = self
                    .pdf
                    .xref()
                    .get::<hayro_syntax::object::Stream<'_>>(r.id())?;
                Some(st.raw_data().into_owned())
            }
            StreamSource::Inline(b) => Some(b.clone()),
        }
    }
}

fn convert_maybe(m: &MaybeRef<Object<'_>>, depth: usize) -> Obj {
    match m {
        MaybeRef::Ref(r) => Obj::Ref(Ref {
            num: r.obj_number,
            generation: r.gen_number,
        }),
        MaybeRef::NotRef(o) => convert(o, depth, None),
    }
}

fn convert_dict(d: &hayro_syntax::object::Dict<'_>, depth: usize, id: Option<Ref>) -> Dict {
    let mut out = Dict {
        entries: Vec::new(),
        // Only the indirect object itself carries an id. hayro's own
        // `obj_id` on a nested dictionary names the object it sits in.
        obj_id: id,
    };
    if depth >= MAX_DEPTH {
        return out;
    }
    // Not `entries()`: it unwraps a re-read that a damaged file can fail,
    // and it sorts by name, where pdf.js keeps the file's order.
    let mut keys: Vec<Vec<u8>> = d.keys().map(|k| k.as_ref().to_vec()).collect();
    let order = key_order(d.data());
    keys.sort_by_key(|k| {
        (
            order.iter().position(|o| o == k).unwrap_or(usize::MAX),
            k.clone(),
        )
    });
    for k in keys {
        if let Some(v) = d.get_raw::<Object<'_>>(&k) {
            out.entries
                .push((Rc::from(latin1(&k).as_str()), convert_maybe(&v, depth + 1)));
        }
    }
    out
}

/// The order a dictionary's keys appear in its bytes, first occurrence
/// first. Values are skipped one object at a time (a reference is three
/// tokens), so a name used as a value is never taken for a key.
fn key_order(data: &[u8]) -> Vec<Vec<u8>> {
    use crate::lexer::{Lexer, Tok};
    let mut toks: Vec<Obj> = Vec::new();
    let mut lexer = Lexer::new(data, false);
    // Collect only as far as the dictionary's own closing ">>": the data
    // hayro hands over can run on past it.
    let mut depth = 0i32;
    while toks.len() < 100_000 {
        let before = lexer.stream.pos;
        match lexer.get_obj() {
            Ok(Tok::Obj(o)) => {
                let (open, close) = match &o {
                    Obj::Cmd(x) => (&**x == "<<" || &**x == "[", &**x == ">>" || &**x == "]"),
                    _ => (false, false),
                };
                if toks.is_empty() && !open {
                    depth = 1;
                }
                if open {
                    depth += 1;
                } else if close {
                    depth -= 1;
                }
                toks.push(o);
                if depth <= 0 {
                    break;
                }
            }
            Ok(Tok::Eof) => break,
            Err(_) if lexer.stream.pos > before => continue,
            Err(_) => break,
        }
    }
    let is_cmd = |o: &Obj, c: &str| matches!(o, Obj::Cmd(x) if &**x == c);
    let mut i = 0;
    if toks.first().is_some_and(|t| is_cmd(t, "<<")) {
        i = 1;
    }
    // Skips one value starting at `i`, returning the index after it.
    fn skip_value(toks: &[Obj], mut i: usize) -> usize {
        let is_cmd = |o: &Obj, c: &str| matches!(o, Obj::Cmd(x) if &**x == c);
        let Some(t) = toks.get(i) else { return i };
        if is_cmd(t, "<<") || is_cmd(t, "[") {
            let mut depth = 0i32;
            while let Some(t) = toks.get(i) {
                if is_cmd(t, "<<") || is_cmd(t, "[") {
                    depth += 1;
                } else if is_cmd(t, ">>") || is_cmd(t, "]") {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                i += 1;
            }
            return i;
        }
        if matches!(t, Obj::Num(_))
            && matches!(toks.get(i + 1), Some(Obj::Num(_)))
            && toks.get(i + 2).is_some_and(|t| is_cmd(t, "R"))
        {
            return i + 3;
        }
        i + 1
    }
    let mut out: Vec<Vec<u8>> = Vec::new();
    while let Some(t) = toks.get(i) {
        if is_cmd(t, ">>") {
            break;
        }
        match t {
            Obj::Name(n) => {
                let k: Vec<u8> = n.chars().map(|c| c as u32 as u8).collect();
                if !out.contains(&k) {
                    out.push(k);
                }
                i = skip_value(&toks, i + 1);
            }
            _ => i += 1,
        }
    }
    out
}

fn convert(o: &Object<'_>, depth: usize, id: Option<Ref>) -> Obj {
    match o {
        Object::Null(_) => Obj::Null,
        Object::Boolean(b) => Obj::Bool(*b),
        Object::Number(n) => Obj::Num(n.as_f64()),
        Object::String(s) => Obj::Str(Rc::from(s.as_bytes())),
        Object::Name(n) => Obj::Name(Rc::from(latin1(n.as_ref()).as_str())),
        Object::Dict(d) => Obj::Dict(Rc::new(convert_dict(d, depth, id))),
        Object::Array(a) => {
            if depth >= MAX_DEPTH {
                return Obj::Array(Rc::new(Vec::new()));
            }
            Obj::Array(Rc::new(
                a.raw_iter().map(|m| convert_maybe(&m, depth + 1)).collect(),
            ))
        }
        Object::Stream(s) => {
            let sid = s.obj_id();
            let r = id.unwrap_or(Ref {
                num: sid.obj_number,
                generation: sid.gen_number,
            });
            let mut dict = convert_dict(s.dict(), depth, Some(r));
            dict.obj_id = Some(r);
            Obj::Stream(Rc::new(Stream {
                dict: Rc::new(dict),
                source: StreamSource::Indirect(r),
            }))
        }
    }
}

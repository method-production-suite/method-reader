//! CMaps, as pdf.js reads them.
//!
//! Ported from pdf.js `src/core/cmap.js` (Mozilla Foundation, Apache License
//! 2.0). One deliberate absence, and it is pdf.js's own under Method Reader's
//! settings: the engine opens documents without `cMapUrl`, so every built-in
//! CMap other than Identity-H/V fails to load. That failure is reproduced
//! (`Err(BuiltInUnavailable)`) rather than fixed, because pdf.js turns it into
//! an unusable font and the parser has only ever seen that.

use std::collections::BTreeMap;

use crate::lexer::{Lexer, Tok};
use crate::obj::Obj;

/// A CMap value: a CID / code point, or a byte string (UTF-16BE for a
/// ToUnicode map, a CID string otherwise).
#[derive(Clone, Debug, PartialEq)]
pub enum CVal {
    Int(i64),
    Str(Vec<u16>),
}

#[derive(Clone, Debug)]
pub struct CMap {
    pub codespace_ranges: [Vec<u32>; 4],
    pub num_codespace_ranges: usize,
    map: BTreeMap<u32, CVal>,
    pub name: String,
    pub vertical: bool,
    pub built_in: bool,
    pub identity: bool,
}

#[derive(Debug)]
pub struct BuiltInUnavailable;

const MAX_MAP_RANGE: u64 = (1 << 24) - 1;

impl CMap {
    pub fn new(built_in: bool) -> Self {
        Self {
            codespace_ranges: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            num_codespace_ranges: 0,
            map: BTreeMap::new(),
            name: String::new(),
            vertical: false,
            built_in,
            identity: false,
        }
    }

    pub fn identity(vertical: bool, n: usize) -> Self {
        let mut c = Self::new(false);
        c.vertical = vertical;
        c.identity = true;
        c.add_codespace_range(n, 0, 0xffff);
        c
    }

    fn add_codespace_range(&mut self, n: usize, low: u32, high: u32) {
        if (1..=4).contains(&n) {
            self.codespace_ranges[n - 1].push(low);
            self.codespace_ranges[n - 1].push(high);
        }
        self.num_codespace_ranges += 1;
    }

    fn map_cid_range(&mut self, mut low: u32, high: u32, mut dst: i64) -> Result<(), ()> {
        if high as u64 >= low as u64 && (high - low) as u64 > MAX_MAP_RANGE {
            return Err(());
        }
        while low <= high {
            self.map.insert(low, CVal::Int(dst));
            dst += 1;
            if low == u32::MAX {
                break;
            }
            low += 1;
        }
        Ok(())
    }

    fn map_bf_range(&mut self, mut low: u32, high: u32, mut dst: Vec<u16>) -> Result<(), ()> {
        if high as u64 >= low as u64 && (high - low) as u64 > MAX_MAP_RANGE {
            return Err(());
        }
        if dst.is_empty() {
            // charCodeAt(-1) is NaN; NaN + 1 > 0xff is false, so pdf.js
            // keeps appending fromCharCode(NaN) = "\0" to an empty string.
            while low <= high {
                self.map.insert(low, CVal::Str(dst.clone()));
                dst = vec![0];
                if low == u32::MAX {
                    break;
                }
                low += 1;
            }
            return Ok(());
        }
        let last = dst.len() - 1;
        while low <= high {
            self.map.insert(low, CVal::Str(dst.clone()));
            if low == u32::MAX {
                break;
            }
            low += 1;
            let next = dst[last] as u32 + 1;
            if next > 0xff {
                // dstLow.substring(0, lastByte - 1) + (charAt(lastByte - 1) + 1) + "\0"
                let mut nd: Vec<u16> = dst[..last.saturating_sub(1)].to_vec();
                if last >= 1 {
                    nd.push(dst[last - 1].wrapping_add(1));
                } else {
                    // charCodeAt(-1) is NaN, fromCharCode(NaN) is "\0".
                    nd.push(0);
                }
                nd.push(0);
                dst = nd;
                continue;
            }
            dst[last] = next as u16;
        }
        Ok(())
    }

    fn map_bf_range_to_array(&mut self, mut low: u32, high: u32, array: &[Obj]) -> Result<(), ()> {
        if high as u64 >= low as u64 && (high - low) as u64 > MAX_MAP_RANGE {
            return Err(());
        }
        let mut i = 0;
        while low <= high && i < array.len() {
            let v = match &array[i] {
                Obj::Str(s) => CVal::Str(s.iter().map(|&b| b as u16).collect()),
                Obj::Num(n) => CVal::Int(*n as i64),
                // Anything else is stored by pdf.js as-is and later yields
                // nothing usable; leave the code unmapped.
                _ => {
                    i += 1;
                    low += 1;
                    continue;
                }
            };
            self.map.insert(low, v);
            i += 1;
            if low == u32::MAX {
                break;
            }
            low += 1;
        }
        Ok(())
    }

    pub fn map_one(&mut self, src: u32, dst: CVal) {
        self.map.insert(src, dst);
    }

    pub fn lookup(&self, code: u32) -> Option<CVal> {
        if self.identity {
            return if code <= 0xffff {
                Some(CVal::Int(code as i64))
            } else {
                None
            };
        }
        self.map.get(&code).cloned()
    }

    pub fn contains(&self, code: u32) -> bool {
        if self.identity {
            return code <= 0xffff;
        }
        self.map.contains_key(&code)
    }

    /// `forEach`, in ascending code order (which both of pdf.js's branches
    /// produce).
    pub fn for_each(&self, mut f: impl FnMut(u32, &CVal)) {
        if self.identity {
            for i in 0..=0xffffu32 {
                f(i, &CVal::Int(i as i64));
            }
            return;
        }
        for (k, v) in &self.map {
            f(*k, v);
        }
    }

    /// `length`: one past the highest code.
    pub fn length(&self) -> u64 {
        if self.identity {
            return 0x10000;
        }
        self.map
            .keys()
            .next_back()
            .map(|k| *k as u64 + 1)
            .unwrap_or(0)
    }

    /// `readCharCode(str, offset)`: the code and how many bytes it took.
    pub fn read_char_code(&self, s: &[u16], offset: usize) -> (u32, usize) {
        let mut c: u32 = 0;
        for n in 0..4 {
            // str.charCodeAt past the end is NaN; `c << 8 | NaN` is `c << 8`.
            let b = s.get(offset + n).copied().unwrap_or(0) as u32;
            c = (c << 8) | b;
            let r = &self.codespace_ranges[n];
            let mut k = 0;
            while k + 1 < r.len() {
                let low = r[k];
                let high = r[k + 1];
                k += 2;
                if c >= low && c <= high {
                    return (c, n + 1);
                }
            }
        }
        (0, 1)
    }

    fn is_identity_cmap(&self) -> bool {
        if !(self.name == "Identity-H" || self.name == "Identity-V") {
            return false;
        }
        if self.length() != 0x10000 {
            return false;
        }
        (0..0x10000u32).all(|i| self.map.get(&i) == Some(&CVal::Int(i as i64)))
    }
}

fn str_to_int(s: &[u8]) -> u32 {
    let mut a: u32 = 0;
    for &b in s {
        a = (a << 8) | b as u32;
    }
    a
}

fn obj(lexer: &mut Lexer<'_>) -> Result<Tok, ()> {
    lexer.get_obj().map_err(|_| ())
}

fn is_cmd(t: &Tok, c: &str) -> bool {
    matches!(t, Tok::Obj(Obj::Cmd(s)) if &**s == c)
}

fn as_str(t: &Tok) -> Option<&[u8]> {
    match t {
        Tok::Obj(Obj::Str(s)) => Some(s),
        _ => None,
    }
}

fn as_int(t: &Tok) -> Option<i64> {
    match t {
        Tok::Obj(Obj::Num(n)) if n.fract() == 0.0 && n.is_finite() => Some(*n as i64),
        _ => None,
    }
}

fn parse_bf_char(c: &mut CMap, l: &mut Lexer<'_>) -> Result<(), ()> {
    loop {
        let o = obj(l)?;
        if matches!(o, Tok::Eof) {
            return Ok(());
        }
        if is_cmd(&o, "endbfchar") {
            return Ok(());
        }
        let src = str_to_int(as_str(&o).ok_or(())?);
        let o = obj(l)?;
        let dst = as_str(&o).ok_or(())?;
        c.map_one(src, CVal::Str(dst.iter().map(|&b| b as u16).collect()));
    }
}

fn parse_bf_range(c: &mut CMap, l: &mut Lexer<'_>) -> Result<(), ()> {
    loop {
        let o = obj(l)?;
        if matches!(o, Tok::Eof) {
            break;
        }
        if is_cmd(&o, "endbfrange") {
            return Ok(());
        }
        let low = str_to_int(as_str(&o).ok_or(())?);
        let o = obj(l)?;
        let high = str_to_int(as_str(&o).ok_or(())?);
        let o = obj(l)?;
        if let Some(i) = as_int(&o) {
            c.map_bf_range(low, high, vec![(i as u32 & 0xffff) as u16])?;
        } else if let Some(s) = as_str(&o) {
            c.map_bf_range(low, high, s.iter().map(|&b| b as u16).collect())?;
        } else if is_cmd(&o, "[") {
            let mut o = obj(l)?;
            let mut array = Vec::new();
            while !is_cmd(&o, "]") && !matches!(o, Tok::Eof) {
                if let Tok::Obj(x) = &o {
                    array.push(x.clone());
                }
                o = obj(l)?;
            }
            c.map_bf_range_to_array(low, high, &array)?;
        } else {
            break;
        }
    }
    Err(())
}

fn parse_cid_char(c: &mut CMap, l: &mut Lexer<'_>) -> Result<(), ()> {
    loop {
        let o = obj(l)?;
        if matches!(o, Tok::Eof) {
            return Ok(());
        }
        if is_cmd(&o, "endcidchar") {
            return Ok(());
        }
        let src = str_to_int(as_str(&o).ok_or(())?);
        let o = obj(l)?;
        let dst = as_int(&o).ok_or(())?;
        c.map_one(src, CVal::Int(dst));
    }
}

fn parse_cid_range(c: &mut CMap, l: &mut Lexer<'_>) -> Result<(), ()> {
    loop {
        let o = obj(l)?;
        if matches!(o, Tok::Eof) {
            return Ok(());
        }
        if is_cmd(&o, "endcidrange") {
            return Ok(());
        }
        let low = str_to_int(as_str(&o).ok_or(())?);
        let o = obj(l)?;
        let high = str_to_int(as_str(&o).ok_or(())?);
        let o = obj(l)?;
        let dst = as_int(&o).ok_or(())?;
        c.map_cid_range(low, high, dst)?;
    }
}

fn parse_codespace_range(c: &mut CMap, l: &mut Lexer<'_>) -> Result<(), ()> {
    loop {
        let o = obj(l)?;
        if matches!(o, Tok::Eof) {
            break;
        }
        if is_cmd(&o, "endcodespacerange") {
            return Ok(());
        }
        let Some(low) = as_str(&o).map(str_to_int) else {
            break;
        };
        let o = obj(l)?;
        let Some(hs) = as_str(&o) else {
            break;
        };
        let high = str_to_int(hs);
        c.add_codespace_range(hs.len(), low, high);
    }
    Err(())
}

/// `parseCMap` over an embedded stream's bytes.
pub fn parse_cmap(data: &[u8]) -> Result<CMap, BuiltInUnavailable> {
    let mut c = CMap::new(false);
    let mut lexer = Lexer::new(data, false);
    let mut previous: Option<String> = None;
    let mut embedded_use_cmap: Option<String> = None;
    loop {
        let before = lexer.stream.pos;
        let o = match lexer.get_obj() {
            Ok(o) => o,
            // "Invalid cMap data", and keep reading. Every lexer error
            // consumes input, so this cannot spin; the check makes sure.
            Err(_) if lexer.stream.pos > before => continue,
            Err(_) => break,
        };
        match &o {
            Tok::Eof => break,
            Tok::Obj(Obj::Name(n)) => {
                if &**n == "WMode" {
                    if let Ok(t) = lexer.get_obj()
                        && let Some(i) = as_int(&t)
                    {
                        c.vertical = i != 0;
                    }
                } else if &**n == "CMapName"
                    && let Ok(Tok::Obj(Obj::Name(nm))) = lexer.get_obj()
                {
                    c.name = nm.to_string();
                }
                previous = Some(n.to_string());
            }
            Tok::Obj(Obj::Cmd(cmd)) => {
                let r = match &**cmd {
                    "endcmap" => break,
                    "usecmap" => {
                        if let Some(p) = &previous {
                            embedded_use_cmap = Some(p.clone());
                        }
                        Ok(())
                    }
                    "begincodespacerange" => parse_codespace_range(&mut c, &mut lexer),
                    "beginbfchar" => parse_bf_char(&mut c, &mut lexer),
                    "begincidchar" => parse_cid_char(&mut c, &mut lexer),
                    "beginbfrange" => parse_bf_range(&mut c, &mut lexer),
                    "begincidrange" => parse_cid_range(&mut c, &mut lexer),
                    _ => Ok(()),
                };
                let _ = r; // errors are warnings; the loop goes on
                // pdf.js does not reset `previous` on a command.
            }
            _ => {}
        }
        // `previous` tracks the last Name only (pdf.js assigns it in the Name
        // branch and nowhere else).
    }
    if let Some(u) = embedded_use_cmap {
        extend_cmap(&mut c, &u)?;
    }
    if c.is_identity_cmap() {
        return create_built_in_cmap(&c.name.clone());
    }
    Ok(c)
}

fn extend_cmap(c: &mut CMap, use_cmap: &str) -> Result<(), BuiltInUnavailable> {
    let base = create_built_in_cmap(use_cmap)?;
    if c.num_codespace_ranges == 0 {
        c.codespace_ranges = base.codespace_ranges.clone();
        c.num_codespace_ranges = base.num_codespace_ranges;
    }
    let mut add = Vec::new();
    base.for_each(|k, v| {
        if !c.contains(k) {
            add.push((k, v.clone()));
        }
    });
    for (k, v) in add {
        c.map_one(k, v);
    }
    Ok(())
}

/// `createBuiltInCMap`: Identity-H/V, and nothing else without `cMapUrl`.
pub fn create_built_in_cmap(name: &str) -> Result<CMap, BuiltInUnavailable> {
    match name {
        "Identity-H" => Ok(CMap::identity(false, 2)),
        "Identity-V" => Ok(CMap::identity(true, 2)),
        _ => Err(BuiltInUnavailable),
    }
}

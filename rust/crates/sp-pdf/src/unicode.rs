//! pdf.js's glyph-name and Unicode helpers, and the lookups over the
//! generated tables.
//!
//! Ported from pdf.js `src/core/unicode.js`, `glyphlist.js`,
//! `standard_fonts.js`, `encodings.js` and `src/shared/util.js` (Mozilla
//! Foundation, Apache License 2.0).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::tables;

/// A 256-entry encoding: glyph names by char code, "" for none.
pub type Encoding = &'static [&'static str; 256];

pub fn get_encoding(name: &str) -> Option<Encoding> {
    Some(match name {
        "WinAnsiEncoding" => &tables::WIN_ANSI_ENCODING,
        "StandardEncoding" => &tables::STANDARD_ENCODING,
        "MacRomanEncoding" => &tables::MAC_ROMAN_ENCODING,
        "SymbolSetEncoding" => &tables::SYMBOL_SET_ENCODING,
        "ZapfDingbatsEncoding" => &tables::ZAPF_DINGBATS_ENCODING,
        "ExpertEncoding" => &tables::EXPERT_ENCODING,
        "MacExpertEncoding" => &tables::MAC_EXPERT_ENCODING,
        _ => return None,
    })
}

fn str_num_map(t: &'static [(&'static str, u32)]) -> HashMap<&'static str, u32> {
    let mut m = HashMap::with_capacity(t.len());
    for (k, v) in t {
        m.insert(*k, *v);
    }
    m
}

/// `getGlyphsUnicode()`.
pub fn glyphs_unicode() -> &'static HashMap<&'static str, u32> {
    static M: OnceLock<HashMap<&'static str, u32>> = OnceLock::new();
    M.get_or_init(|| str_num_map(&tables::GLYPHS_UNICODE))
}

/// `getDingbatsGlyphsUnicode()`.
pub fn dingbats_glyphs_unicode() -> &'static HashMap<&'static str, u32> {
    static M: OnceLock<HashMap<&'static str, u32>> = OnceLock::new();
    M.get_or_init(|| str_num_map(&tables::DINGBATS_GLYPHS_UNICODE))
}

fn str_str_map(t: &'static [(&'static str, &'static str)]) -> HashMap<&'static str, &'static str> {
    t.iter().copied().collect()
}

pub fn std_font_map() -> &'static HashMap<&'static str, &'static str> {
    static M: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    M.get_or_init(|| str_str_map(&tables::STD_FONT_MAP))
}

pub fn non_std_font_map() -> &'static HashMap<&'static str, &'static str> {
    static M: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    M.get_or_init(|| str_str_map(&tables::NON_STD_FONT_MAP))
}

pub fn is_serif_font_name(n: &str) -> bool {
    tables::SERIF_FONTS.contains(&n)
}

pub fn is_symbols_font_name(n: &str) -> bool {
    tables::SYMBOLS_FONTS.contains(&n)
}

/// `normalizeFontName`: commas and underscores to hyphens, whitespace out.
pub fn normalize_font_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            ',' | '_' => out.push('-'),
            c if is_js_whitespace(c as u32) => {}
            c => out.push(c),
        }
    }
    out
}

/// `getStandardFontName`.
pub fn get_standard_font_name(name: &str) -> Option<&'static str> {
    std_font_map()
        .get(normalize_font_name(name).as_str())
        .copied()
}

/// `isKnownFontName`.
pub fn is_known_font_name(name: &str) -> bool {
    let n = normalize_font_name(name);
    std_font_map().contains_key(n.as_str())
        || non_std_font_map().contains_key(n.as_str())
        || is_serif_font_name(&n)
        || is_symbols_font_name(&n)
}

fn in_ranges(r: &[(u32, u32)], cp: u32) -> bool {
    r.binary_search_by(|&(a, b)| {
        if cp < a {
            std::cmp::Ordering::Greater
        } else if cp > b {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    })
    .is_ok()
}

/// JavaScript's `\s`.
pub fn is_js_whitespace(cp: u32) -> bool {
    in_ranges(&tables::JS_WHITESPACE, cp)
}

fn is_mn(cp: u32) -> bool {
    in_ranges(&tables::UNICODE_MN, cp)
}

fn is_cf(cp: u32) -> bool {
    in_ranges(&tables::UNICODE_CF, cp)
}

/// `getCharUnicodeCategory`: `str.match(/^(\s)|(\p{Mn})|(\p{Cf})$/u)`.
#[derive(Clone, Copy, Default, Debug)]
pub struct Category {
    pub is_whitespace: bool,
    pub is_zero_width_diacritic: bool,
    pub is_invisible_format_mark: bool,
}

pub fn char_unicode_category(s: &str) -> Category {
    let cps: Vec<u32> = s.chars().map(|c| c as u32).collect();
    let n = cps.len();
    for (i, &cp) in cps.iter().enumerate() {
        if i == 0 && is_js_whitespace(cp) {
            return Category {
                is_whitespace: true,
                ..Default::default()
            };
        }
        if is_mn(cp) {
            return Category {
                is_zero_width_diacritic: true,
                ..Default::default()
            };
        }
        if is_cf(cp) && i + 1 == n {
            return Category {
                is_invisible_format_mark: true,
                ..Default::default()
            };
        }
    }
    Category::default()
}

/// `String.fromCharCode(x)` for one code: the low 16 bits. A lone surrogate
/// has no Rust `char`; it becomes U+FFFD (a documented difference).
pub fn from_char_code(x: u32) -> char {
    char::from_u32(x & 0xffff).unwrap_or('\u{fffd}')
}

/// `String.fromCodePoint(x)`.
pub fn from_code_point(x: u32) -> char {
    char::from_u32(x).unwrap_or('\u{fffd}')
}

/// `getUnicodeForGlyph`.
pub fn get_unicode_for_glyph(name: &str, map: &HashMap<&'static str, u32>) -> i64 {
    if let Some(u) = map.get(name) {
        return *u as i64;
    }
    if name.is_empty() {
        return -1;
    }
    if let Some(rest) = name.strip_prefix('u') {
        let len = name.chars().count();
        let hex: String = if len == 7 && rest.starts_with("ni") {
            name.chars().skip(3).collect()
        } else if (5..=7).contains(&len) {
            name.chars().skip(1).collect()
        } else {
            return -1;
        };
        if hex == hex.to_uppercase() {
            // parseInt(hexStr, 16): the leading hex digits, NaN for none.
            let digits: String = hex.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
            if digits.is_empty() {
                return -1;
            }
            if let Ok(v) = i64::from_str_radix(&digits, 16) {
                return v;
            }
        }
    }
    -1
}

/// `recoverGlyphName`.
pub fn recover_glyph_name(name: &str) -> String {
    let map = glyphs_unicode();
    if map.contains_key(name) {
        return name.to_string();
    }
    let unicode = get_unicode_for_glyph(name, map);
    if unicode != -1 {
        // In JavaScript's iteration order, which the generated table keeps.
        for (k, v) in tables::GLYPHS_UNICODE.iter() {
            if *v as i64 == unicode {
                return k.to_string();
            }
        }
    }
    name.to_string()
}

/// `mapSpecialUnicodeValues`.
pub fn map_special_unicode_values(code: u32) -> u32 {
    if (0xfff0..=0xffff).contains(&code) {
        0
    } else if (0xf600..=0xf8ff).contains(&code) {
        tables::SPECIAL_PUA_SYMBOLS
            .iter()
            .find(|(k, _)| *k == code)
            .map(|(_, v)| *v)
            .unwrap_or(code)
    } else if code == 0x00ad {
        0x002d
    } else {
        code
    }
}

/// `getVerticalPresentationForm()[code]`.
pub fn vertical_presentation_form(code: u32) -> Option<u32> {
    tables::VERTICAL_PRESENTATION_FORM
        .iter()
        .find(|(k, _)| *k == code)
        .map(|(_, v)| *v)
}

fn nfkc_of(cp: u32) -> Option<&'static str> {
    static M: OnceLock<HashMap<u32, &'static str>> = OnceLock::new();
    M.get_or_init(|| tables::NFKC_MAP.iter().copied().collect())
        .get(&cp)
        .copied()
}

/// `normalizeUnicode`: NFKC for the code points pdf.js's regex picks out,
/// U+FB05 to "ſt", everything else untouched.
pub fn normalize_unicode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let cp = c as u32;
        if cp == 0xfb05 {
            out.push_str("\u{17f}t");
        } else if let Some(n) = nfkc_of(cp) {
            out.push_str(n);
        } else {
            out.push(c);
        }
    }
    out
}

/// `getMetrics()[name]`, the standard 14 fonts' widths.
pub fn std_metrics(name: &str) -> Option<&'static tables::StdMetrics> {
    tables::METRICS
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

//! pdf.js's simplified bidi pass over a text item.
//!
//! Ported from pdf.js `src/core/bidi.js` (Mozilla Foundation, Apache
//! License 2.0). Works on UTF-16 code units, as the original does.

// Index loops are kept: they mirror the JavaScript line for line.
#![allow(clippy::needless_range_loop)]

use crate::tables::{BIDI_ARABIC_TYPES, BIDI_BASE_TYPES};

pub struct BidiText {
    pub str: String,
    pub dir: &'static str,
}

fn create(str: String, is_ltr: bool, vertical: bool) -> BidiText {
    let dir = if vertical {
        "ttb"
    } else if !is_ltr {
        "rtl"
    } else {
        "ltr"
    };
    BidiText { str, dir }
}

fn find_unequal(arr: &[&'static str], start: usize, value: &str) -> usize {
    let mut j = start;
    while j < arr.len() {
        if arr[j] != value {
            return j;
        }
        j += 1;
    }
    j
}

pub fn bidi(s: &str, vertical: bool) -> BidiText {
    let units: Vec<u16> = s.encode_utf16().collect();
    let n = units.len();
    if n == 0 || vertical {
        return create(s.to_string(), true, vertical);
    }
    let mut types: Vec<&'static str> = Vec::with_capacity(n);
    let mut num_bidi = 0;
    for &cc in &units {
        let mut t: &'static str = "L";
        let c = cc as u32;
        if c <= 0xff {
            t = BIDI_BASE_TYPES[c as usize];
        } else if (0x0590..=0x05f4).contains(&c) {
            t = "R";
        } else if (0x0600..=0x06ff).contains(&c) {
            t = BIDI_ARABIC_TYPES[(c & 0xff) as usize];
        } else if (0x0700..=0x08ac).contains(&c)
            || (0xfb50..=0xfdff).contains(&c)
            || (0xfe70..=0xfeff).contains(&c)
        {
            t = "AL";
        }
        if t == "R" || t == "AL" || t == "AN" {
            num_bidi += 1;
        }
        types.push(t);
    }
    if num_bidi == 0 {
        return create(s.to_string(), true, false);
    }
    let (is_ltr, start_level) = if (num_bidi as f64) / (n as f64) < 0.3 && n > 4 {
        (true, 0)
    } else {
        (false, 1)
    };
    let mut levels = vec![start_level; n];
    let e = if start_level & 1 == 1 { "R" } else { "L" };
    let sor = e;
    let eor = sor;
    let mut last_type = sor;
    for t in types.iter_mut() {
        if *t == "NSM" {
            *t = last_type;
        } else {
            last_type = t;
        }
    }
    last_type = sor;
    for t in types.iter_mut() {
        if *t == "EN" {
            *t = if last_type == "AL" { "AN" } else { "EN" };
        } else if *t == "R" || *t == "L" || *t == "AL" {
            last_type = t;
        }
    }
    for t in types.iter_mut() {
        if *t == "AL" {
            *t = "R";
        }
    }
    for i in 1..n.saturating_sub(1) {
        if types[i] == "ES" && types[i - 1] == "EN" && types[i + 1] == "EN" {
            types[i] = "EN";
        }
        if types[i] == "CS"
            && (types[i - 1] == "EN" || types[i - 1] == "AN")
            && types[i + 1] == types[i - 1]
        {
            types[i] = types[i - 1];
        }
    }
    for i in 0..n {
        if types[i] == "EN" {
            let mut j = i as isize - 1;
            while j >= 0 {
                if types[j as usize] != "ET" {
                    break;
                }
                types[j as usize] = "EN";
                j -= 1;
            }
            for j in i + 1..n {
                if types[j] != "ET" {
                    break;
                }
                types[j] = "EN";
            }
        }
    }
    for t in types.iter_mut() {
        if *t == "WS" || *t == "ES" || *t == "ET" || *t == "CS" {
            *t = "ON";
        }
    }
    last_type = sor;
    for t in types.iter_mut() {
        if *t == "EN" {
            *t = if last_type == "L" { "L" } else { "EN" };
        } else if *t == "R" || *t == "L" {
            last_type = t;
        }
    }
    let mut i = 0;
    while i < n {
        if types[i] == "ON" {
            let end = find_unequal(&types, i + 1, "ON");
            let mut before = sor;
            let mut j = i as isize - 1;
            while j >= 0 {
                let tt = types[j as usize];
                if tt == "L" {
                    before = "L";
                    break;
                }
                if tt == "R" || tt == "EN" || tt == "AN" {
                    before = "R";
                    break;
                }
                j -= 1;
            }
            let mut after = eor;
            for &tt in &types[end..n] {
                if tt == "L" {
                    after = "L";
                    break;
                }
                if tt == "R" || tt == "EN" || tt == "AN" {
                    after = "R";
                    break;
                }
            }
            if before == after {
                for t in &mut types[i..end] {
                    *t = before;
                }
            }
            i = end;
            continue;
        }
        i += 1;
    }
    for t in types.iter_mut() {
        if *t == "ON" {
            *t = e;
        }
    }
    for i in 0..n {
        let t = types[i];
        if levels[i] & 1 == 0 {
            if t == "R" {
                levels[i] += 1;
            } else if t == "AN" || t == "EN" {
                levels[i] += 2;
            }
        } else if t == "L" || t == "AN" || t == "EN" {
            levels[i] += 1;
        }
    }
    let mut highest = -1i32;
    let mut lowest_odd = 99i32;
    for &l in &levels {
        if highest < l {
            highest = l;
        }
        if lowest_odd > l && l & 1 == 1 {
            lowest_odd = l;
        }
    }
    let mut chars: Vec<u16> = units;
    let mut level = highest;
    while level >= lowest_odd {
        let mut start: isize = -1;
        for i in 0..n {
            if levels[i] < level {
                if start >= 0 {
                    chars[start as usize..i].reverse();
                    start = -1;
                }
            } else if start < 0 {
                start = i as isize;
            }
        }
        if start >= 0 {
            chars[start as usize..n].reverse();
        }
        level -= 1;
    }
    let out: Vec<u16> = chars
        .into_iter()
        .filter(|&c| c != b'<' as u16 && c != b'>' as u16)
        .collect();
    create(String::from_utf16_lossy(&out), is_ltr, false)
}

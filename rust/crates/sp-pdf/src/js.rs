//! The JavaScript arithmetic pdf.js leans on, reproduced exactly.

use crate::obj::Obj;

/// `Util.transform(m1, m2)`, in pdf.js's operand order.
pub fn transform(m1: &[f64; 6], m2: &[f64]) -> [f64; 6] {
    let g = |i: usize| m2.get(i).copied().unwrap_or(f64::NAN);
    [
        m1[0] * g(0) + m1[2] * g(1),
        m1[1] * g(0) + m1[3] * g(1),
        m1[0] * g(2) + m1[2] * g(3),
        m1[1] * g(2) + m1[3] * g(3),
        m1[0] * g(4) + m1[2] * g(5) + m1[4],
        m1[1] * g(4) + m1[3] * g(5) + m1[5],
    ]
}

/// JavaScript's ToNumber for a content-stream operand.
pub fn js_to_number(o: &Obj) -> f64 {
    match o {
        Obj::Num(n) => *n,
        Obj::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Obj::Null => 0.0,
        Obj::Str(s) => {
            let t: String = s.iter().map(|&b| b as char).collect();
            let t = t.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
        Obj::Array(a) => match a.len() {
            0 => 0.0,
            1 => js_to_number(&a[0]),
            _ => f64::NAN,
        },
        _ => f64::NAN,
    }
}

/// `Math.min` over two values: NaN wins, and -0 is below +0.
pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == b {
        if a.is_sign_negative() { a } else { b }
    } else if a < b {
        a
    } else {
        b
    }
}

/// `Math.max` over two values: NaN wins, and +0 is above -0.
pub fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == b {
        if a.is_sign_positive() { a } else { b }
    } else if a > b {
        a
    } else {
        b
    }
}

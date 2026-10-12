//! The two pieces of Python's own text the original printed: `json.dumps`
//! with its defaults, and `repr` of a string, which its error messages use.

/// A value `json.dumps` can print, keys in insertion order like a dict.
#[derive(Debug, Clone, PartialEq)]
pub enum Py {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    List(Vec<Py>),
    Dict(Vec<(String, Py)>),
}

impl Py {
    /// `json.dumps(value)`: separators ", " and ": ", ASCII only.
    pub fn dumps(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Py::Int(n) => out.push_str(&n.to_string()),
            Py::Float(f) => out.push_str(&float_repr(*f)),
            Py::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Py::Str(s) => json_string(s, out),
            Py::List(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Py::Dict(pairs) => {
                out.push('{');
                for (i, (key, value)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    json_string(key, out);
                    out.push_str(": ");
                    value.write(out);
                }
                out.push('}');
            }
        }
    }
}

/// Python's `repr(float)`: the shortest text that reads back the same, with
/// a ".0" on a whole number. (Rust's Debug for f64 is that, for the
/// magnitudes a page layout holds.)
pub fn float_repr(f: f64) -> String {
    format!("{f:?}")
}

/// Python's `round(f, 3)`: correctly rounded to three places.
pub fn round3(f: f64) -> f64 {
    format!("{f:.3}").parse().expect("a formatted float parses")
}

/// json.dumps's string, with ensure_ascii: everything outside space to
/// tilde is escaped, astral characters as a surrogate pair.
fn json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

/// Python's `repr(str)`: single quotes unless the text holds one and no
/// double quote.
pub fn repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (0x7f..0xa0).contains(&(c as u32)) => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

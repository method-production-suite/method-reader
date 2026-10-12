//! `parseArgs` from `node:util`, strict, with positionals allowed, as Bun
//! runs it: the same tokens, the same last-wins values, and the same error
//! sentences, because the CLI reports them verbatim as `usage` errors.
//!
//! One Bun quirk is kept on purpose: an unknown option met inside a short
//! group (`-hx`) is named without its dash (`Unknown option 'x'`), where a
//! lone one (`-x`) keeps it.

use std::collections::BTreeMap;

/// An option's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Str,
}

/// One option in the schema: its long name, its type, its short letter.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub name: &'static str,
    pub kind: Kind,
    pub short: Option<char>,
}

pub const fn flag(name: &'static str) -> Spec {
    Spec {
        name,
        kind: Kind::Bool,
        short: None,
    }
}

pub const fn string(name: &'static str) -> Spec {
    Spec {
        name,
        kind: Kind::Str,
        short: None,
    }
}

pub const fn short(spec: Spec, letter: char) -> Spec {
    Spec {
        short: Some(letter),
        ..spec
    }
}

/// A value as parseArgs stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Str(String),
}

/// What parseArgs returns: values by long name, and the positionals.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    pub values: BTreeMap<String, Value>,
    pub positionals: Vec<String>,
}

impl Parsed {
    /// A boolean option: given, or its default false.
    pub fn flag(&self, name: &str) -> bool {
        matches!(self.values.get(name), Some(Value::Bool(true)))
    }

    /// A string option, when given.
    pub fn string(&self, name: &str) -> Option<&str> {
        match self.values.get(name) {
            Some(Value::Str(s)) => Some(s),
            _ => None,
        }
    }
}

struct Token {
    name: String,
    raw: String,
    value: Option<String>,
    inline: bool,
}

fn long_for_short(specs: &[Spec], letter: char) -> String {
    specs
        .iter()
        .find(|s| s.short == Some(letter))
        .map_or_else(|| letter.to_string(), |s| s.name.to_owned())
}

fn kind_of(specs: &[Spec], name: &str) -> Option<Kind> {
    specs.iter().find(|s| s.name == name).map(|s| s.kind)
}

/// `parseArgs({ args, options, allowPositionals: true, strict: true })`;
/// `Err` is the thrown error's message.
pub fn parse(args: &[String], specs: &[Spec]) -> Result<Parsed, String> {
    enum Item {
        Opt(Token),
        Pos(String),
    }
    let mut items = Vec::new();
    let mut remaining: std::collections::VecDeque<(String, bool)> =
        args.iter().map(|a| (a.clone(), false)).collect();
    while let Some((arg, from_group)) = remaining.pop_front() {
        let chars: Vec<char> = arg.chars().collect();
        if arg == "--" {
            items.extend(remaining.drain(..).map(|(a, _)| Item::Pos(a)));
            break;
        }
        let next_is_value = remaining.front().is_some();
        // '-f'
        if chars.len() == 2 && chars[0] == '-' && chars[1] != '-' {
            let name = long_for_short(specs, chars[1]);
            let mut value = None;
            if kind_of(specs, &name) == Some(Kind::Str) && next_is_value {
                value = remaining.pop_front().map(|(a, _)| a);
            }
            let raw = if from_group {
                chars[1].to_string()
            } else {
                arg.clone()
            };
            items.push(Item::Opt(Token {
                name,
                raw,
                value,
                inline: false,
            }));
            continue;
        }
        if chars.len() > 2 && chars[0] == '-' && chars[1] != '-' {
            let first = long_for_short(specs, chars[1]);
            if kind_of(specs, &first) != Some(Kind::Str) {
                // A short group: -abc is -a -b -c; a string option in the
                // middle takes the rest as its value.
                let mut expanded = Vec::new();
                for (i, &c) in chars.iter().enumerate().skip(1) {
                    let name = long_for_short(specs, c);
                    if kind_of(specs, &name) != Some(Kind::Str) || i == chars.len() - 1 {
                        expanded.push((format!("-{c}"), true));
                    } else {
                        expanded
                            .push((format!("-{}", chars[i..].iter().collect::<String>()), true));
                        break;
                    }
                }
                for e in expanded.into_iter().rev() {
                    remaining.push_front(e);
                }
                continue;
            }
            // -fVALUE
            items.push(Item::Opt(Token {
                name: first,
                raw: format!("-{}", chars[1]),
                value: Some(chars[2..].iter().collect()),
                inline: true,
            }));
            continue;
        }
        if chars.len() > 2 && arg.starts_with("--") {
            // `arg.includes('=', 3)`: an '=' right after the dashes names
            // an option, it does not start a value.
            let eq = arg[2..]
                .char_indices()
                .skip(1)
                .find(|&(_, c)| c == '=')
                .map(|(i, _)| i + 2);
            match eq {
                None => {
                    let name = arg[2..].to_owned();
                    let mut value = None;
                    if kind_of(specs, &name) == Some(Kind::Str) && next_is_value {
                        value = remaining.pop_front().map(|(a, _)| a);
                    }
                    items.push(Item::Opt(Token {
                        name,
                        raw: arg.clone(),
                        value,
                        inline: false,
                    }));
                }
                Some(eq) => {
                    let name = arg[2..eq].to_owned();
                    items.push(Item::Opt(Token {
                        raw: format!("--{name}"),
                        name,
                        value: Some(arg[eq + 1..].to_owned()),
                        inline: true,
                    }));
                }
            }
            continue;
        }
        items.push(Item::Pos(arg));
    }

    let mut out = Parsed::default();
    for item in items {
        match item {
            Item::Pos(p) => out.positionals.push(p),
            Item::Opt(t) => {
                let Some(spec) = specs.iter().find(|s| s.name == t.name) else {
                    return Err(format!(
                        "Unknown option '{0}'. To specify a positional argument starting with a '-', place it at the end of the command after '--', as in '-- \"{0}\"",
                        t.raw
                    ));
                };
                let both = match spec.short {
                    Some(c) => format!("-{c}, --{}", spec.name),
                    None => format!("--{}", spec.name),
                };
                match spec.kind {
                    Kind::Str => {
                        let Some(v) = t.value else {
                            return Err(format!("Option '{both} <value>' argument missing"));
                        };
                        if !t.inline && v.chars().count() > 1 && v.starts_with('-') {
                            let example = match spec.short {
                                Some(c) => format!("'--{}=-XYZ' or '-{c}-XYZ'", spec.name),
                                None => format!("'--{}=-XYZ'", spec.name),
                            };
                            return Err(format!(
                                "Option '{0}' argument is ambiguous.\nDid you forget to specify the option argument for '{0}'?\nTo specify an option argument starting with a dash use {example}.",
                                t.raw
                            ));
                        }
                        out.values.insert(spec.name.to_owned(), Value::Str(v));
                    }
                    Kind::Bool => {
                        if t.value.is_some() {
                            return Err(format!("Option '{both}' does not take an argument"));
                        }
                        out.values.insert(spec.name.to_owned(), Value::Bool(true));
                    }
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPECS: &[Spec] = &[
        short(string("output"), 'o'),
        flag("library"),
        string("title"),
        flag("json"),
        short(flag("help"), 'h'),
    ];

    fn run(args: &[&str]) -> Result<Parsed, String> {
        let a: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        parse(&a, SPECS)
    }

    fn unknown(o: &str) -> String {
        format!(
            "Unknown option '{o}'. To specify a positional argument starting with a '-', place it at the end of the command after '--', as in '-- \"{o}\""
        )
    }

    // Every case below was run through Bun's parseArgs on 2026-10-11.
    #[test]
    fn errors_read_as_buns() {
        assert_eq!(run(&["--foo"]).unwrap_err(), unknown("--foo"));
        assert_eq!(run(&["-x"]).unwrap_err(), unknown("-x"));
        assert_eq!(run(&["-hx"]).unwrap_err(), unknown("x"));
        assert_eq!(run(&["-h=1"]).unwrap_err(), unknown("="));
        assert_eq!(run(&["--=x"]).unwrap_err(), unknown("--=x"));
        assert_eq!(run(&["--foo=bar"]).unwrap_err(), unknown("--foo"));
        assert_eq!(
            run(&["--title"]).unwrap_err(),
            "Option '--title <value>' argument missing"
        );
        assert_eq!(
            run(&["-o"]).unwrap_err(),
            "Option '-o, --output <value>' argument missing"
        );
        assert_eq!(
            run(&["--title", "--json"]).unwrap_err(),
            "Option '--title' argument is ambiguous.\nDid you forget to specify the option argument for '--title'?\nTo specify an option argument starting with a dash use '--title=-XYZ'."
        );
        assert_eq!(
            run(&["-o", "--", "x"]).unwrap_err(),
            "Option '-o' argument is ambiguous.\nDid you forget to specify the option argument for '-o'?\nTo specify an option argument starting with a dash use '--output=-XYZ' or '-o-XYZ'."
        );
        assert_eq!(
            run(&["--json=1"]).unwrap_err(),
            "Option '--json' does not take an argument"
        );
    }

    #[test]
    fn values_and_positionals() {
        let p = run(&["-ho", "x"]).unwrap();
        assert!(p.flag("help"));
        assert_eq!(p.string("output"), Some("x"));
        assert_eq!(run(&["-oh"]).unwrap().string("output"), Some("h"));
        assert_eq!(run(&["-o=x"]).unwrap().string("output"), Some("=x"));
        assert_eq!(run(&["-hoy"]).unwrap().string("output"), Some("y"));
        assert_eq!(run(&["--title=--x"]).unwrap().string("title"), Some("--x"));
        assert_eq!(run(&["--title", "-"]).unwrap().string("title"), Some("-"));
        assert_eq!(run(&["--title="]).unwrap().string("title"), Some(""));
        let p = run(&["--title", "a", "--title", "b"]).unwrap();
        assert_eq!(p.string("title"), Some("b"));
        let p = run(&["a", "--json", "b"]).unwrap();
        assert!(p.flag("json"));
        assert_eq!(p.positionals, ["a", "b"]);
        assert_eq!(run(&["--", "--json"]).unwrap().positionals, ["--json"]);
        assert_eq!(run(&["-"]).unwrap().positionals, ["-"]);
        assert!(run(&["--"]).unwrap().positionals.is_empty());
    }
}

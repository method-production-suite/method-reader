//! A small XML reader and writer for the AZW3 writer: enough to take apart
//! the XHTML, package and nav documents our own EPUB carries, change them,
//! and write them back.
//!
//! The KF8 chunker ([`crate::chunk`]) depends on one property above all: a
//! subtree serializes to exactly the bytes it occupies inside the serialized
//! whole. Calibre gets that from lxml and has to patch lxml where it is not
//! true (skeleton.py's tostring); here there is one serializer, written to
//! be compositional, so it holds by construction.
//!
//! It reads well-formed XML only. Our EPUB is written by sp-epub and checked
//! by epubcheck, so a document this cannot read is a bug to hear about, not
//! a page to guess at: it fails, naming what it met.
//!
//! Ports src/azw3/xml.ts.

use std::collections::HashSet;

use sp_fountain::js::{self, is_space as is_js_space};

use crate::Azw3Error;

/// A node: an element or a run of text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XNode {
    Element(XElement),
    Text(String),
}

/// An element: its qualified name as written (`p`, `dc:title`), its
/// attributes in document order, and its children.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct XElement {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<XNode>,
}

/// A parsed document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XDocument {
    pub root: XElement,
}

fn named_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    }
}

fn decode_ref(r: &str) -> Result<char, Azw3Error> {
    if let Some(c) = named_entity(r) {
        return Ok(c);
    }
    let undefined = || {
        Azw3Error::new(format!(
            "The document uses the entity &{r};, which XML does not define."
        ))
    };
    let code = if let Some(hex) = r.strip_prefix("#x") {
        if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(undefined());
        }
        u32::from_str_radix(hex, 16).ok()
    } else if let Some(dec) = r.strip_prefix('#') {
        if dec.is_empty() || !dec.bytes().all(|b| b.is_ascii_digit()) {
            return Err(undefined());
        }
        dec.parse::<u32>().ok()
    } else {
        return Err(undefined());
    };
    code.and_then(char::from_u32).ok_or_else(|| {
        Azw3Error::new(format!(
            "The document uses &{r};, which is no Unicode character."
        ))
    })
}

/// `raw.replace(/&([^;\s&]*);/g, ...)`: every reference decoded, and an
/// ampersand that starts none left as it is.
fn decode_entities(raw: &str) -> Result<String, Azw3Error> {
    if !raw.contains('&') {
        return Ok(raw.to_owned());
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let run = after
            .char_indices()
            .find(|&(_, c)| c == ';' || c == '&' || is_js_space(c))
            .map_or(after.len(), |(i, _)| i);
        if after[run..].starts_with(';') {
            out.push(decode_ref(&after[..run])?);
            rest = &after[run + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    Ok(out)
}

struct Parser<'a> {
    xml: &'a str,
    i: usize,
    stack: Vec<XElement>,
    root: Option<XElement>,
}

impl Parser<'_> {
    fn fail(&self, what: &str) -> Azw3Error {
        Azw3Error::new(format!(
            "Could not read the XML: {what} at offset {}.",
            self.i
        ))
    }

    fn add_text(&mut self, text: String) -> Result<(), Azw3Error> {
        if text.is_empty() {
            return Ok(());
        }
        let Some(parent) = self.stack.last_mut() else {
            if !js::trim(&text).is_empty() {
                return Err(self.fail("text outside the root element"));
            }
            return Ok(());
        };
        if let Some(XNode::Text(last)) = parent.children.last_mut() {
            last.push_str(&text);
        } else {
            parent.children.push(XNode::Text(text));
        }
        Ok(())
    }

    /// `/[A-Za-z_][\w.:-]*/y`, ASCII as JavaScript's `\w` is.
    fn read_name(&mut self) -> Result<String, Azw3Error> {
        let b = self.xml.as_bytes();
        let start = self.i;
        if start >= b.len() || !(b[start].is_ascii_alphabetic() || b[start] == b'_') {
            return Err(self.fail("a missing name"));
        }
        let mut end = start + 1;
        while end < b.len()
            && (b[end].is_ascii_alphanumeric() || matches!(b[end], b'_' | b'.' | b':' | b'-'))
        {
            end += 1;
        }
        self.i = end;
        Ok(self.xml[start..end].to_owned())
    }

    fn skip_space(&mut self) {
        let rest = &self.xml[self.i..];
        let n = rest.find(|c: char| !is_js_space(c)).unwrap_or(rest.len());
        self.i += n;
    }

    fn find(&self, needle: &str, from: usize) -> Option<usize> {
        self.xml
            .get(from..)
            .and_then(|s| s.find(needle))
            .map(|n| from + n)
    }

    fn at(&self, s: &str) -> bool {
        self.xml[self.i..].starts_with(s)
    }

    /// Closes an element: into its parent, or as the root.
    fn place(&mut self, el: XElement) -> Result<(), Azw3Error> {
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(XNode::Element(el));
        } else if self.root.is_some() {
            return Err(self.fail("a second root element"));
        } else {
            self.root = Some(el);
        }
        Ok(())
    }

    fn parse(mut self) -> Result<XDocument, Azw3Error> {
        while self.i < self.xml.len() {
            let Some(lt) = self.find("<", self.i) else {
                let text = decode_entities(&self.xml[self.i..])?;
                self.add_text(text)?;
                break;
            };
            if lt > self.i {
                let text = decode_entities(&self.xml[self.i..lt])?;
                self.add_text(text)?;
            }
            self.i = lt;
            if self.at("<!--") {
                let end = self
                    .find("-->", self.i + 4)
                    .ok_or_else(|| self.fail("an unclosed comment"))?;
                self.i = end + 3;
            } else if self.at("<![CDATA[") {
                let end = self
                    .find("]]>", self.i + 9)
                    .ok_or_else(|| self.fail("an unclosed CDATA section"))?;
                let text = self.xml[self.i + 9..end].to_owned();
                self.add_text(text)?;
                self.i = end + 3;
            } else if self.at("<?") {
                let end = self
                    .find("?>", self.i + 2)
                    .ok_or_else(|| self.fail("an unclosed processing instruction"))?;
                self.i = end + 2;
            } else if self.at("<!") {
                // A doctype. Ours have no internal subset; one with brackets
                // is skipped to its closing bracket first.
                let mut end = self.find(">", self.i);
                if let Some(bracket) = self.find("[", self.i)
                    && end.is_some_and(|e| bracket < e)
                {
                    let close = self.find("]", bracket).unwrap_or(0);
                    end = self.find(">", close);
                }
                let end = end.ok_or_else(|| self.fail("an unclosed doctype"))?;
                self.i = end + 1;
            } else if self.at("</") {
                self.i += 2;
                let name = self.read_name()?;
                self.skip_space();
                if !self.at(">") {
                    return Err(self.fail("a malformed end tag"));
                }
                self.i += 1;
                let open = self.stack.pop();
                match open {
                    Some(el) if el.name == name => self.place(el)?,
                    other => {
                        let open_name = other.map_or_else(|| "nothing".to_owned(), |e| e.name);
                        return Err(self.fail(&format!("</{name}> closing <{open_name}>")));
                    }
                }
            } else {
                self.i += 1;
                let mut el = XElement {
                    name: self.read_name()?,
                    ..XElement::default()
                };
                if self.stack.is_empty() && self.root.is_some() {
                    return Err(self.fail("a second root element"));
                }
                loop {
                    self.skip_space();
                    if self.at("/>") {
                        self.i += 2;
                        self.place(el)?;
                        break;
                    }
                    if self.at(">") {
                        self.i += 1;
                        self.stack.push(el);
                        break;
                    }
                    let attr = self.read_name()?;
                    self.skip_space();
                    if !self.at("=") {
                        return Err(self.fail(&format!("an attribute {attr} with no value")));
                    }
                    self.i += 1;
                    self.skip_space();
                    let quote = match self.xml[self.i..].chars().next() {
                        Some(q @ ('"' | '\'')) => q,
                        _ => return Err(self.fail(&format!("an unquoted value for {attr}"))),
                    };
                    let end = self
                        .find(&quote.to_string(), self.i + 1)
                        .ok_or_else(|| self.fail(&format!("an unclosed value for {attr}")))?;
                    let value = decode_entities(&self.xml[self.i + 1..end])?;
                    el.attrs.push((attr, value));
                    self.i = end + 1;
                }
            }
        }
        if let Some(open) = self.stack.last() {
            return Err(self.fail(&format!("<{}> never closed", open.name)));
        }
        let root = self
            .root
            .take()
            .ok_or_else(|| self.fail("no root element"))?;
        Ok(XDocument { root })
    }
}

/// Parse a whole document. Comments, processing instructions and the
/// doctype are dropped (kindlegen drops them too, skeleton.py notes); CDATA
/// becomes text; adjacent text is merged.
pub fn parse_xml(xml: &str) -> Result<XDocument, Azw3Error> {
    Parser {
        xml,
        i: 0,
        stack: Vec::new(),
        root: None,
    }
    .parse()
}

/// The part of a qualified name after its prefix: `epub:type` is `type`.
pub fn local_name(name: &str) -> &str {
    name.find(':').map_or(name, |c| &name[c + 1..])
}

impl XElement {
    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn set_attr(&mut self, name: &str, value: &str) {
        if let Some(found) = self.attrs.iter_mut().find(|(k, _)| k == name) {
            found.1 = value.to_owned();
        } else {
            self.attrs.push((name.to_owned(), value.to_owned()));
        }
    }

    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|(k, _)| k != name);
    }

    pub fn child_elements(&self) -> impl Iterator<Item = &XElement> {
        self.children.iter().filter_map(|c| match c {
            XNode::Element(e) => Some(e),
            XNode::Text(_) => None,
        })
    }

    /// Every element under this one, in document order, itself left out.
    pub fn descendants(&self) -> Vec<&XElement> {
        let mut out = Vec::new();
        fn walk<'a>(el: &'a XElement, out: &mut Vec<&'a XElement>) {
            for c in el.child_elements() {
                out.push(c);
                walk(c, out);
            }
        }
        walk(self, &mut out);
        out
    }

    /// The elements under this one (itself included) whose local name is
    /// `name`.
    pub fn find_all(&self, name: &str) -> Vec<&XElement> {
        let mut out = Vec::new();
        if local_name(&self.name) == name {
            out.push(self);
        }
        out.extend(
            self.descendants()
                .into_iter()
                .filter(|d| local_name(&d.name) == name),
        );
        out
    }

    /// The text content: all descendants' text joined.
    pub fn text_content(&self) -> String {
        let mut out = String::new();
        fn walk(el: &XElement, out: &mut String) {
            for c in &el.children {
                match c {
                    XNode::Text(t) => out.push_str(t),
                    XNode::Element(e) => walk(e, out),
                }
            }
        }
        walk(self, &mut out);
        out
    }
}

pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\t' => out.push_str("&#9;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
    out
}

/// An element's start tag, without its closing `>`.
pub fn open_tag(el: &XElement) -> String {
    let mut s = format!("<{}", el.name);
    for (k, v) in &el.attrs {
        s.push_str(&format!(" {k}=\"{}\"", escape_attr(v)));
    }
    s
}

/// Write a node. `close_empty` names the elements written `<p></p>` rather
/// than `<p/>`: KF8 wants that for every element that can hold text
/// (skeleton.py's close_self_closing_tags).
pub fn serialize(node: &XNode, close_empty: Option<&HashSet<&str>>) -> String {
    let mut out = String::new();
    write_node(node, close_empty, &mut out);
    out
}

/// [`serialize`] for an element.
pub fn serialize_element(el: &XElement, close_empty: Option<&HashSet<&str>>) -> String {
    let mut out = String::new();
    write_element(el, close_empty, &mut out);
    out
}

fn write_node(node: &XNode, close_empty: Option<&HashSet<&str>>, out: &mut String) {
    match node {
        XNode::Text(t) => out.push_str(&escape_text(t)),
        XNode::Element(e) => write_element(e, close_empty, out),
    }
}

fn write_element(el: &XElement, close_empty: Option<&HashSet<&str>>, out: &mut String) {
    out.push_str(&open_tag(el));
    if el.children.is_empty() {
        if close_empty.is_some_and(|s| s.contains(el.name.as_str())) {
            out.push_str(&format!("></{}>", el.name));
        } else {
            out.push_str("/>");
        }
        return;
    }
    out.push('>');
    for c in &el.children {
        write_node(c, close_empty, out);
    }
    out.push_str(&format!("</{}>", el.name));
}

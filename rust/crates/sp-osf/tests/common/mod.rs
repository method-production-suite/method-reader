//! Shared test helpers: sp-fountain's tokens for an inline Fountain string,
//! and a strict XML read-back (roxmltree, which refuses a malformed
//! document or a forbidden character).
#![allow(dead_code)]

use std::collections::BTreeMap;

use sp_model::Token;

/// fountain-js's tokens for an inline Fountain source, as sp-fountain's
/// tokenizer (the fountain-js port) makes them.
pub fn tokens(source: &str) -> Vec<Token> {
    sp_fountain::tokenize(source).expect("sp-fountain tokenizes the test's Fountain")
}

/// The OSF for an inline Fountain source.
pub fn fountain_to_osf(source: &str) -> String {
    sp_osf::tokens_to_osf(&tokens(source))
}

/// An element, owned.
#[derive(Debug, Clone)]
pub struct XmlElement {
    pub name: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<XmlNode>,
}

#[derive(Debug, Clone)]
pub enum XmlNode {
    Element(XmlElement),
    Text(String),
}

fn own(node: roxmltree::Node<'_, '_>) -> XmlElement {
    XmlElement {
        name: node.tag_name().name().to_owned(),
        attrs: node
            .attributes()
            .map(|a| (a.name().to_owned(), a.value().to_owned()))
            .collect(),
        children: node
            .children()
            .filter_map(|c| {
                if c.is_element() {
                    Some(XmlNode::Element(own(c)))
                } else if c.is_text() {
                    Some(XmlNode::Text(c.text().unwrap_or("").to_owned()))
                } else {
                    None
                }
            })
            .collect(),
    }
}

/// Parse `xml` into its root element, or fail saying what is wrong.
pub fn try_parse_xml(xml: &str) -> Result<XmlElement, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    Ok(own(doc.root_element()))
}

pub fn parse_xml(xml: &str) -> XmlElement {
    try_parse_xml(xml).unwrap_or_else(|e| panic!("not well-formed XML: {e}"))
}

impl XmlElement {
    /// Child elements, optionally only those named `name`.
    pub fn elements(&self, name: Option<&str>) -> Vec<&XmlElement> {
        self.children
            .iter()
            .filter_map(|c| match c {
                XmlNode::Element(e) if name.is_none_or(|n| e.name == n) => Some(e),
                _ => None,
            })
            .collect()
    }

    pub fn child(&self, name: &str) -> Option<&XmlElement> {
        self.elements(Some(name)).into_iter().next()
    }

    /// Every descendant's text, in order.
    pub fn text(&self) -> String {
        self.children
            .iter()
            .map(|c| match c {
                XmlNode::Text(t) => t.clone(),
                XmlNode::Element(e) => e.text(),
            })
            .collect()
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    /// Text children that are not whitespace between elements.
    pub fn has_stray_text(&self) -> bool {
        self.children
            .iter()
            .any(|c| matches!(c, XmlNode::Text(t) if !t.trim().is_empty()))
    }
}

/// One `<para>`, read back: its style element's attributes, its runs.
#[derive(Debug, Clone)]
pub struct Para {
    pub el: XmlElement,
    pub style: XmlElement,
    /// (bold, italic, underline, text) per `<text>` run.
    pub runs: Vec<(bool, bool, bool, String)>,
    pub text: String,
}

impl Para {
    pub fn kind(&self) -> &str {
        self.style.attr("basestylename").unwrap_or("")
    }
}

pub fn to_para(el: &XmlElement) -> Para {
    let style = el
        .child("style")
        .unwrap_or_else(|| panic!("a para without a style: {el:?}"))
        .clone();
    let on = |t: &XmlElement, a: &str| t.attr(a) == Some("1");
    let runs: Vec<(bool, bool, bool, String)> = el
        .elements(Some("text"))
        .into_iter()
        .map(|t| (on(t, "bold"), on(t, "italic"), on(t, "underline"), t.text()))
        .collect();
    Para {
        text: runs.iter().map(|r| r.3.as_str()).collect(),
        runs,
        style,
        el: el.clone(),
    }
}

/// The `<para>`s of one block (`paragraphs` or `titlepage`).
pub fn paras_of(root: &XmlElement, block: &str) -> Option<Vec<Para>> {
    let b = root.child(block)?;
    Some(b.elements(Some("para")).into_iter().map(to_para).collect())
}

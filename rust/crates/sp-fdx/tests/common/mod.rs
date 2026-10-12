//! Shared test helpers: fountain-js's tokens for an inline Fountain string,
//! and a strict XML read-back (tests/helpers/mini-xml.ts's job, done by
//! roxmltree, which refuses a malformed document or a forbidden character).
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::OnceLock;

use sp_model::Token;

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

/// What the TypeScript made of one inline Fountain source.
#[derive(serde::Deserialize)]
struct Recorded {
    /// fountain-js 1.2.4's tokens.
    tokens: Vec<Token>,
    /// src/fdx/fdx.ts's `fountainToFdx` of the same source.
    fdx: String,
}

fn store() -> &'static HashMap<String, Recorded> {
    static RECORDED: OnceLock<HashMap<String, Recorded>> = OnceLock::new();
    RECORDED.get_or_init(|| {
        let text = std::fs::read_to_string(data_dir().join("fountain-tokens.json"))
            .expect("tests/data/fountain-tokens.json");
        serde_json::from_str(&text).expect("recorded tokens read into the model")
    })
}

fn recorded(source: &str) -> Option<&'static Recorded> {
    store().get(source)
}

/// Every recorded source.
pub fn recorded_sources() -> Vec<&'static str> {
    let mut out: Vec<&str> = store().keys().map(String::as_str).collect();
    out.sort();
    out
}

/// The TypeScript writer's FDX for `source`, once recorded.
pub fn typescript_fdx(source: &str) -> Option<&'static str> {
    recorded(source).map(|r| r.fdx.as_str())
}

/// fountain-js 1.2.4's tokens for an inline Fountain source: as the
/// TypeScript recorded them for the sources it was asked about, and
/// otherwise as sp-fountain's tokenizer (the fountain-js port, held to
/// fountain-js by its own frozen cases) makes them. A new source has no
/// TypeScript FDX to compare with, since the TypeScript is gone; it is
/// checked by what the test asserts.
pub fn tokens(source: &str) -> Vec<Token> {
    if let Some(r) = recorded(source) {
        return r.tokens.clone();
    }
    sp_fountain::tokenize(source).expect("sp-fountain tokenizes the test's Fountain")
}

/// The FDX for an inline Fountain source.
pub fn fountain_to_fdx(source: &str) -> String {
    sp_fdx::tokens_to_fdx(&tokens(source))
}

/// An element, owned, the shape mini-xml.ts hands its tests.
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

    pub fn has_text_children(&self) -> bool {
        self.children.iter().any(|c| matches!(c, XmlNode::Text(_)))
    }
}

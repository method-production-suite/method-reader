//! The Fountain sources tests/mobi.test.ts parses, with the TypeScript's
//! answers for each (tests/data/mobi-sources.json, written by
//! tests/data/mobi-sources.ts):
//!
//! - `tokens`: fountain-js 1.2.4's tokens, which the TypeScript tests get
//!   from `new Fountain().parse(source, true)`. sp-fountain's tokenizer is
//!   being ported beside this crate; once it lands, these tests can parse
//!   the sources themselves and check the tokenizer against this file.
//! - `epub`: the EPUB renderer's `usedImages` and `coverImage` for three
//!   formats and two picture sets, which the TypeScript tests asked
//!   `tokensToBody` for (sp-epub is being ported too).
//! - `mobi`: this crate's own TypeScript answers, `mobiArt` and
//!   `tokensToMobiHtml`, for six formats: the renderer's parity check.

#![allow(dead_code)]

use std::sync::LazyLock;

use serde::Deserialize;
use sp_model::Token;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct EpubAnswer {
    pub used: Vec<String>,
    pub cover: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobiAnswer {
    pub order: Vec<String>,
    pub cover_index: Option<usize>,
    pub html: String,
}

#[derive(Debug, Deserialize)]
pub struct Source {
    pub source: String,
    pub tokens: Vec<Token>,
    pub epub: BTreeMap<String, EpubAnswer>,
    pub mobi: BTreeMap<String, MobiAnswer>,
}

pub static SOURCES: LazyLock<Vec<Source>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/mobi-sources.json")).expect("mobi-sources.json")
});

/// The entry for `source`, which must be one the generator listed.
pub fn entry(source: &str) -> &'static Source {
    SOURCES
        .iter()
        .find(|s| s.source == source)
        .unwrap_or_else(|| panic!("no tokens for {source:?}: add it to tests/data/mobi-sources.ts"))
}

/// fountain-js's tokens for `source`.
pub fn tokens(source: &str) -> Vec<Token> {
    entry(source).tokens.clone()
}

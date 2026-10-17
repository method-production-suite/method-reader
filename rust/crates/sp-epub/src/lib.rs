//! Method Reader's EPUB3 renderer, ported from src/epub/ (build.ts, html.ts,
//! css.ts): fountain-js tokens, FormatOptions and pictures to the bytes of a
//! book, plus the reader preview the window shows.
//!
//! | Module | Ports |
//! |---|---|
//! | [`html`] | `tokensToBody`, `tokensToPreviewHtml`, `escapeXml` |
//! | [`css`] | `screenplayCss`, `SCREENPLAY_CSS` |
//! | [`build`] | `buildEpub`, `imageIds`, and convert.ts's content identifier |
//! | [`zip`] | the container, written as JSZip 3.10.1 writes it |
//!
//! Parity with the TypeScript is byte for byte on every entry's name, order,
//! method and contents, and on every zip header field but the compressed
//! sizes and offsets (the deflate stream is zlib-rs's, not pako's; see
//! [`zip`]). `tests/oracle.rs` proves it on every oracle dump, and
//! `tests/options_matrix.rs` across the option sets the TypeScript tests
//! exercise.
//!
//! The notes, slug and cover rules are sp-fountain's, the one copy every
//! renderer asks, and so is the JavaScript-semantics regex engine the
//! TypeScript's inline-markup regexes run on here.

pub mod build;
pub mod css;
pub mod html;
mod js;
pub mod zip;

pub use build::{
    BookImage, BookMeta, EpubError, build_epub, content_identifier, image_ids, source_date_epoch,
};
pub use css::{default_css, screenplay_css};
pub use html::{
    BodyFile, BodyOptions, BookBody, PageEntry, TocEntry, escape_xml, tokens_to_body,
    tokens_to_preview_html,
};

//! Method Reader's MOBI 6 writer, and the PalmDB container the AZW3 shares.
//!
//! | Module | Ports |
//! |---|---|
//! | [`palmdb`] | `src/mobi/palmdb.ts`: the container and the byte tools both Kindle writers use |
//! | [`writer`] | `src/mobi/writer.ts`: the MOBI 6 file |
//! | [`html`] | `src/mobi/html.ts`: tokens to MOBI 6 HTML, [`html::mobi_art`] and the picture gate |
//! | [`probe`] | `tools/mobi-picture-probe.ts`'s book builder |
//! | [`clock`] | the build time: `SOURCE_DATE_EPOCH`, else now |
//!
//! The notes, slug and cover rules are sp-fountain's, the one copy every
//! renderer imports.
//!
//! The MOBI takes fountain-js tokens (sp-model's [`sp_model::Token`]) and
//! [`sp_model::FormatOptions`]:
//!
//! ```
//! use std::collections::HashSet;
//! use sp_mobi::{MobiInput, MobiMeta, build_mobi, mobi_art, tokens_to_mobi_html};
//! use sp_model::{DEFAULT_FORMAT_OPTIONS, Token, TokenType};
//!
//! let tokens = vec![Token::with_text(TokenType::Action, "A door opens.")];
//! let meta = MobiMeta { title: "T".into(), author: None };
//! // MOBI_CARRIES_PICTURES is false: the book is told it can ship none.
//! let art = mobi_art(&tokens, &DEFAULT_FORMAT_OPTIONS, &HashSet::new());
//! let html = tokens_to_mobi_html(&tokens, &meta, &DEFAULT_FORMAT_OPTIONS, &art);
//! let mobi = build_mobi(&MobiInput { title: meta.title, html, time: Some(0), ..Default::default() })
//!     .unwrap();
//! assert_eq!(&mobi[60..68], b"BOOKMOBI");
//! ```

pub mod clock;
pub mod html;
pub mod palmdb;
pub mod probe;
pub mod writer;

pub use html::{MOBI_CARRIES_PICTURES, MobiArt, MobiMeta, mobi_art, tokens_to_mobi_html};
pub use writer::{MobiError, MobiInput, build_mobi};

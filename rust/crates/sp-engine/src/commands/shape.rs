//! The `shape` verb: a script's shape as counts, for a problem report.
//!
//! A person reporting a bad conversion may tick a box to add these counts
//! (docs/superpowers/specs/2026-10-21-report-without-github-design.md). A
//! screenplay is confidential by default, so the answer is integers ONLY:
//! no name, no word, no font, no path. [`ShapeResult`] has no string field
//! at all, which is how that holds: there is nowhere for script text to go.
//!
//! The counts come from the `.fountain`, tokenized by sp-fountain exactly as
//! every renderer reads it, so they describe the book that was made.
//!
//! `pages` is the one count that is not always there. The `.fountain`
//! carries a page as a `= pg N` marker (sp-fountain's serialize.rs writes
//! one where each new PDF page starts, from the second page on), so the
//! script's pages are one more than its markers. With no marker at all (a
//! one-page script, a conversion with `showPageMarkers` off, or a
//! `.fountain` written by hand) there is no reliable count, and `pages` is
//! left out rather than guessed. The title page is never counted, and a
//! page holding nothing but page furniture carries no marker.
//!
//! The parser's warnings are not here: they are sentences, not codes, and a
//! library folder's source.json records only where the script came from.

use serde::Serialize;
use sp_fountain::notes::parse_img_note;
use sp_fountain::tokenizer::tokenize;
use sp_model::{Token, TokenType};

use crate::convert::fountain_text;
use crate::errors::{CliError, EngineError, ErrorCode};
use crate::fsx;

/// The answer: counts only. Never add a string field here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeResult {
    /// Pages as the page markers count them; absent when there are none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<u32>,
    pub scene_headings: u32,
    /// Character cues, one per speech (each side of a dual exchange counts).
    pub cues: u32,
    pub dialogue_blocks: u32,
    pub dual_exchanges: u32,
    /// `[[img: ...]]` notes that name a picture.
    pub pictures: u32,
    pub transitions: u32,
}

/// `= pg N` (fountain-js hands a synopsis its text without the `=`): the
/// shape sp-epub's page-marker pattern `^pg\s+(\S+)$` accepts.
fn is_page_marker(t: &Token) -> bool {
    if t.kind != TokenType::Synopsis {
        return false;
    }
    let Some(rest) = t.text_or_empty().trim().strip_prefix("pg") else {
        return false;
    };
    rest.starts_with(char::is_whitespace) && {
        let label = rest.trim_start();
        !label.is_empty() && !label.contains(char::is_whitespace)
    }
}

/// The counts for these tokens.
pub fn shape_of(tokens: &[Token]) -> ShapeResult {
    let mut s = ShapeResult::default();
    let mut markers = 0u32;
    for t in tokens.iter().filter(|t| !t.is_title()) {
        match t.kind {
            TokenType::SceneHeading => s.scene_headings += 1,
            TokenType::Character => s.cues += 1,
            TokenType::DialogueBegin => s.dialogue_blocks += 1,
            TokenType::DualDialogueBegin => s.dual_exchanges += 1,
            TokenType::Transition => s.transitions += 1,
            TokenType::Note if parse_img_note(t.text_or_empty()).is_some() => s.pictures += 1,
            TokenType::Synopsis if is_page_marker(t) => markers += 1,
            _ => {}
        }
    }
    s.pages = (markers > 0).then_some(markers + 1);
    s
}

/// `shape <script.fountain>`. A refusal names neither the file nor anything
/// in it.
pub fn shape_command(fountain: &str) -> Result<ShapeResult, EngineError> {
    let unreadable = |why: &str| -> EngineError {
        CliError::new(ErrorCode::Unreadable, format!("cannot read the script: {why}")).into()
    };
    if !fsx::is_file(fountain) {
        return Err(unreadable("there is no file there"));
    }
    let bytes = std::fs::read(fountain).map_err(|_| unreadable("the file could not be opened"))?;
    let text = fountain_text(bytes).map_err(|_| unreadable("it is not UTF-8 text"))?;
    let tokens = tokenize(&text).map_err(|_| unreadable("it is not Fountain"))?;
    Ok(shape_of(&tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(text: &str) -> Token {
        Token::with_text(TokenType::Synopsis, text)
    }

    #[test]
    fn only_a_pg_synopsis_is_a_page_marker() {
        assert!(is_page_marker(&marker("pg 2")));
        assert!(is_page_marker(&marker("pg 3A")));
        assert!(!is_page_marker(&marker("pg")));
        assert!(!is_page_marker(&marker("pg 2 3")));
        assert!(!is_page_marker(&marker("pages 2")));
        assert!(!is_page_marker(&Token::with_text(TokenType::Action, "pg 2")));
    }

    #[test]
    fn no_marker_means_no_page_count() {
        assert_eq!(shape_of(&[]).pages, None);
        let two = [marker("pg 2"), marker("pg 3")];
        assert_eq!(shape_of(&two).pages, Some(3));
    }
}

//! The token shape fountain-js 1.2.4 produces, as the engine uses it.
//!
//! Every renderer (src/epub/html.ts, src/mobi/html.ts, src/fdx/fdx.ts,
//! src/fountain/cover.ts, src/fountain/slug.ts) receives the array
//! `new Fountain().parse(text, true).tokens` unmodified. A token is a plain
//! object with these own properties (`node_modules/fountain-js/dist/token.js`):
//!
//! | `type` | other fields |
//! |---|---|
//! | `scene_heading` | `text`, `scene_number` (only when the heading carried `#n#`) |
//! | `action`, `centered`, `transition`, `lyrics`, `synopsis`, `note` | `text` |
//! | `section` | `text`, `depth` (count of `#`) |
//! | `dialogue_begin` | `dual` (`"left"`/`"right"`, absent when not dual) |
//! | `character`, `dialogue`, `parenthetical` | `text` |
//! | `dialogue_end`, `dual_dialogue_begin`, `dual_dialogue_end`, `page_break`, `spaces` | none |
//! | a title-page key, lowercased with spaces as `_` (`title`, `credit`, `author`, `draft_date`, ...) | `is_title: true`, `text` |
//!
//! A title-page token's `type` is whatever key the script wrote, so it can
//! collide with a body type (`Action: x` on a title page is type `action`).
//! `is_title` is what tells them apart, never `type` alone.
//!
//! fountain-js builds a dialogue block bottom-up and reverses it, so token
//! ORDER is part of the contract the tokenizer port must reproduce; the
//! oracle's `tokens.json` is the answer key.

use serde::{Deserialize, Serialize};

/// A token's `type`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum TokenType {
    SceneHeading,
    Action,
    Centered,
    Transition,
    Character,
    Dialogue,
    Parenthetical,
    DialogueBegin,
    DialogueEnd,
    DualDialogueBegin,
    DualDialogueEnd,
    Lyrics,
    Section,
    Synopsis,
    Note,
    PageBreak,
    Spaces,
    /// Any other string: a title-page key such as `title`, `credit`,
    /// `author`, `authors`, `source`, `draft_date`, `contact`.
    Other(String),
}

impl TokenType {
    pub fn as_str(&self) -> &str {
        match self {
            TokenType::SceneHeading => "scene_heading",
            TokenType::Action => "action",
            TokenType::Centered => "centered",
            TokenType::Transition => "transition",
            TokenType::Character => "character",
            TokenType::Dialogue => "dialogue",
            TokenType::Parenthetical => "parenthetical",
            TokenType::DialogueBegin => "dialogue_begin",
            TokenType::DialogueEnd => "dialogue_end",
            TokenType::DualDialogueBegin => "dual_dialogue_begin",
            TokenType::DualDialogueEnd => "dual_dialogue_end",
            TokenType::Lyrics => "lyrics",
            TokenType::Section => "section",
            TokenType::Synopsis => "synopsis",
            TokenType::Note => "note",
            TokenType::PageBreak => "page_break",
            TokenType::Spaces => "spaces",
            TokenType::Other(s) => s,
        }
    }
}

impl From<String> for TokenType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "scene_heading" => TokenType::SceneHeading,
            "action" => TokenType::Action,
            "centered" => TokenType::Centered,
            "transition" => TokenType::Transition,
            "character" => TokenType::Character,
            "dialogue" => TokenType::Dialogue,
            "parenthetical" => TokenType::Parenthetical,
            "dialogue_begin" => TokenType::DialogueBegin,
            "dialogue_end" => TokenType::DialogueEnd,
            "dual_dialogue_begin" => TokenType::DualDialogueBegin,
            "dual_dialogue_end" => TokenType::DualDialogueEnd,
            "lyrics" => TokenType::Lyrics,
            "section" => TokenType::Section,
            "synopsis" => TokenType::Synopsis,
            "note" => TokenType::Note,
            "page_break" => TokenType::PageBreak,
            "spaces" => TokenType::Spaces,
            _ => TokenType::Other(s),
        }
    }
}

impl From<&str> for TokenType {
    fn from(s: &str) -> Self {
        TokenType::from(s.to_owned())
    }
}

impl From<TokenType> for String {
    fn from(t: TokenType) -> Self {
        t.as_str().to_owned()
    }
}

/// `dialogue_begin`'s `dual`: which column of a dual exchange this speech is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dual {
    Left,
    Right,
}

/// One fountain-js token. Absent fields are `None` and are left out of JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    #[serde(rename = "type")]
    pub kind: TokenType,
    /// `true` on every title-page token, absent on every other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_title: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The `#n#` of a scene heading, without the `#`s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual: Option<Dual>,
    /// A section's level: how many `#` opened it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
}

impl Token {
    /// A bare token of `kind` with every optional field absent.
    pub fn new(kind: TokenType) -> Self {
        Token {
            kind,
            is_title: None,
            text: None,
            scene_number: None,
            dual: None,
            depth: None,
        }
    }

    /// A token of `kind` carrying `text`.
    pub fn with_text(kind: TokenType, text: impl Into<String>) -> Self {
        Token {
            text: Some(text.into()),
            ..Token::new(kind)
        }
    }

    /// True for a title-page token.
    pub fn is_title(&self) -> bool {
        self.is_title == Some(true)
    }

    /// `token.text ?? ''`, the way most renderers read it.
    pub fn text_or_empty(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }
}

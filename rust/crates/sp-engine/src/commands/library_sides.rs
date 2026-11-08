//! The `library-sides` verb: one character's scenes as a book of their own
//! (docs/superpowers/specs/2026-11-05-sides-design.md).
//!
//! The book is made from the script's library `.fountain`, tokenized as
//! every book is, with the scenes the character is not in left out, so the
//! scenes read as they do in the script's book. The `.fountain`, that book
//! and the script's settings are not touched: the only thing written is
//! `<folder>/sides/<stem> sides - <NAME>.epub`.
//!
//! The `library-lines` verb lives here too, being the same book but for what
//! it keeps and marks: the whole script, with one character's speech in bold
//! (docs/superpowers/specs/2026-11-06-lines-in-bold-design.md).
//!
//! It takes only what the `library` listing would list, by the same check
//! `library-report` makes ([`super::library::listed_script`]), and only a
//! character the report names ([`super::library_report::count`]).

use std::collections::BTreeSet;
use std::fs;

use serde::Serialize;
use sp_epub::preview_with_bold_speech;
use sp_fdx::{is_scene, text_runs};
use sp_fountain::{image_note_files, page_marker_label, strip_notes, tokenize};
use sp_model::{FormatOptions, Token, TokenType};

use super::library::{LibraryScript, listed_script};
use super::library_report::{count, cue_name};
use crate::convert::{
    carried_pictures, detected_author, epub_of_tokens, epub_with_bold_speech, fountain_text,
};
use crate::env::Env;
use crate::errors::{CliError, EngineError, ErrorCode};
use crate::fsx;
use crate::library::library_root;
use crate::library_images::read_images;
use crate::nodepath::host;
use crate::settings::app_defaults::app_default_options;
use crate::settings::sidecar::load_script_settings;

/// The folder inside a script's folder that its sides are made into.
pub const SIDES_FOLDER: &str = "sides";

/// How much of a character's name a file name holds.
const MAX_NAME_IN_FILE: usize = 60;

/// What `library-sides` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySides {
    /// The folder, as it was given.
    pub folder: String,
    /// The character, as the report spells them.
    pub name: String,
    /// The book's title.
    pub title: String,
    /// Scenes kept.
    pub scenes: u32,
    /// Scenes in the script.
    pub of_scenes: u32,
    /// The book.
    pub epub: String,
    /// The book as one document for the window's Preview, as a conversion
    /// answers.
    pub preview_html: String,
}

/// What `library-lines` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryLines {
    /// The folder, as it was given.
    pub folder: String,
    /// The character, as the report spells them.
    pub name: String,
    /// The book's title.
    pub title: String,
    /// Speeches set in bold.
    pub speeches: u32,
    /// The book.
    pub epub: String,
    /// The book as one document for the window's Preview, as a conversion
    /// answers.
    pub preview_html: String,
}

/// The tokens of one character's sides, and how many scenes they hold.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sides {
    pub tokens: Vec<Token>,
    pub scenes: u32,
    pub of_scenes: u32,
}

/// A character as a name is compared: in capitals, with a curly apostrophe
/// read as a straight one.
fn folded(c: char) -> impl Iterator<Item = char> {
    (if c == '\u{2019}' { '\'' } else { c }).to_uppercase()
}

/// How many bytes of `text` spell `key` (already folded) from its start.
fn spelled_len(text: &str, key: &str) -> Option<usize> {
    let mut want = key.chars();
    for (i, c) in text.char_indices() {
        for got in folded(c) {
            if want.next() != Some(got) {
                return None;
            }
        }
        if want.as_str().is_empty() {
            return Some(i + c.len_utf8());
        }
    }
    None
}

/// Whether `text` names the character as a whole word: spelled as the cue
/// spells it, or in any capitals so long as it has one (Mara, McCoy,
/// Jean-Luc, van der Berg). All in lower case it is an ordinary word: the
/// wren on the sill is not WREN.
///
/// A name joined to letters by a hyphen, or by an apostrophe in front of it,
/// is part of a longer name: LUC is not in Jean-Luc, nor BRIEN in O'Brien.
/// An apostrophe after it still names them (Wren's, Wren'll).
fn names(text: &str, name: &str) -> bool {
    let key: String = name.chars().flat_map(folded).collect();
    !key.is_empty()
        && text.char_indices().any(|(i, _)| {
            let Some(len) = spelled_len(&text[i..], &key) else {
                return false;
            };
            let word = &text[i..i + len];
            let mut before = text[..i].chars().rev();
            let mut after = text[i + len..].chars();
            let joined_before = match before.next() {
                Some(c) if c.is_alphanumeric() => true,
                Some('-' | '\'' | '\u{2019}') => before.next().is_some_and(char::is_alphanumeric),
                _ => false,
            };
            let joined_after = match after.next() {
                Some(c) if c.is_alphanumeric() => true,
                Some('-') => after.next().is_some_and(char::is_alphanumeric),
                _ => false,
            };
            !joined_before
                && !joined_after
                && (word.chars().any(char::is_uppercase) || word == name)
        })
}

/// The character `name` is in these tokens: a cue of theirs, or an action
/// line naming them. Dialogue that mentions them does not count.
fn is_in(section: &[&Token], name: &str, key: &str) -> bool {
    section.iter().any(|t| match t.kind {
        TokenType::Character => cue_name(t.text_or_empty()).to_uppercase() == key,
        TokenType::Action => {
            let text: String = text_runs(&strip_notes(t.text_or_empty()))
                .into_iter()
                .map(|r| r.text)
                .collect();
            names(&text, name)
        }
        _ => false,
    })
}

/// `tokens` with only the title page and the scenes `name` is in. What
/// comes before the first heading is kept by the same test. A kept scene
/// opening on a page whose marker sat in a dropped scene gets that marker
/// again just before its heading.
pub fn sides_of(tokens: &[Token], name: &str) -> Sides {
    let key = name.to_uppercase();
    let mut sides = Sides {
        tokens: tokens.iter().filter(|t| t.is_title()).cloned().collect(),
        ..Sides::default()
    };
    let mut sections: Vec<Vec<&Token>> = vec![Vec::new()];
    for t in tokens.iter().filter(|t| !t.is_title()) {
        if is_scene(t) {
            sides.of_scenes += 1;
            sections.push(Vec::new());
        }
        if let Some(section) = sections.last_mut() {
            section.push(t);
        }
    }
    let mut dropped_marker: Option<&Token> = None;
    for (i, section) in sections.iter().enumerate() {
        if is_in(section, name, &key) {
            sides.tokens.extend(dropped_marker.take().cloned());
            sides.tokens.extend(section.iter().map(|t| (*t).clone()));
            sides.scenes += u32::from(i > 0);
        } else if let Some(marker) = section.iter().rfind(|t| {
            t.kind == TokenType::Synopsis && page_marker_label(t.text_or_empty()).is_some()
        }) {
            dropped_marker = Some(marker);
        }
    }
    sides
}

/// A character's name as a file name holds it: anything but a letter, a
/// digit, a space, a dash or a dot becomes `_`.
fn safe_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .take(MAX_NAME_IN_FILE)
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim_matches([' ', '.']);
    if safe.is_empty() {
        "character".to_owned()
    } else {
        safe.to_owned()
    }
}

/// `<stem> sides - <NAME>.epub`: the stem tells it apart on a reader.
pub fn sides_file_name(stem: &str, name: &str) -> String {
    format!("{stem} sides - {}.epub", safe_name(name))
}

/// `<stem> lines - <NAME>.epub`.
pub fn lines_file_name(stem: &str, name: &str) -> String {
    format!("{stem} lines - {}.epub", safe_name(name))
}

/// The command line's plain-words answer.
pub fn sides_line(sides: &LibrarySides) -> String {
    format!(
        "sides for {}: {} of {} {}, {}",
        sides.name,
        sides.scenes,
        sides.of_scenes,
        if sides.of_scenes == 1 {
            "scene"
        } else {
            "scenes"
        },
        sides.epub
    )
}

/// The command line's plain-words answer.
pub fn lines_line(lines: &LibraryLines) -> String {
    format!(
        "lines in bold for {}: {} {}, {}",
        lines.name,
        lines.speeches,
        if lines.speeches == 1 {
            "speech"
        } else {
            "speeches"
        },
        lines.epub
    )
}

/// `library-sides <folder> <character>`: the library a conversion would
/// land in ([`library_root`]), the app's default settings, and the book.
pub fn library_sides_command(
    platform: &str,
    env: &Env,
    settings_path: Option<&str>,
    folder: &str,
    character: &str,
) -> Result<LibrarySides, EngineError> {
    let defaults = settings_path.map_or_else(FormatOptions::default, app_default_options);
    sides_in_library(
        &library_root(platform, env, settings_path),
        folder,
        character,
        &defaults,
    )
}

/// `library-lines <folder> <character>`: the library a conversion would
/// land in ([`library_root`]), the app's default settings, and the book.
pub fn library_lines_command(
    platform: &str,
    env: &Env,
    settings_path: Option<&str>,
    folder: &str,
    character: &str,
) -> Result<LibraryLines, EngineError> {
    let defaults = settings_path.map_or_else(FormatOptions::default, app_default_options);
    lines_in_library(
        &library_root(platform, env, settings_path),
        folder,
        character,
        &defaults,
    )
}

/// A listed script, read, and the character asked for, as the report names
/// them.
struct Chosen {
    script: LibraryScript,
    text: String,
    tokens: Vec<Token>,
    /// What the book's title opens with.
    base: String,
    name: String,
    speeches: u32,
}

fn chosen(verb: &str, root: &str, folder: &str, character: &str) -> Result<Chosen, EngineError> {
    let (_, script) = listed_script(verb, root, folder)?;
    let unreadable = || {
        CliError::new(
            ErrorCode::Unreadable,
            format!("could not read {}", script.fountain),
        )
    };
    let text = fs::read(&script.fountain)
        .ok()
        .and_then(|bytes| fountain_text(bytes).ok())
        .ok_or_else(unreadable)?;
    let tokens = tokenize(&text).map_err(|_| unreadable())?;
    let asked = cue_name(character).to_uppercase();
    let base = script.title.clone().unwrap_or_else(|| script.name.clone());
    let Some(found) = count(&tokens)
        .characters
        .into_iter()
        .find(|c| !asked.is_empty() && c.name.to_uppercase() == asked)
    else {
        return Err(CliError::new(
            ErrorCode::Usage,
            format!("\"{character}\" does not speak in {base}"),
        )
        .into());
    };
    Ok(Chosen {
        script,
        text,
        tokens,
        base,
        name: found.name,
        speeches: found.speeches,
    })
}

/// The pictures `kept` asks for, when the script's settings show pictures.
fn pictures_of(c: &Chosen, kept: &[Token], format: &FormatOptions) -> Vec<sp_epub::BookImage> {
    let pictures = if format.include_images {
        let named: BTreeSet<String> = image_note_files(&c.text)
            .unwrap_or_default()
            .into_iter()
            .filter(|file| {
                kept.iter()
                    .any(|t| t.text_or_empty().contains(file.as_str()))
            })
            .collect();
        Some(read_images(&c.script.fountain, &named))
    } else {
        None
    };
    carried_pictures(pictures.as_ref().map(|p| &p.images))
}

/// The book's pictures as the preview takes them.
fn preview_images(images: &[sp_epub::BookImage]) -> Vec<(String, Vec<u8>)> {
    images
        .iter()
        .map(|i| (i.name.clone(), i.bytes.clone()))
        .collect()
}

/// `bytes` as `file` in the script's sides folder. Answers the path.
fn write_book(c: &Chosen, file: &str, bytes: &[u8]) -> Result<String, EngineError> {
    let dir = host::join(&[c.script.folder.as_str(), SIDES_FOLDER]);
    let epub = host::join(&[dir.as_str(), file]);
    let unwritable = |why: String| CliError::new(ErrorCode::Unwritable, why);
    if fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(unwritable(format!("{dir} is a link")).into());
    }
    fsx::mkdir_all(&dir)
        .and_then(|()| fsx::write_atomic(&epub, bytes))
        .map_err(|e| unwritable(format!("could not write {epub}: {e}")))?;
    Ok(epub)
}

/// The sides of `character` in the script `folder` is in the library at
/// `root`, its own settings over `defaults`.
pub fn sides_in_library(
    root: &str,
    folder: &str,
    character: &str,
    defaults: &FormatOptions,
) -> Result<LibrarySides, EngineError> {
    let c = chosen("library-sides", root, folder, character)?;
    let sides = sides_of(&c.tokens, &c.name);
    let format = load_script_settings(&c.script.fountain, defaults);
    let images = pictures_of(&c, &sides.tokens, &format);
    let title = format!("{}: {} sides", c.base, c.name);
    let bytes = epub_of_tokens(
        &sides.tokens,
        &format!("{}\nsides: {}", c.text, c.name),
        &title,
        detected_author(&c.tokens).as_deref(),
        &format,
        &images,
    )
    .map_err(|e| CliError::new(ErrorCode::Internal, e.to_string()))?;
    let epub = write_book(&c, &sides_file_name(&c.script.name, &c.name), &bytes)?;
    let preview_html =
        preview_with_bold_speech(&sides.tokens, &format, &preview_images(&images), None);
    Ok(LibrarySides {
        folder: folder.to_owned(),
        name: c.name,
        title,
        scenes: sides.scenes,
        of_scenes: sides.of_scenes,
        epub,
        preview_html,
    })
}

/// The whole script `folder` is in the library at `root`, with `character`'s
/// speech in bold, its own settings over `defaults`.
pub fn lines_in_library(
    root: &str,
    folder: &str,
    character: &str,
    defaults: &FormatOptions,
) -> Result<LibraryLines, EngineError> {
    let c = chosen("library-lines", root, folder, character)?;
    let format = load_script_settings(&c.script.fountain, defaults);
    let images = pictures_of(&c, &c.tokens, &format);
    let title = format!("{}: {} lines", c.base, c.name);
    let key = c.name.to_uppercase();
    let is_theirs = |cue: &str| cue_name(cue).to_uppercase() == key;
    let bytes = epub_with_bold_speech(
        &c.tokens,
        &format!("{}\nlines: {}", c.text, c.name),
        &title,
        detected_author(&c.tokens).as_deref(),
        &format,
        &images,
        Some(&is_theirs),
    )
    .map_err(|e| CliError::new(ErrorCode::Internal, e.to_string()))?;
    let epub = write_book(&c, &lines_file_name(&c.script.name, &c.name), &bytes)?;
    let preview_html = preview_with_bold_speech(
        &c.tokens,
        &format,
        &preview_images(&images),
        Some(&is_theirs),
    );
    Ok(LibraryLines {
        folder: folder.to_owned(),
        name: c.name,
        title,
        speeches: c.speeches,
        epub,
        preview_html,
    })
}

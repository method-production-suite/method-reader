//! The `library` verb: every script in the library folder, read from the
//! folder itself (docs/superpowers/specs/2026-11-02-library-view-design.md),
//! so the window can reopen a book without converting its PDF again.
//!
//! A script is a folder directly under the library root whose source.json
//! names a source and which holds `<stem>.fountain`, the stem being the
//! source's own file name ([`crate::library`] names both the same way).
//! Anything else in the root is left out without a word, and links are
//! never followed: a linked folder could be anywhere.
//!
//! What the answer says about a script comes from its `.fountain`, tokenized
//! by sp-fountain as every book is made from it, never from a cache that
//! could disagree with it: the title page's Title and the scene headings
//! the FDX makes Scene Headings of (mini-slugs counted out). Pages are the
//! one exception: the count the PDF conversion answered (title page
//! included), which source.json keeps bound to the `.fountain`'s digest
//! ([`crate::library::PAGES_KEY`]), so an edited `.fountain` never shows a
//! stale count; without it, the highest `= pg N` marker.
//!
//! The listing READS ONLY: it never makes the root, rewrites a source.json
//! or touches the recent list.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sp_fdx::{scene_heading_count, text_runs};
use sp_fountain::{page_marker_label, strip_notes, tokenize};
use sp_model::{Token, TokenType};

use crate::env::Env;
use crate::library::{library_root, read_page_count, recorded_source, stem_of};
use crate::nodepath::host;
use crate::recent::rfc3339;
use crate::settings::sidecar::{legacy_sidecar_path, sidecar_path};

/// The files beside a script's book besides the book itself, by extension,
/// in the order they are listed.
const OTHER_EXTENSIONS: [&str; 4] = ["azw3", "fdx", "kfx", "mobi"];

/// One script in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryScript {
    /// Its folder, directly under the root.
    pub folder: String,
    /// The stem of its source's file name, which names its files.
    pub name: String,
    /// The `.fountain` title page's Title, emphasis and notes stripped.
    pub title: Option<String>,
    /// The file it was converted from, as source.json records it.
    pub source: String,
    /// Whether that file is still there.
    pub source_exists: bool,
    pub fountain: String,
    /// `<stem>.epub`, when the folder has a book yet.
    pub epub: Option<String>,
    /// The page count its conversion answered, kept in source.json for
    /// this very `.fountain`; else the highest `= pg N` marker; else none.
    pub pages: Option<u32>,
    /// Scene headings, mini-slugs counted out.
    pub scenes: usize,
    /// The `.fountain`'s modified time, RFC 3339 UTC to the second.
    pub converted: String,
    /// `<stem>.azw3`, `.fdx`, `.kfx` and `.mobi`, those that are there.
    pub others: Vec<String>,
    /// The script's own settings file, when it has one (the 0.12 name
    /// while the new one is missing, as the settings verb reads it).
    pub settings: Option<String>,
}

/// The answer: the root it read and its scripts, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryListing {
    pub root: String,
    pub scripts: Vec<LibraryScript>,
}

/// A file itself, not a link to one and not a folder.
fn is_plain_file(path: &str) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

/// The Title, plain: the first `title` token on the title page (the one a
/// book made from this `.fountain` takes), its notes and emphasis markers
/// stripped and its lines joined by a space. `None` when there is no Title
/// or nothing is left of it.
fn plain_title(tokens: &[Token]) -> Option<String> {
    let raw = tokens
        .iter()
        .find(|t| t.is_title() && t.kind.as_str() == "title")?
        .text_or_empty();
    let lines: Vec<String> = strip_notes(raw)
        .split('\n')
        .map(|line| {
            let plain: String = text_runs(line).into_iter().map(|r| r.text).collect();
            sp_fountain::js::trim(&plain).to_owned()
        })
        .filter(|line| !line.is_empty())
        .collect();
    (!lines.is_empty()).then(|| lines.join(" "))
}

/// The highest page a `= pg N` marker names, among the markers whose label
/// is a number.
fn highest_page(tokens: &[Token]) -> Option<u32> {
    tokens
        .iter()
        .filter(|t| t.kind == TokenType::Synopsis && !t.is_title())
        .filter_map(|t| page_marker_label(t.text_or_empty())?.parse::<u32>().ok())
        .max()
}

/// The script in `folder`, or `None` when the folder is not one.
fn script_in(folder: &str) -> Option<(LibraryScript, SystemTime)> {
    let source = recorded_source(folder)?;
    let name = stem_of(&source);
    let prefix = host::join(&[folder, name.as_str()]);
    let fountain = format!("{prefix}.fountain");
    let meta = fs::symlink_metadata(&fountain)
        .ok()
        .filter(|m| m.is_file())?;
    let modified = meta.modified().ok()?;
    let text = String::from_utf8(fs::read(&fountain).ok()?).ok()?;
    let tokens = tokenize(&text).ok()?;
    let seconds = match modified.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
        Err(before) => -i64::try_from(before.duration().as_secs()).unwrap_or(i64::MAX),
    };
    let epub = format!("{prefix}.epub");
    let others = OTHER_EXTENSIONS
        .iter()
        .map(|ext| format!("{prefix}.{ext}"))
        .filter(|p| is_plain_file(p))
        .collect();
    let settings = [sidecar_path(&fountain), legacy_sidecar_path(&fountain)]
        .into_iter()
        .find(|p| is_plain_file(p));
    let script = LibraryScript {
        folder: folder.to_owned(),
        title: plain_title(&tokens),
        source_exists: fs::metadata(&source).is_ok(),
        source,
        epub: is_plain_file(&epub).then_some(epub),
        pages: read_page_count(&fountain, &text).or_else(|| highest_page(&tokens)),
        scenes: scene_heading_count(&tokens),
        converted: rfc3339(seconds),
        others,
        settings,
        fountain,
        name,
    };
    Some((script, modified))
}

/// Every script directly under `root`, newest `.fountain` first, then by
/// name (then by folder, for two scripts of one name). A root that does
/// not exist, or is not a folder, holds none.
pub fn list_library(root: &str) -> LibraryListing {
    let mut found: Vec<(LibraryScript, SystemTime)> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            script_in(&host::join(&[root, name.as_str()]))
        })
        .collect();
    found.sort_by(|(a, a_at), (b, b_at)| {
        b_at.cmp(a_at)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.folder.cmp(&b.folder))
    });
    LibraryListing {
        root: root.to_owned(),
        scripts: found.into_iter().map(|(script, _)| script).collect(),
    }
}

/// `library`: the library a conversion would land in
/// ([`library_root`]: METHOD_READER_LIBRARY, else the chosen folder, else
/// the default), listed.
pub fn library_command(platform: &str, env: &Env, settings_path: Option<&str>) -> LibraryListing {
    list_library(&library_root(platform, env, settings_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_loses_its_notes_and_emphasis_and_keeps_its_words() {
        let title = |text: &str| plain_title(&tokenize(text).unwrap());
        assert_eq!(
            title("Title: ***The*** _Moth_ Hour [[draft]]\n\nINT. A - DAY\n"),
            Some("The Moth Hour".into())
        );
        assert_eq!(
            title("Title:\n    THE MOTH\n    HOUR\n\nINT. A - DAY\n"),
            Some("THE MOTH HOUR".into())
        );
        assert_eq!(title("Title: [[only a note]]\n\nINT. A - DAY\n"), None);
        assert_eq!(title("Credit: Written by\n\nINT. A - DAY\n"), None);
    }

    #[test]
    fn only_a_numbered_marker_counts_as_a_page() {
        let pages = |text: &str| highest_page(&tokenize(text).unwrap());
        assert_eq!(pages("INT. A - DAY\n\n= pg 3\n\nB.\n\n= pg 12\n"), Some(12));
        assert_eq!(pages("INT. A - DAY\n\n= pg 3A\n"), None);
        assert_eq!(pages("INT. A - DAY\n\n= a synopsis\n"), None);
    }
}

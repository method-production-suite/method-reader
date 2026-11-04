//! The `library-report` verb: one library script's report, its length and
//! who speaks how much (docs/superpowers/specs/2026-11-04-conversion-report-design.md).
//!
//! It is read from the script's library `.fountain`, tokenized by
//! sp-fountain as every book is made from it, never from numbers kept at
//! convert time, so every script in the library has one (those brought in
//! as a `.fountain` or an `.osf` too) and it cannot disagree with the book.
//! The title and pages are the Library row's own ([`super::library`]).
//!
//! It takes only what the `library` listing would list, by the same check
//! `library-remove` and `library-rename` make
//! ([`super::library::listed_script`]), and it READS ONLY.

use std::collections::{HashMap, HashSet};
use std::fs;

use serde::Serialize;
use sp_fdx::{is_scene, text_runs};
use sp_fountain::{js, opening_page, page_marker_label, strip_notes, tokenize};
use sp_model::{Token, TokenType};
use sp_parser::cue::strip_extensions;

use super::library::listed_script;
use crate::env::Env;
use crate::errors::{CliError, EngineError, ErrorCode};
use crate::library::library_root;

/// One character's share of a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterReport {
    /// The cue's name, extensions stripped, as first spelled.
    pub name: String,
    /// Cues: one per speech, dual dialogue included.
    pub speeches: u32,
    /// Words of their dialogue lines, parentheticals left out.
    pub words: u32,
    /// Scenes they speak in.
    pub scenes: u32,
    /// The first scene they speak in, counting scene headings from 1;
    /// `None` when they speak only before the first.
    pub first_scene: Option<u32>,
    /// The page they first speak on: the last `= pg N` marker's label
    /// before that cue, as printed, and before the first marker the page
    /// before it (sp-fountain's `opening_page`, as the .fdx reads scene
    /// pages); `None` without markers.
    pub first_page: Option<String>,
}

/// What a `.fountain`'s tokens count to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Counts {
    pub scenes: u32,
    pub speeches: u32,
    pub words: u32,
    /// Most speeches first; a tie goes to whoever spoke first.
    pub characters: Vec<CharacterReport>,
}

/// What `library-report` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptReport {
    /// The folder, as it was given.
    pub folder: String,
    /// What the Library row calls it: its kept name, else its title page's
    /// Title, else its file name.
    pub title: String,
    /// The Library row's pages.
    pub pages: Option<u32>,
    pub scenes: u32,
    pub speeches: u32,
    pub words: u32,
    pub characters: Vec<CharacterReport>,
}

/// Text as a reader sees it: notes and emphasis markers gone.
fn plain(text: &str) -> String {
    text_runs(&strip_notes(text))
        .into_iter()
        .map(|r| r.text)
        .collect()
}

/// The character a cue names: forced `@` and dual `^` dropped, then every
/// trailing parenthetical, by the parser's own `strip_extensions`.
pub fn cue_name(cue: &str) -> String {
    let text = plain(cue);
    let text = js::trim(&text);
    let text = text.strip_prefix('@').unwrap_or(text);
    let text = js::trim(text.strip_suffix('^').unwrap_or(text));
    js::trim(&strip_extensions(text)).to_owned()
}

/// Words in `text`: runs of non-space characters holding a letter or a
/// digit.
pub fn word_count(text: &str) -> u32 {
    let n = plain(text)
        .split(js::is_space)
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Count the script `tokens` hold, title page left out.
pub fn count(tokens: &[Token]) -> Counts {
    struct Seen {
        report: CharacterReport,
        scenes: HashSet<u32>,
    }
    let mut seen: Vec<Seen> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut counts = Counts::default();
    let mut scene = 0u32;
    let mut page: Option<String> = opening_page(tokens);
    let mut speaker: Option<usize> = None;
    for t in tokens.iter().filter(|t| !t.is_title()) {
        match &t.kind {
            _ if is_scene(t) => {
                scene += 1;
                speaker = None;
            }
            TokenType::Synopsis => {
                if let Some(label) = page_marker_label(t.text_or_empty()) {
                    page = Some(label.to_owned());
                }
            }
            TokenType::Character => {
                let name = cue_name(t.text_or_empty());
                if name.is_empty() {
                    speaker = None;
                    continue;
                }
                let k = *by_key.entry(name.to_uppercase()).or_insert_with(|| {
                    seen.push(Seen {
                        report: CharacterReport {
                            name: name.clone(),
                            speeches: 0,
                            words: 0,
                            scenes: 0,
                            first_scene: (scene > 0).then_some(scene),
                            first_page: page.clone(),
                        },
                        scenes: HashSet::new(),
                    });
                    seen.len() - 1
                });
                let s = &mut seen[k];
                s.report.speeches += 1;
                if scene > 0 {
                    s.scenes.insert(scene);
                    s.report.first_scene.get_or_insert(scene);
                }
                counts.speeches += 1;
                speaker = Some(k);
            }
            TokenType::Dialogue => {
                if let Some(k) = speaker {
                    let n = word_count(t.text_or_empty());
                    seen[k].report.words += n;
                    counts.words += n;
                }
            }
            TokenType::Parenthetical | TokenType::DialogueBegin | TokenType::DualDialogueBegin => {}
            TokenType::DialogueEnd | TokenType::DualDialogueEnd => {}
            _ => speaker = None,
        }
    }
    counts.scenes = scene;
    counts.characters = seen
        .into_iter()
        .map(|s| CharacterReport {
            scenes: u32::try_from(s.scenes.len()).unwrap_or(u32::MAX),
            ..s.report
        })
        .collect();
    // Stable: speakers stay in the order they first spoke within a tie.
    counts
        .characters
        .sort_by_key(|c| std::cmp::Reverse(c.speeches));
    counts
}

/// `library-report <folder>`: the library a conversion would land in
/// ([`library_root`]), and the report of `folder` in it.
pub fn library_report_command(
    platform: &str,
    env: &Env,
    settings_path: Option<&str>,
    folder: &str,
) -> Result<ScriptReport, EngineError> {
    report_in_library(&library_root(platform, env, settings_path), folder)
}

/// The report of the script `folder` is in the library at `root`.
pub fn report_in_library(root: &str, folder: &str) -> Result<ScriptReport, EngineError> {
    let (_, script) = listed_script("library-report", root, folder)?;
    // The check above read it already; this read can only fail if the
    // file changed in between.
    let tokens = fs::read_to_string(&script.fountain)
        .ok()
        .and_then(|text| tokenize(&text).ok())
        .ok_or_else(|| {
            CliError::new(
                ErrorCode::Unreadable,
                format!("could not read {}", script.fountain),
            )
        })?;
    let counts = count(&tokens);
    Ok(ScriptReport {
        folder: folder.to_owned(),
        title: script.title.unwrap_or(script.name),
        pages: script.pages,
        scenes: counts.scenes,
        speeches: counts.speeches,
        words: counts.words,
        characters: counts.characters,
    })
}

/// `1,234,567`.
pub fn grouped(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "1 scene", "2 scenes", with the number grouped.
pub fn counted(n: u32, one: &str, many: &str) -> String {
    format!("{} {}", grouped(n), if n == 1 { one } else { many })
}

/// The command line's plain-words answer: the title, the totals, then one
/// line per character.
pub fn report_lines(r: &ScriptReport) -> String {
    let mut totals = Vec::new();
    if let Some(pages) = r.pages {
        totals.push(counted(pages, "page", "pages"));
    }
    totals.push(counted(r.scenes, "scene", "scenes"));
    totals.push(counted(r.speeches, "speech", "speeches"));
    totals.push(format!("{} of dialogue", counted(r.words, "word", "words")));
    let mut out = vec![r.title.clone(), totals.join(", ")];
    for c in &r.characters {
        let from = match (c.first_scene, &c.first_page) {
            (Some(s), Some(p)) => format!("from scene {s}, page {p}"),
            (Some(s), None) => format!("from scene {s}"),
            (None, Some(p)) => format!("before the first scene, page {p}"),
            (None, None) => "before the first scene".to_owned(),
        };
        out.push(format!(
            "{}: {}, {}, {}, {from}",
            c.name,
            counted(c.speeches, "speech", "speeches"),
            counted(c.words, "word", "words"),
            counted(c.scenes, "scene", "scenes"),
        ));
    }
    out.join("\n")
}

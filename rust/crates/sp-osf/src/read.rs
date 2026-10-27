//! OSF in: an Open Screenplay Format document read back into a `.fountain`
//! text, which then converts exactly as a `.fountain` someone chose does
//! (docs/osf-input.md). The inverse of this crate's writer and its mapping
//! (docs/superpowers/specs/2026-10-20-osf-out-design.md), read, like the
//! writer, only from the format's own published example documents.
//!
//! The reader writes Fountain the way Method Reader's own serializer does
//! (`@` on every cue, `> ` on every transition, a dot-forced mini-slug), and
//! checks each block it writes by tokenizing it alone: a block that would
//! read as something other than what the paragraph said is forced, so an
//! action line that happens to look like a cue or a heading stays action.
//!
//! Everything the writer leaves out is ignored when another app wrote it:
//! `<info>`, `<settings>`, `<spelling>`, `<lists>`, revision marks, notes,
//! fonts, colors and every attribute not named below. `<styles>` is read
//! only to learn which built-in style a declared style is based on.

use std::collections::HashMap;
use std::fmt;

use roxmltree::{Document, Node};
use sp_fdx::text_runs;
use sp_fountain::{is_primary_slug, tokenize};
use sp_model::TokenType;

/// Why a document could not be read. The messages are the plain words a
/// refusal shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// Not well-formed XML; what the XML reader said.
    NotXml(String),
    /// Well-formed, but no `<paragraphs>` under the root.
    NoParagraphs,
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::NotXml(why) => write!(
                f,
                "this script is not a readable OSF file: it is not well-formed XML ({why})"
            ),
            ReadError::NoParagraphs => {
                f.write_str("this file is XML but not an OSF script: it has no <paragraphs>")
            }
        }
    }
}

impl std::error::Error for ReadError {}

/// The format's eight built-in styles, in the order the examples number
/// them (0 to 7).
const BUILT_INS: [&str; 8] = [
    "Normal Text",
    "Scene Heading",
    "Action",
    "Character",
    "Parenthetical",
    "Dialogue",
    "Transition",
    "Shot",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    SceneHeading,
    Action,
    Character,
    Parenthetical,
    Dialogue,
    Transition,
    Shot,
}

fn kind_of_built_in(name: &str) -> Option<Kind> {
    let index = match name.trim().parse::<usize>() {
        Ok(i) if i < BUILT_INS.len() => i,
        _ => BUILT_INS
            .iter()
            .position(|b| b.eq_ignore_ascii_case(name.trim()))?,
    };
    Some(match index {
        1 => Kind::SceneHeading,
        3 => Kind::Character,
        4 => Kind::Parenthetical,
        5 => Kind::Dialogue,
        6 => Kind::Transition,
        7 => Kind::Shot,
        // Normal Text in the body is plain prose: action.
        _ => Kind::Action,
    })
}

/// A `<style>`'s base style: `basestylename` (1.2 and 2.0), `basestyle`
/// (4.0), or the camel-case spelling one third-party parser documents.
fn base_of<'a>(style: Node<'a, '_>) -> Option<&'a str> {
    ["basestylename", "basestyle", "baseStyleName", "baseStyle"]
        .into_iter()
        .find_map(|a| style.attribute(a))
}

fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn elements<'a, 'i>(node: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children()
        .filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn on(node: Node<'_, '_>, attr: &str) -> bool {
    node.attribute(attr)
        .is_some_and(|v| matches!(v.trim(), "1" | "true" | "yes"))
}

/// The declared styles, by name: what each is based on.
fn declared_styles(root: Node<'_, '_>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(styles) = child(root, "styles") {
        for s in elements(styles, "style") {
            if let (Some(name), Some(base)) = (s.attribute("name"), base_of(s)) {
                out.insert(name.trim().to_lowercase(), base.to_owned());
            }
        }
    }
    out
}

/// A style name to its kind: a built-in directly, a declared style through
/// what it is based on (a few hops at most), anything else as action.
fn resolve(name: &str, declared: &HashMap<String, String>) -> Kind {
    let mut name = name.to_owned();
    for _ in 0..8 {
        if let Some(k) = kind_of_built_in(&name) {
            return k;
        }
        match declared.get(&name.trim().to_lowercase()) {
            Some(base) => name.clone_from(base),
            None => break,
        }
    }
    Kind::Action
}

/// One run of text and its emphasis.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Run {
    text: String,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl Run {
    fn style(&self) -> Option<String> {
        let parts: Vec<&str> = [
            (self.bold, "Bold"),
            (self.italic, "Italic"),
            (self.underline, "Underline"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        (!parts.is_empty()).then(|| parts.join("+"))
    }

    fn plain(&self) -> bool {
        !(self.bold || self.italic || self.underline)
    }
}

/// One `<para>`, read.
struct Para {
    kind: Kind,
    /// Its lines (soft returns split them), each a list of runs, blank
    /// lines dropped as the writer drops them.
    lines: Vec<Vec<Run>>,
    scene_number: Option<String>,
    center: bool,
    page_break: bool,
    dual: bool,
    bookmark: Option<String>,
}

impl Para {
    fn plain_lines(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|l| l.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect()
    }

    /// The text on one line: a heading, a cue, a transition.
    fn plain_one_line(&self) -> String {
        self.plain_lines()
            .iter()
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn read_para(p: Node<'_, '_>, declared: &HashMap<String, String>) -> Para {
    let style = child(p, "style");
    let attr_either = |name: &str| {
        style
            .and_then(|s| s.attribute(name))
            .or_else(|| p.attribute(name))
    };
    let flag_either = |name: &str| style.is_some_and(|s| on(s, name)) || on(p, name);
    let kind = style
        .and_then(base_of)
        .map_or(Kind::Action, |b| resolve(b, declared));

    let mut lines: Vec<Vec<Run>> = vec![Vec::new()];
    for t in elements(p, "text") {
        let text: String = t
            .descendants()
            .filter(|d| d.is_text())
            .filter_map(|d| d.text())
            .collect::<String>()
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let (bold, italic, underline) = (on(t, "bold"), on(t, "italic"), on(t, "underline"));
        for (i, piece) in text.split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
            }
            if !piece.is_empty() {
                lines.last_mut().expect("a line").push(Run {
                    text: piece.replace('\t', "    "),
                    bold,
                    italic,
                    underline,
                });
            }
        }
    }
    lines.retain(|l| l.iter().any(|r| !r.text.trim().is_empty()));

    let scene_number = attr_either("scene_number")
        .map(str::trim)
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || "._-".contains(c)))
        .map(str::to_owned);
    Para {
        kind,
        lines,
        scene_number,
        center: attr_either("align").is_some_and(|a| a.trim().eq_ignore_ascii_case("center")),
        page_break: flag_either("pagebreakbefore"),
        dual: flag_either("dualdialogue"),
        bookmark: attr_either("bookmark")
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(str::to_owned),
    }
}

/// `\*` and `\_`: a literal asterisk and underscore.
fn escape_markers(s: &str) -> String {
    s.replace('*', "\\*").replace('_', "\\_")
}

fn markup_with(runs: &[Run], escape: bool) -> String {
    let mut out = String::new();
    for r in runs {
        let text = if escape {
            escape_markers(&r.text)
        } else {
            r.text.clone()
        };
        let core = text.trim();
        if r.plain() || core.is_empty() {
            out.push_str(&text);
            continue;
        }
        // A marker has to hug its text, so the run's edge spaces sit
        // outside it.
        let lead = &text[..text.len() - text.trim_start().len()];
        let trail = &text[text.trim_end().len()..];
        let stars = match (r.bold, r.italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        };
        let under = if r.underline { "_" } else { "" };
        out.push_str(lead);
        out.push_str(under);
        out.push_str(stars);
        out.push_str(core);
        out.push_str(stars);
        out.push_str(under);
        out.push_str(trail);
    }
    out
}

/// The runs merged where two neighbours share a style, as `text_runs`
/// answers them.
fn normalized(runs: &[Run]) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for r in runs.iter().filter(|r| !r.text.is_empty()) {
        let style = r.style();
        match out.last_mut() {
            Some(last) if last.1 == style => last.0.push_str(&r.text),
            _ => out.push((r.text.clone(), style)),
        }
    }
    out
}

/// One line of runs as Fountain: emphasis as markers, written bare where
/// that already reads back as the same runs, with literal markers escaped
/// where it does not. Never a boneyard delimiter.
fn markup(runs: &[Run]) -> String {
    let want = normalized(runs);
    let reads_back = |s: &str| {
        let got: Vec<(String, Option<String>)> =
            text_runs(s).into_iter().map(|r| (r.text, r.style)).collect();
        got == want
    };
    let bare = markup_with(runs, false);
    if !bare.contains("/*") && !bare.contains("*/") && reads_back(&bare) {
        return bare;
    }
    markup_with(runs, true)
}

/// Whether `block`, tokenized alone, is exactly one token of `kind`.
fn reads_alone_as(block: &str, kind: TokenType) -> bool {
    tokenize(block).is_ok_and(|tokens| {
        let content: Vec<_> = tokens.iter().filter(|t| t.text.is_some()).collect();
        content.len() == 1 && content[0].kind == kind && !content[0].is_title()
    })
}

/// Action as Fountain, forced with `!` when it would read as anything else.
fn action_block(p: &Para) -> String {
    let lines: Vec<String> = p.lines.iter().map(|l| markup(l)).collect();
    if p.center {
        let centered: Vec<String> = lines.iter().map(|l| format!("> {} <", l.trim())).collect();
        let block = centered.join("\n");
        if reads_alone_as(&block, TokenType::Centered) {
            return block;
        }
    }
    let block = lines.join("\n");
    if reads_alone_as(&block, TokenType::Action) {
        block
    } else {
        format!("!{block}")
    }
}

fn heading_block(p: &Para) -> String {
    let text = markup(&p.lines.concat()).trim().to_owned();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let numbered = |t: String| match (&p.scene_number, p.kind) {
        (Some(n), Kind::SceneHeading) => format!("{t} #{n}#"),
        _ => t,
    };
    // A heading that opens like one is written bare; anything else takes
    // the dot-force, which Method Reader reads as a mini-slug (the Shot it
    // writes is one). A leading dot is the one text the dot cannot carry.
    let candidate = if p.kind == Kind::SceneHeading && is_primary_slug(&text) {
        numbered(text.clone())
    } else {
        numbered(format!(".{text}"))
    };
    if !text.starts_with('.') && reads_alone_as(&candidate, TokenType::SceneHeading) {
        candidate
    } else {
        format!("!{text}")
    }
}

fn transition_block(p: &Para) -> String {
    let text = markup(&p.lines.concat());
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let block = format!("> {text}");
    if reads_alone_as(&block, TokenType::Transition) {
        block
    } else {
        format!("!{text}")
    }
}

fn parenthetical_line(p: &Para) -> String {
    let text = p.plain_one_line();
    let text = text.trim();
    if text.starts_with('(') && text.ends_with(')') {
        text.to_owned()
    } else {
        format!("({})", text.trim_start_matches('(').trim_end_matches(')'))
    }
}

/// The script's body, as Fountain blocks.
fn body_blocks(paras: &[Para]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // The open speech: its cue line and the lines under it.
    let mut speech: Option<(String, Vec<String>)> = None;
    // The cue after a dual-marked first cue takes the `^`.
    let mut dual_next = false;

    let close = |out: &mut Vec<String>, speech: &mut Option<(String, Vec<String>)>| {
        if let Some((cue, lines)) = speech.take() {
            if lines.is_empty() {
                // A cue with nothing under it is not a speech in Fountain.
                let text = cue.trim_start_matches('@').trim_end_matches(" ^");
                out.push(format!("!{text}"));
            } else {
                out.push(format!("{cue}\n{}", lines.join("\n")));
            }
        }
    };

    // A page break on an empty paragraph waits for the next one.
    let mut page_break = false;
    for p in paras {
        page_break |= p.page_break;
        if p.lines.is_empty() {
            continue;
        }
        if std::mem::take(&mut page_break) {
            close(&mut out, &mut speech);
            out.push("===".to_owned());
        }
        match p.kind {
            Kind::Character => {
                close(&mut out, &mut speech);
                let cue: String = p
                    .plain_one_line()
                    .chars()
                    .filter(|&c| c != '^')
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                // The writer marks the first cue only; a document that marks
                // both still makes one pair.
                let caret = dual_next;
                dual_next = p.dual && !caret;
                let caret = if caret { " ^" } else { "" };
                speech = Some((format!("@{cue}{caret}"), Vec::new()));
            }
            Kind::Parenthetical | Kind::Dialogue if speech.is_some() => {
                let lines = &mut speech.as_mut().expect("an open speech").1;
                if p.kind == Kind::Parenthetical {
                    lines.push(parenthetical_line(p));
                } else {
                    lines.extend(p.lines.iter().map(|l| markup(l).trim().to_owned()));
                }
            }
            kind => {
                close(&mut out, &mut speech);
                dual_next = false;
                out.push(match kind {
                    Kind::SceneHeading | Kind::Shot => heading_block(p),
                    Kind::Transition => transition_block(p),
                    // Action, Normal Text, any style we do not know, and
                    // speech with no cue above it.
                    _ => action_block(p),
                });
            }
        }
    }
    close(&mut out, &mut speech);
    out
}

/// The title-page keys, in the order the writer lays them out.
const TITLE_KEYS: [&str; 8] = [
    "Title",
    "Credit",
    "Author",
    "Source",
    "Contact",
    "Draft date",
    "Copyright",
    "Notes",
];

/// A paragraph bookmark (the 2.0 example names Title, Author, Copyright,
/// Draft and Contact) as a Fountain title-page key.
fn key_for_bookmark(bookmark: &str) -> Option<&'static str> {
    let b = bookmark.to_lowercase();
    Some(match b.as_str() {
        "title" => "Title",
        "credit" => "Credit",
        "author" | "authors" | "writer" | "written by" => "Author",
        "source" => "Source",
        "contact" => "Contact",
        "draft" | "draft date" | "date" => "Draft date",
        "copyright" => "Copyright",
        "notes" => "Notes",
        _ => return None,
    })
}

fn looks_like_credit(lines: &[String]) -> bool {
    let text = lines.join(" ").trim().to_lowercase();
    text == "by" || (text.ends_with(" by") && text.split_whitespace().count() <= 4)
}

fn looks_like_copyright(lines: &[String]) -> bool {
    let text = lines.join(" ").to_lowercase();
    text.contains('©') || text.contains("(c)") || text.starts_with("copyright")
}

fn looks_like_date(lines: &[String]) -> bool {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let text = lines.join(" ").to_lowercase();
    text.contains("draft")
        || (text.chars().any(|c| c.is_ascii_digit())
            && (text.contains('/') || MONTHS.iter().any(|m| text.contains(m))))
}

/// The title page as Fountain key/value lines. Bookmarks name a key where
/// the document gives them; otherwise the writer's layout is read back: the
/// centered groups are the title, the credit (when it reads like one), the
/// author and the source, in that order, and the groups at the foot are the
/// contact, a draft date and a copyright by what they say.
fn title_block(paras: &[Para]) -> Option<String> {
    let mut keyed: Vec<(&'static str, Vec<String>)> = Vec::new();
    // Unkeyed groups: centered or not, and their lines.
    let mut groups: Vec<(bool, Vec<String>)> = Vec::new();
    let mut open_key: Option<&'static str> = None;
    let mut open_group = false;
    let mut push_keyed = |keyed: &mut Vec<(&'static str, Vec<String>)>, key, line: String| {
        match keyed.iter_mut().find(|(k, _)| *k == key) {
            Some((_, lines)) => lines.push(line),
            None => keyed.push((key, vec![line])),
        }
    };
    for p in paras {
        let lines: Vec<String> = p
            .plain_lines()
            .iter()
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty() {
            open_key = None;
            open_group = false;
            continue;
        }
        if let Some(key) = p.bookmark.as_deref().and_then(key_for_bookmark) {
            open_key = Some(key);
            open_group = false;
        }
        if let Some(key) = open_key {
            for l in lines {
                push_keyed(&mut keyed, key, l);
            }
            continue;
        }
        if open_group && let Some(g) = groups.last_mut() {
            g.1.extend(lines);
        } else {
            groups.push((p.center, lines));
            open_group = true;
        }
    }

    let taken = |keyed: &[(&'static str, Vec<String>)], key: &str| keyed.iter().any(|(k, _)| *k == key);
    for (center, lines) in groups {
        let key = if center {
            if !taken(&keyed, "Title") {
                "Title"
            } else if looks_like_credit(&lines) && !taken(&keyed, "Credit") {
                "Credit"
            } else {
                ["Author", "Source"]
                    .into_iter()
                    .find(|k| !taken(&keyed, k))
                    .unwrap_or("Notes")
            }
        } else if looks_like_copyright(&lines) && !taken(&keyed, "Copyright") {
            "Copyright"
        } else if looks_like_date(&lines) && !taken(&keyed, "Draft date") {
            "Draft date"
        } else if !taken(&keyed, "Contact") {
            "Contact"
        } else {
            "Notes"
        };
        for l in lines {
            push_keyed(&mut keyed, key, l);
        }
    }
    if keyed.is_empty() {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    for key in TITLE_KEYS {
        let Some((_, lines)) = keyed.iter().find(|(k, _)| *k == key) else {
            continue;
        };
        if let [one] = lines.as_slice() {
            out.push(format!("{key}: {one}"));
        } else {
            out.push(format!("{key}:"));
            out.extend(lines.iter().map(|l| format!("   {l}")));
        }
    }
    Some(out.join("\n"))
}

/// An OSF document as a `.fountain` text. Refuses a document that is not
/// well-formed XML, or has no `<paragraphs>`; an empty `<paragraphs>` reads
/// as an empty script, for the engine's own guard to refuse.
pub fn osf_to_fountain(xml: &str) -> Result<String, ReadError> {
    let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
    let doc = Document::parse(xml).map_err(|e| ReadError::NotXml(e.to_string()))?;
    let root = doc.root_element();
    let paragraphs = child(root, "paragraphs").ok_or(ReadError::NoParagraphs)?;
    let declared = declared_styles(root);
    let body: Vec<Para> = elements(paragraphs, "para")
        .map(|p| read_para(p, &declared))
        .collect();
    let title: Vec<Para> = child(root, "titlepage")
        .map(|t| elements(t, "para").map(|p| read_para(p, &declared)).collect())
        .unwrap_or_default();

    let mut blocks: Vec<String> = Vec::new();
    if let Some(t) = title_block(&title) {
        blocks.push(t);
    }
    blocks.extend(body_blocks(&body));
    if blocks.is_empty() {
        return Ok(String::new());
    }
    Ok(blocks.join("\n\n") + "\n")
}

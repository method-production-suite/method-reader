//! fountain-js tokens to the Open Screenplay Format (.osf): the third
//! editable script, beside the .fountain and the .fdx and fed the same
//! tokens from the same .fountain. OSF is an open XML format; what is
//! written here, and why, is docs/superpowers/specs/2026-10-20-osf-out-design.md,
//! read only from the format's own published example documents (the OSF
//! SDK). Nothing here comes from any app's software or files.
//!
//! Deliberately minimal, like sp-fdx: a `document` root, the script's
//! paragraphs and its title page, and nothing else. No info, settings,
//! styles or lists blocks: every paragraph names one of the format's
//! built-in styles, and leaving page size, margins, fonts and indents out
//! lets the receiving app's own screenplay template decide how the page
//! looks.
//!
//! Format-option-free on purpose: this is a reconstruction of the script,
//! not a rendering of the book, so no reading knob changes it. Notes are
//! stripped through sp-fountain's notes.rs and a mini-slug is a Shot
//! through its slug.rs, as in every renderer; the emphasis pairing is
//! sp-fdx's `text_runs`, so the two editable scripts agree on every style.

use sp_model::{Token, TokenType};

use sp_fdx::{TextRun, text_runs};
use sp_fountain::{is_mini_slug, js, strip_notes};

/// The root's `version` attribute. ONE constant. The format calls itself
/// 2.0, and the SDK's own 2.0 example document declares "30" (its 1.2
/// example "12", its 4.0 example "40"), so "30" is the value a reader of
/// 2.0 documents has certainly been shown. It also goes with the
/// `basestylename` spelling written here, which 4.0 renamed `basestyle`.
pub const OSF_VERSION: &str = "30";

/// The root's `type` attribute, as every published example spells it.
pub const OSF_DOCUMENT_TYPE: &str = "Open Screenplay Format document";

const XML_DECLARATION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>"#;

/// XML 1.0's Char production, inverted: what may never appear in a
/// document, escaped or not.
fn is_forbidden_xml(c: char) -> bool {
    matches!(
        c,
        '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}'
    )
}

fn escape_text(s: &str, out: &mut String) {
    for c in s.chars().filter(|&c| !is_forbidden_xml(c) && c != '\r') {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

fn escape_attr(s: &str, out: &mut String) {
    let kept: String = s
        .chars()
        .map(|c| {
            if matches!(c, '\t' | '\n' | '\r') {
                ' '
            } else {
                c
            }
        })
        .collect();
    escape_text(&kept, out);
}

type Attrs<'a> = [(&'a str, Option<&'a str>)];

fn attrs(list: &Attrs<'_>, out: &mut String) {
    for (name, value) in list {
        if let Some(value) = value {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            escape_attr(value, out);
            out.push('"');
        }
    }
}

/// One `<text>` run: `bold`, `italic` and `underline` set to "1" where the
/// run carries them, nothing on a plain run.
fn run(r: &TextRun, indent: &str, out: &mut String) {
    let style = r.style.as_deref().unwrap_or("");
    let has = |name: &str| style.split('+').any(|s| s == name);
    out.push_str(indent);
    out.push_str("<text");
    attrs(
        &[
            ("bold", has("Bold").then_some("1")),
            ("italic", has("Italic").then_some("1")),
            ("underline", has("Underline").then_some("1")),
        ],
        out,
    );
    out.push('>');
    escape_text(&r.text, out);
    out.push_str("</text>\n");
}

/// What one paragraph says: its built-in style, the attributes on its
/// `<para>` and on its `<style>`, and its text.
struct Para<'a> {
    style: &'static str,
    scene_number: Option<&'a str>,
    align: Option<&'static str>,
    page_break: bool,
    dual: bool,
    text: String,
}

impl Para<'_> {
    fn plain(style: &'static str, align: Option<&'static str>, text: String) -> Self {
        Para {
            style,
            scene_number: None,
            align,
            page_break: false,
            dual: false,
            text,
        }
    }

    /// The `<para>` element at `indent`, the way the published examples lay
    /// one out: the style first, then the runs, one element per line. The
    /// whitespace between them is markup, not text, in OSF.
    fn write(&self, indent: &str, out: &mut String) {
        let inner = format!("{indent}  ");
        out.push_str(indent);
        out.push_str("<para");
        attrs(&[("scene_number", self.scene_number)], out);
        out.push_str(">\n");
        out.push_str(&inner);
        out.push_str("<style");
        attrs(
            &[
                ("basestylename", Some(self.style)),
                ("align", self.align),
                ("dualdialogue", self.dual.then_some("1")),
                ("pagebreakbefore", self.page_break.then_some("1")),
            ],
            out,
        );
        out.push_str("/>\n");
        let runs = text_runs(&self.text);
        if runs.is_empty() {
            out.push_str(&inner);
            out.push_str("<text></text>\n");
        }
        for r in &runs {
            run(r, &inner, out);
        }
        out.push_str(indent);
        out.push_str("</para>\n");
    }
}

/// The lines of a token's text worth keeping (`split(/\r?\n/)`, blank
/// lines dropped).
fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|p| p.strip_suffix('\r').unwrap_or(p))
        .filter(|line| !js::trim(line).is_empty())
        .collect()
}

/// The built-in style each content token becomes; tokens absent here (title
/// page, sections, synopses, notes, boneyard, speech and dual markers) are
/// structure or invisible, never a paragraph of their own. A lyric has no
/// style of its own in OSF: inside a speech it is sung dialogue, outside one
/// it is action.
fn style_for(t: &Token, in_speech: bool) -> Option<&'static str> {
    Some(match t.kind {
        TokenType::SceneHeading if is_mini_slug(t) => "Shot",
        TokenType::SceneHeading => "Scene Heading",
        TokenType::Action | TokenType::Centered => "Action",
        TokenType::Character => "Character",
        TokenType::Parenthetical => "Parenthetical",
        TokenType::Dialogue => "Dialogue",
        TokenType::Lyrics if in_speech => "Dialogue",
        TokenType::Lyrics => "Action",
        TokenType::Transition => "Transition",
        _ => return None,
    })
}

/// `text.replace(/^>\s*/, '')`: fountain-js drops a forced transition's
/// `>` itself; this is the belt every renderer wears in case it does not.
fn drop_forced_transition(text: &str) -> &str {
    match text.strip_prefix('>') {
        Some(rest) => rest.trim_start_matches(js::is_space),
        None => text,
    }
}

fn render_body(tokens: &[Token], out: &mut String) {
    // A Fountain page break has no paragraph of its own in OSF: it is
    // `pagebreakbefore` on whatever paragraph comes next.
    let mut new_page = false;
    // Inside a dialogue block, so a lyric there is sung dialogue.
    let mut in_speech = false;
    // The published examples mark a dual exchange on its FIRST cue only.
    let mut dual_cue_pending = false;

    for t in tokens {
        if t.is_title() {
            continue;
        }
        match t.kind {
            TokenType::PageBreak => {
                new_page = true;
                continue;
            }
            TokenType::DialogueBegin => {
                in_speech = true;
                continue;
            }
            TokenType::DialogueEnd => {
                in_speech = false;
                continue;
            }
            TokenType::DualDialogueBegin => {
                dual_cue_pending = true;
                continue;
            }
            TokenType::DualDialogueEnd => {
                dual_cue_pending = false;
                continue;
            }
            _ => {}
        }
        let Some(style) = style_for(t, in_speech) else {
            continue;
        };
        let stripped = strip_notes(t.text_or_empty());
        let text = if t.kind == TokenType::Transition {
            drop_forced_transition(&stripped)
        } else {
            &stripped
        };
        // One paragraph per token, its lines joined by soft returns: Action
        // has a blank line before it, so one paragraph per line would open
        // a block up into spaced paragraphs.
        let lines = lines_of(text);
        if lines.is_empty() {
            continue;
        }
        let dual = dual_cue_pending && t.kind == TokenType::Character;
        if dual {
            dual_cue_pending = false;
        }
        let scene_number = match (style, &t.scene_number) {
            ("Scene Heading", Some(n)) if !n.is_empty() => Some(n.as_str()),
            _ => None,
        };
        Para {
            style,
            scene_number,
            align: (t.kind == TokenType::Centered).then_some("center"),
            page_break: std::mem::take(&mut new_page),
            dual,
            text: lines.join("\n"),
        }
        .write("    ", out);
    }
}

/// About a page of title-page lines, the .fdx's own layout: the centered
/// block starts a third of the way down and the contact block finishes
/// near the bottom.
const TITLE_PAGE_LINES: usize = 54;
const TITLE_TOP_BLANKS: usize = 18;

fn render_title_page(tokens: &[Token]) -> Vec<Para<'static>> {
    let title_tokens: Vec<&Token> = tokens.iter().filter(|t| t.is_title()).collect();
    if title_tokens.is_empty() {
        return Vec::new();
    }
    let lines_for = |types: &[&str]| -> Vec<String> {
        title_tokens
            .iter()
            .filter(|t| types.contains(&t.kind.as_str()))
            .flat_map(|t| {
                lines_of(&strip_notes(t.text_or_empty()))
                    .into_iter()
                    .map(|l| js::trim(l).to_owned())
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let groups = |sets: &[&[&str]]| -> Vec<Vec<String>> {
        sets.iter()
            .map(|types| lines_for(types))
            .filter(|g| !g.is_empty())
            .collect()
    };
    let centered = groups(&[&["title"], &["credit"], &["author", "authors"], &["source"]]);
    let bottom = groups(&[
        &["contact"],
        &["draft_date", "date"],
        &["revision", "revisions"],
        &["copyright"],
        &["notes"],
    ]);
    if centered.is_empty() && bottom.is_empty() {
        return Vec::new();
    }

    const NORMAL: &str = "Normal Text";
    let blank = |align| Para::plain(NORMAL, align, String::new());
    let mut out: Vec<Para> = Vec::new();
    for _ in 0..TITLE_TOP_BLANKS {
        out.push(blank(None));
    }
    for (i, group) in centered.iter().enumerate() {
        if i > 0 {
            out.push(blank(Some("center")));
        }
        for line in group {
            out.push(Para::plain(NORMAL, Some("center"), line.clone()));
        }
    }
    let bottom_lines = bottom.iter().map(Vec::len).sum::<usize>() + bottom.len().saturating_sub(1);
    let room = TITLE_PAGE_LINES as isize - out.len() as isize - bottom_lines as isize;
    if !bottom.is_empty() {
        for _ in 0..room.max(2) {
            out.push(blank(None));
        }
    }
    for (i, group) in bottom.iter().enumerate() {
        if i > 0 {
            out.push(blank(None));
        }
        for line in group {
            out.push(Para::plain(NORMAL, None, line.clone()));
        }
    }
    out
}

/// The whole OSF document for a script's fountain-js tokens.
pub fn tokens_to_osf(tokens: &[Token]) -> String {
    let mut out = String::new();
    out.push_str(XML_DECLARATION);
    out.push('\n');
    out.push_str(&format!(
        "<document type=\"{OSF_DOCUMENT_TYPE}\" version=\"{OSF_VERSION}\">\n"
    ));
    out.push_str("  <paragraphs>\n");
    render_body(tokens, &mut out);
    out.push_str("  </paragraphs>\n");
    let title = render_title_page(tokens);
    if !title.is_empty() {
        out.push_str("  <titlepage>\n");
        for p in &title {
            p.write("    ", &mut out);
        }
        out.push_str("  </titlepage>\n");
    }
    out.push_str("</document>\n");
    out
}

//! fountain-js tokens to Final Draft (.fdx): the editable screenplay, beside
//! the EPUB and MOBI renderers and fed the same tokens from the same
//! .fountain. Where those two make a book to READ, this makes a script to
//! keep WRITING in Final Draft, Fade In, WriterDuet, Highland or Celtx.
//! Ported from src/fdx/fdx.ts, byte for byte (rust/oracle/*/book.fdx).
//!
//! Deliberately minimal: a FinalDraft root, the script's paragraphs and its
//! title page, and nothing else. No ElementSettings, PageLayout or SmartType
//! blocks: Final Draft fills its own defaults for every one of them, and the
//! other importers ignore them anyway. Paragraphs carry only their element
//! Type (plus Number, Alignment and StartsNewPage where the Fountain says
//! so), never fonts or indents, so the receiving app's own screenplay
//! template decides how the page looks.
//!
//! Format-option-free on purpose: this is a reconstruction of the script,
//! not a rendering of the book, so no reading knob (scene numbers shown,
//! title page included) changes it. Whatever the .fountain carries, the FDX
//! carries.
//!
//! The TypeScript's `fountainToFdx(text)` is `tokens_to_fdx` of the
//! tokenizer's answer; that tokenizer is sp-fountain's, so the text-in entry
//! point is TODO(sp-fountain) and the CLI composes the two meanwhile.

mod js;

use sp_model::{Token, TokenType};

use sp_fountain::{is_mini_slug, strip_notes};

/// The FinalDraft root's Version attribute. ONE constant, because the
/// owner's Final Draft is an older licence and this may have to come down:
/// version 2 is what Final Draft 8 to 10 write, and newer releases (version
/// 5 and up) still open it. Nothing written here needs a newer schema.
pub const FDX_VERSION: u32 = 2;

const XML_DECLARATION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>"#;

/// One styled stretch of a paragraph: a `<Text>` element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    pub text: String,
    /// "Bold", "Italic", "Underline" joined with "+" in that order; `None`
    /// for plain text.
    pub style: Option<String>,
}

// Sentinels for the emphasis pass: Unicode noncharacters, which no real text
// carries (any that arrive are removed first), so they can mark where a
// style opens and closes without colliding with the script's own words.
const B_OPEN: char = '\u{FDD0}';
const B_CLOSE: char = '\u{FDD1}';
const I_OPEN: char = '\u{FDD2}';
const I_CLOSE: char = '\u{FDD3}';
const U_OPEN: char = '\u{FDD4}';
const U_CLOSE: char = '\u{FDD5}';
const LITERAL_STAR: char = '\u{FDD6}';
const LITERAL_UNDERSCORE: char = '\u{FDD7}';

fn is_sentinel(c: char) -> bool {
    ('\u{FDD0}'..='\u{FDEF}').contains(&c)
}

/// XML 1.0's Char production, inverted: what may never appear in a
/// document, escaped or not. (The TypeScript also drops lone surrogates; a
/// Rust string cannot hold one.)
fn is_forbidden_xml(c: char) -> bool {
    matches!(
        c,
        '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}'
    )
}

/// The four emphasis pairings, in the order they run: the marker, how many
/// of it, and what an opening and closing become.
const EMPHASIS: [(char, usize, &[char], &[char]); 4] = [
    ('*', 3, &[B_OPEN, I_OPEN], &[I_CLOSE, B_CLOSE]),
    ('*', 2, &[B_OPEN], &[B_CLOSE]),
    ('*', 1, &[I_OPEN], &[I_CLOSE]),
    ('_', 1, &[U_OPEN], &[U_CLOSE]),
];

/// Where a match of `M{n}(?!\s)([^M]+?)(?<!\s)M{n}` starting at `i` ends
/// its content, if one starts there. The content cannot hold the marker, so
/// the first marker after it is the only place the closing can be.
fn emphasis_at(s: &[char], i: usize, marker: char, n: usize) -> Option<usize> {
    let all_marker = |from: usize| {
        s.get(from..from + n)
            .is_some_and(|w| w.iter().all(|&c| c == marker))
    };
    if !all_marker(i) {
        return None;
    }
    let start = i + n;
    let first = *s.get(start)?;
    if first == marker || js::is_space(first) {
        return None;
    }
    let k = start + s[start..].iter().position(|&c| c == marker)?;
    (all_marker(k) && !js::is_space(s[k - 1])).then_some(k)
}

/// One global `String.prototype.replace` of one pairing.
fn emphasis_pass(s: &[char], marker: char, n: usize, open: &[char], close: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(s.len() + 4);
    let mut i = 0;
    while i < s.len() {
        if let Some(k) = emphasis_at(s, i, marker, n) {
            out.extend_from_slice(open);
            out.extend_from_slice(&s[i + n..k]);
            out.extend_from_slice(close);
            i = k + n;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// `.replace(/\\<marker>/g, literal)`.
fn escape_pass(s: Vec<char>, marker: char, literal: char) -> Vec<char> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == '\\' && s.get(i + 1) == Some(&marker) {
            out.push(literal);
            i += 2;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// Fountain inline emphasis as styled runs. `*italic*`, `**bold**`,
/// `***bold italic***` and `_underline_` become styles; `\*` and `\_` are a
/// literal asterisk and underscore. Unpaired markers stay as written. A
/// marker must hug its text (`2 * 3` is not emphasis), the same pairing the
/// EPUB and MOBI renderers use, but run to a fixed point so a style can nest
/// inside another. Text XML cannot carry is removed here, so every run is
/// safe to escape and write.
pub fn text_runs(source: &str) -> Vec<TextRun> {
    let s: Vec<char> = source
        .chars()
        .filter(|&c| !is_forbidden_xml(c) && !is_sentinel(c))
        .collect();
    let s = escape_pass(s, '*', LITERAL_STAR);
    let mut s = escape_pass(s, '_', LITERAL_UNDERSCORE);
    loop {
        let before = s.clone();
        for (marker, n, open, close) in EMPHASIS {
            s = emphasis_pass(&s, marker, n, open, close);
        }
        if s == before {
            break;
        }
    }

    let mut runs: Vec<TextRun> = Vec::new();
    let (mut bold, mut italic, mut underline) = (0i32, 0i32, 0i32);
    let mut current = String::new();
    let flush = |current: &mut String, runs: &mut Vec<TextRun>, b: i32, i: i32, u: i32| {
        if current.is_empty() {
            return;
        }
        let mut parts = Vec::new();
        if b > 0 {
            parts.push("Bold");
        }
        if i > 0 {
            parts.push("Italic");
        }
        if u > 0 {
            parts.push("Underline");
        }
        let style = parts.join("+");
        // Two runs in the same style (an empty style pair between them) are one.
        if let Some(last) = runs.last_mut()
            && last.style.as_deref().unwrap_or("") == style
        {
            last.text.push_str(current);
        } else {
            runs.push(TextRun {
                text: current.clone(),
                style: (!style.is_empty()).then_some(style),
            });
        }
        current.clear();
    };
    for ch in s {
        match ch {
            B_OPEN | B_CLOSE | I_OPEN | I_CLOSE | U_OPEN | U_CLOSE => {
                flush(&mut current, &mut runs, bold, italic, underline);
                match ch {
                    B_OPEN => bold += 1,
                    B_CLOSE => bold -= 1,
                    I_OPEN => italic += 1,
                    I_CLOSE => italic -= 1,
                    U_OPEN => underline += 1,
                    _ => underline -= 1,
                }
            }
            LITERAL_STAR => current.push('*'),
            LITERAL_UNDERSCORE => current.push('_'),
            _ => current.push(ch),
        }
    }
    flush(&mut current, &mut runs, bold, italic, underline);
    runs
}

fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
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
        .filter(|&c| !is_forbidden_xml(c))
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

fn open_tag(attrs: &Attrs<'_>) -> String {
    let mut out = String::from("<Paragraph");
    for (name, value) in attrs {
        if let Some(value) = value {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            escape_attr(value, &mut out);
            out.push('"');
        }
    }
    out.push('>');
    out
}

/// A whole paragraph on one line. Whitespace inside a Paragraph is text to
/// Final Draft, so nothing is pretty-printed between its runs.
fn paragraph(attrs: &Attrs<'_>, text: &str) -> String {
    let mut out = open_tag(attrs);
    let runs = text_runs(text);
    if runs.is_empty() {
        out.push_str("<Text></Text>");
    }
    for r in runs {
        match &r.style {
            Some(style) => {
                out.push_str("<Text Style=\"");
                out.push_str(style);
                out.push_str("\">");
            }
            None => out.push_str("<Text>"),
        }
        escape_text(&r.text, &mut out);
        out.push_str("</Text>");
    }
    out.push_str("</Paragraph>");
    out
}

/// The lines of a token's text worth a paragraph each (`split(/\r?\n/)`,
/// blank lines dropped). A lyrics block or a multi-line speech arrives as
/// ONE token with newlines inside, and Final Draft keeps one line per
/// paragraph (a hard return IS a new paragraph there), so each line becomes
/// its own, in the token's own type.
fn lines_of(text: &str) -> Vec<&str> {
    let pieces: Vec<&str> = text.split('\n').collect();
    let last = pieces.len() - 1;
    pieces
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            if i < last {
                p.strip_suffix('\r').unwrap_or(p)
            } else {
                p
            }
        })
        .filter(|line| !js::trim(line).is_empty())
        .collect()
}

/// The element Type each content token becomes; tokens absent here (title
/// page, sections, synopses, notes, boneyard, speech and dual markers) are
/// structure or invisible, never a paragraph of their own.
fn paragraph_type(t: &Token) -> Option<&'static str> {
    Some(match t.kind {
        TokenType::SceneHeading if is_mini_slug(t) => "Shot",
        TokenType::SceneHeading => "Scene Heading",
        TokenType::Action | TokenType::Centered => "Action",
        TokenType::Character => "Character",
        TokenType::Parenthetical => "Parenthetical",
        TokenType::Dialogue => "Dialogue",
        TokenType::Lyrics => "Lyrics",
        TokenType::Transition => "Transition",
        _ => return None,
    })
}

/// `text.replace(/^>\s*/, '')`.
fn drop_forced_transition(text: &str) -> &str {
    match text.strip_prefix('>') {
        Some(rest) => rest.trim_start_matches(js::is_space),
        None => text,
    }
}

fn render_body(tokens: &[Token]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // A Fountain page break has no paragraph of its own in Final Draft: it
    // is a flag on whatever paragraph comes next.
    let mut new_page = false;
    // Dual dialogue gathers both speeches, then lands as one wrapper.
    let mut dual: Option<Vec<String>> = None;

    for t in tokens {
        if t.is_title() {
            continue;
        }
        match t.kind {
            TokenType::PageBreak => {
                new_page = true;
                continue;
            }
            TokenType::DualDialogueBegin => {
                dual = Some(Vec::new());
                continue;
            }
            TokenType::DualDialogueEnd => {
                if let Some(inner) = dual.take().filter(|d| !d.is_empty()) {
                    // Final Draft nests a dual exchange this way: one outer
                    // paragraph holding a DualDialogue, holding both
                    // speakers' paragraphs in order.
                    let starts = std::mem::take(&mut new_page).then_some("Yes");
                    out.push(format!(
                        "{}<DualDialogue>{}</DualDialogue></Paragraph>",
                        open_tag(&[("Type", Some("General")), ("StartsNewPage", starts)]),
                        inner.concat()
                    ));
                }
                dual = None;
                continue;
            }
            _ => {}
        }

        let Some(kind) = paragraph_type(t) else {
            continue;
        };
        let stripped = strip_notes(t.text_or_empty());
        // fountain-js drops a forced transition's `>` itself; this is the
        // same belt the EPUB renderer wears in case it ever does not.
        let text = if t.kind == TokenType::Transition {
            drop_forced_transition(&stripped)
        } else {
            &stripped
        };
        let number = match (&t.kind, &t.scene_number) {
            (TokenType::SceneHeading, Some(n)) if !n.is_empty() => Some(n.as_str()),
            _ => None,
        };
        let alignment = (t.kind == TokenType::Centered).then_some("Center");

        for line in lines_of(text) {
            // Inside a dual exchange the page flag belongs to the wrapper.
            let starts = if dual.is_some() {
                None
            } else {
                std::mem::take(&mut new_page).then_some("Yes")
            };
            let p = paragraph(
                &[
                    ("Type", Some(kind)),
                    ("Number", number),
                    ("Alignment", alignment),
                    ("StartsNewPage", starts),
                ],
                line,
            );
            match dual.as_mut() {
                Some(d) => d.push(p),
                None => out.push(p),
            }
        }
    }
    out
}

/// About a page of title-page lines in Final Draft's default layout. The
/// centered block starts a third of the way down and the contact block
/// finishes near the bottom, the way Final Draft's own title page sits.
const TITLE_PAGE_LINES: usize = 54;
const TITLE_TOP_BLANKS: usize = 18;

fn render_title_page(tokens: &[Token]) -> Vec<String> {
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
                    .map(str::to_owned)
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

    let blank = |alignment: &str| paragraph(&[("Alignment", Some(alignment))], "");
    let mut out: Vec<String> = Vec::new();
    for _ in 0..TITLE_TOP_BLANKS {
        out.push(blank("Left"));
    }
    for (i, group) in centered.iter().enumerate() {
        if i > 0 {
            out.push(blank("Center"));
        }
        for line in group {
            out.push(paragraph(&[("Alignment", Some("Center"))], line));
        }
    }
    // A blank line between bottom groups, counted before the filler so the
    // last line lands near the foot of the page.
    let bottom_lines = bottom.iter().map(Vec::len).sum::<usize>() + bottom.len().saturating_sub(1);
    let room = TITLE_PAGE_LINES as isize - out.len() as isize - bottom_lines as isize;
    let filler = room.max(2) as usize;
    if !bottom.is_empty() {
        for _ in 0..filler {
            out.push(blank("Left"));
        }
    }
    for (i, group) in bottom.iter().enumerate() {
        if i > 0 {
            out.push(blank("Left"));
        }
        for line in group {
            out.push(paragraph(&[("Alignment", Some("Left"))], line));
        }
    }
    out
}

/// The whole FDX document for a script's fountain-js tokens.
pub fn tokens_to_fdx(tokens: &[Token]) -> String {
    let body = render_body(tokens);
    let title = render_title_page(tokens);
    let mut lines: Vec<String> = vec![
        XML_DECLARATION.to_owned(),
        format!(r#"<FinalDraft DocumentType="Script" Template="No" Version="{FDX_VERSION}">"#),
        "  <Content>".to_owned(),
    ];
    lines.extend(body.iter().map(|p| format!("    {p}")));
    lines.push("  </Content>".to_owned());
    if !title.is_empty() {
        lines.push("  <TitlePage>".to_owned());
        lines.push("    <Content>".to_owned());
        lines.extend(title.iter().map(|p| format!("      {p}")));
        lines.push("    </Content>".to_owned());
        lines.push("  </TitlePage>".to_owned());
    }
    lines.push("</FinalDraft>".to_owned());
    lines.push(String::new());
    lines.join("\n")
}

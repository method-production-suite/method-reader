//! fountain-js 1.2.4's tokenizer: `.fountain` text to the tokens every
//! renderer receives, `new Fountain().parse(text, true).tokens`.
//!
//! A line-for-line port of `node_modules/fountain-js/dist/{lexer,token,
//! rules}.js`, with the rules copied verbatim and run on [`crate::jsre`],
//! which matches the way JavaScript does. Token order is part of the
//! contract: the script is split on blank lines and lexed BOTTOM-UP, each
//! dialogue block pushing its own tokens in reverse, and the whole list is
//! reversed at the end. That is how a `^` on a cue reaches back to make the
//! block above it the left column.
//!
//! **fountain-js keeps state between parses.** Two things live on module
//! statics and are never reset: `Lexer.lastLineWasDualDialogue`, and the
//! `lastIndex` of the global `blank_lines` rule, which `SpacesToken.
//! matchedBy` calls `.test()` on. So the same text can tokenize differently
//! the second time a process parses it (a script whose FIRST dialogue block
//! is a `^` right column makes the LAST block of the next parse a left
//! column). That is a fountain-js bug, and the port does NOT reproduce it
//! (decided 2026-10-11, rust/PARITY.md Decisions): every [`tokenize`] starts
//! fresh, exactly as a clean process's FIRST parse does. Within one parse the
//! state still moves as fountain-js moves it, which is part of what one parse
//! answers.
//!
//! The renderers' inline markup (emphasis to HTML) is not here: fountain-js
//! leaves it in the token text, and each TypeScript renderer converts it
//! itself.

use std::fmt;
use std::sync::LazyLock;

use sp_model::{Dual, Token, TokenType};

use crate::js;
use crate::jsre::{Caps, Regex, group};

struct Rules {
    title_page: Regex,
    scene_heading: Regex,
    scene_number: Regex,
    transition: Regex,
    dialogue: Regex,
    parenthetical: Regex,
    centered: Regex,
    lyrics: Regex,
    section: Regex,
    synopsis: Regex,
    note: Regex,
    boneyard: Regex,
    page_break: Regex,
    line_break: Regex,
    blank_lines: Regex,
    // Patterns written inline in lexer.js and token.js.
    boneyard_end_at_line_start: Regex,
    newlines: Regex,
    title_page_block: Regex,
    key_value_pair: Regex,
    key_value_split: Regex,
    trim_lines: Regex,
    centered_marks: Regex,
    lyric_marks: Regex,
    lyric_mark: Regex,
    action_bang: Regex,
    action_tabs: Regex,
    leading_at: Regex,
}

static RULES: LazyLock<Rules> = LazyLock::new(|| Rules {
    title_page: Regex::new(
        r"^\s*((?:title|credit|authors?|source|notes|draft ?date|date|contact|copyright|revisions?)\:)(?=[^\S\n]*(?:\n(?: {3,}|\t))?\S.*)",
        "i",
    ),
    scene_heading: Regex::new(
        r"^\s*((?:\*{0,3}_?)?(?:(?:int|i)\.?\/(?:ext|e)|int|ext|est)[. ].+$)|^\s*\.(?!\.+)(\S.*)$",
        "i",
    ),
    scene_number: Regex::new(r"\s*#([\w.-]+?)#\s*$", ""),
    transition: Regex::new(
        r"^\s*((?:FADE (?:TO BLACK|OUT)|CUT TO BLACK)\.|.+ TO\:)[^\S\n]*$|^\s*> *(.+)$",
        "",
    ),
    dialogue: Regex::new(
        r"(?!^\s*\\@|^\s*[!.>~=#])(?!^\s*[0-9 _*]+(?:\(.*\))?[*_]*(?:\^?)?\s*\n)(^\s*(?:@[^^()\n]+|[^^()\na-z]+(?<!(?:BLACK|OUT)\.|TO:\s*))(?:\(.*\))?[ *_]*)(\^?)?\s*\n(?!\n+)([\s\S]+)",
        "",
    ),
    parenthetical: Regex::new(
        r"^ *(?:(?<u1>_{0,1})(?<s1>\*{0,3})(?=.+\k<s1>\k<u1>)|(?<s2>\*{0,3})(?<u2>_{0,1})(?=.+\k<u2>\k<s2>))(\(.+?\))(\k<s1>\k<u1>|\k<u2>\k<s2>) *$",
        "",
    ),
    centered: Regex::new(r"^\s*>.+<[^\S\r\n]*(?:\s*>.+<[^\S\r\n]*)*$", ""),
    lyrics: Regex::new(r"^\s*~(?! ).+(?:\n\s*~(?! ).+)*$", ""),
    section: Regex::new(r"^\s*(#+) *(.*)$", ""),
    synopsis: Regex::new(r"^\s*=(?!=+) *(.*)$", ""),
    note: Regex::new(r"^\[{2}(?!\[+)(.+)]{2}(?!\[+)$", ""),
    boneyard: Regex::new(r"\/\*[\S\s]*?\*\/", "g"),
    page_break: Regex::new(r"^={3,}$", ""),
    line_break: Regex::new(r"^ {2}$", ""),
    blank_lines: Regex::new(
        r"\n(?:(?! {2}\n)(?:[^\S\n]*| {3,}[^\S\n]*)(?:\n|$))+|^[^\S\n]*(?:\n|$)",
        "g",
    ),
    boneyard_end_at_line_start: Regex::new(r"^[^\S\n]*\*\/", "m"),
    newlines: Regex::new(r"\r\n|\r", "g"),
    title_page_block: Regex::new(
        r"^\s*(?:[\w ]+(?<!\\)\:[^\S\n]*(?:(?:\n(?: {3}|\t))?[^\S\n]*\S.*)+(?:\n|$))+",
        "",
    ),
    key_value_pair: Regex::new(
        r"^\s*[\w ]+(?<!\\)\:[^\S\n]*(?:(?:\n(?![\w ]+\:)(?: {3}|\t))?[^\S\n]*\S.*)+(?:\n|$)",
        "",
    ),
    key_value_split: Regex::new(r"(\:[^\S\n]*\n?)([\S\s]*)", ""),
    trim_lines: Regex::new(r"^\s*|\s*$", "gm"),
    centered_marks: Regex::new(r"[^\S\n]*[><][^\S\n]*", "g"),
    lyric_marks: Regex::new(r"^\s*~(?! )", "gm"),
    lyric_mark: Regex::new(r"^~", ""),
    action_bang: Regex::new(r"^(\s*)!(?! )", "gm"),
    action_tabs: Regex::new(r"^( *)(\t+)", "gm"),
    leading_at: Regex::new(r"^@", ""),
});

/// What fountain-js throws. One input shape does it: a dialogue block whose
/// first line under the cue is a lyric (`@BOB\n~la la`), where its reducer
/// reads `previousToken.type` with no previous token. The message is the one
/// the TypeScript engine reports under Bun, issue link included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FountainError {
    pub message: String,
}

impl fmt::Display for FountainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FountainError {}

fn lyric_first_error() -> FountainError {
    FountainError {
        message: "undefined is not an object (evaluating 'previousToken.type')\n\
                  Please submit an issue to https://github.com/jonnygreenwald/fountain-js/issues"
            .to_owned(),
    }
}

/// fountain-js's module state, for the length of ONE parse: see the module
/// documentation for why it never outlives it.
#[derive(Debug, Default)]
struct Lexer {
    /// `rules.blank_lines.lastIndex`.
    blank_lines_last_index: usize,
    /// `Lexer.lastLineWasDualDialogue`.
    last_line_was_dual: bool,
}

/// `new Fountain().parse(script, true).tokens`, as a clean process's first
/// parse answers it. Errs where fountain-js throws.
///
/// The patterns run on jsre, which recurses once per repetition of a group
/// (one blank line, one lyric line), where V8 keeps its backtrack stack on
/// the heap. So the parse runs on a thread of its own with a large stack
/// (reserved address space; only what is used is ever committed), and a
/// search that would outgrow even that, or that backtracks without end on a
/// crafted input, is refused with an `Err` rather than aborting the process
/// or hanging it.
pub fn tokenize(script: &str) -> Result<Vec<Token>, FountainError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("method-reader-fountain".into())
            .stack_size(PARSE_STACK)
            .spawn_scoped(scope, || {
                crate::jsre::with_stack_budget(PARSE_STACK_BUDGET, || {
                    let before = crate::jsre::trips();
                    let tokens = Lexer::default().parse(script)?;
                    if crate::jsre::trips() != before {
                        return Err(too_complex_error());
                    }
                    Ok(tokens)
                })
            })
            .map_err(|e| FountainError {
                message: format!("The script could not be read: {e}"),
            })?
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// The parse thread's stack, and how much of it one search may use (the
/// rest is the Lexer's own frames and headroom).
const PARSE_STACK: usize = 256 * 1024 * 1024;
const PARSE_STACK_BUDGET: usize = 192 * 1024 * 1024;

fn too_complex_error() -> FountainError {
    FountainError {
        message: "This script is too complex to read: a passage in it would take the \
                  Fountain reader too long. Check it for a very long run of blank lines \
                  or repeated marks."
            .to_owned(),
    }
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn text_token(kind: TokenType, text: String) -> Token {
    Token::with_text(kind, text)
}

impl Lexer {
    fn parse(&mut self, script: &str) -> Result<Vec<Token>, FountainError> {
        let r = &*RULES;
        // Boneyard out first; a boneyard whose `*/` opens a line leaves a
        // blank line behind so the paragraphs around it stay apart.
        let source = r.boneyard.replace_with(script, true, |s, caps: &Caps| {
            let (a, b) = caps[0].unwrap();
            let m: String = s[a..b].iter().collect();
            if r.boneyard_end_at_line_start.test(&m) {
                "\n\n".to_owned()
            } else {
                String::new()
            }
        });
        let source = r.newlines.replace_all(&source, "\n");
        let source = chars(&source);

        let (title_tokens, scan_index) = title_page(&source);
        let rest = &source[scan_index.min(source.len())..];
        let mut tokens = title_tokens;
        tokens.extend(self.tokenize_script(rest)?);
        Ok(tokens)
    }

    /// `SpacesToken.matchedBy`: `.test()` on a GLOBAL regex, so it searches
    /// from, and moves, the shared `lastIndex`.
    fn spaces_matched_by(&mut self, line: &[char]) -> bool {
        let r = &*RULES;
        if self.blank_lines_last_index > line.len() {
            self.blank_lines_last_index = 0;
            return false;
        }
        match r.blank_lines.find_from(line, self.blank_lines_last_index) {
            Some(caps) => {
                self.blank_lines_last_index = caps[0].unwrap().1;
                true
            }
            None => {
                self.blank_lines_last_index = 0;
                false
            }
        }
    }

    fn tokenize_script(&mut self, source: &[char]) -> Result<Vec<Token>, FountainError> {
        let r = &*RULES;
        let mut lines = r.blank_lines.split_chars(source, None);
        lines.reverse();
        let mut out: Vec<Token> = Vec::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            let lc = chars(&line);
            if self.spaces_matched_by(&lc) {
                out.push(Token::new(TokenType::Spaces));
            } else if r.scene_heading.test(&line) {
                out.push(scene_heading(&line));
            } else if r.centered.test(&line) {
                let m = r.centered.captures(&line).unwrap();
                let text = r.centered_marks.replace_all(m[0].as_deref().unwrap(), "");
                out.push(text_token(TokenType::Centered, text));
            } else if r.transition.test(&line) {
                let m = r.transition.captures(&line).unwrap();
                let text = first_truthy(&m[1], &m[2]);
                out.push(text_token(
                    TokenType::Transition,
                    js::trim(&text).to_owned(),
                ));
            } else if r.dialogue.test(&line) {
                let (block, dual) = dialogue_block(&line, self.last_line_was_dual)?;
                self.last_line_was_dual = dual;
                out.extend(block);
            } else if r.section.test(&line) {
                let m = r.section.captures(&line).unwrap();
                let mut t = text_token(TokenType::Section, m[2].clone().unwrap_or_default());
                t.depth = Some(m[1].as_deref().unwrap().chars().count() as u32);
                out.push(t);
            } else if r.synopsis.test(&line) {
                let m = r.synopsis.captures(&line).unwrap();
                out.push(text_token(
                    TokenType::Synopsis,
                    m[1].clone().unwrap_or_default(),
                ));
            } else if r.note.test(&line) {
                let m = r.note.captures(&line).unwrap();
                out.push(text_token(
                    TokenType::Note,
                    m[1].clone().unwrap_or_default(),
                ));
            } else if r.lyrics.test(&line) {
                out.push(lyrics_token(&line));
            } else if r.page_break.test(&line) {
                out.push(Token::new(TokenType::PageBreak));
            } else {
                out.push(action_token(&line));
            }
        }
        out.reverse();
        Ok(out)
    }
}

/// `match[1] || match[2]`: the first group that took part with any text.
fn first_truthy(a: &Option<String>, b: &Option<String>) -> String {
    match a {
        Some(s) if !s.is_empty() => s.clone(),
        _ => b.clone().unwrap_or_default(),
    }
}

/// `Lexer.tokenizeTitlePage`: the title page tokens and how much of the
/// source they used (`Lexer.scanIndex`).
fn title_page(source: &[char]) -> (Vec<Token>, usize) {
    let r = &*RULES;
    let text: String = source.iter().collect();
    if !r.title_page.test(&text) {
        return (Vec::new(), 0);
    }
    // `new TitlePageBlock(source)`. With no block match its scanIndex stays
    // undefined, and `source.substring(undefined)` is the whole source.
    let Some(block) = r.title_page_block.find_from(source, 0) else {
        return (Vec::new(), 0);
    };
    let (a, b) = block[0].unwrap();
    let mut data: Vec<char> = source[a..b].to_vec();
    let mut scan_index = data.len();
    let mut scan_position = 0;
    let mut tokens = Vec::new();
    while scan_position < scan_index {
        match r.key_value_pair.find_from(&data, 0) {
            Some(pair) => {
                let (pa, pb) = pair[0].unwrap();
                let pair_text: String = data[pa..pb].iter().collect();
                tokens.push(title_page_token(&pair_text));
                // `titlePageData.substring(pair[0].length)`: by LENGTH, from
                // the front, whatever index the match started at.
                let len = pb - pa;
                data = data[len.min(data.len())..].to_vec();
                scan_position += len;
            }
            None => {
                scan_index = scan_position;
                break;
            }
        }
    }
    (tokens, scan_index)
}

/// `new TitlePageToken(pair)`.
fn title_page_token(pair: &str) -> Token {
    let r = &*RULES;
    let parts = r.key_value_split.split(pair, Some(3));
    let key = parts.first().cloned().unwrap_or_default();
    let value = parts.get(2).cloned().unwrap_or_default();
    let kind = js::trim(&key).to_lowercase().replace(' ', "_");
    let text = r.trim_lines.replace_all(&value, "");
    Token {
        is_title: Some(true),
        ..text_token(TokenType::from(kind), text)
    }
}

/// `new SceneHeadingToken(line)`.
fn scene_heading(line: &str) -> Token {
    let r = &*RULES;
    let m = r.scene_heading.captures(line).unwrap();
    let mut text = js::trim(&first_truthy(&m[1], &m[2])).to_owned();
    let mut scene_number = None;
    if let Some(meta) = r.scene_number.captures(&text) {
        scene_number = meta[1].clone();
        text = r
            .scene_number
            .replace_with(&text, false, |_, _| String::new());
    }
    Token {
        scene_number,
        ..text_token(TokenType::SceneHeading, text)
    }
}

/// `new LyricsToken(line)`.
fn lyrics_token(line: &str) -> Token {
    text_token(TokenType::Lyrics, RULES.lyric_marks.replace_all(line, ""))
}

/// `new ActionToken(line)`: forced `!` off, leading tabs as four spaces each.
fn action_token(line: &str) -> Token {
    let r = &*RULES;
    let text = r.action_bang.replace_with(line, true, |s, caps| {
        let (a, b) = caps[1].unwrap();
        s[a..b].iter().collect()
    });
    let text = r.action_tabs.replace_with(&text, true, |s, caps| {
        let leading = group(s, caps, 1).unwrap_or_default();
        let tabs = group(s, caps, 2).unwrap_or_default();
        leading + &"    ".repeat(tabs.chars().count())
    });
    text_token(TokenType::Action, text)
}

/// `new DialogueBlock(line, dual)`: the block's tokens in the bottom-up
/// order the lexer pushes them, and whether this block was a `^` right
/// column (the next block up becomes its left).
fn dialogue_block(line: &str, dual: bool) -> Result<(Vec<Token>, bool), FountainError> {
    let r = &*RULES;
    let m = r.dialogue.captures(line).unwrap();
    let name = js::trim(m[1].as_deref().unwrap()).to_owned();
    let is_dual = m[2].as_deref().is_some_and(|s| !s.is_empty());
    let mut tokens = Vec::new();
    if is_dual {
        tokens.push(Token::new(TokenType::DualDialogueEnd));
    }
    tokens.push(Token::new(TokenType::DialogueEnd));

    let mut speech: Vec<Token> = Vec::new();
    for part in m[3].as_deref().unwrap().split('\n') {
        if part.is_empty() {
            continue;
        }
        let part = if r.line_break.test(part) { "" } else { part };
        let text = js::trim(part).to_owned();
        if r.parenthetical.test(&text) {
            speech.push(text_token(TokenType::Parenthetical, text));
            continue;
        }
        if r.lyrics.test(&text) {
            let Some(prev) = speech.last_mut() else {
                return Err(lyric_first_error());
            };
            if prev.kind == TokenType::Lyrics {
                let joined = format!(
                    "{}\n{}",
                    prev.text_or_empty(),
                    r.lyric_mark
                        .replace_with(&text, false, |_, _| String::new())
                );
                prev.text = Some(joined);
            } else {
                speech.push(lyrics_token(&text));
            }
            continue;
        }
        if let Some(prev) = speech.last_mut()
            && prev.kind == TokenType::Dialogue
        {
            prev.text = Some(format!("{}\n{}", prev.text_or_empty(), text));
            continue;
        }
        speech.push(text_token(TokenType::Dialogue, text));
    }
    speech.reverse();
    tokens.extend(speech);

    let cue = if name.starts_with('@') {
        js::trim(
            &r.leading_at
                .replace_with(&name, false, |_, _| String::new()),
        )
        .to_owned()
    } else {
        js::trim(&name).to_owned()
    };
    tokens.push(text_token(TokenType::Character, cue));
    let mut begin = Token::new(TokenType::DialogueBegin);
    begin.dual = if is_dual {
        Some(Dual::Right)
    } else if dual {
        Some(Dual::Left)
    } else {
        None
    };
    tokens.push(begin);
    if dual {
        tokens.push(Token::new(TokenType::DualDialogueBegin));
    }
    Ok((tokens, is_dual))
}

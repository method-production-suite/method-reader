//! Python 3's `textwrap.wrap(text, width)`, default options, ported.
//!
//! The committed fixtures were wrapped by Python's textwrap, and a fixture
//! is pinned byte for byte, so this is its algorithm rather than a greedy
//! wrap of our own: the same chunking regex (hyphenated words break after
//! the hyphen, an em dash of two or more hyphens stands alone), the same
//! whitespace dropping at line edges, and the same long-word breaking.
//! Lengths count characters, as Python's `len` does.

use std::sync::OnceLock;

use fancy_regex::Regex;

/// textwrap's `_whitespace`: what counts as a space between chunks.
const WHITESPACE: &str = "\t\n\x0b\x0c\r ";

/// textwrap's `TextWrapper.wordsep_re`. It needs look-behind, hence
/// fancy-regex rather than the regex crate.
fn wordsep_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let ws = r"[\t\n\x0b\x0c\r ]";
        let nws = r"[^\t\n\x0b\x0c\r ]";
        let wp = r#"[\w!"'&.,?]"#;
        let lt = r"[^\d\W]";
        let pattern = format!(
            "({ws}+\
             |(?<={wp})-{{2,}}(?=\\w)\
             |{nws}+?(?:\
             -(?:(?<={lt}{{2}}-)|(?<={lt}-{lt}-))(?={lt}-?{lt})\
             |(?={ws}|\\z)\
             |(?<={wp})(?=-{{2,}}\\w)\
             ))"
        );
        Regex::new(&pattern).expect("textwrap's word separator compiles")
    })
}

/// `str.expandtabs(8)`.
fn expand_tabs(text: &str) -> String {
    let mut out = String::new();
    let mut column = 0usize;
    for c in text.chars() {
        match c {
            '\t' => {
                let pad = 8 - column % 8;
                out.extend(std::iter::repeat_n(' ', pad));
                column += pad;
            }
            '\n' | '\r' => {
                out.push(c);
                column = 0;
            }
            _ => {
                out.push(c);
                column += 1;
            }
        }
    }
    out
}

fn len(s: &str) -> usize {
    s.chars().count()
}

fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

/// `TextWrapper._split`: the indivisible chunks, whitespace runs included.
fn split(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut pos = 0;
    for m in wordsep_re().find_iter(text) {
        let m = m.expect("textwrap's word separator never fails to run");
        if m.start() > pos {
            chunks.push(text[pos..m.start()].to_string());
        }
        if !m.as_str().is_empty() {
            chunks.push(m.as_str().to_string());
        }
        pos = m.end();
    }
    if pos < text.len() {
        chunks.push(text[pos..].to_string());
    }
    chunks
}

/// `TextWrapper._handle_long_word`, with break_long_words and
/// break_on_hyphens both on.
fn handle_long_word(
    chunks: &mut [String],
    cur_line: &mut Vec<String>,
    cur_len: usize,
    width: usize,
) {
    let space_left = if width < 1 { 1 } else { width - cur_len };
    if space_left == 0 {
        return;
    }
    let last = chunks.len() - 1;
    let chunk: Vec<char> = chunks[last].chars().collect();
    let mut end = space_left;
    if chunk.len() > space_left
        && let Some(hyphen) = chunk[..space_left].iter().rposition(|&c| c == '-')
        && hyphen > 0
        && chunk[..hyphen].iter().any(|&c| c != '-')
    {
        end = hyphen + 1;
    }
    let end = end.min(chunk.len());
    cur_line.push(chunk[..end].iter().collect());
    chunks[last] = chunk[end..].iter().collect();
}

/// `textwrap.wrap(text, width)`: the lines, with no trailing newlines. An
/// empty or all-space text gives no lines at all.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    assert!(width > 0, "invalid width {width} (must be > 0)");
    let munged: String = expand_tabs(text)
        .chars()
        .map(|c| if WHITESPACE.contains(c) { ' ' } else { c })
        .collect();
    let mut chunks = split(&munged);
    chunks.reverse();

    let mut lines: Vec<String> = Vec::new();
    while !chunks.is_empty() {
        let mut cur_line: Vec<String> = Vec::new();
        let mut cur_len = 0usize;
        if !lines.is_empty() && chunks.last().is_some_and(|c| is_blank(c)) {
            chunks.pop();
        }
        while let Some(next) = chunks.last() {
            let l = len(next);
            if cur_len + l <= width {
                cur_line.push(chunks.pop().expect("just looked at it"));
                cur_len += l;
            } else {
                break;
            }
        }
        if chunks.last().is_some_and(|c| len(c) > width) {
            handle_long_word(&mut chunks, &mut cur_line, cur_len, width);
        }
        if cur_line.last().is_some_and(|c| is_blank(c)) {
            cur_line.pop();
        }
        if !cur_line.is_empty() {
            lines.push(cur_line.concat());
        }
    }
    lines
}

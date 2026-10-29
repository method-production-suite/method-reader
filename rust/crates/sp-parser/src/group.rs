//! Port of `src/parser/group.ts`.

use std::sync::LazyLock;

use regex::Regex;
use sp_model::{Fmt, RawLine, TextBlock, indent_ranges as R};

use crate::classify::SCENE_HEADING;
use crate::js;

static TRUNCATED_CUE: LazyLock<Regex> = LazyLock::new(|| js::re(r"^[A-Z\s().']+\.{3}$"));

/// Two fmts are the same shift. Both absent counts as the same.
fn same_fmt(a: Option<&Fmt>, b: Option<&Fmt>) -> bool {
    a.and_then(|f| f.family) == b.and_then(|f| f.family)
        && a.and_then(|f| f.size) == b.and_then(|f| f.size)
}

/// Group raw PDF lines into logical text blocks.
/// Lines that share the same indent type and are contiguous get merged.
/// Scene headings, character names, and type changes always break blocks.
pub fn group_blocks(lines: &[RawLine]) -> Vec<TextBlock> {
    let mut blocks = Vec::new();
    let mut current: Vec<RawLine> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let prev = if i > 0 { Some(&lines[i - 1]) } else { None };

        // Check if this line should start a new block
        if !current.is_empty() && should_break(line, prev, &current) {
            blocks.push(build_block(std::mem::take(&mut current)));
        }

        // Clean up ellipsis artifacts from PDF extraction
        current.push(clean_ellipsis(line));
    }

    if !current.is_empty() {
        blocks.push(build_block(current));
    }

    blocks
}

fn should_break(line: &RawLine, prev: Option<&RawLine>, current: &[RawLine]) -> bool {
    let Some(prev) = prev else {
        return false;
    };

    // Empty line detection: large Y gap between lines on same page
    if line.page_num == prev.page_num && (prev.y - line.y).abs() > 20.0 {
        return true;
    }

    // Page break
    if line.page_num != prev.page_num {
        return true;
    }

    // A font shift is a block boundary (registry #18): a chyron glued to the
    // action above it must isolate, or the block would carry no fmt at all
    // and the shift would vanish.
    if !same_fmt(line.fmt.as_ref(), prev.fmt.as_ref()) {
        return true;
    }

    // Scene headings always start new blocks
    if SCENE_HEADING.is_match(&line.text) {
        return true;
    }

    // A parenthetical still open carries on. Printed parentheticals can sit
    // just inside the cue band (35-50), so without this each wrapped line
    // opened a block of its own, neither half was a whole parenthetical, and
    // both fell to action with the speech beneath them. Only an open "("
    // joins, only at a parenthetical's indent (classify's own 40 limit) and
    // only one character's step from the line above.
    if continues_open_parenthetical(line, current) {
        return false;
    }

    // Classify both lines
    let prev_type = quick_classify(prev.indent);
    let curr_type = quick_classify(line.indent);

    // Character names always start new blocks
    if curr_type == QuickType::Character {
        return true;
    }

    // Transitions always break
    if curr_type == QuickType::Transition || prev_type == QuickType::Transition {
        return true;
    }

    // Type changes break blocks
    if prev_type != curr_type {
        return true;
    }

    // Same type: allow if indent differs by <= 2
    let prev_indent = current[current.len() - 1].indent;
    (line.indent - prev_indent).abs() > 2.0
}

/// The block so far opens a parenthetical that has not closed, and `line`
/// sits where its next line would.
fn continues_open_parenthetical(line: &RawLine, current: &[RawLine]) -> bool {
    let (Some(first), Some(last)) = (current.first(), current.last()) else {
        return false;
    };
    let open =
        js::trim(&first.text).starts_with('(') && !current.iter().any(|l| l.text.contains(')'));
    open && (R::DIALOGUE_MIN..=40.0).contains(&line.indent)
        && (line.indent - last.indent).abs() <= 2.0
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuickType {
    Action,
    Dialogue,
    Character,
    Transition,
}

fn quick_classify(indent: f64) -> QuickType {
    if indent < R::ACTION_MAX {
        return QuickType::Action;
    }
    if (R::DIALOGUE_MIN..=R::DIALOGUE_MAX).contains(&indent) {
        return QuickType::Dialogue;
    }
    if indent > R::CHARACTER_MIN && indent <= R::CHARACTER_MAX {
        return QuickType::Character;
    }
    if indent > R::TRANSITION_MIN {
        return QuickType::Transition;
    }
    QuickType::Action // gap zone defaults to action
}

fn clean_ellipsis(line: &RawLine) -> RawLine {
    // PDF artifact: character names sometimes get truncated with "..."
    if TRUNCATED_CUE.is_match(&line.text) {
        let text = line
            .text
            .strip_suffix("...")
            .unwrap_or(&line.text)
            .to_owned();
        return RawLine {
            text,
            ..line.clone()
        };
    }
    line.clone()
}

fn build_block(lines: Vec<RawLine>) -> TextBlock {
    let text = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let styled_text = lines
        .iter()
        .any(|l| l.styled.as_deref().is_some_and(|s| !s.is_empty()))
        .then(|| {
            lines
                .iter()
                .map(|l| l.styled.as_deref().unwrap_or(&l.text))
                .collect::<Vec<_>>()
                .join(" ")
        });

    // Block-level only, by design: every line must agree, or the block
    // carries no shift. should_break above already splits on disagreement,
    // so this is the belt to that brace rather than a second policy.
    let first = lines[0].fmt;
    let fmt = if lines
        .iter()
        .all(|l| same_fmt(l.fmt.as_ref(), first.as_ref()))
    {
        first
    } else {
        None
    };

    let indents = lines.iter().map(|l| l.indent);
    let min_indent = indents.clone().fold(f64::INFINITY, f64::min);
    let max_indent = indents.fold(f64::NEG_INFINITY, f64::max);

    TextBlock {
        text,
        styled_text,
        fmt,
        dual_right: lines
            .iter()
            .any(|l| l.dual_right == Some(true))
            .then_some(true),
        indent: lines[0].indent,
        min_indent,
        max_indent,
        page_num: lines[0].page_num,
        y_position: lines[0].y,
        lines,
    }
}

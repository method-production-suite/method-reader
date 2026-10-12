//! Port of `src/fountain/serialize.ts`: parsed elements to Fountain text.
//!
//! Fountain forcing syntax is used defensively (`@cue`, `> transition`,
//! `!action`) so fountain-js re-parses the output without relying on its own
//! guesswork. The `.fountain` is a durable artifact and the app's cache
//! boundary, so it is byte-stable under rendering options: only the
//! stage-1 options (rejoin, contdMode, showPageMarkers, dualDialogue) move it.

use std::sync::LazyLock;

use sp_model::{
    ContdMode, DualDialogue, ElementType, FamilyBucket, Fmt, FormatOptions, ImageRef,
    ParsedScreenplay, ScreenplayElement, SizeStep,
};
use unicode_general_category::{GeneralCategory, get_general_category};

use crate::js;
use crate::jsre::Regex;

/// `TitleMeta`: what the title page said. Empty strings are kept as the
/// TypeScript keeps them: falsy for the serializer, but not absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TitleMeta {
    pub title: Option<String>,
    pub author: Option<String>,
}

struct Patterns {
    by_line: Regex,
    more_paren: Regex,
    contd: Regex,
    needs_force: Regex,
    by_line_rest: Regex,
    page_number: Regex,
    whitespace_run: Regex,
    emphasis_run: Regex,
    edge_slash_lead: Regex,
    edge_slash_trail: Regex,
    any_delimiter: Regex,
    leading_dot: Regex,
}

static P: LazyLock<Patterns> = LazyLock::new(|| Patterns {
    by_line: Regex::new(r"\b(written by|screenplay by|by)\b", "i"),
    more_paren: Regex::new(r"^\(\s*MORE\s*\)$", "i"),
    contd: Regex::new(r"\(\s*CONT['’]?D\.?\s*\)", "i"),
    // Leading chars that carry Fountain meaning at line start; ! forces action.
    needs_force: Regex::new(r"^[!.>=~@#]", ""),
    by_line_rest: Regex::new(r"^[\s/:–—-]+", ""),
    page_number: Regex::new(r"^(\d{1,3})\.?$", ""),
    whitespace_run: Regex::new(r"\s+", "g"),
    // A delimiter, content carrying no delimiter, the same delimiter again.
    emphasis_run: Regex::new(r"(\*{1,3}|_)([^*_]+?)\1", "g"),
    edge_slash_lead: Regex::new(r"^\s*\/\s*", ""),
    edge_slash_trail: Regex::new(r"\s*\/\s*$", ""),
    any_delimiter: Regex::new(r"\*{1,3}|_", "g"),
    leading_dot: Regex::new(r"^\.", ""),
});

fn family_word(f: FamilyBucket) -> &'static str {
    match f {
        FamilyBucket::Mono => "mono",
        FamilyBucket::Serif => "serif",
        FamilyBucket::Sans => "sans",
        FamilyBucket::Cursive => "cursive",
    }
}

fn size_word(s: SizeStep) -> &'static str {
    match s {
        SizeStep::Smaller => "-1",
        SizeStep::Larger => "+1",
        SizeStep::MuchLarger => "+2",
    }
}

/// A block's font shift as a Fountain note (registry #18). Emitted
/// unconditionally: `preserveFontShifts` gates rendering, not serialization.
fn fmt_note(fmt: Option<&Fmt>) -> Option<String> {
    let fmt = fmt?;
    let parts: Vec<&str> = [fmt.family.map(family_word), fmt.size.map(size_word)]
        .into_iter()
        .flatten()
        .collect();
    (!parts.is_empty()).then(|| format!("[[fmt: {}]]", parts.join(" ")))
}

/// `Math.round`: half rounds toward +Infinity.
fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// A whole number as JavaScript's template literal writes it.
fn js_int(x: f64) -> String {
    if x == 0.0 {
        return "0".to_owned();
    }
    if x.is_finite() && x.abs() < 1e21 {
        format!("{x:.0}")
    } else if x.is_nan() {
        "NaN".to_owned()
    } else if x.is_infinite() {
        if x > 0.0 { "Infinity" } else { "-Infinity" }.to_owned()
    } else {
        // 1e21 and up: JavaScript switches to exponent form.
        let s = format!("{x:e}");
        s.replace('e', "e+").replace("e+-", "e-")
    }
}

/// An image as a Fountain note (registry #19), read back by
/// `notes::parse_img_note`. THE WIDTH MUST BE AN INTEGER: a fractional `w=`
/// does not parse and the picture would render full-column.
fn img_note(img: &ImageRef) -> String {
    let w = js_int(js_round(img.width_pct));
    let full = if img.full_page { " full" } else { "" };
    format!("[[img: {} w={w}{full}]]", img.file)
}

/// `text.toUpperCase()` compared to text, then title-case: ALL-CAPS text
/// becomes Title Case, mixed-case text is left alone.
fn humanize_caps(text: &str) -> String {
    if text != text.to_uppercase() {
        return text.to_owned();
    }
    // `.toLowerCase().replace(/\b\p{L}/gu, c => c.toUpperCase())`: \b is
    // ASCII-word based even under the u flag.
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    let mut out = String::new();
    for (k, &c) in lower.iter().enumerate() {
        let prev_word = k > 0 && js::is_word(lower[k - 1]);
        let boundary = prev_word != js::is_word(c);
        if boundary && is_letter(c) {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn is_letter(c: char) -> bool {
    matches!(
        get_general_category(c),
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
    )
}

/// `extractTitleMeta`: title and author from the detected title page.
/// Readable elements before the by-line form the title; the by-line's own
/// remainder, or the first readable element after it, is the author.
pub fn extract_title_meta(elements: &[ScreenplayElement]) -> TitleMeta {
    let title_page: Vec<&ScreenplayElement> = elements
        .iter()
        .filter(|el| el.is_title_page && el.is_readable)
        .collect();
    if title_page.is_empty() {
        return TitleMeta::default();
    }
    let by_idx = title_page.iter().position(|el| P.by_line.test(&el.text));
    let Some(by_idx) = by_idx else {
        return TitleMeta {
            title: Some(humanize_caps(js::trim(&title_page[0].text))),
            author: None,
        };
    };

    let parts: Vec<&str> = title_page[..by_idx]
        .iter()
        .map(|el| js::trim(&el.text))
        .filter(|s| !s.is_empty())
        .collect();
    let title = (!parts.is_empty()).then(|| humanize_caps(&parts.join(" ")));

    // Author inline in the by-line ("written by Jane Doe") or the next element.
    let by_text: Vec<char> = title_page[by_idx].text.chars().collect();
    let m = P.by_line.find_from(&by_text, 0).unwrap();
    let after = m[1].unwrap().1;
    let rest: String = by_text[after..].iter().collect();
    let inline = js::trim(
        &P.by_line_rest
            .replace_with(&rest, false, |_, _| String::new()),
    )
    .to_owned();
    let author = if !inline.is_empty() {
        Some(inline)
    } else {
        title_page
            .get(by_idx + 1)
            .map(|el| js::trim(&el.text).to_owned())
            .filter(|s| !s.is_empty())
    };
    TitleMeta { title, author }
}

/// True when this element belongs in the Fountain body. A picture on a title
/// page is admitted: it is the designed title card.
fn is_body(el: &ScreenplayElement) -> bool {
    if el.kind == ElementType::Image {
        return el.is_readable;
    }
    !el.is_title_page && el.is_readable && el.kind != ElementType::PageNumber
}

/// True when a page marker may be anchored to this element: a POSITIVE,
/// exhaustive list, so a new element type anchors nothing until someone says
/// it does. Dialogue and parentheticals are not anchors (a page turning
/// mid-speech must not break it), nor is a picture.
fn anchors_page_marker(el: &ScreenplayElement) -> bool {
    match el.kind {
        ElementType::Scene
        | ElementType::Character
        | ElementType::Action
        | ElementType::Transition
        | ElementType::MiniSlug => true,
        ElementType::Dialogue
        | ElementType::Parenthetical
        | ElementType::Image
        | ElementType::PageNumber => false,
    }
}

/// A PICTURE IS NOT EVIDENCE ABOUT THE ELEMENTS AROUND IT: the nearest
/// element that is not an image, backwards from `from`.
fn prev_non_image(els: &[ScreenplayElement], from: Option<usize>) -> Option<&ScreenplayElement> {
    let mut i = from?;
    loop {
        let el = els.get(i)?;
        if el.kind != ElementType::Image {
            return Some(el);
        }
        i = i.checked_sub(1)?;
    }
}

/// The same, forwards.
fn next_non_image(els: &[ScreenplayElement], from: usize) -> Option<&ScreenplayElement> {
    els[from.min(els.len())..]
        .iter()
        .find(|el| el.kind != ElementType::Image)
}

fn speaks(el: Option<&ScreenplayElement>) -> bool {
    el.is_some_and(|e| matches!(e.kind, ElementType::Dialogue | ElementType::Parenthetical))
}

/// The pre-passes: drop `(MORE)` markers, drop `(CONT'D)` cues that directly
/// continue the previous speech (page-break splits) so it rejoins, and
/// demote cues with no speech after them to action.
fn prepare(elements: &[ScreenplayElement], rejoin: bool) -> Vec<ScreenplayElement> {
    let mut merged: Vec<ScreenplayElement> = Vec::new();
    for el in elements.iter().filter(|el| is_body(el)) {
        if P.more_paren.test(js::trim(&el.text)) {
            continue;
        }
        let prev = prev_non_image(&merged, merged.len().checked_sub(1));
        if rejoin
            && el.kind == ElementType::Character
            && P.contd.test(&el.text)
            && speaks(prev)
            && prev.is_some_and(|p| p.character == el.character)
        {
            continue; // same speech continues: drop the repeated cue
        }
        merged.push(el.clone());
    }
    (0..merged.len())
        .map(|i| {
            let mut el = merged[i].clone();
            if el.kind == ElementType::Character && !speaks(next_non_image(&merged, i + 1)) {
                el.kind = ElementType::Action;
            }
            el
        })
        .collect()
}

/// Sheet to printed-page offset, from the PDF's own printed page numbers:
/// the most common offset wins (first seen on a tie).
fn printed_page_offset(elements: &[ScreenplayElement]) -> Option<i64> {
    let mut counts: Vec<(i64, u32)> = Vec::new();
    for el in elements {
        if el.kind != ElementType::PageNumber {
            continue;
        }
        let Some(m) = P.page_number.captures(js::trim(&el.text)) else {
            continue;
        };
        let n: i64 = m[1].as_deref().unwrap().parse().unwrap();
        let offset = n - i64::from(el.page_num);
        match counts.iter_mut().find(|(o, _)| *o == offset) {
            Some((_, c)) => *c += 1,
            None => counts.push((offset, 1)),
        }
    }
    let mut best = None;
    let mut best_count = 0;
    for (offset, count) in counts {
        if count > best_count {
            best = Some(offset);
            best_count = count;
        }
    }
    best
}

/// `disarmBoneyard`: Fountain deletes everything between `/*` and `*/`, and
/// neither is ever written on purpose. A slash at an emphasis run's edge
/// moves OUT of the run (whitespace inside the delimiters would stop the
/// emphasis); if a delimiter pair still survives, every marker on the line
/// goes and the words stay.
pub fn disarm_boneyard(styled: &str) -> String {
    let moved = P.emphasis_run.replace_with(styled, true, |s, caps| {
        let whole: String = {
            let (a, b) = caps[0].unwrap();
            s[a..b].iter().collect()
        };
        let delim: String = {
            let (a, b) = caps[1].unwrap();
            s[a..b].iter().collect()
        };
        let body: String = {
            let (a, b) = caps[2].unwrap();
            s[a..b].iter().collect()
        };
        let lead = P.edge_slash_lead.test(&body);
        let trail = P.edge_slash_trail.test(&body);
        if !lead && !trail {
            return whole;
        }
        let mut inner = body;
        if lead {
            inner = P
                .edge_slash_lead
                .replace_with(&inner, false, |_, _| String::new());
        }
        if trail {
            inner = P
                .edge_slash_trail
                .replace_with(&inner, false, |_, _| String::new());
        }
        if js::trim(&inner).is_empty() {
            return " / ".to_owned();
        }
        format!(
            "{}{delim}{inner}{delim}{}",
            if lead { " / " } else { "" },
            if trail { " / " } else { "" }
        )
    });
    if !moved.contains("/*") && !moved.contains("*/") {
        return moved;
    }
    P.any_delimiter.replace_all(&moved, "")
}

/// `text.replace(/\s+/g, ' ')`.
fn collapse_spaces(s: &str) -> String {
    P.whitespace_run.replace_all(s, " ")
}

/// `toFountain`: serialize a parsed screenplay to Fountain text.
pub fn to_fountain(
    screenplay: &ParsedScreenplay,
    meta: Option<&TitleMeta>,
    format: &FormatOptions,
) -> String {
    let mut out: Vec<String> = Vec::new();
    let truthy = |s: &Option<String>| s.as_deref().is_some_and(|s| !s.is_empty());

    if let Some(meta) = meta
        && (truthy(&meta.title) || truthy(&meta.author))
    {
        let mut head: Vec<String> = Vec::new();
        if truthy(&meta.title) {
            head.push(format!("Title: {}", meta.title.as_deref().unwrap()));
        }
        if truthy(&meta.author) {
            head.push("Credit: Written by".to_owned());
            head.push(format!("Author: {}", meta.author.as_deref().unwrap()));
        }
        out.push(head.join("\n"));
    }

    let page_offset = if format.show_page_markers {
        printed_page_offset(&screenplay.elements).unwrap_or(0)
    } else {
        0
    };
    let mut last_marked_page: Option<u32> = None;

    // An open dialogue block accumulates cue/parenthetical/dialogue lines
    // until a non-dialogue element closes it. Image notes that arrive while
    // it is open wait for it to close; INVARIANT: `deferred` is empty
    // whenever `block` is None, because close_block drains it.
    let mut last_speaker: Option<String> = None;
    let mut block: Option<Vec<String>> = None;
    let mut deferred: Vec<String> = Vec::new();
    let close_block =
        |out: &mut Vec<String>, block: &mut Option<Vec<String>>, deferred: &mut Vec<String>| {
            if let Some(b) = block.take()
                && !b.is_empty()
            {
                out.push(b.join("\n"));
            }
            out.append(deferred);
        };

    for el in prepare(&screenplay.elements, format.rejoin_split_dialogue) {
        let text = js::trim(&el.text).to_owned();
        if text.is_empty() {
            continue;
        }

        // Page markers land only at block boundaries, never inside a speech
        // and never on a plate (anchors_page_marker).
        if format.show_page_markers
            && anchors_page_marker(&el)
            && last_marked_page.is_none_or(|p| el.page_num > p)
        {
            let label = i64::from(el.page_num) + page_offset;
            if label >= 1 && last_marked_page.is_some() {
                close_block(&mut out, &mut block, &mut deferred);
                out.push(format!("= pg {label}"));
            }
            last_marked_page = Some(el.page_num);
        }

        match el.kind {
            ElementType::Character => {
                close_block(&mut out, &mut block, &mut deferred);
                let mut cue = collapse_spaces(&text);
                if format.contd_mode != ContdMode::Keep {
                    let stripped = P.contd.replace_with(&cue, false, |_, _| String::new());
                    cue = js::trim(&collapse_spaces(&stripped)).to_owned();
                    let character = el.character.as_deref().filter(|c| !c.is_empty());
                    if format.contd_mode == ContdMode::Auto
                        && character.is_some()
                        && el.character == last_speaker
                    {
                        cue = format!("{cue} (CONT'D)");
                    }
                }
                let caret = if el.dual_right == Some(true)
                    && format.dual_dialogue == DualDialogue::SideBySide
                {
                    " ^"
                } else {
                    ""
                };
                block = Some(vec![format!("@{cue}{caret}")]);
                last_speaker = el.character.clone();
            }
            ElementType::Parenthetical => {
                // Markers would break "(...)" recognition: always plain.
                match &mut block {
                    Some(b) => b.push(text),
                    None => out.push(text),
                }
            }
            ElementType::Dialogue => {
                let styled = el.styled_text.as_deref().unwrap_or(&el.text);
                let line = disarm_boneyard(js::trim(styled));
                let with_note = match fmt_note(el.fmt.as_ref()) {
                    Some(note) => format!("{note} {line}"),
                    None => line,
                };
                match &mut block {
                    Some(b) => b.push(with_note),
                    // Stray dialogue without a cue reads as action.
                    None => out.push(with_note),
                }
            }
            ElementType::Scene => {
                close_block(&mut out, &mut block, &mut deferred);
                last_speaker = None;
                match el.scene_number.as_deref().filter(|n| !n.is_empty()) {
                    Some(n) => out.push(format!("{text} #{n}#")),
                    None => out.push(text),
                }
            }
            ElementType::Transition => {
                close_block(&mut out, &mut block, &mut deferred);
                last_speaker = None;
                out.push(format!("> {text}"));
            }
            ElementType::MiniSlug => {
                // The forced heading form; a leading dot is the one text the
                // dot-force cannot carry, so that falls back to forced action.
                // A mini-slug is a cut in time or place: the (CONT'D) chain
                // resets (#8a).
                close_block(&mut out, &mut block, &mut deferred);
                last_speaker = None;
                if P.leading_dot.test(&text) {
                    out.push(format!("!{text}"));
                } else {
                    out.push(format!(".{text}"));
                }
            }
            ElementType::Image => {
                // Its own block, so fountain-js makes it a standalone note;
                // inside an open speech it WAITS, because a cue alone in its
                // paragraph re-parses as action with the "@" showing.
                let Some(img) = &el.img else {
                    continue;
                };
                let note = img_note(img);
                if block.is_some() {
                    deferred.push(note);
                } else {
                    out.push(note);
                }
            }
            ElementType::Action | ElementType::PageNumber => {
                // `default:` in the TypeScript: action, and a page number
                // never arrives (is_body drops it). Only action carries a
                // styled variant or an fmt note.
                close_block(&mut out, &mut block, &mut deferred);
                let is_action = el.kind == ElementType::Action;
                let body = if is_action {
                    disarm_boneyard(js::trim(el.styled_text.as_deref().unwrap_or(&el.text)))
                } else {
                    text
                };
                let forced = if P.needs_force.test(&body) {
                    format!("!{body}")
                } else {
                    body
                };
                let note = if is_action {
                    fmt_note(el.fmt.as_ref())
                } else {
                    None
                };
                // Glued above with a single newline, so fountain-js folds the
                // note into the block's own token.
                match note {
                    Some(note) => out.push(format!("{note}\n{forced}")),
                    None => out.push(forced),
                }
            }
        }
    }
    close_block(&mut out, &mut block, &mut deferred);

    out.join("\n\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_like_math_round() {
        assert_eq!(js_round(71.5), 72.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(0.499_999_999_999_999_94), 0.0);
        assert_eq!(js_int(js_round(71.4)), "71");
        assert_eq!(js_int(-0.0), "0");
    }

    #[test]
    fn humanizes_caps_like_the_typescript() {
        assert_eq!(humanize_caps("MOONQUAKE JAMBOREE"), "Moonquake Jamboree");
        assert_eq!(humanize_caps("Mixed Case"), "Mixed Case");
        assert_eq!(humanize_caps("O'NEIL-SMITH"), "O'Neil-Smith");
        // \b is ASCII-word based, so accented letters land oddly; these are
        // the answers bun gives.
        assert_eq!(humanize_caps("ÉTÉ CAFÉ"), "éTÉ CafÉ");
        // "ß" uppercases to "SS", so the text is not all caps.
        assert_eq!(humanize_caps("STRASSE ß"), "STRASSE ß");
    }
}

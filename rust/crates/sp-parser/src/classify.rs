//! Port of `src/parser/classify.ts`: one text block in, one screenplay
//! element out.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

use regex::Regex;
use sp_model::{AgeValue, ElementType, ScreenplayElement, TextBlock, indent_ranges as R};

use crate::boilerplate::is_boilerplate_line;
use crate::cue::{is_cue_text, strip_extensions};
use crate::js;

// Regex patterns ported from v2 spec
// group.rs starts a new block on this same test, so a heading is never
// glued to the lines above it: one copy, so the line group.rs isolates and
// the line classified below as a heading cannot drift apart. Non-global.
pub static SCENE_HEADING: LazyLock<Regex> =
    LazyLock::new(|| js::re(r"^(INT\.|EXT\.|INT\.\/EXT\.|I\/E\.)"));
// Shooting-script number printed in BOTH margins of the heading row itself
// ("2 EXT. WOODS - DAY 2") — extraction joins the row into one line, so the
// pair is stripped here and attached as the scene number. The same-token
// requirement keeps false positives out.
//
// The TypeScript is `/^(\d{1,3}[A-Z]?)\s+(.*\S)\s+\1$/`, whose backreference
// the regex crate cannot express: see `dual_margin_heading`.
static DUAL_MARGIN_NUMBER: LazyLock<Regex> = LazyLock::new(|| js::re(r"^\d{1,3}[A-Z]?$"));
// Wider than SCENE_HEADING above: the openers fountain-js accepts for an
// UNFORCED heading (EST., the space forms, bare I/E). Mini-slugs serialize
// as FORCED sluglines (".LATER"), and fountain-js promotes a forced line
// back to a full scene heading — section, TOC entry and all — when its text
// also matches this shape. So a block matching it must never classify as a
// mini-slug; it falls through to action instead. The identical literal
// is sp_fountain::PRIMARY_SLUG_SOURCE, the renderers' shared
// discriminator; sp-epub's tests/epub.rs pins the two together, and
// sp-fountain's tests/slug.rs pins that one to fountain-js's behaviour.
pub static PRIMARY_SLUG: LazyLock<Regex> =
    LazyLock::new(|| js::re_i(r"^(?:\*{0,3}_?)?(?:(?:int|i)\.?\/(?:ext|e)|int|ext|est)[. ]"));
static TRANSITION: LazyLock<Regex> = LazyLock::new(|| js::re(r"^[A-Z\s]+:$"));
static PAGE_NUMBER_BARE: LazyLock<Regex> = LazyLock::new(|| js::re(r"^\d+\.?$"));
static PAGE_NUMBER_DASHED: LazyLock<Regex> = LazyLock::new(|| js::re(r"^-\s*\d+\s*-$"));
static PAGE_NUMBER_LABELED: LazyLock<Regex> =
    LazyLock::new(|| js::re_i(r"^(?:page|p\.)\s*\d+\.?$"));
// Shooting script scene numbers: "2.2.", "1A.", "42.", "1-A.", "A1.", "1A", "2.2"
static SCENE_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| js::re(r"^\d+[A-Z]?(?:[.\-](?:\d+[A-Z]?|[A-Z]))*\.?$"));
static PARENTHETICAL: LazyLock<Regex> = LazyLock::new(|| js::re(r"^\([^)]+\)$"));
static PARENTHETICAL_TRUNCATED: LazyLock<Regex> = LazyLock::new(|| js::re(r"^\([^)]+\)\.{3}$"));
// Cue SHAPE lives in cue.rs, imported above; these moved there with it, and
// so did CHARACTER_EXTENSIONS, which classify imports back: one copy, so the
// name stripped here cannot drift from the name cue.rs judges.

// Mini-slug shape (see is_mini_slug_shaped). A slugline ends bare or on a
// colon; the last character carries most of the signal. A closing paren is
// NOT in the class: the only lines it would have admitted are whole
// parentheticals, which have to be refused anyway, and no true slug in any
// fixture ends on one.
static MINI_SLUG_TAIL: LazyLock<Regex> = LazyLock::new(|| js::re(r"[A-Z0-9:]$"));
static TRANSITION_TAIL: LazyLock<Regex> = LazyLock::new(|| js::re(r"\bTO:$"));
// PRIMARY_SLUG anchors at the start, but an unpaired shooting-script number
// pushes the opener off it ("2 EXT. WOODS - DAY 3" — the dual-margin strip
// above needs MATCHING numbers, so a mismatch leaves the heading whole).
// Stripping a leading number lets the same literal judge that line too.
static LEADING_SCENE_NUMBER: LazyLock<Regex> = LazyLock::new(|| js::re(r"^\d{1,3}[A-Z]?\s+"));

static ACTION_POSSESSIVE: LazyLock<Regex> = LazyLock::new(|| js::re(r"^[A-Z][a-z]+['’]s "));
static ACTION_PRONOUNS: LazyLock<Regex> =
    LazyLock::new(|| js::re_i(r"^(He |She |They |It |The |A |An |His |Her |Their |Its )"));
static ACTION_VERBS: LazyLock<Regex> = LazyLock::new(|| {
    js::re_i(r"(walks|turns|looks|stands|sits|moves|enters|exits|fades|drifts|floats|pays|rises)")
});
static ACTION_CAMERA: LazyLock<Regex> =
    LazyLock::new(|| js::re(r"^(INT\.|EXT\.|FADE|CUT|ANGLE|CLOSE|WIDE|PAN|ZOOM)"));

// Age variant patterns
static AGE_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| js::re(r"^(YOUNG|OLD|OLDER|YOUNGER)\s+(.+)$"));
static AGE_DECADE: LazyLock<Regex> = LazyLock::new(|| {
    js::re_i(r"^(.+?)\s*\(\s*(TWENTIES|THIRTIES|FORTIES|FIFTIES|SIXTIES|SEVENTIES|EIGHTIES)\s*\)")
});
static AGE_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| js::re_i(r"^(.+?)\s*[(,]\s*(?:AGE\s+)?(\d+)"));
static AGE_STAGE: LazyLock<Regex> =
    LazyLock::new(|| js::re_i(r"^(.+?)\s*\(\s*(CHILD|TEEN|ADULT|ELDERLY)\s*\)"));
static AGE_LITTLE: LazyLock<Regex> = LazyLock::new(|| js::re(r"^(LITTLE|BABY)\s+(.+)$"));

static SMART_QUOTES: LazyLock<Regex> = LazyLock::new(|| js::re(r"[‘’ʼ]"));
static CUE_EXTENSIONS_LOOSE: LazyLock<Regex> = LazyLock::new(|| js::re(r"(\s*\([^)]*\))+\s*$"));

// Fallback counter used only when classify_block is called directly without
// an explicit `next_id` (e.g. unit tests exercising classification in
// isolation). Real parses (`parse_lines` in lib.rs, the only entry point)
// thread their own call-scoped counter through `next_id` instead, so
// concurrent parses never share state.
static DEFAULT_ELEMENT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// `resetCounter`.
pub fn reset_counter() {
    DEFAULT_ELEMENT_COUNTER.store(0, Ordering::SeqCst);
}

/// The fallback id source, `elem0`, `elem1`, ... shared by every call that
/// does not bring its own.
pub fn default_next_id() -> String {
    format!(
        "elem{}",
        DEFAULT_ELEMENT_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// `DUAL_MARGIN_HEADING` matched against trimmed text: the number and the
/// heading between the two copies of it, or `None`.
///
/// Written out because the pattern needs a backreference. What the
/// backtracking match amounts to: the first token (everything before the
/// first whitespace) is the number, since `\s+` must follow it at once; the
/// line must end in whitespace and then that same token; and the heading is
/// what sits between, without its surrounding whitespace (the greedy `\s+`
/// on the left and the greedy `.*\S` on the right put it there). It may hold
/// no line terminator, which `.` refuses.
fn dual_margin_heading(t: &str) -> Option<(&str, &str)> {
    let first_ws = t.find(js::is_ws)?;
    let number = &t[..first_ws];
    if !DUAL_MARGIN_NUMBER.is_match(number) {
        return None;
    }
    let rest = t[first_ws..].trim_start_matches(js::is_ws);
    let before = rest.strip_suffix(number)?;
    let heading = before.trim_end_matches(js::is_ws);
    if heading.len() == before.len() || heading.is_empty() {
        return None;
    }
    if heading.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return None;
    }
    Some((number, heading))
}

/// Classify a text block into a screenplay element type.
/// Stateless — takes `prev_element` instead of tracking internal state.
/// Priority chain: page-number -> scene -> transition -> character ->
/// parenthetical -> action -> dialogue -> mini-slug -> default
pub fn classify_block(
    block: &TextBlock,
    prev_element: Option<&ScreenplayElement>,
    next_id: &mut dyn FnMut() -> String,
) -> ScreenplayElement {
    let text = block.text.as_str();
    let indent = block.indent;
    let id = next_id();
    // fmt rides from block to element mechanically and is NEVER a
    // classification input: the parser stays format-option-free (#18).
    // `y` rides across mechanically like `fmt`, and for the same reason it is
    // never a classification input: it is where the block was, not what it
    // is. `yPosition` is the block's FIRST line, which extraction orders
    // topmost-first — so it is the BASELINE of the block's top line, not the
    // block's top edge, and lib.rs's merge is written knowing that.
    let base = ScreenplayElement {
        kind: ElementType::Action,
        text: text.to_owned(),
        id,
        page_num: block.page_num,
        is_title_page: false,
        is_readable: true,
        character: None,
        base_character: None,
        age_modifier: None,
        age_value: None,
        scene_number: None,
        styled_text: block.styled_text.clone(),
        dual_right: block.dual_right,
        fmt: block.fmt,
        img: None,
        y: Some(block.y_position),
    };
    let with = |kind: ElementType| ScreenplayElement {
        kind,
        ..base.clone()
    };

    // Get current character from previous element chain
    let current_character = get_active_character(prev_element);

    // Priority 1: Page number
    // Bare numbers (e.g. "42", "42.") are page numbers at any indent — a
    // standalone number block is almost never meaningful dialogue or action.
    // Dashed ("- 42 -") and labeled ("Page 42") formats require moderate
    // indent.
    let trimmed = js::trim(text);
    if PAGE_NUMBER_BARE.is_match(trimmed) {
        return with(ElementType::PageNumber);
    }
    if PAGE_NUMBER_DASHED.is_match(trimmed) || PAGE_NUMBER_LABELED.is_match(&js::ci(trimmed)) {
        return with(ElementType::PageNumber);
    }
    // Shooting script scene numbers (e.g., "2.2.", "1A.", "42A") — classified
    // as page-number so they're hidden. The parser pipeline attaches them to
    // the following scene heading element as `sceneNumber`.
    //
    // Carrying `sceneNumber` here is what makes attach_scene_numbers able to
    // tell these apart from ordinary page numbers LATER. It cannot re-derive
    // the difference from the text: PAGE_NUMBER_BARE above already consumed
    // "1." and "42.", and a bare number is a scene number or a page number
    // depending only on which branch classified it. Without this marker,
    // every page that opens on a scene heading donates its page number to
    // that scene.
    if SCENE_NUMBER.is_match(trimmed) && js::len(trimmed) <= 10 {
        return ScreenplayElement {
            scene_number: Some(trimmed.to_owned()),
            ..with(ElementType::PageNumber)
        };
    }

    // Priority 1.5: production-draft furniture (revision slugs, draft stamps,
    // header dates) — routed to page-number so every consumer skips it.
    if is_boilerplate_line(trimmed) {
        return with(ElementType::PageNumber);
    }

    // Priority 2: Scene heading (no indent check — most reliable)
    if SCENE_HEADING.is_match(text) {
        return with(ElementType::Scene);
    }
    if let Some((number, heading)) = dual_margin_heading(trimmed)
        && SCENE_HEADING.is_match(heading)
    {
        return ScreenplayElement {
            text: heading.to_owned(),
            scene_number: Some(number.to_owned()),
            ..with(ElementType::Scene)
        };
    }

    // Priority 3: Transition
    if indent > R::TRANSITION_MIN && TRANSITION.is_match(text) {
        return with(ElementType::Transition);
    }

    // Priority 4: Truncated character name
    if text.ends_with("...") && (R::CHARACTER_MIN..=R::CHARACTER_MAX).contains(&indent) {
        let stripped = text.strip_suffix("...").unwrap_or(text);
        if is_likely_character_name(stripped, indent) {
            let info = extract_character_info(stripped);
            return ScreenplayElement {
                text: stripped.to_owned(),
                character: Some(info.name),
                base_character: Some(info.base_name),
                age_modifier: info.age_modifier,
                age_value: info.age_value,
                ..with(ElementType::Character)
            };
        }
    }

    // Priority 5: Character name
    if (R::CHARACTER_MIN..=R::CHARACTER_MAX).contains(&indent)
        && is_likely_character_name(text, indent)
    {
        let info = extract_character_info(text);
        return ScreenplayElement {
            character: Some(info.name),
            base_character: Some(info.base_name),
            age_modifier: info.age_modifier,
            age_value: info.age_value,
            ..with(ElementType::Character)
        };
    }

    // Priority 6: Parenthetical. Inside a speech one may sit as deep as the
    // cue band's edge: some scripts print it right of the cue itself (cue
    // 41%, parenthetical 43%, and 41% mid-speech), and the bracketed text,
    // which the cue rule above has already refused, is the signal there.
    if let Some(current) = &current_character
        && (R::DIALOGUE_MIN..=R::CHARACTER_MAX).contains(&indent)
        && (PARENTHETICAL.is_match(text) || PARENTHETICAL_TRUNCATED.is_match(text))
    {
        return ScreenplayElement {
            character: Some(current.clone()),
            ..with(ElementType::Parenthetical)
        };
    }

    // Priority 7: Action by pattern
    if indent < R::ACTION_MAX && is_action_by_pattern(text) {
        return with(ElementType::Action);
    }

    // Priority 8: Dialogue (requires active character)
    if let Some(current) = &current_character
        && (R::DIALOGUE_MIN..=R::DIALOGUE_MAX).contains(&indent)
        && !SCENE_HEADING.is_match(text)
        && !TRANSITION.is_match(text)
    {
        return ScreenplayElement {
            character: Some(current.clone()),
            ..with(ElementType::Dialogue)
        };
    }

    // Priority 9: Mini-slug (secondary slugline)
    if is_mini_slug_shaped(block) {
        return with(ElementType::MiniSlug);
    }

    // Priority 10: Default to action
    with(ElementType::Action)
}

/// Attach scene numbers from shooting scripts to the following scene
/// heading. A page-number element immediately before a scene element is a
/// scene number — but only when its text actually matches the scene-number
/// shape (revision slugs and other boilerplate also classify as page-number
/// and must never become scene numbers). Lives here (not lib.rs) so it
/// shares the private SCENE_NUMBER pattern and stays unit-testable on its
/// own.
/// Mutates elements in place — sets `scene_number` on the scene element.
pub fn attach_scene_numbers(elements: &mut [ScreenplayElement]) {
    for i in 0..elements.len().saturating_sub(1) {
        // Only a page-number element that classify_block MARKED as a scene
        // number is promoted. Re-testing SCENE_NUMBER here would match a bare
        // page number ("1.", "42.") just as well, because by this point the
        // two are the same type carrying the same shape of text, and every
        // page opening on a scene heading would hand its page number to that
        // scene.
        if elements[i].kind == ElementType::PageNumber
            && elements[i + 1].kind == ElementType::Scene
            && elements[i].scene_number.is_some()
        {
            let number = elements[i].scene_number.clone();
            elements[i + 1].scene_number = number;
        }
    }
}

fn get_active_character(prev: Option<&ScreenplayElement>) -> Option<String> {
    let prev = prev?;
    // JavaScript truthiness: an empty name is no active character.
    let named = || prev.character.clone().filter(|c| !c.is_empty());
    match prev.kind {
        ElementType::Character | ElementType::Dialogue | ElementType::Parenthetical => named(),
        // Action/scene/transition reset character context
        _ => None,
    }
}

/// `isLikelyCharacterName`.
pub fn is_likely_character_name(text: &str, indent: f64) -> bool {
    // Geometry here, shape in cue.rs. The indent band is this caller's own
    // concern: the dual-dialogue detector has no indent to check because
    // clusterSplit settles its geometry upstream, and forcing it to invent
    // one is how a second definition of "cue" got written and drifted.
    if !(R::CHARACTER_MIN..=R::CHARACTER_MAX).contains(&indent) {
        return false;
    }
    is_cue_text(text)
}

struct CharacterInfo {
    name: String,
    base_name: String,
    age_modifier: Option<String>,
    age_value: Option<AgeValue>,
}

fn extract_character_info(text: &str) -> CharacterInfo {
    // Strip extensions: "JACK (V.O.) (CONT'D)" -> "JACK"
    let name = js::trim(&strip_extensions(text)).to_owned();
    let (base_name, age_modifier, age_value) = extract_age_variant(&name);
    CharacterInfo {
        name,
        base_name,
        age_modifier,
        age_value,
    }
}

fn extract_age_variant(name: &str) -> (String, Option<String>, Option<AgeValue>) {
    if let Some(m) = AGE_PREFIX.captures(name) {
        return (
            m[2].to_owned(),
            Some(m[1].to_owned()),
            Some(AgeValue::Text(m[1].to_lowercase())),
        );
    }

    let folded = js::ci(name);
    if let Some(m) = AGE_DECADE.captures(&folded) {
        let base = &name[m.get(1).map_or(0..0, |g| g.range())];
        let decade = &name[m.get(2).map_or(0..0, |g| g.range())];
        let value = match decade.to_lowercase().as_str() {
            "twenties" => "20s",
            "thirties" => "30s",
            "forties" => "40s",
            "fifties" => "50s",
            "sixties" => "60s",
            "seventies" => "70s",
            "eighties" => "80s",
            _ => decade,
        };
        return (
            base.to_owned(),
            None,
            Some(AgeValue::Text(value.to_owned())),
        );
    }

    if let Some(m) = AGE_NUMBER.captures(&folded) {
        let base = &name[m.get(1).map_or(0..0, |g| g.range())];
        let digits = &name[m.get(2).map_or(0..0, |g| g.range())];
        // parseInt(digits, 10) of ASCII digits.
        let value: f64 = digits.parse().unwrap_or(f64::NAN);
        return (base.to_owned(), None, Some(AgeValue::Number(value)));
    }

    if let Some(m) = AGE_STAGE.captures(&folded) {
        let base = &name[m.get(1).map_or(0..0, |g| g.range())];
        let stage = &name[m.get(2).map_or(0..0, |g| g.range())];
        return (
            base.to_owned(),
            None,
            Some(AgeValue::Text(stage.to_lowercase())),
        );
    }

    if let Some(m) = AGE_LITTLE.captures(name) {
        return (
            m[2].to_owned(),
            Some(m[1].to_owned()),
            Some(AgeValue::Text("child".to_owned())),
        );
    }

    (name.to_owned(), None, None)
}

/// Canonical key for comparing a candidate cue against the character
/// roster: smart quotes straightened, case folded, extensions dropped.
/// Deliberately lossy — it exists to match "dev" and "MARGO’S MOM" to
/// names the script already established, not to display.
pub fn normalize_cue_name(text: &str) -> String {
    let straight = SMART_QUOTES.replace_all(text, "'");
    let bare = CUE_EXTENSIONS_LOOSE.replace(&straight, "");
    js::trim(&js::collapse_ws(&bare)).to_uppercase()
}

/// Mini-slug (secondary slugline) shape test — "END OF MONTAGE",
/// "IN THE PROJECTION BOOTH", "QUICK MONTAGE:".
///
/// Runs LAST in the chain (priority 9), so every existing veto keeps its
/// say: a candidate that reads as a cue, a speech, or action-by-pattern
/// has already returned. This is deliberate. The band it works in also
/// holds all-caps sound effects ("THUD.", "CRASH!") and all-caps action
/// sentences ("THE ENGINE STARTS."), and a promoted sound effect renders
/// as a bold heading — a visible defect — while a missed slug renders as
/// action, which is harmless. Precision beats recall here, so the rule
/// only claims blocks that nothing else objects to.
///
/// Every guard, and why (calibrated against the five generator fixtures,
/// 2026-07-30 — 655 all-caps blocks in the action band, 92 promoted):
fn is_mini_slug_shaped(block: &TextBlock) -> bool {
    let t = js::trim(&block.text);

    // Action band, measured from the PAGE edge — where extraction puts it.
    // The old `indent < 5` assumed a margin-relative measure that no
    // generator produces (the real modal action indent is 15-18), which is
    // why PDF input produced zero mini-slugs for as long as the type existed.
    // ACTION_MAX (20) also sits structurally below DIALOGUE_MIN (25) and
    // CHARACTER_MIN (35), so this can never fire in the cue or speech band.
    if block.indent >= R::ACTION_MAX {
        return false;
    }

    // One line, bounded by blank lines — group_blocks breaks on a >20pt Y
    // gap, so a single-line block IS a standalone beat. Prose long enough to
    // be a sentence wraps, and a wrapped block is never a heading.
    if block.lines.len() != 1 {
        return false;
    }

    // Strict all-caps. Not the cue heuristic's 0.8 ratio — a slugline is
    // typed in caps, entirely. This alone rejects the "Dev CROSSES TO THE
    // COUNTER" emphasis style that some writers use for action.
    if t != t.to_uppercase() {
        return false;
    }

    // Observed true slugs top out at 52 characters; 55 leaves a margin
    // without opening the door to a full line of caps prose.
    let n = js::len(t);
    if !(2..=55).contains(&n) {
        return false;
    }

    // A slugline is words. Letters must carry the line — this is what keeps
    // a WGA registration number ("WGA 1234567 555-0100") off the heading.
    let dense: String = t.chars().filter(|&c| !js::is_ws(c)).collect();
    let upper = dense.chars().filter(char::is_ascii_uppercase).count();
    #[allow(clippy::cast_precision_loss)]
    if (upper as f64 / js::len(&dense) as f64) < 0.5 {
        return false;
    }

    // THE discriminator. Mini-slugs end bare or on a colon; sound effects and
    // action sentences end on terminal punctuation. Across all six fixtures
    // every sound effect and shouted beat carried a "." or "!" and every true
    // slug ended bare or on ":" — the ambiguity the type invited ("SILENCE"
    // vs "END DREAM") does not actually occur in the corpus. Also rejects the
    // trailing dash of a broken-off line, the "***" of a revision note, and
    // (via the missing paren) a bare "(BEAT)" with no speaker above it.
    if !MINI_SLUG_TAIL.is_match(t) {
        return false;
    }

    // Fountain's own transition rule: uppercase, ends in "TO:". The format
    // says that is a transition, so it is not a slug — this is what keeps
    // "DISSOLVE TO:" and "SMASH CUT TO:" (both left-flush in real scripts)
    // out, since the transition branch above only fires at right-flush
    // indent.
    if TRANSITION_TAIL.is_match(t) {
        return false;
    }

    // PRIMARY_SLUG, not SCENE_HEADING: a mini-slug serializes as a FORCED
    // slugline, and fountain-js promotes a forced line back to a full scene
    // heading — section, TOC entry and all — whenever its text also matches
    // that wider unforced shape (EST., the space forms, bare I/E). Minting one
    // here would round-trip into a scene. Tested against the line as written
    // AND with a leading shooting-script number stripped, since that number
    // pushes the opener off the anchor.
    let unnumbered = LEADING_SCENE_NUMBER.replace(t, "");
    !PRIMARY_SLUG.is_match(&js::ci(t)) && !PRIMARY_SLUG.is_match(&js::ci(&unnumbered))
}

fn is_action_by_pattern(text: &str) -> bool {
    let folded = js::ci(text);
    ACTION_POSSESSIVE.is_match(text)
        || ACTION_PRONOUNS.is_match(&folded)
        || ACTION_VERBS.is_match(&folded)
        || ACTION_CAMERA.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::dual_margin_heading;

    #[test]
    fn dual_margin_heading_needs_the_same_number_both_sides() {
        assert_eq!(
            dual_margin_heading("2 EXT. WOODS - DAY 2"),
            Some(("2", "EXT. WOODS - DAY"))
        );
        assert_eq!(
            dual_margin_heading("12A  INT. HALL  12A"),
            Some(("12A", "INT. HALL"))
        );
        assert_eq!(dual_margin_heading("2 EXT. WOODS - DAY 3"), None);
        assert_eq!(dual_margin_heading("12 EXT. WOODS 2"), None);
        assert_eq!(dual_margin_heading("2   2"), None);
        assert_eq!(dual_margin_heading("1234 EXT. X 1234"), None);
    }
}

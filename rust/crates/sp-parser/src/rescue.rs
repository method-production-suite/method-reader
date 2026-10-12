//! Port of `src/parser/rescue.ts`.

use std::collections::HashMap;

use sp_model::{ElementType, ScreenplayElement, TextBlock, indent_ranges as R};

use crate::classify::normalize_cue_name;

/// Second pass: promote `action` elements back to `character` when the
/// document's own evidence says they are cues.
///
/// Classification is geometric, but `is_likely_character_name` layers text
/// heuristics on top that can overrule position — a dropped period, a smart
/// quote, a missed shift key. Each rejection costs TWO elements, because
/// with no active character the speech underneath falls to action too.
///
/// Every name this rescues is already recognized elsewhere in the same
/// script, so the roster is the corroboration. It cannot invent characters.
///
/// `elements[i]` pairs with `blocks[i]` — parse_lines pushes exactly one
/// element per block, in order. Mutates `elements` in place.
#[allow(clippy::needless_range_loop)] // `j` also reads blocks[j]
pub fn rescue_cues(elements: &mut [ScreenplayElement], blocks: &[TextBlock]) {
    // normalized -> established baseCharacter
    let mut roster: HashMap<String, String> = HashMap::new();
    for el in elements.iter() {
        if el.kind == ElementType::Character
            && let Some(base) = el.base_character.as_deref().filter(|b| !b.is_empty())
        {
            roster.insert(normalize_cue_name(base), base.to_owned());
        }
    }
    if roster.is_empty() {
        return;
    }

    let indent_of = |i: usize| blocks.get(i).map_or(-1.0, |b| b.indent);
    let in_cue_band = |i: usize| (R::CHARACTER_MIN..=R::CHARACTER_MAX).contains(&indent_of(i));
    let in_speech_band = |i: usize| (R::DIALOGUE_MIN..=R::DIALOGUE_MAX).contains(&indent_of(i));

    for i in 0..elements.len().saturating_sub(1) {
        if elements[i].kind != ElementType::Action {
            continue;
        }
        // Centered title text lands at cue-band indent, so a script whose
        // title (or author) matches a character name would grow a spurious
        // cue on page 1. Title pages are never dialogue; skip them outright.
        if elements[i].is_title_page {
            continue;
        }
        if !in_cue_band(i) {
            continue;
        }

        let Some(established) = roster.get(&normalize_cue_name(&elements[i].text)).cloned() else {
            continue;
        };

        // A cue is only a cue if a speech follows it.
        let next = &elements[i + 1];
        if next.kind != ElementType::Action || !in_speech_band(i + 1) {
            continue;
        }

        let el = &mut elements[i];
        el.kind = ElementType::Character;
        el.character = Some(established.clone());
        el.base_character = Some(established.clone());

        for j in i + 1..elements.len() {
            if elements[j].kind != ElementType::Action || !in_speech_band(j) {
                break;
            }
            if elements[j].is_title_page {
                break;
            }
            // Don't swallow the NEXT missed cue: the bands overlap at 35, so a
            // cue sitting exactly there is in both. Consuming it as dialogue
            // would merge two speakers and put it beyond rescue.
            if in_cue_band(j) && roster.contains_key(&normalize_cue_name(&elements[j].text)) {
                break;
            }
            elements[j].kind = ElementType::Dialogue;
            elements[j].character = Some(established.clone());
        }
    }
}

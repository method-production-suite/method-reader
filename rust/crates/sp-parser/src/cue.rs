//! Port of `src/parser/cue.ts`.
//!
//! The ONE definition of what a character cue looks like as TEXT.
//!
//! There were three, and they disagreed. `isLikelyCharacterName` in
//! classify.ts carried forty lines of accumulated rules; `isCueShaped` in
//! extract.ts, used only by dual-dialogue detection, carried three; and
//! `rescueCues` reasons from the document's roster instead. The first two
//! disagreeing is not hypothetical, it shipped: a script whose lead is named
//! "Q" rendered every ordinary Q cue correctly, because the classifier
//! accepts a one-letter name, while the dual detector rejected it and
//! collapsed an entire scene into interleaved action.
//!
//! Same doctrine CLAUDE.md already applies to slug.ts and notes.ts: one copy
//! of a discriminator that both callers import, because two copies drift.
//!
//! WHAT BELONGS HERE AND WHAT DOES NOT. This answers about text only.
//! Geometry stays with the caller, because the callers genuinely differ:
//! classify checks the cue indent band itself, while the dual detector has
//! already had its geometry settled upstream by clusterSplit, which demands
//! a left cluster inside 42% of the page and a right one past 48%. Pushing
//! an indent argument in here would force the dual path to invent a fake
//! one, which is how the second definition got written in the first place.
//!
//! A NOTE ON WHERE THIS IS HEADED. Every rule below is a veto, and vetoes
//! are unbounded: there is always another way for a real cue to look wrong,
//! which is why this list grew by accretion and why each addition can
//! overrule the geometric signal. The intended direction is to let the
//! document's own roster carry more of the decision, so a token the script
//! repeatedly treats as a speaker is a cue regardless of shape. `rescueCues`
//! is that idea already, positioned as a mop-up. Unifying first is what
//! makes that change measurable: one definition to move, one corpus diff to
//! read.

use std::sync::LazyLock;

use regex::Regex;

use crate::js;

/// Names longer than this are prose that drifted into the cue band.
const MAX_LENGTH: usize = 50;
/// The same, measured after extensions like "(CONT'D)" are stripped.
const MAX_NAME_LENGTH: usize = 30;

// Allows shared cues (MARGO/DEV), numbered (COP #2), and paired (MOM & DAD).
static CHARACTER_NAME: LazyLock<Regex> =
    LazyLock::new(|| js::re(r"^[A-Z][A-Z0-9\s'’\/&#.-]*(\s*\([^)]+\))*\.{0,3}$"));
static COMPANY_NAME: LazyLock<Regex> =
    LazyLock::new(|| js::re_i(r"\b(LLC|LLP|INC|CORP|CO|LTD)\.?$"));
/// Sentence punctuation a cue never carries.
static PUNCTUATION_EXCLUDE: LazyLock<Regex> = LazyLock::new(|| js::re(r"[!?;,]"));
// The closing period is optional: writers routinely type "(O.S)" for
// "(O.S.)", and a script can spell the SAME speaker both ways. Without the
// `\.?` the unpunctuated form misses this pattern, then trips the
// "periods only in ellipsis" guard below — so the cue and the speech under
// it both fall through to action.
static DIALOGUE_EXTENSIONS: LazyLock<Regex> = LazyLock::new(|| {
    js::re_i(r"\((?:V\.O\.?|O\.S\.?|O\.C\.?|CONT'D|CONT\.|INTO PHONE|FILTERED|PRE-LAP)\)")
});
/// Every trailing parenthetical on a cue: "JACK (V.O.) (CONT'D)" -> "JACK".
/// classify.rs strips a cue's name with this same one.
pub static CHARACTER_EXTENSIONS: LazyLock<Regex> = LazyLock::new(|| js::re(r"(\s*\([^)]+\))+\s*$"));
static ABBREV_CHAIN: LazyLock<Regex> = LazyLock::new(|| js::re(r"^(?:[A-Z0-9]{1,4}\.)+$"));
static INITIALS_ONLY: LazyLock<Regex> = LazyLock::new(|| js::re(r"^(?:[A-Z0-9]\.)+$"));

/// `text.replace(CHARACTER_EXTENSIONS, '')`: the cue with every trailing
/// parenthetical dropped. The pattern is anchored at the end, so the one
/// match JavaScript's global replace finds is the only one there is.
pub fn strip_extensions(text: &str) -> String {
    CHARACTER_EXTENSIONS.replace(text, "").into_owned()
}

fn count(s: &str, f: impl Fn(char) -> bool) -> usize {
    s.chars().filter(|&c| f(c)).count()
}

/// Is this text shaped like a character cue? Says nothing about where it
/// sits — see the note above.
pub fn is_cue_text(text: &str) -> bool {
    let t = js::trim(text);
    if t.is_empty() || js::len(t) > MAX_LENGTH {
        return false;
    }
    if PUNCTUATION_EXCLUDE.is_match(t) {
        return false;
    }

    // A cue wearing an extension is judged on the name alone: "JACK (CONT'D)"
    // is a cue even though the parenthetical is mixed case.
    if DIALOGUE_EXTENSIONS.is_match(&js::ci(t)) {
        let name_only = strip_extensions(t);
        let name_only = js::trim(&name_only);
        let upper = count(name_only, |c| c.is_ascii_uppercase());
        let letters = count(name_only, |c| c.is_ascii_alphabetic());
        #[allow(clippy::cast_precision_loss)]
        return letters > 0 && upper as f64 / letters as f64 > 0.7;
    }

    let upper = count(t, |c| c.is_ascii_uppercase());
    let letters = count(t, |c| c.is_ascii_alphabetic());
    #[allow(clippy::cast_precision_loss)]
    if letters == 0 || (upper as f64 / letters as f64) < 0.8 {
        return false;
    }

    // Periods: legitimate only in abbreviation position. Mid-name a period may
    // cap a 1-4 letter run ("MR. SMITH", "E.B. WHITE", "CAPT. MILLER"); at the
    // END only single-letter initials qualify ("ANNA B.", "J.J."). A period
    // closing a longer final word is sentence punctuation, i.e. all-caps
    // action prose drifting into the cue band. This discrimination matters at
    // the dialogue/cue band overlap: a shouted "STOP." must not become a
    // phantom speaker that swallows the next line as its speech (registry 9e).
    let undotted = t.replace("...", " ");
    if undotted.contains('.') {
        let tokens = js::split_ws(js::trim(&undotted));
        for (i, tok) in tokens.iter().enumerate() {
            if !tok.contains('.') {
                continue;
            }
            let shape: &Regex = if i == tokens.len() - 1 {
                &INITIALS_ONLY
            } else {
                &ABBREV_CHAIN
            };
            if !shape.is_match(tok) {
                return false;
            }
        }
    }

    if !CHARACTER_NAME.is_match(t) {
        return false;
    }
    if COMPANY_NAME.is_match(&js::ci(t)) {
        return false;
    }
    js::len(js::trim(&strip_extensions(t))) <= MAX_NAME_LENGTH
}

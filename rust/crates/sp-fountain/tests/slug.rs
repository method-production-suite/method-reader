//! The token half of tests/epub.test.ts's "PRIMARY_SLUG stays in lockstep
//! with fountain-js" (its XHTML half, and the classify.ts pin, belong to
//! sp-epub and sp-parser, which import `PRIMARY_SLUG_SOURCE` from here).

use sp_fountain::{PRIMARY_SLUG_SOURCE, is_mini_slug, is_primary_slug, tokenize};
use sp_model::TokenType;

const CASES: &[(&str, bool)] = &[
    ("INT. HOUSE - DAY", true),
    ("INT HOUSE - DAY", true),
    ("EXT. YARD - NIGHT", true),
    ("EST. THE HOUSE", true),
    ("I/E. CAR - DAY", true),
    ("I/E CAR - DAY", true),
    ("INT./EXT. CAR - DAY", true),
    ("INT/EXT CAR - DAY", true),
    ("LATER", false),
    ("THE BACK ROOM", false),
    ("ESTABLISHING SHOT", false),
    ("INTO THE WOODS", false),
    ("BACK TO:", false),
];

#[test]
fn primary_slug_stays_in_lockstep_with_fountain_js() {
    for &(text, promotes) in CASES {
        let unforced = tokenize(&format!("{text}\n\nAction.\n")).unwrap();
        assert_eq!(
            unforced[0].kind == TokenType::SceneHeading,
            promotes,
            "{text} unforced"
        );
        // Forced, every line is a scene heading; only the real openers stay
        // headings, the rest are mini-slugs.
        let forced = tokenize(&format!(".{text}\n\nAction.\n")).unwrap();
        assert_eq!(forced[0].kind, TokenType::SceneHeading, "{text} forced");
        assert_eq!(is_mini_slug(&forced[0]), !promotes, "{text} mini-slug");
        assert_eq!(is_primary_slug(text), promotes, "{text} primary");
    }
}

#[test]
fn primary_slug_is_fountain_js_first_alternative_opener() {
    // The literal classify.ts must carry too.
    assert_eq!(
        PRIMARY_SLUG_SOURCE,
        r"^(?:\*{0,3}_?)?(?:(?:int|i)\.?\/(?:ext|e)|int|ext|est)[. ]"
    );
}

#[test]
fn only_scene_headings_are_mini_slugs() {
    for t in tokenize("LATER\n\n@BOB\nHi.\n").unwrap() {
        assert!(!is_mini_slug(&t), "{:?}", t.kind);
    }
}

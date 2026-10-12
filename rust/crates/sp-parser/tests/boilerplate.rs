//! Port of tests/boilerplate.test.ts.
//!
//! Every test ports as it stands: `isBoilerplateLine` and
//! `suppressBoilerplate` are pure over strings and hand-built elements.

use sp_model::{ElementType, ScreenplayElement};
use sp_parser::boilerplate::{is_boilerplate_line, suppress_boilerplate};

fn el(id: &str, text: &str, page_num: u32, kind: ElementType) -> ScreenplayElement {
    ScreenplayElement {
        kind,
        text: text.to_owned(),
        id: id.to_owned(),
        page_num,
        is_title_page: false,
        is_readable: true,
        character: None,
        base_character: None,
        age_modifier: None,
        age_value: None,
        scene_number: None,
        styled_text: None,
        dual_right: None,
        fmt: None,
        img: None,
        y: None,
    }
}

fn action(id: &str, text: &str, page_num: u32) -> ScreenplayElement {
    el(id, text, page_num, ElementType::Action)
}

mod is_boilerplate_line_revision_slugs {
    use super::is_boilerplate_line;

    #[test]
    fn amas_literal_doubled_slug_with_page_mark() {
        assert!(is_boilerplate_line(
            "Blue Rev. (mm/dd/yy)Blue Rev. (mm/dd/yy) 15.15"
        ));
    }

    #[test]
    fn single_slug_variants() {
        assert!(is_boilerplate_line("Blue Rev. (6/12/26)"));
        assert!(is_boilerplate_line("Pink Revision 06/12/2026"));
        assert!(is_boilerplate_line("2nd Blue Rev. (6/12/26) 22."));
        assert!(is_boilerplate_line("Goldenrod Rev."));
    }

    #[test]
    fn draft_stamps_and_header_dates() {
        assert!(is_boilerplate_line("FIRST DRAFT"));
        assert!(is_boilerplate_line("SHOOTING SCRIPT 3/4/26"));
        assert!(is_boilerplate_line("PRODUCTION DRAFT — 03/04/2026"));
        assert!(is_boilerplate_line("6/12/26"));
    }

    #[test]
    fn trailing_page_marks_alone() {
        assert!(is_boilerplate_line("15.15"));
        assert!(is_boilerplate_line("A15."));
    }

    #[test]
    fn near_misses_must_not_match() {
        assert!(!is_boilerplate_line("Blue reveals the knife slowly."));
        assert!(!is_boilerplate_line(
            "The first draft of her letter sat unread."
        ));
        assert!(!is_boilerplate_line(
            "It happened on 6/12/26, the day everything changed."
        ));
        assert!(!is_boilerplate_line(
            "He counted: 15.15 seconds exactly, then jumped."
        ));
    }

    #[test]
    fn bare_color_words_must_not_match_color_named_character_cues() {
        assert!(!is_boilerplate_line("CHERRY"));
        assert!(!is_boilerplate_line("BLUE"));
        assert!(is_boilerplate_line("Blue Rev."));
    }
}

mod suppress_boilerplate_recurrence {
    use super::{ElementType, ScreenplayElement, action, el, suppress_boilerplate};

    const PAGES: u32 = 97;

    fn watermarked(on_pages: &[u32]) -> Vec<ScreenplayElement> {
        let mut els = Vec::new();
        let mut i = 0;
        for p in 1..=PAGES {
            if on_pages.contains(&p) {
                els.push(action(
                    &format!("w{i}"),
                    "Property of Placeholder Pictures — A. N. Placeholder",
                    p,
                ));
                i += 1;
            }
            els.push(action(
                &format!("a{i}"),
                &format!("Something happens on page {p}."),
                p,
            ));
            i += 1;
        }
        els
    }

    #[test]
    fn watermark_on_40_of_97_pages_is_suppressed() {
        let pages: Vec<u32> = (1..=40).collect();
        let out = suppress_boilerplate(&watermarked(&pages), PAGES);
        assert!(
            out.iter()
                .filter(|e| e.text.starts_with("Property of"))
                .all(|e| e.kind == ElementType::PageNumber)
        );
        assert!(
            out.iter()
                .filter(|e| e.text.starts_with("Something"))
                .all(|e| e.kind == ElementType::Action)
        );
    }

    #[test]
    fn two_page_recurrence_is_kept() {
        let out = suppress_boilerplate(&watermarked(&[1, 2]), PAGES);
        assert!(
            out.iter()
                .filter(|e| e.text.starts_with("Property of"))
                .all(|e| e.kind == ElementType::Action)
        );
    }

    #[test]
    fn repeated_dialogue_is_never_suppressed() {
        let els: Vec<ScreenplayElement> = (0..50)
            .map(|p| {
                let mut e = el(&format!("d{p}"), "REDRUM", p + 1, ElementType::Dialogue);
                e.character = Some("DANNY".to_owned());
                e
            })
            .collect();
        let out = suppress_boilerplate(&els, PAGES);
        assert!(out.iter().all(|e| e.kind == ElementType::Dialogue));
    }

    #[test]
    fn long_repeated_action_lines_are_kept_length_cap() {
        let long = "The same long ritual description repeats in this script for deliberate effect, well over the cap.";
        let els: Vec<ScreenplayElement> = (0..50)
            .map(|p| action(&format!("l{p}"), long, p + 1))
            .collect();
        assert!(
            suppress_boilerplate(&els, PAGES)
                .iter()
                .all(|e| e.kind == ElementType::Action)
        );
    }

    #[test]
    fn pattern_layer_slugs_with_per_page_varying_page_marks_are_suppressed_without_recurrence() {
        // Ama's literal: the embedded page mark changes every page, so each
        // page's text is unique and the recurrence layer alone never fires.
        let els: Vec<ScreenplayElement> = (0..PAGES)
            .map(|k| {
                action(
                    &format!("s{k}"),
                    &format!("Blue Rev. (6/12/26) {}.{}", k + 1, k + 1),
                    k + 1,
                )
            })
            .collect();
        let out = suppress_boilerplate(&els, PAGES);
        assert!(out.iter().all(|e| e.kind == ElementType::PageNumber));
    }

    #[test]
    fn a_recurring_mini_slug_is_suppressed_exactly_like_action() {
        // Classification runs first, so a left-flush all-caps watermark can reach
        // here already typed as a mini-slug. One rule for both types: a document
        // gets ONE verdict per watermark, and a mark that types action on some
        // pages and mini-slug on others cannot come out half hidden. #5b.
        let els: Vec<ScreenplayElement> = (0..50)
            .map(|p| {
                el(
                    &format!("w{p}"),
                    "CONFIDENTIAL",
                    p + 1,
                    ElementType::MiniSlug,
                )
            })
            .collect();
        assert!(
            suppress_boilerplate(&els, PAGES)
                .iter()
                .all(|e| e.kind == ElementType::PageNumber)
        );
    }

    #[test]
    fn a_mini_slug_under_the_threshold_keeps_its_heading() {
        let els: Vec<ScreenplayElement> = (0..3)
            .map(|p| el(&format!("s{p}"), "LATER", p + 1, ElementType::MiniSlug))
            .collect();
        assert!(
            suppress_boilerplate(&els, PAGES)
                .iter()
                .all(|e| e.kind == ElementType::MiniSlug)
        );
    }

    #[test]
    fn a_mixed_type_watermark_gets_one_verdict_not_two() {
        // The pool must see what it saw before classify.ts learned to mint
        // mini-slugs, or the same watermark typed action on one page and
        // mini-slug on the next stops reaching the threshold — and both halves
        // must land on the same type, or the same mark reads hidden on 39 pages
        // and visible on the 40th.
        let mut els: Vec<ScreenplayElement> = (0..39)
            .map(|p| {
                el(
                    &format!("m{p}"),
                    "PROPERTY OF THE STUDIO",
                    p + 1,
                    ElementType::MiniSlug,
                )
            })
            .collect();
        els.push(action("a1", "PROPERTY OF THE STUDIO", 40));
        assert!(
            suppress_boilerplate(&els, PAGES)
                .iter()
                .all(|e| e.kind == ElementType::PageNumber)
        );
    }

    #[test]
    fn idempotent_for_mini_slugs_too() {
        let els: Vec<ScreenplayElement> = (0..50)
            .map(|p| {
                el(
                    &format!("w{p}"),
                    "CONFIDENTIAL",
                    p + 1,
                    ElementType::MiniSlug,
                )
            })
            .collect();
        let once = suppress_boilerplate(&els, PAGES);
        assert_eq!(suppress_boilerplate(&once, PAGES), once);
    }

    #[test]
    fn idempotent_and_index_preserving() {
        let pages: Vec<u32> = (1..=40).collect();
        let once = suppress_boilerplate(&watermarked(&pages), PAGES);
        let twice = suppress_boilerplate(&once, PAGES);
        assert_eq!(twice, once);
        assert_eq!(
            twice.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
            watermarked(&pages)
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>()
        );
    }
}

//! Port of `src/parser/title-page.ts`.

use std::sync::LazyLock;

use regex::Regex;
use sp_model::{ElementType, ScreenplayElement};

use crate::js;

static BY_LINE: LazyLock<Regex> = LazyLock::new(|| js::re_i(r"\b(by|written by|screenplay by)\b"));
static CONTACT_INFO: LazyLock<Regex> = LazyLock::new(|| {
    js::re_i(r"(email|phone|@|\.com|draft|copyright|©|wga|all rights|production)")
});

/// Detect title pages and front matter.
/// Everything before the first scene heading is analyzed.
/// Page 1: title + author readable, contact info skipped.
/// Pages 2+ before first scene: front matter, all readable.
pub fn detect_title_pages(elements: &[ScreenplayElement]) -> Vec<ScreenplayElement> {
    let mut result = elements.to_vec();
    let first_scene = elements.iter().position(|el| el.kind == ElementType::Scene);
    let Some(first_scene) = first_scene.filter(|&i| i > 0) else {
        return result;
    };

    let mut found_by_line = false;
    for el in result.iter_mut().take(first_scene) {
        if el.page_num == 1 {
            // Title page logic
            if found_by_line {
                // After "by" line: skip contact info
                let is_contact = CONTACT_INFO.is_match(&js::ci(&el.text));
                el.is_title_page = true;
                el.is_readable = !is_contact;
            } else {
                // Title and "by" line are readable
                if BY_LINE.is_match(&js::ci(&el.text)) {
                    found_by_line = true;
                }
                el.is_title_page = true;
                el.is_readable = true;
            }
        } else {
            // Pages 2+ before first scene: front matter (dedications, quotes)
            el.is_title_page = false;
            el.is_readable = true;
        }
    }

    result
}

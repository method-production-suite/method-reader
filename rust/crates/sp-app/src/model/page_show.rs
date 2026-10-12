//! What the page show decides: `desktop/ui/page-show.js`, above its line.

use serde_json::Value;
use unicode_general_category::{GeneralCategory as G, get_general_category};
use unicode_normalization::UnicodeNormalization;

use super::page_timing::{has_page, plan_show};

pub const HEADING: &str = "Converting";
pub const STARTING: &str = "Starting up";
pub const PICTURES: &str = "Reading the pictures";
pub const BUILDING: &str = "Building the book";

/// How long the typed page takes to lift away at the end.
pub const FADE_MS: f64 = 250.0;

/// The page's text from the engine's page line, or None when the line is
/// not one.
pub fn page_paragraphs(line: &Value) -> Option<Vec<String>> {
    if line.get("stage").and_then(Value::as_str) != Some("page") {
        return None;
    }
    Some(
        line.get("paragraphs")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|p| p.as_str().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default(),
    )
}

/// Whether a page line starts a show: Reduce Motion off, "Animate the page"
/// not off, and a page to type.
pub fn can_show(reduced_motion: bool, animate_page: Option<bool>, line: &Value) -> bool {
    if reduced_motion || animate_page == Some(false) {
        return false;
    }
    page_paragraphs(line).is_some_and(|p| has_page(&plan_show(&p)))
}

/// The two switches off an app-settings answer, or None when either is
/// not a boolean.
pub fn page_switches_from(answer: &Value) -> Option<(bool, bool)> {
    Some((
        answer.get("animatePage")?.as_bool()?,
        answer.get("typewriterSounds")?.as_bool()?,
    ))
}

/// Where the slim line is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    pub line: String,
    pub page: u32,
    pub stage: Option<&'static str>,
}

impl Default for Reading {
    fn default() -> Self {
        Self {
            line: STARTING.into(),
            page: 0,
            stage: None,
        }
    }
}

/// The slim line after one progress line; never back a page or a stage.
pub fn reading_line(was: &Reading, line: &Value) -> Reading {
    let stage = line.get("stage").and_then(Value::as_str);
    match stage {
        Some("render") => {
            return Reading {
                line: BUILDING.into(),
                page: was.page,
                stage: Some("render"),
            };
        }
        Some("pictures") => {
            if was.stage == Some("render") {
                return was.clone();
            }
            return Reading {
                line: PICTURES.into(),
                page: was.page,
                stage: Some("pictures"),
            };
        }
        _ => {}
    }
    if stage != Some("parse") || matches!(was.stage, Some("render" | "pictures")) {
        return was.clone();
    }
    let int = |key: &str| {
        line.get(key)
            .and_then(Value::as_f64)
            .filter(|n| n.fract() == 0.0)
    };
    if let (Some(page), Some(pages)) = (int("page"), int("pages"))
        && page >= 1.0
        && page <= pages
    {
        let page = page as u32;
        if page < was.page {
            return was.clone();
        }
        return Reading {
            line: format!("Reading page {page} of {}", pages as u32),
            page,
            stage: Some("parse"),
        };
    }
    let Some(percent) = line.get("percent").and_then(Value::as_f64) else {
        return was.clone();
    };
    if was.page > 0 {
        return was.clone();
    }
    let shown = percent.round().clamp(0.0, 100.0) as u32;
    Reading {
        line: format!("Reading the pages ({shown}%)"),
        page: 0,
        stage: Some("parse"),
    }
}

/// A block's words reduced to what two copies of them share.
pub fn normalise(words: &str) -> String {
    words
        .nfkc()
        .flat_map(char::to_uppercase)
        .filter(|&c| is_letter_or_number(c))
        .collect()
}

fn is_letter_or_number(c: char) -> bool {
    matches!(
        get_general_category(c),
        G::UppercaseLetter
            | G::LowercaseLetter
            | G::TitlecaseLetter
            | G::ModifierLetter
            | G::OtherLetter
            | G::DecimalNumber
            | G::LetterNumber
            | G::OtherNumber
    )
}

fn same_block(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (short, long) = if a.chars().count() <= b.chars().count() {
        (a, b)
    } else {
        (b, a)
    };
    long.starts_with(short) && short.chars().count() * 2 >= long.chars().count()
}

/// How far past the last match the next block may be found.
const LOOK_AHEAD: usize = 4;

/// Which of the book's blocks each typed paragraph is, by its words.
pub fn align_blocks<P: AsRef<str>, B: AsRef<str>>(
    paragraphs: &[P],
    blocks: &[B],
) -> Vec<Option<usize>> {
    let have: Vec<String> = blocks.iter().map(|b| normalise(b.as_ref())).collect();
    let mut out = Vec::new();
    let mut from = 0;
    let mut anchored = false;
    for p in paragraphs {
        let words = normalise(p.as_ref());
        let to = if anchored {
            have.len().min(from + LOOK_AHEAD)
        } else {
            have.len()
        };
        let found = (from..to).find(|&j| same_block(&words, &have[j]));
        out.push(found);
        if let Some(j) = found {
            from = j + 1;
            anchored = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_show_needs_motion_the_switch_and_a_page() {
        let line = json!({"stage": "page", "page": 1, "paragraphs": ["INT. A - DAY"]});
        assert!(can_show(false, None, &line));
        assert!(can_show(false, Some(true), &line));
        assert!(!can_show(true, Some(true), &line));
        assert!(!can_show(false, Some(false), &line));
        assert!(!can_show(
            false,
            None,
            &json!({"stage": "page", "paragraphs": ["", " "]})
        ));
        assert!(!can_show(
            false,
            None,
            &json!({"stage": "parse", "percent": 4})
        ));
    }

    #[test]
    fn the_switches_come_off_an_answer_only_as_booleans() {
        assert_eq!(
            page_switches_from(&json!({"animatePage": true, "typewriterSounds": false})),
            Some((true, false))
        );
        assert_eq!(page_switches_from(&json!({"animatePage": true})), None);
        assert_eq!(
            page_switches_from(&json!({"animatePage": 1, "typewriterSounds": false})),
            None
        );
    }

    #[test]
    fn the_slim_line_names_the_page_and_never_walks_back() {
        let start = Reading::default();
        assert_eq!(start.line, "Starting up");
        let p3 = reading_line(
            &start,
            &json!({"stage":"parse","page":3,"pages":19,"percent":9}),
        );
        assert_eq!(p3.line, "Reading page 3 of 19");
        let back = reading_line(&p3, &json!({"stage":"parse","page":2,"pages":19}));
        assert_eq!(back, p3);
        let share = reading_line(&start, &json!({"stage":"parse","percent":12.4}));
        assert_eq!(share.line, "Reading the pages (12%)");
        assert_eq!(
            reading_line(&p3, &json!({"stage":"parse","percent":40})),
            p3
        );
        let pics = reading_line(&p3, &json!({"stage":"pictures","percent":70}));
        assert_eq!(pics.line, "Reading the pictures");
        let build = reading_line(&pics, &json!({"stage":"render","percent":90}));
        assert_eq!(build.line, "Building the book");
        assert_eq!(reading_line(&build, &json!({"stage":"pictures"})), build);
        assert_eq!(
            reading_line(&build, &json!({"stage":"parse","page":9,"pages":19})),
            build
        );
    }

    #[test]
    fn two_copies_of_a_block_reduce_to_the_same_words() {
        assert_eq!(normalise("MARA (CONT’D)"), "MARACONTD");
        assert_eq!(normalise("“Again?” she says."), "AGAINSHESAYS");
        assert_eq!(normalise("ﬁeld"), "FIELD");
    }

    #[test]
    fn paragraphs_pair_with_the_books_blocks_in_order() {
        let typed = ["MARA", "Again?", "Rain.", "Not in the book at all"];
        let book = ["Title card", "MARA (V.O.)", "Again?", "Rain."];
        assert_eq!(
            align_blocks(&typed, &book),
            vec![Some(1), Some(2), Some(3), None]
        );
        // Half a speech is not the whole speech.
        assert_eq!(
            align_blocks(&["Again"], &["Again and again and again"]),
            vec![None]
        );
        // After the first match, the next is looked for only just after it.
        let far = ["A one", "B", "C", "D", "E", "F", "far away"];
        assert_eq!(
            align_blocks(&["A one", "far away"], &far),
            vec![Some(0), None]
        );
    }
}

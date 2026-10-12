//! What the Read surface decides: `desktop/ui/read.js`, above its line.

/// What the surface has to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaderState {
    Closed,
    Blank,
    Ready,
}

pub fn reader_state(script: Option<&super::convert::Script>) -> ReaderState {
    match script {
        None => ReaderState::Closed,
        Some(s) if s.preview_html.trim().is_empty() => ReaderState::Blank,
        Some(_) => ReaderState::Ready,
    }
}

pub const BLANK_SLUG: &str = "No pages came back";
pub const BLANK_LINE: &str = "The engine converted this script but sent no pages to read. The book itself is fine and can still be sent; converting again is the way to get the pages back.";
pub const BLANK_WAY: &str = "Convert it again";

/// The word the engine's own contents uses for the run before the first
/// slugline.
pub const OPENING: &str = "Opening";

/// A slugline in two parts, cut at the LAST " - " (or en/em dash).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneLabel {
    pub place: String,
    pub time: Option<String>,
}

pub fn scene_label(heading: &str) -> SceneLabel {
    let whole = heading.split_whitespace().collect::<Vec<_>>().join(" ");
    if whole.is_empty() {
        return SceneLabel {
            place: OPENING.into(),
            time: None,
        };
    }
    // The last " X " where X is -, – or —, as a char index into `whole`.
    let chars: Vec<char> = whole.chars().collect();
    let mut cut = None;
    for i in 0..chars.len().saturating_sub(2) {
        if chars[i].is_whitespace()
            && matches!(chars[i + 1], '-' | '–' | '—')
            && chars[i + 2].is_whitespace()
        {
            cut = Some(i);
        }
    }
    let Some(cut) = cut.filter(|&c| c > 0) else {
        return SceneLabel {
            place: whole,
            time: None,
        };
    };
    let place: String = chars[..cut].iter().collect::<String>().trim().to_string();
    let time: String = chars[cut + 3..]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();
    if place.is_empty() || time.is_empty() {
        return SceneLabel {
            place: whole,
            time: None,
        };
    }
    SceneLabel {
        place,
        time: Some(time),
    }
}

/// How far one key press moves the script, in points. `Top`/`Bottom` for
/// Home and End.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scroll {
    By(f32),
    Top,
    Bottom,
}

pub const READER_LINE: f32 = 60.0;
pub const PAGE_SHARE: f32 = 0.9;
pub const PAGE_FLOOR: f32 = 120.0;

/// The keys a reader scrolls the script with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadKey {
    Down,
    Up,
    PageDown,
    PageUp,
    Space,
    Home,
    End,
}

pub fn scroll_step(key: ReadKey, frame_height: f32) -> Scroll {
    let page = if frame_height.is_finite() && frame_height > 0.0 {
        PAGE_FLOOR.max((frame_height * PAGE_SHARE).round())
    } else {
        PAGE_FLOOR
    };
    match key {
        ReadKey::Down => Scroll::By(READER_LINE),
        ReadKey::Up => Scroll::By(-READER_LINE),
        ReadKey::PageDown | ReadKey::Space => Scroll::By(page),
        ReadKey::PageUp => Scroll::By(-page),
        ReadKey::Home => Scroll::Top,
        ReadKey::End => Scroll::Bottom,
    }
}

/// The index control names what the CLICK will do; the state is separate.
pub fn index_toggle(open: bool) -> (&'static str, bool) {
    if open {
        ("Hide scenes", true)
    } else {
        ("Show scenes", false)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailEntry {
    pub id: String,
    pub label: SceneLabel,
}

/// The rail, from the engine's own scenes in the engine's own order: each
/// scene's id and its heading's words (without the page number inside it),
/// as the laid-out book reports them.
pub fn rail_entries(scenes: &[(Mark, String)]) -> Vec<RailEntry> {
    scenes
        .iter()
        .filter(|(m, _)| !m.id.is_empty())
        .map(|(m, heading)| RailEntry {
            id: m.id.clone(),
            label: scene_label(heading),
        })
        .collect()
}

/// The dash a slugline is written with, between the halves.
pub const SEPARATOR: char = '\u{2013}';

pub fn rail_count(total: usize) -> String {
    if total == 1 {
        "1 scene".into()
    } else {
        format!("{total} scenes")
    }
}

/// Where a key moves the index's one stop; stops at the ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailKey {
    Down,
    Up,
    Home,
    End,
}

pub fn rail_step(key: RailKey, at: usize, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match key {
        RailKey::Down => (at + 1).min(count - 1),
        RailKey::Up => at.saturating_sub(1),
        RailKey::Home => 0,
        RailKey::End => count - 1,
    })
}

pub const NO_SCENES: &str = "No scene headings in this script.";

/// One scene, where it is now.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub id: String,
    pub top: f32,
    pub height: f32,
}

/// Where the reader is, as a scene and a fraction into it: survives a
/// reflow, which a pixel offset does not.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub id: String,
    pub into: f32,
}

pub fn reader_place(marks: &[Mark], scroll_top: f32) -> Option<Place> {
    let first = marks.first()?;
    let at = if scroll_top.is_finite() {
        scroll_top.max(0.0)
    } else {
        0.0
    };
    let mut found = first;
    for mark in marks {
        if mark.top <= at {
            found = mark;
        } else {
            break;
        }
    }
    let height = if found.height > 0.0 {
        found.height
    } else {
        1.0
    };
    Some(Place {
        id: found.id.clone(),
        into: ((at - found.top) / height).clamp(0.0, 1.0),
    })
}

/// The same place in the reflowed document; a scene that is gone is the top.
pub fn scroll_target(marks: &[Mark], place: Option<&Place>) -> f32 {
    let Some(place) = place else { return 0.0 };
    let Some(mark) = marks.iter().find(|m| m.id == place.id) else {
        return 0.0;
    };
    let height = mark.height.max(0.0);
    (mark.top + height * place.into).round().max(0.0)
}

/// The page margins a reading device puts round the book: 1.6em top and
/// bottom, 7% each side.
pub const PAGE_MARGIN_BLOCK_EM: f32 = 1.6;
pub const PAGE_MARGIN_INLINE_PCT: f32 = 7.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slugline_reads_as_where_then_when() {
        assert_eq!(
            scene_label("INT./EXT. DELIVERY VAN - MOVING - LATER"),
            SceneLabel {
                place: "INT./EXT. DELIVERY VAN - MOVING".into(),
                time: Some("LATER".into())
            }
        );
        assert_eq!(
            scene_label("EXT. FIELD – NIGHT").time.as_deref(),
            Some("NIGHT")
        );
        assert_eq!(scene_label("INT. HALL").time, None);
        assert_eq!(scene_label("  ").place, OPENING);
        assert_eq!(scene_label(" - DAY").place, "- DAY");
        assert_eq!(scene_label("EXT.   ROOF  -   DAY").place, "EXT. ROOF");
    }

    #[test]
    fn the_keys_move_the_script_by_lines_pages_and_ends() {
        assert_eq!(scroll_step(ReadKey::Down, 500.0), Scroll::By(60.0));
        assert_eq!(scroll_step(ReadKey::Up, 500.0), Scroll::By(-60.0));
        assert_eq!(scroll_step(ReadKey::PageDown, 500.0), Scroll::By(450.0));
        assert_eq!(scroll_step(ReadKey::Space, 500.0), Scroll::By(450.0));
        assert_eq!(scroll_step(ReadKey::PageUp, 100.0), Scroll::By(-120.0));
        assert_eq!(scroll_step(ReadKey::PageDown, f32::NAN), Scroll::By(120.0));
        assert_eq!(scroll_step(ReadKey::Home, 500.0), Scroll::Top);
        assert_eq!(scroll_step(ReadKey::End, 500.0), Scroll::Bottom);
    }

    #[test]
    fn the_index_control_names_the_click() {
        assert_eq!(index_toggle(true), ("Hide scenes", true));
        assert_eq!(index_toggle(false), ("Show scenes", false));
    }

    #[test]
    fn the_rail_counts_and_walks_without_wrapping() {
        assert_eq!(rail_count(1), "1 scene");
        assert_eq!(rail_count(11), "11 scenes");
        assert_eq!(rail_step(RailKey::Down, 2, 3), Some(2));
        assert_eq!(rail_step(RailKey::Up, 0, 3), Some(0));
        assert_eq!(rail_step(RailKey::End, 0, 3), Some(2));
        assert_eq!(rail_step(RailKey::Home, 2, 3), Some(0));
        assert_eq!(rail_step(RailKey::Down, 0, 0), None);
    }

    #[test]
    fn the_rail_comes_from_the_engines_scenes() {
        let mark = |id: &str| Mark {
            id: id.into(),
            top: 0.0,
            height: 1.0,
        };
        let scenes = vec![
            (mark("sc-000"), String::new()),
            (mark("sc-001"), "EXT. FIELD STATION - DAY".to_string()),
            (mark(""), "lost".to_string()),
        ];
        let rail = rail_entries(&scenes);
        assert_eq!(rail.len(), 2);
        assert_eq!(rail[0].label.place, OPENING);
        assert_eq!(rail[1].label.place, "EXT. FIELD STATION");
        assert_eq!(rail[1].label.time.as_deref(), Some("DAY"));
    }

    fn marks() -> Vec<Mark> {
        vec![
            Mark {
                id: "a".into(),
                top: 0.0,
                height: 100.0,
            },
            Mark {
                id: "b".into(),
                top: 100.0,
                height: 200.0,
            },
        ]
    }

    #[test]
    fn the_place_is_a_scene_and_a_fraction_and_survives_a_reflow() {
        let place = reader_place(&marks(), 150.0).unwrap();
        assert_eq!(
            place,
            Place {
                id: "b".into(),
                into: 0.25
            }
        );
        let reflowed = vec![
            Mark {
                id: "a".into(),
                top: 0.0,
                height: 300.0,
            },
            Mark {
                id: "b".into(),
                top: 300.0,
                height: 400.0,
            },
        ];
        assert_eq!(scroll_target(&reflowed, Some(&place)), 400.0);
        assert_eq!(reader_place(&[], 5.0), None);
        assert_eq!(reader_place(&marks(), -10.0).unwrap().into, 0.0);
    }

    #[test]
    fn a_place_whose_scene_is_gone_is_the_top() {
        let gone = Place {
            id: "z".into(),
            into: 0.5,
        };
        assert_eq!(scroll_target(&marks(), Some(&gone)), 0.0);
        assert_eq!(scroll_target(&marks(), None), 0.0);
    }

    #[test]
    fn the_reader_has_three_states() {
        use crate::model::convert::script_from;
        assert_eq!(reader_state(None), ReaderState::Closed);
        let blank = script_from("/a.pdf", &serde_json::json!({"previewHtml": "  "}));
        assert_eq!(reader_state(Some(&blank)), ReaderState::Blank);
        let ready = script_from(
            "/a.pdf",
            &serde_json::json!({"previewHtml": "<html></html>"}),
        );
        assert_eq!(reader_state(Some(&ready)), ReaderState::Ready);
    }
}

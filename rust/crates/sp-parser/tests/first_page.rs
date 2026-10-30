//! Port of tests/first-page.test.ts.
//!
//! While a script converts, the window types out its first real page as plain
//! paragraphs (spec decision 17). Under --progress the engine sends that page
//! once, on stderr, as soon as it has read it. These tests pin the two pure
//! judgements (where paragraphs break, what counts as a title page) and the
//! page the engine picks for the invented Field Station feature.
//!
//! The `--progress` test spawned the CLI. Its parser-level half ports here:
//! the same per-page callback convert.ts hangs on `extractText`, driven from
//! the oracle's dump of tests/fixtures/field-station.pdf, must pick page 2 and
//! give paragraphs that open on the first scene heading.
//!
//! MOVED: "Field Station: page 2, opening on its first scene heading, plain
//! words only": the stderr event stream, the exit codes and the stdout
//! contract with and without --progress need sp-engine convertPdf and the
//! CLI, and port with that crate. The first heading is compared with the
//! oracle's .fountain (the TypeScript engine's) until sp-fountain toFountain
//! can write one.

mod common;

use sp_model::{ElementType, RawLine};
use sp_parser::extract::extract_text;
use sp_parser::first_page::{is_first_real_page, is_title_like, page_paragraphs};

fn line(text: &str, y: f64, indent: f64) -> RawLine {
    RawLine {
        text: text.to_owned(),
        indent,
        y,
        page_num: 2,
        styled: None,
        dual_right: None,
        fonts: None,
        fmt: None,
        placed: false,
    }
}

fn l(text: &str, y: f64) -> RawLine {
    line(text, y, 18.0)
}

mod page_paragraphs {
    use super::{l, line, page_paragraphs};

    #[test]
    fn a_gap_of_twice_the_line_gap_starts_a_new_paragraph_a_wrapped_line_joins_with_one_space() {
        let lines = [
            l("EXT. RIDGE ROAD - DAY", 720.0),
            l(
                "Wind comes down off the ridge in long flat gusts, pushing",
                696.0,
            ),
            l(
                "the grass one way and then, for no reason, the other.",
                684.0,
            ),
            l("A road climbs the ridge.", 660.0),
            l("Nobody has driven it.", 648.0),
        ];
        assert_eq!(
            page_paragraphs(&lines),
            [
                "EXT. RIDGE ROAD - DAY",
                "Wind comes down off the ridge in long flat gusts, pushing the grass one way and then, for no reason, the other.",
                "A road climbs the ridge. Nobody has driven it.",
            ]
        );
    }

    #[test]
    fn a_change_of_indent_with_no_gap_still_starts_a_new_paragraph_so_a_cue_never_runs_into_its_speech()
     {
        let lines = [
            l("He counts.", 564.0),
            line("DELACROIX", 540.0, 44.0),
            line("Thirty-one. Thirty-two.", 528.0, 29.0),
            line("Thirty-three.", 516.0, 29.0),
        ];
        assert_eq!(
            page_paragraphs(&lines),
            [
                "He counts.",
                "DELACROIX",
                "Thirty-one. Thirty-two. Thirty-three."
            ]
        );
    }

    #[test]
    fn a_page_mark_is_page_furniture_not_words_to_type() {
        let lines = [
            line("2.", 756.0, 87.0),
            l("EXT. FIELD STATION - DAY", 720.0),
            l("The hut door opens before", 696.0),
            l("they reach it.", 684.0),
        ];
        assert_eq!(
            page_paragraphs(&lines),
            [
                "EXT. FIELD STATION - DAY",
                "The hut door opens before they reach it."
            ]
        );
    }

    #[test]
    fn a_page_of_short_speeches_where_blank_lines_outnumber_wrapped_ones_still_breaks_a_heading_from_its_action()
     {
        // Every block here is one or two lines, so the 24pt gap between blocks
        // is the commonest gap on the page. The line gap is still 12pt.
        let lines = [
            l("INT. HUT - NIGHT", 720.0),
            l("Rain on the roof. She waits.", 696.0),
            l("Then she moves.", 672.0),
            line("DELACROIX", 648.0, 44.0),
            line("Now.", 636.0, 29.0),
            line("MARGO", 612.0, 44.0),
            line("Not yet.", 600.0, 29.0),
            line("DELACROIX", 576.0, 44.0),
            line("Now, or never, and I mean", 564.0, 29.0),
            line("never.", 552.0, 29.0),
        ];
        assert_eq!(
            page_paragraphs(&lines),
            [
                "INT. HUT - NIGHT",
                "Rain on the roof. She waits.",
                "Then she moves.",
                "DELACROIX",
                "Now.",
                "MARGO",
                "Not yet.",
                "DELACROIX",
                "Now, or never, and I mean never.",
            ]
        );
    }

    #[test]
    fn a_line_that_wraps_at_a_hyphen_joins_without_a_space() {
        let lines = [
            l("A fourteen-", 720.0),
            l("year-old girl.", 708.0),
            l("She waits for the", 684.0),
            l("rain.", 672.0),
        ];
        assert_eq!(
            page_paragraphs(&lines),
            ["A fourteen-year-old girl.", "She waits for the rain."]
        );
    }

    #[test]
    fn an_empty_page_has_no_paragraphs() {
        assert!(page_paragraphs(&[]).is_empty());
    }
}

mod is_title_like {
    use super::{RawLine, is_title_like, l, line};

    const W: f64 = 612.0;

    // Each centred line has its own left edge: the text is centred, so its
    // start moves with its length.
    fn centred(text: &str, y: f64) -> RawLine {
        #[allow(clippy::cast_precision_loss)]
        let width_pct = (sp_parser::js::len(text) as f64 * 7.2 / W) * 100.0;
        line(text, y, 50.0 - width_pct / 2.0)
    }

    #[test]
    fn true_for_a_mostly_centred_6_line_page() {
        let lines = [
            centred("FIELD STATION", 504.0),
            centred("Written by", 468.0),
            centred("A. Writer", 444.0),
            centred("Based on the novel", 408.0),
            centred("by Someone Else", 396.0),
            l("hello@example.com", 96.0),
        ];
        assert!(is_title_like(&lines, W));
    }

    #[test]
    fn false_for_a_left_aligned_30_line_page() {
        let lines: Vec<RawLine> = (0..30)
            .map(|i| {
                l(
                    "Wind comes down off the ridge in long flat gusts, pushing",
                    720.0 - f64::from(i) * 12.0,
                )
            })
            .collect();
        assert!(!is_title_like(&lines, W));
    }

    // Dialogue sits 2.5in to 6in on an 8.5in page, so a speech IS geometrically
    // centred. Shared left edges are what separate it from a title block.
    #[test]
    fn false_for_a_page_of_cues_and_speeches_which_sit_on_the_page_centre_but_share_their_left_edges()
     {
        let mut lines: Vec<RawLine> = Vec::new();
        let mut y = 720.0;
        for _ in 0..6 {
            y -= 24.0;
            lines.push(line("TEO", y, 44.0));
            y -= 12.0;
            lines.push(line("It sings in a north wind. One long", y, 29.0));
            y -= 12.0;
            lines.push(line("note. In a west wind it is two.", y, 29.0));
        }
        assert!(!is_title_like(&lines, W));
    }
}

mod progress_sends_the_first_real_page_once {
    use super::{ElementType, RawLine, common, extract_text, is_first_real_page, page_paragraphs};

    #[derive(Debug)]
    enum Event {
        Page { page: u32, paragraphs: Vec<String> },
        Parse,
    }

    /// The parser-level half of "Field Station: page 2, opening on its first
    /// scene heading, plain words only" (the CLI half is MOVED: see the file
    /// header). The callback is convert.ts's own: the first page that passes
    /// `isFirstRealPage` is sent once as `pageParagraphs`, then every page
    /// ticks the parse stage.
    #[test]
    fn field_station_page_2_opening_on_its_first_scene_heading_plain_words_only() {
        let facts = common::facts("field-station");
        let mut events: Vec<Event> = Vec::new();
        let mut sent = false;
        let mut on_progress =
            |page: u32, _pages: u32, lines: Option<&[RawLine]>, width: Option<f64>| {
                if let (false, Some(lines), Some(width)) = (sent, lines, width)
                    && width != 0.0
                    && is_first_real_page(lines, width)
                {
                    sent = true;
                    events.push(Event::Page {
                        page,
                        paragraphs: page_paragraphs(lines),
                    });
                }
                events.push(Event::Parse);
            };
        let _ = extract_text(&facts, None, Some(&mut on_progress));

        let pages: Vec<(u32, &Vec<String>)> = events
            .iter()
            .filter_map(|e| match e {
                Event::Page { page, paragraphs } => Some((*page, paragraphs)),
                Event::Parse => None,
            })
            .collect();
        assert_eq!(pages.len(), 1);
        let (first_page, paragraphs) = pages[0];

        // Page 1 is the title page; the first page after it is page 2.
        assert_eq!(first_page, 2);

        // The scene heading the Preview page shows first is the .fountain's first one.
        let fountain =
            std::fs::read_to_string(common::oracle("field-station").join("script.fountain"))
                .expect("oracle script.fountain");
        let first_heading = fountain
            .split('\n')
            .find(|l| {
                (l.starts_with("INT") || l.starts_with("EXT"))
                    && matches!(l.as_bytes().get(3), Some(b'.' | b' '))
            })
            .expect("a scene heading in the .fountain");
        assert_eq!(paragraphs[0], first_heading);
        // And the parser's own first scene heading, which the .fountain writes.
        let parsed = common::parse("field-station");
        let first_scene = parsed
            .elements
            .iter()
            .find(|e| e.kind == ElementType::Scene)
            .expect("a scene element");
        assert_eq!(paragraphs[0], first_scene.text);

        assert!(paragraphs.len() > 3);
        for p in paragraphs {
            assert!(!p.contains("**"), "{p:?}");
            assert!(!p.contains('_'), "{p:?}");
            assert!(!p.contains("[["), "{p:?}");
            assert!(!p.contains('\n'), "{p:?}");
        }

        // The page line arrives in the parse stage, before the bar reaches the end.
        let page_at = events
            .iter()
            .position(|e| matches!(e, Event::Page { .. }))
            .expect("a page event");
        assert!(
            events[page_at + 1..]
                .iter()
                .any(|e| matches!(e, Event::Parse))
        );
    }
}

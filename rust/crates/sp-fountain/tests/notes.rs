//! Port of tests/notes.test.ts.

use std::collections::HashSet;

use sp_fountain::jsre::Regex;
use sp_fountain::{
    fmt_classes, fmt_size_step, image_note_files, is_safe_image_file, parse_img_note,
    renders_image, strip_notes, strip_screepub_notes,
};
use sp_model::{ImageRef, SizeStep};

fn img(file: &str, width_pct: f64, full_page: bool) -> Option<ImageRef> {
    Some(ImageRef {
        file: file.to_owned(),
        width_pct,
        full_page,
    })
}

fn set(names: &[&str]) -> HashSet<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

// ── stripNotes ───────────────────────────────────────────

#[test]
fn strip_a_leading_note_and_the_newline_it_sat_on_both_go() {
    assert_eq!(strip_notes("[[fmt: sans]]\nCHYRON: LIVE."), "CHYRON: LIVE.");
}

#[test]
fn strip_an_inline_note_and_its_extra_space_go() {
    assert_eq!(strip_notes("[[fmt: sans]] Read it back."), "Read it back.");
}

#[test]
fn strip_a_note_in_the_middle_of_a_line_goes() {
    assert_eq!(
        strip_notes("He leaves [[check this]] quickly."),
        "He leaves quickly."
    );
}

#[test]
fn strip_newlines_inside_a_multi_line_token_survive() {
    assert_eq!(
        strip_notes("[[fmt: +1]] one\ntwo\nthree"),
        "one\ntwo\nthree"
    );
}

#[test]
fn strip_text_with_no_notes_is_returned_untouched() {
    let s = "  double  spaced  and\n  indented  ";
    assert_eq!(strip_notes(s), s);
}

#[test]
fn strip_an_unterminated_bracket_is_left_alone() {
    assert_eq!(
        strip_notes("He said [[ and stopped"),
        "He said [[ and stopped"
    );
}

// ── fmtClasses ───────────────────────────────────────────

#[test]
fn fmt_classes_a_family_note_yields_its_class() {
    assert_eq!(fmt_classes("[[fmt: sans]]\nX"), " fmt-sans");
}

#[test]
fn fmt_classes_all_four_families_and_all_three_sizes_map() {
    for (word, class) in [
        ("mono", "fmt-mono"),
        ("serif", "fmt-serif"),
        ("sans", "fmt-sans"),
        ("cursive", "fmt-cursive"),
        ("-1", "fmt-minus1"),
        ("+1", "fmt-plus1"),
        ("+2", "fmt-plus2"),
    ] {
        assert_eq!(
            fmt_classes(&format!("[[fmt: {word}]]\nX")),
            format!(" {class}"),
            "{word}"
        );
    }
}

#[test]
fn fmt_classes_family_and_size_together_yield_both() {
    assert_eq!(fmt_classes("[[fmt: sans +2]]\nX"), " fmt-sans fmt-plus2");
}

#[test]
fn fmt_classes_only_a_leading_note_counts() {
    assert_eq!(fmt_classes("Text first [[fmt: sans]]"), "");
}

#[test]
fn fmt_classes_a_non_fmt_note_yields_no_classes() {
    assert_eq!(fmt_classes("[[a production note]]\nX"), "");
}

#[test]
fn fmt_classes_malformed_content_is_ignored_word_by_word() {
    assert_eq!(fmt_classes("[[fmt: teal +9 sans]]\nX"), " fmt-sans");
    assert_eq!(fmt_classes("[[fmt:]]\nX"), "");
    assert_eq!(fmt_classes("[[fmt: ]]\nX"), "");
}

// ── fmtSizeStep ──────────────────────────────────────────

#[test]
fn fmt_size_step_reads_the_size_word_out_of_a_leading_note() {
    assert_eq!(
        fmt_size_step("[[fmt: sans +2]]\nX"),
        Some(SizeStep::MuchLarger)
    );
    assert_eq!(fmt_size_step("[[fmt: -1]]\nX"), Some(SizeStep::Smaller));
}

#[test]
fn fmt_size_step_a_family_only_note_has_none() {
    assert_eq!(fmt_size_step("[[fmt: sans]]\nX"), None);
}

#[test]
fn fmt_size_step_no_note_at_all_has_none() {
    assert_eq!(fmt_size_step("Plain action."), None);
}

// ── parseImgNote ─────────────────────────────────────────

#[test]
fn parse_img_reads_a_filename_and_a_width() {
    assert_eq!(
        parse_img_note("img: p012-01.png w=71"),
        img("p012-01.png", 71.0, false)
    );
}

#[test]
fn parse_img_reads_the_full_page_flag() {
    assert_eq!(
        parse_img_note("img: p001-01.png w=100 full").map(|i| i.full_page),
        Some(true)
    );
}

#[test]
fn parse_img_tolerates_a_missing_width() {
    assert_eq!(
        parse_img_note("img: p003-02.png"),
        img("p003-02.png", 100.0, false)
    );
}

#[test]
fn parse_img_ignores_words_it_does_not_know() {
    assert_eq!(
        parse_img_note("img: a.png w=40 sideways lolcat"),
        img("a.png", 40.0, false)
    );
}

#[test]
fn parse_img_the_words_may_come_in_any_order() {
    assert_eq!(
        parse_img_note("img: a.png full w=71"),
        img("a.png", 71.0, true)
    );
}

#[test]
fn parse_img_a_repeated_width_the_last_one_wins() {
    assert_eq!(
        parse_img_note("img: a.png w=20 w=80").map(|i| i.width_pct),
        Some(80.0)
    );
}

#[test]
fn parse_img_is_not_a_note_of_another_kind() {
    assert_eq!(parse_img_note("fmt: sans +1"), None);
    assert_eq!(parse_img_note("a normal writer note"), None);
}

#[test]
fn parse_img_the_keyword_is_case_sensitive() {
    assert_eq!(parse_img_note("IMG: p001-01.png w=71"), None);
    assert_eq!(parse_img_note("Img: p001-01.png"), None);
}

#[test]
fn parse_img_an_uppercase_extension_is_not_a_png() {
    assert_eq!(parse_img_note("img: p001-01.PNG"), None);
}

#[test]
fn parse_img_a_note_that_is_only_the_keyword_yields_nothing() {
    assert_eq!(parse_img_note("img:"), None);
    assert_eq!(parse_img_note("img: "), None);
    assert_eq!(parse_img_note("img:   w=71"), None);
}

#[test]
fn parse_img_refuses_path_traversal() {
    assert_eq!(parse_img_note("img: ../../etc/passwd.png"), None);
    assert_eq!(parse_img_note("img: ..%2f..%2fx.png"), None);
    assert_eq!(parse_img_note("img: ..png"), img("..png", 100.0, false));
}

#[test]
fn parse_img_refuses_separators_of_both_kinds_and_absolute_paths() {
    assert_eq!(parse_img_note("img: sub/dir.png"), None);
    assert_eq!(parse_img_note("img: sub\\dir.png"), None);
    assert_eq!(parse_img_note("img: /tmp/x.png"), None);
}

#[test]
fn parse_img_the_allowlist_is_anchored_at_the_end() {
    assert_eq!(parse_img_note("img: a.png/../../x"), None);
    assert_eq!(parse_img_note("img: a.png.exe"), None);
    assert_eq!(parse_img_note("img: a.png\\..\\x"), None);
    assert_eq!(parse_img_note("img: a.png%2f..%2fx"), None);
}

#[test]
fn parse_img_refuses_anything_that_is_not_a_png() {
    assert_eq!(parse_img_note("img: script.fountain"), None);
    assert_eq!(parse_img_note("img: payload.svg"), None);
    assert_eq!(parse_img_note("img: .png"), None);
}

#[test]
fn parse_img_refuses_an_absurdly_long_name() {
    assert_eq!(
        parse_img_note(&format!("img: {}.png", "a".repeat(200))),
        None
    );
}

#[test]
fn parse_img_the_length_bound_is_on_the_stem_and_it_is_64() {
    let ok = format!("{}.png", "a".repeat(64));
    assert_eq!(
        parse_img_note(&format!("img: {ok}")).map(|i| i.file),
        Some(ok)
    );
    assert_eq!(
        parse_img_note(&format!("img: {}.png", "a".repeat(65))),
        None
    );
}

#[test]
fn parse_img_clamps_a_nonsense_width() {
    assert_eq!(
        parse_img_note("img: a.png w=9999").map(|i| i.width_pct),
        Some(100.0)
    );
    assert_eq!(
        parse_img_note("img: a.png w=-5").map(|i| i.width_pct),
        Some(5.0)
    );
}

#[test]
fn parse_img_a_width_too_large_for_a_double_is_still_clamped() {
    let big = "9".repeat(400);
    assert_eq!(
        parse_img_note(&format!("img: a.png w={big}")).map(|i| i.width_pct),
        Some(100.0)
    );
    assert_eq!(
        parse_img_note(&format!("img: a.png w=-{big}")).map(|i| i.width_pct),
        Some(5.0)
    );
}

#[test]
fn parse_img_a_width_that_is_not_an_integer_is_not_a_width() {
    for note in ["img: a.png w=abc", "img: a.png w=71.5", "img: a.png w="] {
        assert_eq!(parse_img_note(note).map(|i| i.width_pct), Some(100.0));
    }
}

#[test]
fn parse_img_every_filename_the_parser_mints_survives_its_own_allowlist() {
    for f in ["p001-01.png", "p999-99.png", "p1000-01.png"] {
        assert_eq!(
            parse_img_note(&format!("img: {f} w=71")).map(|i| i.file),
            Some(f.to_owned()),
            "{f}"
        );
    }
}

// ── isSafeImageFile ──────────────────────────────────────

#[test]
fn safe_file_refuses_a_name_with_anything_around_it() {
    for name in ["x.png\n", "\nx.png", "x.png\0", "x.png ", "a.png\n../../b"] {
        assert!(!is_safe_image_file(name), "{name:?}");
    }
}

#[test]
fn safe_file_accepts_the_names_this_engine_writes() {
    assert!(is_safe_image_file("p001-01.png"));
}

// ── rendersImage ─────────────────────────────────────────

#[test]
fn renders_a_note_that_passes_every_gate() {
    let have = set(&["p001-01.png", "p002-01.png"]);
    assert_eq!(
        renders_image("img: p001-01.png w=42", true, &have),
        img("p001-01.png", 42.0, false)
    );
    assert_eq!(
        renders_image("img: p002-01.png w=100 full", true, &have),
        img("p002-01.png", 100.0, true)
    );
}

#[test]
fn renders_nothing_with_the_option_off() {
    let have = set(&["p001-01.png", "p002-01.png"]);
    assert_eq!(renders_image("img: p001-01.png w=42", false, &have), None);
}

#[test]
fn renders_nothing_for_a_hostile_name_even_if_the_set_holds_it() {
    let poisoned = set(&["p001-01.png", "p002-01.png", "../../x.png", "a/b.png"]);
    assert_eq!(
        renders_image("img: ../../x.png w=42", true, &poisoned),
        None
    );
    assert_eq!(renders_image("img: a/b.png w=42", true, &poisoned), None);
}

#[test]
fn renders_nothing_for_a_name_the_caller_cannot_ship() {
    let have = set(&["p001-01.png", "p002-01.png"]);
    assert_eq!(renders_image("img: p009-01.png w=42", true, &have), None);
    assert_eq!(
        renders_image("img: p001-01.png w=42", true, &HashSet::new()),
        None
    );
}

#[test]
fn renders_nothing_for_a_note_that_is_not_a_picture_note() {
    let have = set(&["p001-01.png", "p002-01.png"]);
    assert_eq!(renders_image("fmt: sans +1", true, &have), None);
    assert_eq!(renders_image("a writer's note", true, &have), None);
}

// ── imageNoteFiles ───────────────────────────────────────

#[test]
fn image_note_files_names_every_picture_note_once_each() {
    let text = [
        "INT. ROOM - DAY",
        "[[img: p001-01.png w=40]]",
        "A line of action.",
        "[[img: p002-01.png w=100 full]]",
        "[[img: p001-01.png w=40]]",
    ]
    .join("\n\n");
    let mut files = image_note_files(&text).unwrap();
    files.sort();
    assert_eq!(files, vec!["p001-01.png", "p002-01.png"]);
}

#[test]
fn image_note_files_a_note_no_renderer_would_show_asks_for_nothing() {
    let text = [
        "INT. ROOM - DAY",
        "[[fmt: sans]]",
        "[[a writer's note]]",
        "[[img: ../x.png w=40]]",
        "[[img: p003-01.jpg w=40]]",
        "A line with [[img: p004-01.png w=40]] folded into it.",
        "/* [[img: p005-01.png w=40]] */",
    ]
    .join("\n\n");
    assert!(image_note_files(&text).unwrap().is_empty());
}

// ── stripScreepubNotes ───────────────────────────────────

fn cached() -> String {
    [
        "Title: Field Notes",
        "Author: Somebody",
        "",
        "INT. HUT - NIGHT",
        "",
        "[[fmt: sans +1]]",
        "CHYRON: LIVE.",
        "",
        "@MARA",
        "(quietly)",
        "[[fmt: mono]] Read it back.",
        "Slower.",
        "",
        "[[img: p002-01.png w=71]]",
        "",
        "She looks up. [[a writer's own note]]",
        "",
        "[[img: p003-01.png w=100 full]]",
        "",
    ]
    .join("\n")
}

#[test]
fn strip_screepub_drops_the_notes_the_serializer_wrote_and_nothing_else() {
    let want = [
        "Title: Field Notes",
        "Author: Somebody",
        "",
        "INT. HUT - NIGHT",
        "",
        "CHYRON: LIVE.",
        "",
        "@MARA",
        "(quietly)",
        "Read it back.",
        "Slower.",
        "",
        "She looks up. [[a writer's own note]]",
        "",
    ]
    .join("\n");
    assert_eq!(strip_screepub_notes(&cached()), want);
}

#[test]
fn strip_screepub_keeps_every_other_line_byte_for_byte_in_order() {
    let ours = Regex::new(r"\[\[\s*(fmt|img):", "");
    let stripped = strip_screepub_notes(&cached());
    let kept: Vec<&str> = stripped
        .split('\n')
        .filter(|l| !l.is_empty() && *l != "Read it back.")
        .collect();
    let source = cached();
    let original: Vec<&str> = source
        .split('\n')
        .filter(|l| !l.is_empty() && !ours.test(l))
        .collect();
    assert_eq!(kept, original);
}

#[test]
fn strip_screepub_text_with_no_note_of_ours_is_returned_identical() {
    let plain = "INT. HUT - NIGHT\n\n[[a writer's note]]\n\n[[FMT: not ours]]\n\nShe waits.\n";
    assert_eq!(strip_screepub_notes(plain), plain);
}

#[test]
fn strip_screepub_a_picture_note_opening_the_file_leaves_no_blank_line() {
    assert_eq!(
        strip_screepub_notes("[[img: p001-01.png w=80]]\n\nINT. HUT - NIGHT\n"),
        "INT. HUT - NIGHT\n"
    );
}

#[test]
fn strip_screepub_crlf_lines_keep_their_own_endings() {
    assert_eq!(
        strip_screepub_notes("INT. HUT\r\n\r\n[[fmt: sans]]\r\nCHYRON.\r\n"),
        "INT. HUT\r\n\r\nCHYRON.\r\n"
    );
}

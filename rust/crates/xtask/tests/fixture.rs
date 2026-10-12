//! `cargo xtask make-fixture`, the port of tools/make-fixture.py.
//!
//! Ports tests/fixture-stability.test.ts (6), tests/torture-layout.test.ts
//! (4) and tests/torture-markup.test.ts (10), plus the checks that pin what
//! those three took from Python for free: json.dumps's exact text and
//! textwrap's hyphen rules.

use std::path::PathBuf;

use serde_json::Value;
use xtask::fixture;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

/// A scratch folder of this test's own, removed when it drops.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "screepub-xtask-fixture-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch folder");
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---- tests/fixture-stability.test.ts ------------------------------------
//
// The committed fixtures are generated, not hand-made, and every kind
// shares its layout and PDF-emission code. Without this, a refactor for one
// kind could silently change screenplay.pdf and every integration test
// would quietly start asserting against different input. (torture.pdf is
// not pinned here; its placement is checked through --emit-layout.)
//
// images.pdf's six pictures are deflated by zlib-rs at level 9, whose
// stream is byte-identical to zlib 1.2.12's, the zlib the fixture was
// authored with. If images.pdf ALONE fails here, suspect the compressor
// (a zlib-rs bump) before the emitter: inflate both sides and compare the
// pixels, which are what the file promises.

fn regenerates_byte_for_byte(kind: &str, committed: &str) {
    let scratch = Scratch::new(kind);
    let out = scratch.0.join(format!("{kind}.pdf"));
    let outcome = fixture::run(&args(&[kind, out.to_str().unwrap()]));
    assert_eq!(outcome.stderr, "");
    assert_eq!(outcome.code, 0);
    let made = std::fs::read(&out).expect("the generator wrote its PDF");
    let want = std::fs::read(xtask::paths::repo_root().join(committed)).unwrap();
    assert!(
        made == want,
        "{kind}: {} bytes made, {} committed in {committed}",
        made.len(),
        want.len()
    );
}

#[test]
fn stability_screenplay() {
    regenerates_byte_for_byte("screenplay", "tests/fixtures/screenplay.pdf");
}

#[test]
fn stability_prose() {
    regenerates_byte_for_byte("prose", "tests/fixtures/prose.pdf");
}

#[test]
fn stability_blank() {
    regenerates_byte_for_byte("blank", "tests/fixtures/blank-pages.pdf");
}

#[test]
fn stability_demo() {
    // The invented feature the README and site pictures show.
    regenerates_byte_for_byte("demo", "tests/fixtures/field-station.pdf");
}

#[test]
fn stability_images() {
    regenerates_byte_for_byte("images", "tests/fixtures/images.pdf");
}

#[test]
fn stability_jpx() {
    // No deflate at all: its J2K codestream is copied through verbatim, so
    // a failure here is the emitter.
    regenerates_byte_for_byte("jpx", "tests/fixtures/jpx.pdf");
}

// ---- tests/torture-layout.test.ts ---------------------------------------

fn emit_layout(kind: &str) -> Vec<Value> {
    let outcome = fixture::run(&args(&["--emit-layout", kind]));
    assert_eq!(outcome.code, 0, "make-fixture failed: {}", outcome.stderr);
    let parsed: Value = serde_json::from_str(&outcome.stdout).expect("JSON");
    parsed.as_array().expect("a list of pages").clone()
}

#[test]
fn emit_layout_screenplay_is_several_pages_numbered_from_1() {
    let pages = emit_layout("screenplay");
    assert!(pages.len() > 1);
    assert_eq!(pages[0]["page"], 1);
    for (i, page) in pages.iter().enumerate() {
        assert_eq!(page["page"].as_u64(), Some(i as u64 + 1));
    }
}

#[test]
fn emit_layout_every_row_has_line_x_and_a_run() {
    for page in emit_layout("screenplay") {
        for row in page["rows"].as_array().unwrap() {
            assert!(row["line"].is_number());
            assert!(row["x"].is_number());
            assert!(!row["runs"].as_array().unwrap().is_empty());
        }
    }
}

#[test]
fn emit_layout_title_page_carries_the_invented_author() {
    let first = &emit_layout("screenplay")[0];
    let texts: Vec<&str> = first["rows"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["runs"].as_array().unwrap())
        .map(|run| run["text"].as_str().unwrap())
        .collect();
    assert!(texts.contains(&"A. N. Placeholder"));
}

#[test]
fn atline_raises_when_the_page_is_already_past_the_line() {
    // Silently sliding past an anchor would turn the (MORE) test into a test
    // of nothing the first time content above it grew by a sentence.
    let outcome = fixture::run(&args(&["--atline-overflow-check"]));
    assert_ne!(outcome.code, 0);
    assert!(outcome.stderr.contains("already at line"));
    assert!(outcome.stderr.contains("shorten it, or move the anchor"));
}

// ---- tests/torture-markup.test.ts ---------------------------------------

type Spans = Vec<(u64, u64, String)>;

/// (plain, spans), or the error text.
fn parse_markup(s: &str) -> Result<(String, Spans), String> {
    let outcome = fixture::run(&args(&["--parse-markup", s]));
    if outcome.code != 0 {
        return Err(outcome.stderr.trim().to_string());
    }
    let v: Value = serde_json::from_str(&outcome.stdout).unwrap();
    let spans = v["spans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s[0].as_u64().unwrap(),
                s[1].as_u64().unwrap(),
                s[2].as_str().unwrap().to_string(),
            )
        })
        .collect();
    Ok((v["plain"].as_str().unwrap().to_string(), spans))
}

#[test]
fn markup_strips_markers_and_records_plain_offsets() {
    let (plain, spans) = parse_markup("a {b}bold{/b} c").unwrap();
    assert_eq!(plain, "a bold c");
    assert_eq!(spans, vec![(2, 6, "b".to_string())]);
}

#[test]
fn markup_two_styles_over_one_run() {
    let (plain, spans) = parse_markup("{b}{i}both{/i}{/b}").unwrap();
    assert_eq!(plain, "both");
    let mut styles: Vec<String> = spans.into_iter().map(|s| s.2).collect();
    styles.sort();
    assert_eq!(styles, ["b", "i"]);
}

#[test]
fn markup_plain_text_is_unchanged() {
    let (plain, spans) = parse_markup("plain text").unwrap();
    assert_eq!(plain, "plain text");
    assert!(spans.is_empty());
}

#[test]
fn markup_unclosed_marker_is_an_error() {
    // A typo buried in pages of content must fail at generation.
    assert!(parse_markup("a {b}bold").is_err());
}

#[test]
fn markup_unmatched_closer_is_an_error() {
    assert!(parse_markup("a bold{/b}").is_err());
}

type Line = Vec<(Vec<String>, String)>;

fn wrap_spans(s: &str, width: usize) -> Vec<Line> {
    let outcome = fixture::run(&args(&["--wrap", &width.to_string(), s]));
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    let v: Value = serde_json::from_str(&outcome.stdout).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|line| {
            line.as_array()
                .unwrap()
                .iter()
                .map(|run| {
                    let styles = run["styles"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| s.as_str().unwrap().to_string())
                        .collect();
                    (styles, run["text"].as_str().unwrap().to_string())
                })
                .collect()
        })
        .collect()
}

#[test]
fn wrap_styled_span_survives_a_line_break() {
    // The case a naive wrap of the marked-up string gets wrong: it would
    // split "{b}" across the break or count its 3 characters as width.
    let lines = wrap_spans("aaa {b}bbb ccc{/b} ddd", 7);
    let bolded: Vec<String> = lines
        .iter()
        .flatten()
        .filter(|r| r.0.iter().any(|s| s == "b"))
        .map(|r| r.1.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    assert_eq!(bolded, ["bbb", "ccc"]);
}

#[test]
fn wrap_ignores_marker_characters_when_measuring() {
    // Plain text is 11 characters, so width 11 is ONE line, though the
    // marked-up string is 18 long.
    assert_eq!(wrap_spans("aaa {b}bbb{/b} ccc", 11).len(), 1);
}

#[test]
fn wrap_unstyled_text_breaks_on_words() {
    let texts: Vec<String> = wrap_spans("one two three four", 8)
        .iter()
        .map(|l| l.iter().map(|r| r.1.as_str()).collect())
        .collect();
    assert_eq!(texts, ["one two", "three", "four"]);
}

#[test]
fn wrap_merges_equal_styled_neighbours() {
    let lines = wrap_spans("{b}bold{/b}", 20);
    assert_eq!(lines[0].len(), 1);
    assert_eq!(lines[0][0], (vec!["b".to_string()], "bold".to_string()));
}

#[test]
fn wrap_carries_underline_as_a_style() {
    let underlined: Vec<String> = wrap_spans("a {u}line{/u} here", 40)
        .into_iter()
        .flatten()
        .filter(|r| r.0.iter().any(|s| s == "u"))
        .map(|r| r.1)
        .collect();
    assert_eq!(underlined, ["line"]);
}

// ---- what Python gave the originals for free -----------------------------

#[test]
fn json_is_printed_the_way_python_prints_it() {
    // Separators ", " and ": ", non-ASCII escaped, a trailing newline.
    let outcome = fixture::run(&args(&["--parse-markup", "\u{e9} \"q\" {i}x{/i}"]));
    assert_eq!(
        outcome.stdout,
        "{\"plain\": \"\\u00e9 \\\"q\\\" x\", \"spans\": [[6, 7, \"i\"]]}\n"
    );
    let outcome = fixture::run(&args(&["--wrap", "7", "aaa {b}bbb ccc{/b} ddd"]));
    assert_eq!(
        outcome.stdout,
        "[[{\"styles\": [], \"text\": \"aaa \"}, {\"styles\": [\"b\"], \"text\": \"bbb\"}], \
         [{\"styles\": [\"b\"], \"text\": \"ccc\"}, {\"styles\": [], \"text\": \" ddd\"}]]\n"
    );
}

#[test]
fn layout_floats_keep_their_decimal_point() {
    // Python prints 6.0, not 6: a transition's indent is a float.
    let outcome = fixture::run(&args(&["--emit-layout", "screenplay"]));
    assert!(outcome.stdout.contains("\"x\": 6.0,"));
    assert!(
        outcome
            .stdout
            .starts_with("[{\"page\": 1, \"rows\": [{\"line\": 24, \"x\": ")
    );
}

#[test]
fn textwrap_matches_python() {
    // Expected values are Python 3's textwrap.wrap, hyphen and em-dash rules
    // included: the committed fixtures were wrapped by it.
    let cases: &[(&str, usize, &[&str])] = &[
        (
            "A fourteen-year-old who thinks movies started with streaming.",
            20,
            &[
                "A fourteen-year-old",
                "who thinks movies",
                "started with",
                "streaming.",
            ],
        ),
        (
            "Static, then a countdown leader \u{2014} 5, 4, 3.",
            12,
            &["Static, then", "a countdown", "leader \u{2014} 5,", "4, 3."],
        ),
        (
            "well-known--but not-really x-ray",
            9,
            &["well-", "known--", "but not-", "really", "x-ray"],
        ),
        (
            "supercalifragilisticexpialidocious-word here",
            10,
            &[
                "supercalif",
                "ragilistic",
                "expialidoc",
                "ious-word",
                "here",
            ],
        ),
        ("", 10, &[]),
        ("  lead and trail  ", 5, &["lead", "and", "trail"]),
    ];
    for (text, width, want) in cases {
        assert_eq!(
            fixture::textwrap::wrap(text, *width),
            *want,
            "{text:?} at {width}"
        );
    }
}

#[test]
fn usage_errors_exit_1() {
    let outcome = fixture::run(&args(&["--emit-layout", "nope"]));
    assert_eq!(outcome.code, 1);
    assert!(
        outcome
            .stderr
            .contains("--emit-layout <screenplay|prose|blank|images|jpx|torture|demo>")
    );
    let outcome = fixture::run(&args(&[]));
    assert_eq!(outcome.code, 1);
    assert!(
        outcome
            .stderr
            .contains("<screenplay|prose|blank|images|jpx|torture|demo> <out.pdf>")
    );
}

//! Port of tests/first-page.test.ts, the half that runs the CLI: under
//! --progress the engine sends the first real page once, on stderr, as soon
//! as it has read it, and stdout's answer is the same with or without.
//! (The parser half, pageParagraphs and isTitleLike, is
//! sp-parser/tests/first_page.rs.)

mod common;

use common::{Sandbox, fixture, keys, progress_events};
use serde_json::Value;

#[test]
fn field_station_sends_page_2_opening_on_its_first_scene_heading_in_plain_words_only() {
    let sb = Sandbox::new();
    let pdf = fixture("field-station.pdf").to_string_lossy().into_owned();
    let with = sb.run(&[&pdf, "--json", "--progress", "-o", &sb.p("with.epub")]);
    assert_eq!(with.code, 0, "{}", with.stdout);
    let events = progress_events(&with.stderr);
    let pages: Vec<&Value> = events.iter().filter(|e| e["stage"] == "page").collect();
    assert_eq!(pages.len(), 1);
    let first = pages[0];

    // Page 1 is the title page; the first page after it is page 2.
    assert_eq!(first["page"], 2);

    // The scene heading the Preview page shows first is the .fountain's first.
    let mut answer = with.json();
    let fountain =
        std::fs::read_to_string(answer["fountainPath"].as_str().unwrap()).expect("the .fountain");
    let first_heading = fountain
        .lines()
        .find(|l| {
            l.starts_with("INT.")
                || l.starts_with("INT ")
                || l.starts_with("EXT.")
                || l.starts_with("EXT ")
        })
        .expect("a scene heading");
    let paragraphs = first["paragraphs"].as_array().expect("paragraphs");
    assert_eq!(paragraphs[0], first_heading);
    assert!(paragraphs.len() > 3);
    for p in paragraphs {
        let p = p.as_str().expect("a string");
        for bad in ["**", "_", "[[", "\n"] {
            assert!(!p.contains(bad), "{bad:?} in {p:?}");
        }
    }

    // The page line arrives in the parse stage, before the bar reaches the end.
    let page_at = events.iter().position(|e| e["stage"] == "page").unwrap();
    assert!(events[page_at + 1..].iter().any(|e| e["stage"] == "parse"));

    // stdout is the app's contract: same shape, same answer, with or without.
    let without = sb.run(&[&pdf, "--json", "-o", &sb.p("without.epub")]);
    assert_eq!(without.code, 0);
    assert_eq!(without.stderr, "");
    let mut plain = without.json();
    assert_eq!(keys(&answer), keys(&plain));
    for k in ["output", "fountainPath", "epubPath"] {
        answer[k] = Value::from("");
        plain[k] = Value::from("");
    }
    assert_eq!(answer, plain);
}

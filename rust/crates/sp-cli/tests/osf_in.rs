//! OSF in, on the command line: `method-reader script.osf` reads the OSF
//! into a `.fountain` text and converts it as a chosen `.fountain`, with
//! the same flags, guards and `--json` answer; under `--library` the
//! script folder keeps that `.fountain` beside the book, and its
//! source.json names the .osf (docs/osf-input.md).
//!
//! The OSF is Method Reader's own, written from the invented fixture
//! tests/fixtures/weather-log.fountain. Every run is in a Sandbox.

mod common;

use std::path::Path;

use common::Sandbox;
use serde_json::{Value, json};

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn ok(sb: &Sandbox, args: &[&str]) -> Value {
    let ran = sb.run(args);
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    ran.json()
}

fn weather_log_osf() -> String {
    let fountain = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/weather-log.fountain"),
    )
    .expect("the fixture");
    let tokens = sp_fountain::tokenize(&fountain).expect("tokens");
    sp_osf::tokens_to_osf(&tokens)
}

fn osf(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<document type=\"Open Screenplay Format document\" version=\"30\">\n<paragraphs>\n{body}\n</paragraphs>\n</document>\n"
    )
}

fn files(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

#[test]
fn an_osf_converts_beside_itself_like_a_fountain() {
    let sb = Sandbox::new();
    let input = s(&sb.write("scripts/Weather Log.osf", weather_log_osf()));
    let answer = ok(&sb, &[&input, "--json"]);
    assert_eq!(answer["ok"], json!(true));
    assert_eq!(answer["title"], json!("Weather Log"));
    assert_eq!(answer["author"], json!("the Method Reader project"));
    assert_eq!(answer["warnings"], json!([]));
    assert_eq!(answer["epubPath"], json!(sb.p("scripts/Weather Log.epub")));
    // Outside the library nothing but the book is written beside it.
    assert!(answer.get("fountainPath").is_none(), "{answer}");
    assert_eq!(
        files(&sb.path("scripts")),
        ["Weather Log.epub", "Weather Log.osf"]
    );
    let body = common::zip_texts(&sb.path("scripts/Weather Log.epub"), ".xhtml");
    assert!(body.contains("Nine ninety-one. Falling."), "{body}");
    assert!(body.contains("<em>slowly</em>"), "emphasis carries over");
}

#[test]
fn under_library_the_folder_keeps_the_fountain_it_was_read_into() {
    let sb = Sandbox::new();
    let input = s(&sb.write("scripts/Weather Log.osf", weather_log_osf()));
    let answer = ok(&sb, &[&input, "--library", "--json"]);
    let folder = sb.path("library/Weather Log");
    assert_eq!(
        answer["fountainPath"],
        json!(s(&folder.join("Weather Log.fountain")))
    );
    assert_eq!(
        files(&folder),
        [
            "Weather Log.epub",
            "Weather Log.fountain",
            "Weather Log.method-reader.json",
            "source.json"
        ]
    );
    let source: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("source.json")).unwrap())
            .unwrap();
    assert_eq!(source["source"], json!(input));
    // The copy is the reader's .fountain: it converts again, as the window
    // does on every tune, with no guard in the way.
    let fountain = std::fs::read_to_string(folder.join("Weather Log.fountain")).unwrap();
    assert!(fountain.starts_with("Title: Weather Log\n"), "{fountain}");
    let tuned = ok(
        &sb,
        &[
            answer["fountainPath"].as_str().unwrap(),
            "--json",
            "-o",
            answer["epubPath"].as_str().unwrap(),
            "--options-json",
            "{}",
        ],
    );
    assert_eq!(tuned["title"], json!("Weather Log"));
    assert_eq!(files(&sb.path("scripts")), ["Weather Log.osf"]);
}

#[test]
fn text_that_is_not_xml_is_refused_as_unreadable() {
    let sb = Sandbox::new();
    let input = s(&sb.write("notes.osf", "INT. HUT - NIGHT\n\nRain.\n"));
    let ran = sb.run(&[&input, "--library", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unreadable");
    assert!(
        ran.error_message()
            .starts_with("this script is not a readable OSF file: it is not well-formed XML"),
        "{}",
        ran.error_message()
    );
    assert!(files(&sb.path("library")).is_empty());
}

#[test]
fn xml_with_no_paragraphs_is_refused_as_unreadable() {
    let sb = Sandbox::new();
    let input = s(&sb.write("page.osf", "<html><body>hello</body></html>"));
    let ran = sb.run(&[&input, "--json", "--force"]);
    assert_eq!(ran.error_code(), "unreadable");
    assert_eq!(
        ran.error_message(),
        "this file is XML but not an OSF script: it has no <paragraphs>"
    );
}

#[test]
fn an_osf_with_nothing_in_it_is_empty_even_with_force() {
    let sb = Sandbox::new();
    let input = s(&sb.write("blank.osf", osf("")));
    let ran = sb.run(&[&input, "--json", "--force"]);
    assert_eq!(ran.error_code(), "unreadable");
    assert_eq!(
        ran.error_message(),
        "this script is empty: there is nothing in it to make a book from"
    );
}

#[test]
fn prose_is_not_a_screenplay_unless_forced() {
    let sb = Sandbox::new();
    let input = s(&sb.write(
        "list.osf",
        osf("<para><style basestylename=\"Action\"/><text>Bread, tea, batteries.</text></para>"),
    ));
    let ran = sb.run(&[&input, "--library", "--json"]);
    assert_eq!(ran.error_code(), "not-screenplay");
    assert!(files(&sb.path("library")).is_empty());
    let forced = ok(&sb, &[&input, "--library", "--force", "--json"]);
    assert_eq!(
        forced["warnings"],
        json!(["No scene headings detected: the EPUB will have no scene navigation."])
    );
}

#[test]
fn an_osf_takes_no_fountain_flag() {
    let sb = Sandbox::new();
    let input = s(&sb.write("w.osf", weather_log_osf()));
    let ran = sb.run(&[&input, "--fountain", &sb.p("w.fountain"), "--json"]);
    assert_eq!(ran.error_code(), "usage", "{}", ran.stdout);
    assert_eq!(
        ran.error_message(),
        "a .osf input takes no --fountain (only a PDF conversion writes a .fountain)"
    );
    assert!(!sb.path("w.epub").exists());
}

#[test]
fn the_unsupported_type_refusal_names_osf() {
    let sb = Sandbox::new();
    let input = s(&sb.write("x.docx", "x"));
    let ran = sb.run(&[&input, "--json"]);
    assert_eq!(ran.error_code(), "unsupported-type");
    assert_eq!(
        ran.error_message(),
        "unsupported input type \".docx\": expected .pdf, .fountain, .txt, or .osf"
    );
}

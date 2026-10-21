//! Fountain in, on the command line: `method-reader script.fountain`
//! converts like a PDF does, with the same flags and the same `--json`
//! answer, holds a script someone chose to the .fountain guards, and under
//! `--library` stores the script the way a PDF conversion is stored.
//!
//! Every run is in a Sandbox: its own settings folder, library and volume
//! roots, so nothing here reaches the real ones.

mod common;

use std::path::Path;

use common::{Sandbox, keys, png, zip_entries, zip_text};
use serde_json::{Value, json};

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn ok(sb: &Sandbox, args: &[&str]) -> Value {
    let ran = sb.run(args);
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    ran.json()
}

/// The invented fixture, copied into the sandbox under `rel`.
fn weather_log(sb: &Sandbox, rel: &str) -> String {
    s(&sb.copy_fixture("weather-log.fountain", rel))
}

/// Every file under `dir`, relative, sorted.
fn tree(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.push(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn a_fountain_converts_beside_itself_like_a_pdf() {
    let sb = Sandbox::new();
    let input = weather_log(&sb, "scripts/Weather Log.fountain");
    let answer = ok(&sb, &[&input, "--json"]);
    assert_eq!(answer["ok"], json!(true));
    assert_eq!(answer["title"], json!("Weather Log"));
    assert_eq!(answer["author"], json!("the Method Reader project"));
    assert_eq!(answer["warnings"], json!([]));
    assert_eq!(answer["epubPath"], json!(sb.p("scripts/Weather Log.epub")));
    // The .fountain is the input itself: no second one is written, and the
    // keys a PDF's pages give (pages, scenes, characters) stay out, as
    // they always have for a .fountain.
    assert_eq!(
        keys(&answer),
        [
            "author",
            "epubPath",
            "ok",
            "spacingRepairs",
            "title",
            "topCharacters",
            "warnings"
        ]
    );
    assert_eq!(
        tree(&sb.path("scripts")),
        ["Weather Log.epub", "Weather Log.fountain"]
    );
    let body = common::zip_texts(&sb.path("scripts/Weather Log.epub"), ".xhtml");
    assert!(body.contains("Nine ninety-one. Falling."));
}

#[test]
fn every_output_flag_works_on_a_fountain() {
    let sb = Sandbox::new();
    let input = weather_log(&sb, "in/w.fountain");
    sb.mkdir("out");
    let answer = ok(
        &sb,
        &[
            &input,
            "-o",
            &sb.p("out/w.epub"),
            "--mobi",
            "--azw3",
            &sb.p("out/w.azw3"),
            "--fdx",
            &sb.p("out/w.fdx"),
            "--preview-html",
            &sb.p("out/w.html"),
            "--json",
        ],
    );
    assert_eq!(answer["epubPath"], json!(sb.p("out/w.epub")));
    assert_eq!(answer["mobiPath"], json!(sb.p("out/w.mobi")));
    assert_eq!(answer["azw3Path"], json!(sb.p("out/w.azw3")));
    assert_eq!(answer["fdxPath"], json!(sb.p("out/w.fdx")));
    assert_eq!(answer["previewHtmlPath"], json!(sb.p("out/w.html")));
    assert_eq!(
        tree(&sb.path("out")),
        ["w.azw3", "w.epub", "w.fdx", "w.html", "w.mobi"]
    );
    let fdx = std::fs::read_to_string(sb.path("out/w.fdx")).unwrap();
    assert!(fdx.contains("Scene Heading"), "{fdx}");
    assert!(fdx.contains("Nine ninety-one. Falling."));
    assert!(!fdx.contains("invisible in the book"), "notes stay out");
}

#[test]
fn the_human_answer_names_what_was_written() {
    let sb = Sandbox::new();
    let input = weather_log(&sb, "w.fountain");
    let ran = sb.run(&[&input]);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert_eq!(
        ran.stdout,
        format!(
            "Weather Log, by the Method Reader project\n  wrote {}\n",
            sb.p("w.epub")
        )
    );
}

// ---- --library: stored the way a PDF conversion is stored ----

#[test]
fn under_library_the_script_gets_its_folder_its_copy_and_its_source() {
    let sb = Sandbox::new();
    let input = weather_log(&sb, "scripts/Weather Log.fountain");
    let answer = ok(&sb, &[&input, "--library", "--json"]);
    let folder = sb.path("library/Weather Log");
    assert_eq!(
        answer["epubPath"],
        json!(s(&folder.join("Weather Log.epub")))
    );
    assert_eq!(
        answer["fountainPath"],
        json!(s(&folder.join("Weather Log.fountain")))
    );
    assert_eq!(
        tree(&folder),
        [
            "Weather Log.epub",
            "Weather Log.fountain",
            "Weather Log.method-reader.json",
            "source.json"
        ]
    );
    // The copy is the chosen script, byte for byte, and the folder records
    // that script as its source.
    assert_eq!(
        std::fs::read(folder.join("Weather Log.fountain")).unwrap(),
        std::fs::read(&input).unwrap()
    );
    let source: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("source.json")).unwrap())
            .unwrap();
    assert_eq!(source["source"], json!(input));
    // Nothing lands beside the chosen script.
    assert_eq!(tree(&sb.path("scripts")), ["Weather Log.fountain"]);

    // The same script again lands in the same folder.
    let again = ok(&sb, &[&input, "--library", "--json"]);
    assert_eq!(again["epubPath"], answer["epubPath"]);
    assert_eq!(tree(&sb.path("library")).len(), 4);
}

#[test]
fn the_window_s_convert_argv_works_on_a_fountain() {
    // Request::Convert's argv (sp-app/src/engine.rs), with a .fountain path.
    let sb = Sandbox::new();
    let input = weather_log(&sb, "Weather Log.fountain");
    let ran = sb.run(&[
        &input,
        "--json",
        "--progress",
        "--preview-inline",
        "--library",
    ]);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    let answer = ran.json();
    // What the window reads off a conversion: the book, the .fountain it
    // tunes and exports from, and the pages to show.
    for key in ["epubPath", "fountainPath", "previewHtml", "title"] {
        assert!(answer.get(key).is_some(), "{key}: {answer}");
    }
    assert!(
        answer["previewHtml"]
            .as_str()
            .unwrap()
            .contains("Nine ninety-one.")
    );
    // A .fountain has no pages to read, so --progress prints the bar's
    // last stage and nothing about pages.
    assert!(!ran.stderr.contains("\"page\""), "{}", ran.stderr);
}

#[test]
fn the_library_copy_takes_the_pictures_its_notes_name() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("scripts");
    std::fs::write(
        dir.join("Lamp.fountain"),
        "Title: Lamp\n\nINT. HUT - NIGHT\n\nBefore.\n\n[[img: p001-01.png w=40]]\n\nAfter.\n",
    )
    .unwrap();
    std::fs::create_dir(dir.join("Lamp.images")).unwrap();
    std::fs::write(dir.join("Lamp.images/p001-01.png"), png(7)).unwrap();
    // A picture no note names stays where it is.
    std::fs::write(dir.join("Lamp.images/p009-01.png"), png(9)).unwrap();

    let answer = ok(
        &sb,
        &[&s(&dir.join("Lamp.fountain")), "--library", "--json"],
    );
    let folder = sb.path("library/Lamp");
    assert_eq!(answer["imagesPath"], json!(s(&folder.join("Lamp.images"))));
    assert_eq!(tree(&folder.join("Lamp.images")), ["p001-01.png"]);
    assert_eq!(
        std::fs::read(folder.join("Lamp.images/p001-01.png")).unwrap(),
        png(7)
    );
    let names: Vec<String> = zip_entries(&folder.join("Lamp.epub"))
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| n.starts_with("OEBPS/images/") && !n.ends_with('/'))
        .collect();
    assert_eq!(names, ["OEBPS/images/p001-01.png"]);

    // And the copy renders again from the library on its own, pictures
    // included, the way the window's tune does.
    let tuned = sb.p("library/Lamp/Lamp.epub");
    let again = ok(
        &sb,
        &[
            answer["fountainPath"].as_str().unwrap(),
            "--json",
            "--preview-inline",
            "-o",
            &tuned,
            "--options-json",
            "{}",
        ],
    );
    assert_eq!(again["ok"], json!(true));
    assert!(zip_text(Path::new(&tuned), "OEBPS/text/body001.xhtml").contains("p001-01.png"));
}

#[test]
fn no_fountain_under_library_keeps_no_copy() {
    let sb = Sandbox::new();
    let input = weather_log(&sb, "w.fountain");
    let answer = ok(&sb, &[&input, "--library", "--no-fountain", "--json"]);
    assert!(answer.get("fountainPath").is_none(), "{answer}");
    assert!(!sb.path("library/w/w.fountain").exists());
}

// ---- the guards ----

#[test]
fn a_script_that_is_not_utf8_is_refused_as_unreadable() {
    let sb = Sandbox::new();
    let input = sb.write("Caf.fountain", b"INT. CAF\xc9 - DAY\n\nTea.\n".as_slice());
    let ran = sb.run(&[&s(&input), "--library", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unreadable");
    assert_eq!(
        ran.error_message(),
        "this script is not UTF-8 text. Save it as UTF-8 plain text and try again"
    );
    assert!(tree(&sb.path("library")).is_empty());
}

#[test]
fn an_empty_script_is_refused_even_with_force() {
    let sb = Sandbox::new();
    let input = sb.write("blank.fountain", "\n  \n");
    for extra in [None, Some("--force")] {
        let mut args = vec![s(&input), "--json".into()];
        args.extend(extra.map(str::to_owned));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let ran = sb.run(&args);
        assert_eq!(ran.error_code(), "unreadable", "{extra:?}");
        assert_eq!(
            ran.error_message(),
            "this script is empty: there is nothing in it to make a book from"
        );
    }
    assert!(!sb.path("blank.epub").exists());
}

#[test]
fn text_with_no_scene_heading_and_no_dialogue_is_not_a_screenplay_unless_forced() {
    let sb = Sandbox::new();
    let input = s(&sb.write(
        "notes.txt",
        "Bread, tea, batteries.\n\nCall the boatyard.\n",
    ));
    let ran = sb.run(&[&input, "--library", "--json"]);
    assert_eq!(ran.error_code(), "not-screenplay");
    assert_eq!(
        ran.error_message(),
        "No scene headings and no dialogue found: this does not look like a screenplay. \
         Pass --force to convert it anyway."
    );
    assert!(
        tree(&sb.path("library")).is_empty(),
        "a refusal stores nothing"
    );

    let forced = ok(&sb, &[&input, "--library", "--force", "--json"]);
    assert_eq!(
        forced["warnings"],
        json!(["No scene headings detected: the EPUB will have no scene navigation."])
    );
    // Its library copy is the library's own, so tuning it later needs no
    // --force: the guards were passed (or overruled) on the way in.
    let tuned = ok(
        &sb,
        &[
            forced["fountainPath"].as_str().unwrap(),
            "--json",
            "--preview-inline",
            "-o",
            forced["epubPath"].as_str().unwrap(),
            "--options-json",
            "{}",
        ],
    );
    assert_eq!(tuned["ok"], json!(true));
}

#[test]
fn the_committed_fixture_round_trips_through_the_command_line() {
    // field-station.pdf to its .fountain, then that .fountain as a chosen
    // script: the same EPUB bytes.
    let sb = Sandbox::new();
    let pdf = s(&sb.copy_fixture("field-station.pdf", "a/Field Station.pdf"));
    let first = ok(&sb, &[&pdf, "--json"]);
    let fountain = first["fountainPath"].as_str().unwrap().to_owned();
    sb.mkdir("b");
    let second = ok(&sb, &[&fountain, "-o", &sb.p("b/again.epub"), "--json"]);
    assert_eq!(second["title"], first["title"]);
    assert!(
        std::fs::read(sb.path("a/Field Station.epub")).unwrap()
            == std::fs::read(sb.path("b/again.epub")).unwrap(),
        "the round trip changed the EPUB"
    );
}

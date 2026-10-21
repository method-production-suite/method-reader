//! The `shape` verb: a script's shape as counts, for a problem report. The
//! engine's half (the counts, the leak test) is sp-engine/tests/shape.rs;
//! this is the CLI's face of it.

mod common;

use common::{Sandbox, repo_root};

#[test]
fn shape_help_describes_the_verb_and_exits_0() {
    let sb = Sandbox::new();
    let ran = sb.run(&["shape", "--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.starts_with("method-reader shape:"));
    assert!(ran.stdout.contains("numbers only"));
    let main = sb.run(&["--help"]);
    assert!(
        main.stdout
            .contains("  method-reader shape <file.fountain> [--json]")
    );
}

#[test]
fn the_field_station_answer_is_integers_only() {
    let sb = Sandbox::new();
    let path = repo_root().join("rust/oracle/field-station/script.fountain");
    let ran = sb.run(&["shape", &path.to_string_lossy(), "--json"]);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    let v = ran.json();
    assert_eq!(
        v,
        serde_json::json!({
            "ok": true, "pages": 18, "sceneHeadings": 11, "cues": 103,
            "dialogueBlocks": 103, "dualExchanges": 0, "pictures": 0, "transitions": 3,
        })
    );
}

#[test]
fn no_word_from_the_script_reaches_the_line_printed() {
    let sb = Sandbox::new();
    let path = sb.write(
        "Gribblefen.fountain",
        "Title: Gribblefen\n\nINT. WOMBLECASTLE - DAY\n\nNARWHALIO\nOctarine sprockets.\n\n= pg 2\n\nSMASH CUT TO:\n",
    );
    let path = path.to_string_lossy();
    let ran = sb.run(&["shape", &path, "--json"]);
    assert_eq!(ran.code, 0);
    let human = sb.run(&["shape", &path]);
    for out in [ran.stdout.to_lowercase(), human.stdout.to_lowercase()] {
        for word in [
            "gribblefen",
            "womblecastle",
            "narwhalio",
            "octarine",
            "sprockets",
            "smash",
        ] {
            assert!(!out.contains(word), "{word} in {out}");
        }
    }
    assert_eq!(ran.json()["pages"], 2);
    assert!(human.stdout.contains("cues: 1"));
}

#[test]
fn the_verb_wants_exactly_one_script() {
    let sb = Sandbox::new();
    let ran = sb.run(&["shape", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    let missing = sb.p("missing.fountain");
    let ran = sb.run(&["shape", &missing, "--json"]);
    assert_eq!(ran.error_code(), "unreadable");
    assert!(!ran.error_message().contains("missing"));
}

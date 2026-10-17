//! Port of tests/cli-verb-flags.test.ts: every verb refuses every flag that
//! is not its own.
//!
//! The verbs share ONE parser (src/verbs.rs VERB_SPECS), so every flag any
//! verb takes parses for all of them, and each verb has to refuse the ones
//! it cannot act on. This checks the whole matrix. Every positional is a
//! path that does not exist, so a refusal that answered only AFTER reading
//! the file would come back as `unreadable`, not `usage`: this also pins
//! that the refusal comes first.

mod common;

use common::{Sandbox, repo_root};
use serde_json::json;

/// Every flag in the shared schema but --json and --help, with a value
/// where it takes one.
const FLAGS: &[(&str, &[&str])] = &[
    ("--device", &["--device", "kindle-1"]),
    ("--set", &["--set", "{}"]),
    ("--for", &["--for", "kindle"]),
    ("--fountain", &["--fountain", "/nowhere/x.fountain"]),
    ("--options-json", &["--options-json", "{}"]),
    ("--out", &["--out", "/nowhere/out.epub"]),
    ("--offered", &["--offered", "1.0.0"]),
    ("--current", &["--current", "1.0.0"]),
    ("--opted-in", &["--opted-in"]),
    ("--last-checked", &["--last-checked", "5"]),
    ("--quick", &["--quick"]),
    ("--check", &["--check"]),
];

fn flag_args(flag: &str) -> &'static [&'static str] {
    FLAGS.iter().find(|(f, _)| *f == flag).expect("a flag").1
}

/// What each verb owns, and a well-formed set of positionals for it, in
/// VERBS order.
const VERB_SHAPE: &[(&str, &[&str], &[&str])] = &[
    ("devices", &["--quick"], &[]),
    ("send", &["--device"], &["/nowhere/x.epub"]),
    ("settings", &["--set"], &["/nowhere/x.fountain"]),
    (
        "export",
        &["--for", "--fountain", "--options-json", "--out", "--check"],
        &["/nowhere/x.epub"],
    ),
    ("update-decision", &["--offered", "--current"], &[]),
    (
        "update-should-check",
        &["--opted-in", "--last-checked"],
        &[],
    ),
    ("kfx-status", &[], &[]),
    ("kfx-install", &[], &[]),
    ("app-settings", &["--set"], &[]),
    ("reveal", &[], &["/nowhere/x.epub"]),
    ("routes", &["--quick"], &["/nowhere/x.epub"]),
    (
        "route",
        &["--out", "--fountain", "--options-json"],
        &["save-kindle", "/nowhere/x.epub"],
    ),
    ("eject", &[], &["/nowhere/Kindle"]),
];

/// sp-cli's verbs.rs, whose VERBS and VERB_SPECS lists are read below.
fn verbs_rs() -> String {
    std::fs::read_to_string(repo_root().join("rust/crates/sp-cli/src/verbs.rs")).expect("verbs.rs")
}

/// From `start` to the end of the list it opens.
fn block<'a>(src: &'a str, start: &str) -> &'a str {
    let i = src
        .find(start)
        .unwrap_or_else(|| panic!("{start} in verbs.rs"));
    let rest = &src[i..];
    &rest[..rest.find("];").expect("the list's end")]
}

/// Every "quoted" string in `text`, in order.
fn quoted(text: &str) -> Vec<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_table_covers_every_verb_and_every_shared_flag_so_nothing_new_slips_past_it() {
    let src = verbs_rs();
    let mut verbs = quoted(block(&src, "pub const VERBS"));
    verbs.sort();
    let mut shaped: Vec<String> = VERB_SHAPE.iter().map(|(v, _, _)| (*v).to_owned()).collect();
    shaped.sort();
    assert_eq!(shaped, verbs);
    // Read the shared schema out of verbs.rs itself: a flag added there
    // that this table does not know fails here rather than going unchecked.
    let mut declared: Vec<String> = quoted(block(&src, "const VERB_SPECS"))
        .into_iter()
        .map(|n| format!("--{n}"))
        .filter(|f| f != "--json" && f != "--help")
        .collect();
    declared.sort();
    let mut table: Vec<String> = FLAGS.iter().map(|(f, _)| (*f).to_owned()).collect();
    table.sort();
    assert_eq!(declared, table);
}

/// The whole refusal, owners and all, owners in VERBS order.
fn refusal(verb: &str, flag: &str) -> String {
    let owners: Vec<&str> = VERB_SHAPE
        .iter()
        .filter(|(_, own, _)| own.contains(&flag))
        .map(|(v, _, _)| *v)
        .collect();
    let aside = if verb == "devices" && flag == "--device" {
        ": it lists every reader"
    } else {
        ""
    };
    format!(
        "{verb} takes no {flag}{aside} ({flag} belongs to {})",
        owners.join(" and ")
    )
}

#[test]
fn every_verb_refuses_every_flag_that_is_not_its_own() {
    let sb = Sandbox::new();
    let mut got = Vec::new();
    let mut want = Vec::new();
    for (verb, own, positionals) in VERB_SHAPE {
        for (flag, values) in FLAGS {
            if own.contains(flag) {
                continue;
            }
            let mut args: Vec<&str> = vec![verb];
            args.extend_from_slice(positionals);
            args.extend_from_slice(values);
            args.push("--json");
            let ran = sb.run(&args);
            let v = ran.json();
            got.push(format!(
                "{verb} {flag}: {} {} {}",
                ran.code,
                v["error"]["code"].as_str().unwrap_or("-"),
                v["error"]["message"].as_str().unwrap_or("-")
            ));
            want.push(format!("{verb} {flag}: 1 usage {}", refusal(verb, flag)));
        }
    }
    assert_eq!(got, want);
}

#[test]
fn the_two_verbs_that_used_to_let_foreign_flags_through_refuse_them_in_the_common_wording() {
    let cases: &[(&[&str], &str)] = &[
        (
            &["settings", "/nowhere/x.fountain", "--device", "kindle-1"],
            "settings takes no --device (--device belongs to send)",
        ),
        (
            &["settings", "/nowhere/x.fountain", "--for", "kindle"],
            "settings takes no --for (--for belongs to export)",
        ),
        (
            &[
                "settings",
                "/nowhere/x.fountain",
                "--fountain",
                "/nowhere/y.fountain",
            ],
            "settings takes no --fountain (--fountain belongs to export and route)",
        ),
        (
            &["settings", "/nowhere/x.fountain", "--options-json", "{}"],
            "settings takes no --options-json (--options-json belongs to export and route)",
        ),
        (
            &["export", "/nowhere/x.epub", "--device", "kindle-1"],
            "export takes no --device (--device belongs to send)",
        ),
        (
            &["export", "/nowhere/x.epub", "--set", "{}"],
            "export takes no --set (--set belongs to settings and app-settings)",
        ),
    ];
    let sb = Sandbox::new();
    for (args, message) in cases {
        let mut args = args.to_vec();
        args.push("--json");
        let ran = sb.run(&args);
        assert_eq!(ran.code, 1, "{message}");
        assert_eq!(
            ran.json(),
            json!({ "ok": false, "error": { "code": "usage", "message": message } })
        );
    }
}

#[test]
fn a_refused_settings_call_writes_nothing_even_with_a_real_script_and_a_set_beside_the_foreign_flag()
 {
    let sb = Sandbox::new();
    let fountain = sb.write("settings/Script.fountain", "INT. ROOM - DAY\n\nA beat.\n");
    let ran = sb.run(&[
        "settings",
        &fountain.to_string_lossy(),
        "--set",
        "{\"justifyText\":true}",
        "--device",
        "kindle-1",
        "--json",
    ]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    assert!(!sb.path("settings/Script.method-reader.json").exists());
}

#[test]
fn on_the_conversion_path_a_verbs_flag_is_refused_as_usage_not_ignored() {
    let sb = Sandbox::new();
    for flag in ["--device", "--set", "--for", "--out"] {
        let mut args = vec!["/nowhere/x.pdf"];
        args.extend_from_slice(flag_args(flag));
        args.push("--json");
        let ran = sb.run(&args);
        assert_eq!(
            format!(
                "{flag}: {} {} {}",
                ran.code,
                ran.error_code(),
                ran.error_message().contains(flag)
            ),
            format!("{flag}: 1 usage true")
        );
    }
}

#[test]
fn fountain_on_an_input_that_is_not_a_pdf_is_refused_since_only_a_pdf_conversion_writes_one() {
    let sb = Sandbox::new();
    for ext in [".fountain", ".txt"] {
        let input = format!("/nowhere/x{ext}");
        let ran = sb.run(&[&input, "--fountain", "/nowhere/y.fountain", "--json"]);
        assert_eq!(ran.code, 1);
        assert_eq!(
            ran.json()["error"],
            json!({
                "code": "usage",
                "message": format!("a {ext} input takes no --fountain (only a PDF conversion writes a .fountain)"),
            })
        );
    }
    // An input of no supported kind still gets the refusal that says so.
    let ran = sb.run(&[
        "/nowhere/x.docx",
        "--fountain",
        "/nowhere/y.fountain",
        "--json",
    ]);
    assert_eq!(ran.error_code(), "unsupported-type");
}

#[test]
fn fountain_beside_no_fountain_is_refused_rather_than_one_silently_losing() {
    let ran = Sandbox::new().run(&[
        "/nowhere/x.pdf",
        "--fountain",
        "/nowhere/y.fountain",
        "--no-fountain",
        "--json",
    ]);
    assert_eq!(ran.code, 1);
    assert_eq!(
        ran.json()["error"],
        json!({
            "code": "usage",
            "message": "pass --fountain or --no-fountain, not both (--no-fountain writes no .fountain for --fountain to name)",
        })
    );
}

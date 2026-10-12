//! The help texts are src/cli.ts's, word for word but for the lines in
//! [`CHANGED_ON_PURPOSE`]: every verb's `--help` prints the text between
//! that constant's backticks. src/usage.rs was
//! generated from it; the texts themselves were read out of src/cli.ts the
//! day it was deleted (tests/data/typescript-usage.json, one per USAGE
//! constant, unescaped as the template literal unescapes them).

mod common;

use common::Sandbox;

/// Every `const NAME_USAGE = \`...\`` src/cli.ts had, unescaped.
fn typescript_usages() -> Vec<(String, String)> {
    let v: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("data/typescript-usage.json")).expect("JSON");
    v.iter()
        .map(|u| {
            (
                u["name"].as_str().unwrap().to_owned(),
                u["text"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn verb_of(name: &str) -> Option<&'static str> {
    Some(match name {
        "DEVICES_USAGE" => "devices",
        "SEND_USAGE" => "send",
        "SETTINGS_USAGE" => "settings",
        "EXPORT_USAGE" => "export",
        "APP_SETTINGS_USAGE" => "app-settings",
        "REVEAL_USAGE" => "reveal",
        "EJECT_USAGE" => "eject",
        "KFX_STATUS_USAGE" => "kfx-status",
        "KFX_INSTALL_USAGE" => "kfx-install",
        "ROUTES_USAGE" => "routes",
        "ROUTE_USAGE" => "route",
        "UPDATE_DECISION_USAGE" => "update-decision",
        "UPDATE_SHOULD_CHECK_USAGE" => "update-should-check",
        _ => return None,
    })
}

/// The help lines changed on purpose since the TypeScript, as (the
/// TypeScript's line, the line now), each with its reason. The parity
/// table's help rows were moved with `cargo xtask parity --accept`.
const CHANGED_ON_PURPOSE: &[(&str, &str, &str)] = &[(
    "USAGE",
    "  --debug                also dump classified elements, and let pdf.js's\n                         internal warnings through to stderr",
    "  --debug                also write the classified elements beside the book,\n                         as <book>.elements.json",
    // There is no pdf.js in the Rust reader, so --debug has no warnings to
    // let through (PARITY.md, Decisions: "No pdf.js warnings").
)];

#[test]
fn every_help_text_is_the_typescripts_word_for_word() {
    let usages = typescript_usages();
    for (name, was, _) in CHANGED_ON_PURPOSE {
        let (_, text) = usages.iter().find(|(n, _)| n == name).expect(name);
        assert!(text.contains(was), "{name}: the TypeScript said {was:?}");
    }
    let usages: Vec<(String, String)> = usages
        .into_iter()
        .map(|(name, mut text)| {
            for (n, was, now) in CHANGED_ON_PURPOSE {
                if *n == name {
                    text = text.replace(was, now);
                }
            }
            (name, text)
        })
        .collect();
    assert_eq!(usages.len(), 14, "the conversion's and 13 verbs'");
    let sb = Sandbox::new();
    for (name, text) in usages {
        let ran = match verb_of(&name) {
            Some(verb) => sb.run(&[verb, "--help"]),
            None => sb.run(&["--help"]),
        };
        assert_eq!(ran.code, 0, "{name}");
        // console.log adds one newline to the text's own.
        assert_eq!(ran.stdout, format!("{text}\n"), "{name}");
    }
}

#[test]
fn help_in_json_mode_is_one_object() {
    let sb = Sandbox::new();
    let v = sb.run(&["--help", "--json"]).json();
    assert_eq!(v["ok"], true);
    assert!(
        v["usage"]
            .as_str()
            .unwrap()
            .starts_with("screepub: screenplay PDF")
    );
    let v = sb.run(&["devices", "-h", "--json"]).json();
    assert!(
        v["usage"]
            .as_str()
            .unwrap()
            .starts_with("screepub devices:")
    );
}

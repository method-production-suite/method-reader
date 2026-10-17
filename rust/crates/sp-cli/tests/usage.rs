//! The help texts are src/cli.ts's, word for word but for the lines in
//! [`CHANGED_ON_PURPOSE`] and the product's new name ([`renamed`]): every
//! verb's `--help` prints the text between
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

/// The TypeScript's text with the product's name as it is now: Screepub
/// became Method Reader in 0.13, and the command `screepub` became
/// `method-reader`, five characters longer, so a description written beside
/// a command in the verb list moves five spaces back to keep its column.
/// Nothing else is read differently. "builtBy: calibre or screepub" names
/// a --json token, which kept its spelling, and is left alone.
fn renamed(text: &str) -> String {
    let text = text
        .replace(".screepub.json", ".method-reader.json")
        .replace("SCREEPUB_", "METHOD_READER_")
        .replace("Screepub", "Method Reader")
        .replace("screepub ", "method-reader ")
        .replace("screepub:", "method-reader:");
    text.split('\n')
        .map(|line| {
            let Some(rest) = line.strip_prefix("  method-reader ") else {
                return line.to_owned();
            };
            match rest.find("       ") {
                Some(at) => format!("  method-reader {}{}", &rest[..at], &rest[at + 5..]),
                None => line.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_help_text_is_the_typescripts_word_for_word() {
    let usages = typescript_usages();
    for (name, was, _) in CHANGED_ON_PURPOSE {
        let (_, text) = usages.iter().find(|(n, _)| n == name).expect(name);
        assert!(text.contains(was), "{name}: the TypeScript said {was:?}");
    }
    let usages: Vec<(String, String)> = usages
        .into_iter()
        .map(|(name, text)| {
            let mut text = renamed(&text);
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
            .starts_with("method-reader: screenplay PDF")
    );
    let v = sb.run(&["devices", "-h", "--json"]).json();
    assert!(
        v["usage"]
            .as_str()
            .unwrap()
            .starts_with("method-reader devices:")
    );
}

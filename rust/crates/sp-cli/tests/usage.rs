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
const CHANGED_ON_PURPOSE: &[(&str, &str, &str)] = &[
    // Method Reader's own AZW3 is no longer beta (the owner, 2026-10-29,
    // after a Kindle opened one; PARITY.md, Decisions: "The AZW3 is not
    // beta"): --azw3, the export help's kindle paragraph and --for list,
    // and the route help's save-kindle line.
    (
        "USAGE",
        "  --azw3 <file>          also write an AZW3 for Kindles over USB (beta),\n",
        "  --azw3 <file>          also write an AZW3 for Kindles over USB,\n",
    ),
    (
        "EXPORT_USAGE",
        "otherwise an AZW3\n(beta) that ",
        "otherwise an AZW3\nthat ",
    ),
    (
        "EXPORT_USAGE",
        "kfx, azw3 (beta), mobi,",
        "kfx, azw3, mobi,",
    ),
    (
        "ROUTE_USAGE",
        "  save-kindle            save a Kindle file (KFX, or an AZW3 that is beta)\n                         at --out\n",
        "  save-kindle            save a Kindle file (KFX or AZW3) at --out\n",
    ),
    (
        "USAGE",
        "  --debug                also dump classified elements, and let pdf.js's\n                         internal warnings through to stderr",
        "  --debug                also write the classified elements beside the book,\n                         as <book>.elements.json",
        // There is no pdf.js in the Rust reader, so --debug has no warnings to
        // let through (PARITY.md, Decisions: "No pdf.js warnings").
    ),
    // The OSF, a third editable script (2026-10-20, PARITY.md Decisions:
    // "OSF out"): export --for osf, in the verb list and the export help.
    (
        "USAGE",
        "|mobi|fountain|fdx] [--json]",
        "|mobi|fountain|fdx|osf] [--json]",
    ),
    (
        "EXPORT_USAGE",
        " --for fdx --fountain <f> [--out <path>] [--json]   (beta)\n",
        " --for fdx --fountain <f> [--out <path>] [--json]   (beta)\n  method-reader export <file.epub> --for osf --fountain <f> [--out <path>] [--json]   (beta)\n",
    ),
    (
        "EXPORT_USAGE",
        "the exact page breaks, do not carry over.\n\nOptions:",
        "the exact page breaks, do not carry over.\n\nosf (beta) is the same script in the Open Screenplay Format, an open\nformat that screenwriting apps can read. It is built and rebuilt the same\nway as fdx, and leaves the same things behind.\n\nOptions:",
    ),
    (
        "EXPORT_USAGE",
        "mobi, fountain or fdx (beta)\n  --fountain <file>      the script's .fountain: required by mobi, fountain\n                         and fdx\n",
        "mobi, fountain, fdx (beta) or\n                         osf (beta)\n  --fountain <file>      the script's .fountain: required by mobi, fountain,\n                         fdx and osf\n",
    ),
    (
        "EXPORT_USAGE",
        "An FDX\n                         is written only there, not beside the book\n",
        "An FDX\n                         or an OSF is written only there, not beside the\n                         book\n",
    ),
    (
        "USAGE",
        "effect here (asking to strip (CONT'D) warns rather than failing silently),\nand the scanned-PDF and not-a-screenplay guards are PDF-only. See\ndocs/fountain-input.md.",
        "effect here (asking to strip (CONT'D) warns rather than failing silently).\nA .fountain must be UTF-8 text with something in it, and one with no scene\nheading and no dialogue is refused as a PDF would be (--force overrules\nthat); the scanned-PDF guard is PDF-only. Under --library a .fountain is\ncopied into its library folder beside the book. See docs/fountain-input.md.\nAn .osf (Open Screenplay Format) is read into a .fountain and converted as\none, the same guards included; under --library its folder keeps that\n.fountain. See docs/osf-input.md.",
        // A .fountain someone chose has guards of its own, and a library
        // conversion keeps a copy of it (PARITY.md, Decisions: "Fountain
        // in"), and an .osf is read into one (docs/osf-input.md).
    ),
    (
        "USAGE",
        "settings command. The first --library conversion of a PDF with none saves\nthe settings it started from as the script's own, so a later change to the\napp defaults reaches only new scripts. Turn that off with",
        "settings command. The first --library conversion of a script (PDF or\n.fountain) with none saves the settings it started from as its own, so a\nlater change to the app defaults reaches only new scripts. Turn that off with",
        // The pin reaches a .fountain's library copy too (the same Decision).
    ),
    (
        "USAGE",
        "                                            may a check be made now? (offline)\n\nA verb",
        "                                            may a check be made now? (offline)\n  method-reader shape <file.fountain> [--json]\n                                            a script's shape as counts, no words from it\n  method-reader device-books <volume> [--json]\n                                            the books on a connected reader\n  method-reader device-remove <volume> <book> [--json]\n                                            take one book off a connected reader\n  method-reader mcp [--root <dir>]... [--read-only]\n                                            these tools for an MCP client, over stdio\n\nA verb",
        // A problem report's counts (PARITY.md, Decisions: "The shape verb"),
        // the device library's two verbs (docs/device-library.md), then the
        // MCP server, Rust's alone (PARITY.md, Decisions: "method-reader
        // mcp"): the TypeScript had none of them to list. The TypeScript's
        // line said screepub; renamed() moves it first.
    ),
];

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

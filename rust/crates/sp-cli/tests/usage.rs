//! The help texts are src/cli.ts's, word for word, while the TypeScript
//! exists to be read (src/usage.rs is generated from it): every verb's
//! `--help` prints the text between that constant's backticks.

mod common;

use common::{Sandbox, repo_root};

/// Every `const NAME_USAGE = \`...\`` in src/cli.ts, unescaped.
fn typescript_usages() -> Vec<(String, String)> {
    let Ok(src) = std::fs::read_to_string(repo_root().join("src/cli.ts")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut rest = src.as_str();
    while let Some(i) = rest.find("const ") {
        rest = &rest[i + 6..];
        let Some(eq) = rest.find(" = `") else { break };
        let name = &rest[..eq];
        if !name.ends_with("USAGE") || name.contains(char::is_whitespace) {
            continue;
        }
        let body = &rest[eq + 4..];
        let mut text = String::new();
        let mut chars = body.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(n) = chars.next() {
                        text.push(n);
                    }
                }
                '`' => break,
                _ => text.push(c),
            }
        }
        out.push((name.to_owned(), text));
    }
    out
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

#[test]
fn every_help_text_is_the_typescripts_word_for_word() {
    let usages = typescript_usages();
    if usages.is_empty() {
        eprintln!("src/cli.ts is gone: nothing to compare the help texts with");
        return;
    }
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

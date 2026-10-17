//! Port of tests/beta-marks.test.ts, its CLI half: the help says "beta"
//! beside each beta feature, and nowhere else. (The badge and the README
//! and site checks are the window's and the docs', not the CLI's.)

mod common;

use common::Sandbox;

fn help(args: &[&str]) -> String {
    let ran = Sandbox::new().run(args);
    assert_eq!(ran.code, 0, "{args:?}");
    ran.stdout
}

fn flat(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The help line that starts with `start`, with its wrapped continuation
/// (lines indented 20 spaces or more).
fn help_entry(text: &str, start: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let i = lines
        .iter()
        .position(|l| l.trim_start().starts_with(start))
        .unwrap_or_else(|| panic!("no help line starts with {start:?}"));
    let mut out = vec![lines[i]];
    for l in &lines[i + 1..] {
        let indent = l.len() - l.trim_start_matches(' ').len();
        if indent >= 20 && l.len() > indent {
            out.push(l);
        } else {
            break;
        }
    }
    flat(&out.join(" "))
}

#[test]
fn fdx_on_a_conversion_says_beta() {
    assert!(help_entry(&help(&["--help"]), "--fdx <file>").contains("(beta)"));
}

#[test]
fn azw3_on_a_conversion_and_the_azw3_in_export_say_beta() {
    assert!(help_entry(&help(&["--help"]), "--azw3 <file>").contains("(beta)"));
    let text = help(&["export", "--help"]);
    assert!(flat(&text).contains("an AZW3 (beta) that Method Reader writes itself"));
    assert!(help_entry(&text, "--for <type>").contains("azw3 (beta)"));
}

#[test]
fn pictures_in_books_say_beta_in_the_conversion_help() {
    let text = flat(&help(&["--help"]));
    assert!(text.contains("Pictures (beta) carried out of a PDF"));
    // And what the .mobi does about them, as long as the gate is shut.
    assert!(text.contains("The .mobi is text-only for now"));
}

#[test]
fn export_for_fdx_says_beta() {
    let text = help(&["export", "--help"]);
    assert!(help_entry(&text, "method-reader export <file.epub> --for fdx").contains("(beta)"));
    assert!(flat(&text).contains("fdx (beta) is the script itself"));
}

#[test]
fn eject_says_beta_on_windows_and_linux_only() {
    assert!(
        help_entry(&help(&["--help"]), "method-reader eject <volume>")
            .contains("(beta on Windows and Linux)")
    );
    assert!(flat(&help(&["eject", "--help"])).contains("beta on Windows and Linux"));
}

#[test]
fn beta_is_said_exactly_where_it_belongs_and_nowhere_else() {
    let count = |t: &str| t.to_lowercase().matches("beta").count();
    // Pictures, --fdx, --azw3 and eject in the conversion help.
    assert_eq!(count(&help(&["--help"])), 4);
    // The fdx usage line, its paragraph, the AZW3 in the kindle paragraph,
    // and azw3 and fdx in the --for list.
    assert_eq!(count(&help(&["export", "--help"])), 5);
    assert_eq!(count(&help(&["eject", "--help"])), 1);
}

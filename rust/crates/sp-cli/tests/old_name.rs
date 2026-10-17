//! The old command name, `screepub`, kept for a few releases: one line on
//! stderr saying it is now method-reader, then exactly what method-reader
//! does with the same arguments (stdout, the rest of stderr, the exit code).

mod common;

use common::Sandbox;

const NOTICE: &str =
    "screepub is now called method-reader. This old name will stop working in a later release.";

fn same_as_the_new_name(args: &[&str]) {
    let sb = Sandbox::new();
    let new = sb.run(args);
    let old = sb.run_old_name(args);
    assert_eq!(old.code, new.code, "{args:?}");
    assert_eq!(old.stdout, new.stdout, "{args:?}");
    assert_eq!(old.stderr, format!("{NOTICE}\n{}", new.stderr), "{args:?}");
}

#[test]
fn version_under_the_old_name() {
    same_as_the_new_name(&["--version"]);
    let sb = Sandbox::new();
    assert_eq!(
        sb.run(&["--version"]).stdout,
        format!(
            "method-reader {} (Method Reader)\n",
            env!("CARGO_PKG_VERSION")
        )
    );
}

#[test]
fn help_and_a_verb_under_the_old_name() {
    same_as_the_new_name(&["--help"]);
    same_as_the_new_name(&["devices", "--json"]);
}

#[test]
fn a_failure_keeps_its_exit_code_and_json_under_the_old_name() {
    same_as_the_new_name(&["/no/such/script.pdf", "--json"]);
    same_as_the_new_name(&["/no/such/script.pdf"]);
}

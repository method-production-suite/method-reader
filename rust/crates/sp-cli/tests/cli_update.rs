//! Port of tests/cli-update.test.ts: `method-reader update-decision` and
//! `method-reader update-should-check`, the updater's second opinion for
//! everything that is not the window.
//!
//! BOTH VERBS ARE OFFLINE: the caller hands over the versions or the
//! timestamps and gets a judgement back. The last test checks the Rust
//! source for anything that could reach a network.
//!
//! The handlers are sp-engine's (src/update/mod.rs); its
//! tests/update_compare.rs ports update-compare.test.ts (the comparator
//! itself) and tests/parity_cli.rs runs both verbs against the TypeScript.
//! Here: the handler checks this file made in process, and every spawned
//! check through the built `method-reader`. `resolveCommand(.., exists)` is the
//! binary's own, so "a real file of that name wins" runs through the CLI
//! with such a file in its working folder.

mod common;

use common::Sandbox;
use serde_json::json;
use sp_engine::update::{UpdateDecision, update_decision_command, update_should_check_command};

// ---- updateDecisionCommand ----

#[test]
fn a_newer_release_is_offered() {
    let d = update_decision_command(Some("0.7.0"), Some("0.6.0")).expect("judged");
    assert_eq!(
        serde_json::to_value(&d).unwrap(),
        json!({ "offer": true, "version": "0.7.0" })
    );
}

#[test]
fn the_downgrade_to_a_post_tag_dev_build_is_refused_with_the_reason() {
    let d = update_decision_command(Some("0.6.0"), Some("0.6.0-1-g965cb10")).expect("judged");
    match d {
        UpdateDecision::Refuse { offer, reason } => {
            assert!(!offer);
            assert!(reason.contains("not newer"), "{reason}");
        }
        other => panic!("offered: {other:?}"),
    }
}

#[test]
fn a_missing_flag_is_a_usage_error_not_a_refusal() {
    let e = update_decision_command(Some("0.7.0"), None).expect_err("nothing judged");
    assert!(e.contains("--current"), "{e}");
    let e = update_decision_command(None, Some("0.6.0")).expect_err("nothing judged");
    assert!(e.contains("--offered"), "{e}");
}

// ---- updateShouldCheckCommand ----

const NOW: f64 = 1_800_000_000_000.0;

#[test]
fn never_without_opt_in() {
    let a = update_should_check_command(false, None, NOW).expect("judged");
    assert_eq!(serde_json::to_value(a).unwrap(), json!({ "check": false }));
}

#[test]
fn first_opted_in_launch_checks_an_hour_later_does_not_a_day_later_does() {
    assert!(update_should_check_command(true, None, NOW).unwrap().check);
    let hour = (NOW - 3_600_000.0).to_string();
    assert!(
        !update_should_check_command(true, Some(&hour), NOW)
            .unwrap()
            .check
    );
    let day = (NOW - 25.0 * 3_600_000.0).to_string();
    assert!(
        update_should_check_command(true, Some(&day), NOW)
            .unwrap()
            .check
    );
}

#[test]
fn a_last_checked_that_is_not_a_number_is_a_usage_error_not_never_checked() {
    let e = update_should_check_command(true, Some("yesterday"), NOW).expect_err("refused");
    assert!(e.contains("--last-checked"), "{e}");
}

// ---- the verbs exist and resolve ----

#[test]
fn both_are_registered_verbs() {
    // `VERBS` is the binary's; registered means the word runs the verb,
    // not a conversion of a file by that name.
    let sb = Sandbox::new();
    let d = sb.run(&["update-decision", "--json"]);
    assert_eq!(d.error_code(), "usage");
    assert!(d.error_message().contains("--offered"), "{}", d.stdout);
    let s = sb.run(&["update-should-check", "--json"]);
    assert_eq!(s.json(), json!({ "ok": true, "check": false }));
}

#[test]
fn a_real_file_of_that_name_still_wins_per_the_verb_rule() {
    let sb = Sandbox::new();
    // No file: the verb.
    let verb = sb.run(&["update-decision", "--json"]);
    assert!(
        verb.error_message().contains("--offered"),
        "{}",
        verb.stdout
    );
    // A file of that name in the working folder: a conversion of it.
    sb.write("update-decision", "not a screenplay");
    let convert = sb.run(&["update-decision", "--json"]);
    let answer = convert.json();
    assert_eq!(answer["ok"], json!(false), "{answer}");
    let message = answer["error"]["message"].as_str().unwrap_or_default();
    assert!(!message.contains("--offered"), "ran the verb: {answer}");
}

// ---- end to end, through the real CLI ----

#[test]
fn update_decision_json_answers_one_json_object() {
    let r = Sandbox::new().run(&[
        "update-decision",
        "--offered",
        "0.7.0",
        "--current",
        "0.6.0",
        "--json",
    ]);
    assert_eq!(r.code, 0);
    assert_eq!(
        r.json(),
        json!({ "ok": true, "offer": true, "version": "0.7.0" })
    );
}

#[test]
fn a_refusal_is_exit_0_with_offer_false_it_is_an_answer_not_an_error() {
    let r = Sandbox::new().run(&[
        "update-decision",
        "--offered",
        "0.6.0",
        "--current",
        "0.6.0-1-g965cb10",
        "--json",
    ]);
    assert_eq!(r.code, 0);
    let d = r.json();
    assert_eq!(d["ok"], json!(true));
    assert_eq!(d["offer"], json!(false));
    assert!(
        d["reason"].as_str().unwrap().contains("0.6.0-1-g965cb10"),
        "{d}"
    );
}

#[test]
fn a_missing_flag_is_a_usage_error_with_a_non_zero_exit() {
    let r = Sandbox::new().run(&["update-decision", "--offered", "0.7.0", "--json"]);
    assert_ne!(r.code, 0);
    let d = r.json();
    assert_eq!(d["ok"], json!(false));
    assert_eq!(d["error"]["code"], json!("usage"));
    assert!(
        d["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--current")
    );
}

#[test]
fn update_should_check_json_answers_the_throttle() {
    let r = Sandbox::new().run(&["update-should-check", "--opted-in", "--json"]);
    assert_eq!(r.code, 0);
    assert_eq!(r.json(), json!({ "ok": true, "check": true }));
}

#[test]
fn without_opted_in_the_answer_is_no_even_never_having_checked() {
    let r = Sandbox::new().run(&["update-should-check", "--json"]);
    assert_eq!(r.json(), json!({ "ok": true, "check": false }));
}

#[test]
fn a_flag_from_another_verb_is_rejected_not_ignored() {
    let r = Sandbox::new().run(&[
        "update-decision",
        "--offered",
        "0.7.0",
        "--current",
        "0.6.0",
        "--device",
        "kindle",
        "--json",
    ]);
    assert_ne!(r.code, 0);
    assert_eq!(r.error_code(), "usage");
    assert!(r.error_message().contains("--device belongs to send"));
}

// ---- the new flags do not leak into the OTHER verbs ----

#[test]
fn devices_refuses_each_update_flag_and_names_its_owner() {
    let sb = Sandbox::new();
    let cases: [(&[&str], &str); 4] = [
        (
            &["--offered", "1.0"],
            "--offered belongs to update-decision",
        ),
        (
            &["--current", "1.0"],
            "--current belongs to update-decision",
        ),
        (
            &["--last-checked", "5"],
            "--last-checked belongs to update-should-check",
        ),
        (&["--opted-in"], "--opted-in belongs to update-should-check"),
    ];
    for (flags, expected) in cases {
        let mut args = vec!["devices"];
        args.extend_from_slice(flags);
        args.push("--json");
        let r = sb.run(&args);
        assert_ne!(r.code, 0, "{flags:?}");
        assert_eq!(r.error_code(), "usage", "{flags:?}");
        assert!(r.error_message().contains(expected), "{}", r.stdout);
    }
}

#[test]
fn the_update_verbs_refuse_each_others_flags_too() {
    let sb = Sandbox::new();
    let a = sb.run(&[
        "update-decision",
        "--offered",
        "1.0",
        "--current",
        "0.9",
        "--opted-in",
        "--json",
    ]);
    assert!(
        a.error_message()
            .contains("--opted-in belongs to update-should-check")
    );
    let b = sb.run(&["update-should-check", "--offered", "1.0", "--json"]);
    assert!(
        b.error_message()
            .contains("--offered belongs to update-decision")
    );
}

// ---- the verbs stay offline ----

#[test]
fn the_update_verbs_import_nothing_that_can_reach_a_network() {
    // The TypeScript reads src/cli-update.ts and src/update/compare.ts for
    // fetch( and node:http/https/net/dns/tls. Their Rust is sp-engine's
    // src/update/, and the verbs' printing in sp-cli's src/verbs.rs (which
    // the other verbs share, so only its update arms are read).
    let mut texts = vec![
        common::seams::source("rust/crates/sp-engine/src/update/mod.rs"),
        common::seams::source("rust/crates/sp-engine/src/update/compare.rs"),
    ];
    let verbs = common::seams::source("rust/crates/sp-cli/src/verbs.rs");
    let from = verbs
        .find("\"update-decision\" => {")
        .expect("the update-decision arm");
    let to = verbs[from..]
        .find("\"kfx-status\" => {")
        .expect("the arm after the update ones");
    texts.push(verbs[from..from + to].to_owned());
    for text in texts {
        for needle in [
            "std::net",
            "TcpStream",
            "UdpSocket",
            "reqwest",
            "ureq",
            "hyper",
            "curl",
            "std::process::Command",
            "Command::new",
        ] {
            assert!(!text.contains(needle), "{needle} in the update verbs");
        }
    }
}

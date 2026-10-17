//! Port of tests/cli-kfx.test.ts: `method-reader kfx-status` and
//! `method-reader kfx-install`.
//!
//! The handlers are sp-engine's (src/commands/kfx.rs), tested here in
//! process with a stand-in probe and a stand-in installer, as the
//! TypeScript does; sp-engine's tests/parity_cli.rs runs kfx-status against
//! the TypeScript. NOTHING here may reach the real installer: it downloads
//! a plugin and writes it into the Calibre of whatever machine runs the
//! suite. The only kfx-install spawned is `--help`, which exits before the
//! verb runs, and the order that keeps a mistyped kfx-install from running
//! is pinned in the Rust source (sp-cli's src/verbs.rs), never by letting
//! it run.
//!
//! The spawned kfx-status asks this machine's Calibre for its plugins, so
//! it runs with CALIBRE_CONFIG_DIRECTORY at a scratch COPY of the real
//! Calibre folder (common::seams::calibre_config_copy), never the real one.
//!
//! `resolveCommand(.., exists)` and `VERBS` are the binary's own: "both are
//! verbs, and a file of either name still converts" runs through the CLI.

mod common;

use std::cell::Cell;

use common::Sandbox;
use common::seams::{calibre_copy_str, source};
use serde_json::Value;
use sp_engine::commands::kfx::{
    KfxCommandDeps, KfxInstallAnswer, install_lines, kfx_install_command, kfx_status_command,
    setup_lines,
};
use sp_engine::errors::{EngineError, ErrorCode};
use sp_engine::export::kfx::{KfxInstallResult, KfxStatus};
use sp_engine::export::kfx_setup::{KfxFix, KfxSetup, KfxStepId};

const MISSING_PLUGIN: KfxStatus = KfxStatus {
    calibre: true,
    previewer: true,
    plugin_installed: false,
    ready: false,
};
const ALL_THERE: KfxStatus = KfxStatus {
    calibre: true,
    previewer: true,
    plugin_installed: true,
    ready: true,
};

fn status(calibre: bool, previewer: bool, plugin_installed: bool) -> KfxStatus {
    KfxStatus {
        calibre,
        previewer,
        plugin_installed,
        ready: false,
    }
}

/// `kfxStatusCommand({ status, platform })`.
fn setup_for(s: KfxStatus, platform: &str) -> KfxSetup {
    kfx_status_command(&KfxCommandDeps {
        status: Some(Box::new(move || s)),
        platform: Some(platform.into()),
        env: Some(common::seams::guarded_env()),
        ..Default::default()
    })
}

/// `kfxInstallCommand({ install, status, platform })`, the installer and
/// the probe both stand-ins.
fn install_with<'a>(
    install: impl Fn() -> KfxInstallResult + 'a,
    status: impl Fn() -> KfxStatus + 'a,
    platform: &str,
) -> Result<KfxInstallAnswer, EngineError> {
    kfx_install_command(&KfxCommandDeps {
        status: Some(Box::new(status)),
        install: Some(Box::new(install)),
        platform: Some(platform.into()),
        env: Some(common::seams::guarded_env()),
    })
}

fn failed(reason: Option<&str>, removed: Option<&[&str]>) -> KfxInstallResult {
    KfxInstallResult {
        ok: false,
        version: None,
        reason: reason.map(str::to_owned),
        removed: removed.map(|r| r.iter().map(|s| (*s).to_owned()).collect()),
    }
}

fn kfx_install_failed(err: EngineError) -> String {
    match err {
        EngineError::Cli(e) => {
            assert_eq!(e.code, ErrorCode::KfxInstallFailed, "{}", e.message);
            e.message
        }
        other => panic!("not a CliError: {other:?}"),
    }
}

// ---- kfxStatusCommand ----

#[test]
fn answers_the_checklist_for_the_probed_status_and_the_given_platform() {
    let setup = setup_for(MISSING_PLUGIN, "darwin");
    assert!(!setup.ready);
    assert!(setup.possible);
    let plugin = setup
        .steps
        .iter()
        .find(|s| s.id == KfxStepId::Plugin)
        .expect("a plugin step");
    assert_eq!(
        plugin.fix,
        Some(KfxFix::Install {
            label: "Install".into()
        })
    );
}

// ---- kfxInstallCommand ----

#[test]
fn success_reports_the_version_the_removed_forks_and_a_fresh_checklist() {
    // The probe answers by whether the install has happened yet, so one
    // that probes BEFORE installing gets the wrong answer, not just a count.
    let done = Cell::new(false);
    let probed = Cell::new(0);
    let answer = install_with(
        || {
            done.set(true);
            KfxInstallResult {
                ok: true,
                version: Some("2.20.1".into()),
                reason: None,
                removed: Some(vec!["KFX Output (fork)".into()]),
            }
        },
        || {
            probed.set(probed.get() + 1);
            if done.get() {
                ALL_THERE
            } else {
                MISSING_PLUGIN
            }
        },
        "darwin",
    )
    .expect("installed");
    assert_eq!(probed.get(), 1);
    assert_eq!(answer.version, "2.20.1");
    assert_eq!(answer.removed, vec!["KFX Output (fork)".to_owned()]);
    assert!(answer.setup.ready);
}

#[test]
fn a_missing_removed_list_reads_as_empty_never_undefined() {
    let answer = install_with(
        || KfxInstallResult {
            ok: true,
            version: Some("2.20.1".into()),
            reason: None,
            removed: None,
        },
        || ALL_THERE,
        "darwin",
    )
    .expect("installed");
    assert!(answer.removed.is_empty());
    let json = serde_json::to_value(&answer).unwrap();
    assert_eq!(json["removed"], serde_json::json!([]));
}

#[test]
fn a_failure_is_a_kfx_install_failed_cli_error_carrying_calibres_reason() {
    let probed = Cell::new(0);
    let err = install_with(
        || failed(Some("downloaded 10 bytes, index says 20"), None),
        || {
            probed.set(probed.get() + 1);
            MISSING_PLUGIN
        },
        "darwin",
    )
    .expect_err("a failure");
    assert_eq!(
        kfx_install_failed(err),
        "could not install the KFX plugin: downloaded 10 bytes, index says 20"
    );
    // No point probing after a failure.
    assert_eq!(probed.get(), 0);
}

#[test]
fn a_failure_after_an_older_copy_was_cleared_says_so_and_what_to_do() {
    let err = install_with(
        || failed(Some("add_plugin failed"), Some(&["KFX Output (fork)"])),
        || MISSING_PLUGIN,
        "darwin",
    )
    .expect_err("a failure");
    assert_eq!(
        kfx_install_failed(err),
        "could not install the KFX plugin: add_plugin failed. An older copy was removed before the failure: KFX Output (fork). Try again, or reinstall it in Calibre."
    );
}

#[test]
fn a_reason_that_already_ends_a_sentence_is_not_given_a_second_full_stop() {
    let err = install_with(
        || {
            failed(
                Some(
                    "Calibre's plugin site could not be reached. Check the internet connection, then try again.",
                ),
                Some(&["KFX Output (fork)", "Old KFX"]),
            )
        },
        || MISSING_PLUGIN,
        "darwin",
    )
    .expect_err("a failure");
    assert_eq!(
        kfx_install_failed(err),
        "could not install the KFX plugin: Calibre's plugin site could not be reached. Check the internet connection, then try again. An older copy was removed before the failure: KFX Output (fork), Old KFX. Try again, or reinstall it in Calibre."
    );
}

#[test]
fn a_failure_that_removed_nothing_says_nothing_about_removing() {
    let err = install_with(
        || failed(Some("size mismatch"), Some(&[])),
        || MISSING_PLUGIN,
        "darwin",
    )
    .expect_err("a failure");
    assert_eq!(
        kfx_install_failed(err),
        "could not install the KFX plugin: size mismatch"
    );
}

#[test]
fn ok_without_a_version_is_still_a_failure_nothing_can_say_what_was_installed() {
    let err = install_with(
        || KfxInstallResult {
            ok: true,
            ..Default::default()
        },
        || ALL_THERE,
        "darwin",
    )
    .expect_err("a failure");
    kfx_install_failed(err);
}

#[test]
fn a_failure_with_no_reason_still_carries_a_sentence() {
    let err =
        install_with(|| failed(None, None), || MISSING_PLUGIN, "darwin").expect_err("a failure");
    assert_eq!(
        kfx_install_failed(err),
        "could not install the KFX plugin: Calibre did not say why"
    );
}

#[test]
fn refuses_before_installing_where_amazon_makes_no_kindle_previewer_and_never_calls_the_installer()
{
    for (platform, system) in [("linux", "Linux"), ("freebsd", "this system")] {
        let install_calls = Cell::new(0);
        let err = install_with(
            || {
                install_calls.set(install_calls.get() + 1);
                KfxInstallResult {
                    ok: true,
                    version: Some("2.20.1".into()),
                    ..Default::default()
                }
            },
            || ALL_THERE,
            platform,
        )
        .expect_err("refused");
        assert_eq!(
            kfx_install_failed(err),
            format!(
                "the KFX plugin is of no use here: it drives Kindle Previewer, which Amazon does not make for {system}"
            )
        );
        assert_eq!(install_calls.get(), 0, "{platform}");
    }
}

// ---- human output ----

#[test]
fn status_the_summary_then_one_aligned_line_per_step_with_its_fix() {
    let setup = setup_for(status(true, false, false), "darwin");
    assert_eq!(
        setup_lines(&setup),
        vec![
            "Kindles get AZW3 (beta) for now, which Method Reader makes itself. KFX looks better, and needs the three free tools below.",
            "  Calibre           installed",
            "  Kindle Previewer  not installed: https://kdp.amazon.com/en_US/help/topic/G202131170",
            "  KFX plugin        not installed: run method-reader kfx-install",
        ]
    );
}

#[test]
fn status_an_after_or_unavailable_fix_prints_its_reason() {
    let lines = setup_lines(&setup_for(status(false, false, false), "linux"));
    assert_eq!(
        lines[2],
        "  Kindle Previewer  not available: Amazon does not make it for Linux"
    );
    assert_eq!(
        lines[3],
        "  KFX plugin        not available: Of no use without Kindle Previewer"
    );
    let mac = setup_lines(&setup_for(status(false, false, false), "darwin"));
    assert_eq!(
        mac[3],
        "  KFX plugin        not installed: Install Calibre first"
    );
}

#[test]
fn install_the_version_each_removed_fork_then_the_full_checklist() {
    let setup = setup_for(status(true, false, true), "darwin");
    let lines = install_lines(&KfxInstallAnswer {
        version: "2.20.1".into(),
        removed: vec!["KFX Output (fork)".into()],
        setup: setup.clone(),
    });
    assert_eq!(lines[0], "installed the KFX plugin 2.20.1");
    assert_eq!(lines[1], "removed an older copy: KFX Output (fork)");
    assert_eq!(lines[2..].to_vec(), setup_lines(&setup));
    assert!(lines.contains(
        &"  Kindle Previewer  not installed: https://kdp.amazon.com/en_US/help/topic/G202131170"
            .to_owned()
    ));
}

#[test]
fn no_line_carries_an_em_dash() {
    let setup = setup_for(MISSING_PLUGIN, "darwin");
    let mut lines = setup_lines(&setup);
    lines.extend(install_lines(&KfxInstallAnswer {
        version: "1".into(),
        removed: vec!["x".into()],
        setup,
    }));
    for line in lines {
        assert!(!line.contains('\u{2014}'), "{line}");
    }
}

// ---- the verbs ----

/// A sandbox whose Calibre is the scratch copy, never the real folder.
fn calibre_sandbox() -> Sandbox {
    let mut sb = Sandbox::new();
    sb.set("CALIBRE_CONFIG_DIRECTORY", calibre_copy_str());
    sb
}

#[test]
fn both_are_verbs_and_a_file_of_either_name_still_converts() {
    // --help exits before any verb runs, so neither outcome can install.
    let sb = Sandbox::new();
    for verb in ["kfx-status", "kfx-install"] {
        let usage = sb.run(&[verb, "--help", "--json"]).json()["usage"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            usage.starts_with(&format!("method-reader {verb}:")),
            "{verb}: {usage}"
        );
    }
    // A file of that name in the working folder: the conversion's help,
    // not the verb's.
    sb.write("kfx-install", "not a screenplay");
    let usage = sb.run(&["kfx-install", "--help", "--json"]).json()["usage"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!usage.starts_with("method-reader kfx-install:"), "{usage}");
    assert!(usage.contains("--mobi"), "{usage}");
}

#[test]
fn kfx_status_json_is_one_object_with_the_checklist() {
    let r = calibre_sandbox().run(&["kfx-status", "--json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let answer = r.json();
    assert_eq!(answer["ok"], Value::Bool(true));
    assert!(answer["ready"].is_boolean());
    assert!(answer["possible"].is_boolean());
    assert!(answer["summary"].is_string());
    let ids: Vec<&str> = answer["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .map(|s| s["id"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(ids, ["calibre", "previewer", "plugin"]);
}

#[test]
fn kfx_status_prints_the_summary_for_a_person_not_json() {
    let r = calibre_sandbox().run(&["kfx-status"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let first = r.stdout.lines().next().unwrap_or_default();
    assert!(
        first.starts_with("Kindles get") || first.starts_with("Amazon does not make"),
        "{first}"
    );
    assert!(!r.stdout.contains('{'), "{}", r.stdout);
}

#[test]
fn each_verb_has_its_own_help_naming_itself_and_not_the_conversion_flags() {
    let sb = Sandbox::new();
    for verb in ["kfx-status", "kfx-install"] {
        let r = sb.run(&[verb, "--json", "--help"]);
        assert_eq!(r.code, 0);
        let usage = r.json()["usage"].as_str().unwrap_or_default().to_owned();
        assert!(usage.contains(&format!("method-reader {verb}")), "{usage}");
        assert!(!usage.contains("--mobi"), "{usage}");
    }
}

#[test]
fn the_main_usage_lists_both() {
    let r = Sandbox::new().run(&["--help"]);
    assert!(r.stdout.contains("method-reader kfx-status"));
    assert!(r.stdout.contains("method-reader kfx-install"));
}

#[test]
fn kfx_status_takes_no_arguments() {
    // Refused before the probe, so no Calibre is asked anything; the
    // scratch copy is there all the same.
    let r = calibre_sandbox().run(&["kfx-status", "extra", "--json"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.error_code(), "usage");
    assert_eq!(
        r.error_message(),
        "kfx-status takes no arguments (got \"extra\")"
    );
}

// ---- kfx-install refuses before it installs (the Rust source) ----
//
// The TypeScript reads src/cli.ts's one kfx branch. The Rust dispatches
// each verb in its own arm of run_verb (sp-cli's src/verbs.rs); these read
// those arms, so the order is pinned without the installer ever running.

const VERBS_RS: &str = "rust/crates/sp-cli/src/verbs.rs";

/// One verb's arm of run_verb: from `"<verb>" => {` to the next arm.
fn arm(src: &str, verb: &str, next: &str) -> String {
    let from = src
        .find(&format!("\"{verb}\" => {{"))
        .unwrap_or_else(|| panic!("the {verb} arm moved"));
    let len = src[from..]
        .find(&format!("\"{next}\" => {{"))
        .unwrap_or_else(|| panic!("the arm after {verb} moved"));
    src[from..from + len].to_owned()
}

#[test]
fn in_the_source_each_kfx_handler_is_called_once_and_only_in_its_own_arm() {
    let src = source(VERBS_RS);
    for (handler, verb, next) in [
        ("kfx_install_command(", "kfx-install", "settings"),
        ("kfx_status_command(", "kfx-status", "kfx-install"),
    ] {
        assert_eq!(
            src.matches(handler).count(),
            1,
            "{handler} is called outside its arm"
        );
        assert!(arm(&src, verb, next).contains(handler), "{handler}");
    }
    // And nowhere else in the binary.
    for other in ["main.rs", "convert.rs", "args.rs", "out.rs"] {
        let text = source(&format!("rust/crates/sp-cli/src/{other}"));
        assert!(!text.contains("kfx_install_command"), "{other}");
        assert!(!text.contains("kfx_status_command"), "{other}");
    }
}

#[test]
fn in_the_source_the_flag_table_gives_both_kfx_verbs_no_flag_of_their_own() {
    let src = source(VERBS_RS);
    let from = src
        .find("fn verb_flags(")
        .expect("verbs.rs has no verb_flags table");
    let table = &src[from..from + src[from..].find("\n}\n").expect("its end")];
    // Neither kfx verb has a row, so both fall to the empty default.
    assert!(!table.contains("kfx"), "{table}");
    assert!(table.contains("_ => &[],"), "{table}");
}

#[test]
fn in_the_source_the_installing_line_is_said_only_where_an_install_can_run() {
    let src = source(VERBS_RS);
    let branch = arm(&src, "kfx-install", "settings");
    let said = branch
        .find("installing the KFX plugin from")
        .expect("the installing line moved");
    assert_eq!(src.matches("installing the KFX plugin from").count(), 1);
    // The nearest `if ` above it is its own guard: no statement between.
    let guard = &branch[branch[..said].rfind("if ").expect("a guard")..said];
    assert!(guard.contains("kfx_possible(host_platform())"), "{guard}");
    assert!(!guard.contains(';'), "{guard}");
}

#[test]
fn in_the_source_every_refusal_comes_before_either_handler_is_called() {
    let src = source(VERBS_RS);
    for (handler, verb, next) in [
        ("kfx_install_command(", "kfx-install", "settings"),
        ("kfx_status_command(", "kfx-status", "kfx-install"),
    ] {
        let branch = arm(&src, verb, next);
        let call = branch.find(handler).expect("the handler call");
        let (before, after) = branch.split_at(call);
        // The arm's own refusal (the positional check) sits before the
        // call, and none after it.
        assert_eq!(
            before.matches("no_arguments(verb, pos)?").count(),
            1,
            "{verb}"
        );
        for refusal in ["no_arguments(", "Stop::usage(", "Stop::fail("] {
            assert!(!after.contains(refusal), "{verb}: {refusal} after the call");
        }
    }
    // The foreign-flag refusal, every verb's, is made once in run_verb
    // above the match that holds both arms.
    let run_verb = src.find("pub fn run_verb(").expect("run_verb");
    let check = src
        .find("foreign_flag_refusal(verb, &parsed)")
        .expect("the foreign-flag check");
    assert!(
        check > run_verb,
        "the foreign-flag check moved out of run_verb"
    );
    assert!(
        check < src.find("\"kfx-status\" => {").expect("the kfx-status arm"),
        "the foreign-flag check moved below the kfx arms"
    );
    assert_eq!(
        src.matches("foreign_flag_refusal(").count(),
        2,
        "defined once, called once"
    );
}

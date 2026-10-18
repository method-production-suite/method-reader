//! Ports tests/verify-signing.test.ts: `cargo xtask verify-signing`. Only a
//! Mac with a real signed artifact could run codesign here, so every check
//! is on the argv and on a fake runner.
//!
//! What every installed copy of the retired Swift app runs, quoted from its
//! UpdateInstaller, unchanged in every Swift release from v0.4.0 on (see it
//! at any tag before F3, e.g. v0.7.4):
//!
//! ```text
//! public static let teamID = "XSRB3D643J"
//! public static let bundleID = "com.darkwell.screepub"
//!
//! public static let appRequirement: String =
//!     "anchor apple generic and identifier \"\(bundleID)\""
//!         + " and certificate 1[field.1.2.840.113635.100.6.2.6] exists"
//!         + " and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
//!         + " and certificate leaf[subject.OU] = \"\(teamID)\""
//!
//! public static let dmgRequirement: String =
//!     "anchor apple generic"
//!         + " and certificate 1[field.1.2.840.113635.100.6.2.6] exists"
//!         + " and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
//!         + " and certificate leaf[subject.OU] = \"\(teamID)\""
//! ```
//!
//! verified with `codesign --verify --deep --strict --test-requirement
//! "=\(requirement)"`. Those installs cannot be updated to want anything
//! else, so these literals are a contract with apps in the wild.

mod cli_build_common;

use cli_build_common::strings;
use xtask::cli_build::RunResult;
use xtask::cli_build::signing::{
    CHAIN_CLAUSES, Expectation, Notarization, SWIFT_BUNDLE_ID, SigningVerdict, TEAM_ID,
    app_requirement, codesign_display_argv, codesign_verify_argv, describe_verdict,
    dmg_requirement, judge_signing, parse_identifier, parse_notarization, spctl_assess_argv,
    stapler_validate_argv,
};

fn ok(stdout: &str, stderr: &str) -> RunResult {
    RunResult::ok(stdout, stderr)
}

fn bad(stderr: &str) -> RunResult {
    RunResult::failed(1, stderr)
}

const APP_REQUIREMENT_AS_SHIPPED: &str = concat!(
    "anchor apple generic and identifier \"com.darkwell.screepub\"",
    " and certificate 1[field.1.2.840.113635.100.6.2.6] exists",
    " and certificate leaf[field.1.2.840.113635.100.6.1.13] exists",
    " and certificate leaf[subject.OU] = \"XSRB3D643J\""
);
const DMG_REQUIREMENT_AS_SHIPPED: &str = concat!(
    "anchor apple generic",
    " and certificate 1[field.1.2.840.113635.100.6.2.6] exists",
    " and certificate leaf[field.1.2.840.113635.100.6.1.13] exists",
    " and certificate leaf[subject.OU] = \"XSRB3D643J\""
);

// ── the pinned requirements ─────────────────────────────────────────────

#[test]
fn the_team_id_and_bundle_id_are_the_installers_own_constants() {
    assert_eq!(TEAM_ID, "XSRB3D643J");
    assert_eq!(SWIFT_BUNDLE_ID, "com.darkwell.screepub");
}

#[test]
fn app_requirement_is_character_for_character_the_one_installed_apps_check() {
    assert_eq!(app_requirement(SWIFT_BUNDLE_ID), APP_REQUIREMENT_AS_SHIPPED);
}

#[test]
fn the_dmg_requirement_is_character_for_character_the_one_installed_apps_check() {
    assert_eq!(dmg_requirement(), DMG_REQUIREMENT_AS_SHIPPED);
}

#[test]
fn app_requirement_is_the_dmg_requirement_plus_exactly_one_identifier_clause() {
    let req = app_requirement("com.example.thing");
    assert!(req.contains("identifier \"com.example.thing\""));
    for clause in CHAIN_CLAUSES {
        assert!(req.contains(clause));
    }
    assert!(req.contains(TEAM_ID));
    assert_eq!(
        req.replace(" and identifier \"com.example.thing\"", ""),
        dmg_requirement()
    );
}

#[test]
fn the_dmg_requirement_pins_the_chain_and_the_team_but_not_an_identifier() {
    let dmg = dmg_requirement();
    assert!(!dmg.contains("identifier"));
    assert!(dmg.contains(TEAM_ID));
    for clause in CHAIN_CLAUSES {
        assert!(dmg.contains(clause.trim()));
    }
}

#[test]
fn the_plans_shorthand_is_a_strict_subset_which_is_why_it_is_not_used() {
    let shorthand =
        format!("anchor apple generic and certificate leaf[subject.OU] = \"{TEAM_ID}\"");
    assert_ne!(dmg_requirement(), shorthand);
    assert!(dmg_requirement().len() > shorthand.len());
    assert_eq!(CHAIN_CLAUSES.len(), 2);
}

// ── the invocations ─────────────────────────────────────────────────────

#[test]
fn verify_passes_deep_strict_and_an_equals_prefixed_requirement() {
    assert_eq!(
        codesign_verify_argv("/V/Screepub.app", "anchor apple generic"),
        strings(&[
            "codesign",
            "--verify",
            "--deep",
            "--strict",
            "--test-requirement",
            "=anchor apple generic",
            "/V/Screepub.app"
        ])
    );
}

#[test]
fn the_equals_prefix_is_present_because_without_it_codesign_reads_a_file() {
    assert!(codesign_verify_argv("/x.app", &dmg_requirement())[5].starts_with('='));
}

#[test]
fn display_asks_for_verbose_4_which_is_what_prints_identifier() {
    assert_eq!(
        codesign_display_argv("/V/Screepub.app"),
        strings(&["codesign", "-dv", "--verbose=4", "/V/Screepub.app"])
    );
}

#[test]
fn spctl_is_asked_the_open_question_with_the_primary_signature_context() {
    assert_eq!(
        spctl_assess_argv("/tmp/x.dmg"),
        strings(&[
            "spctl",
            "-a",
            "-t",
            "open",
            "--context",
            "context:primary-signature",
            "-v",
            "/tmp/x.dmg"
        ])
    );
}

#[test]
fn stapler_is_asked_to_validate_never_to_staple() {
    let argv = stapler_validate_argv("/tmp/x.dmg");
    assert_eq!(
        argv,
        strings(&["xcrun", "stapler", "validate", "/tmp/x.dmg"])
    );
    assert!(!argv.iter().any(|a| a == "staple"));
}

// ── reading codesign and spctl back ─────────────────────────────────────

const SAMPLE: &str = "Executable=/Volumes/Screepub/Screepub.app/Contents/MacOS/Screepub\nIdentifier=com.darkwell.screepub.desktop\nFormat=app bundle with Mach-O universal (x86_64 arm64)\nTeamIdentifier=XSRB3D643J";

#[test]
fn the_identifier_is_read_from_stderr() {
    assert_eq!(
        parse_identifier(&ok("", SAMPLE)).as_deref(),
        Some("com.darkwell.screepub.desktop")
    );
}

#[test]
fn stdout_is_read_too_so_a_future_codesign_that_moves_it_still_works() {
    assert_eq!(
        parse_identifier(&ok(SAMPLE, "")).as_deref(),
        Some("com.darkwell.screepub.desktop")
    );
}

#[test]
fn output_with_no_identifier_line_answers_none_rather_than_guessing() {
    assert_eq!(
        parse_identifier(&ok("", "code object is not signed at all")),
        None
    );
}

// Real output, captured from the v0.6.0 artifacts on 2026-09-21, on stderr
// (the certificate's company name replaced with an invented one).
const NOTARIZED: &str = "/tmp/Screepub-macOS.dmg: accepted\nsource=Notarized Developer ID\norigin=Developer ID Application: Example Studio, LLC (XSRB3D643J)";
const UNNOTARIZED: &str = "/tmp/Screepub-macOS.dmg: rejected\nsource=Unnotarized Developer ID";

#[test]
fn a_notarized_image_is_read_as_notarized() {
    assert_eq!(
        parse_notarization(&ok("", NOTARIZED)),
        Notarization::Notarized
    );
}

#[test]
fn the_v0_6_0_tauri_dmgs_actual_output_is_read_as_unnotarized() {
    assert_eq!(
        parse_notarization(&RunResult::failed(3, UNNOTARIZED)),
        Notarization::Unnotarized
    );
}

#[test]
fn notarized_is_not_matched_by_a_loose_search() {
    assert_ne!(
        parse_notarization(&ok("", UNNOTARIZED)),
        Notarization::Notarized
    );
}

#[test]
fn anything_else_answers_unknown_rather_than_guessing_either_way() {
    assert_eq!(
        parse_notarization(&ok("", "some future phrasing")),
        Notarization::Unknown
    );
    assert_eq!(parse_notarization(&bad("")), Notarization::Unknown);
}

// ── the verdict ─────────────────────────────────────────────────────────

struct Artifact {
    dmg_signed: bool,
    app_signed: bool,
    identifier: &'static str,
    dmg_notarized: bool,
}

/// A runner that answers by what is asked, so a test states the artifact's
/// real properties once and every call agrees.
fn artifact(a: Artifact) -> impl Fn(&[String]) -> RunResult {
    move |argv: &[String]| {
        let target = argv.last().unwrap();
        let is_dmg = target.ends_with(".dmg");
        if argv[0] == "spctl" {
            return if a.dmg_notarized {
                ok(
                    "",
                    &format!("{target}: accepted\nsource=Notarized Developer ID\n"),
                )
            } else {
                RunResult::failed(
                    3,
                    &format!("{target}: rejected\nsource=Unnotarized Developer ID\n"),
                )
            };
        }
        if argv[0] == "xcrun" {
            return if a.dmg_notarized {
                ok("The validate action worked!\n", "")
            } else {
                bad(&format!("{target} does not have a ticket stapled to it."))
            };
        }
        if argv[1] == "-dv" {
            return if a.app_signed || is_dmg {
                let id = if is_dmg {
                    "Screepub-macOS"
                } else {
                    a.identifier
                };
                ok("", &format!("Identifier={id}\n"))
            } else {
                bad("not signed at all")
            };
        }
        let requirement = &argv[5];
        if is_dmg {
            return if a.dmg_signed {
                ok("", "")
            } else {
                bad("not signed at all")
            };
        }
        if !a.app_signed {
            return bad("not signed at all");
        }
        // Only the identifier clause can fail a signed app here.
        let wants = requirement
            .split("identifier \"")
            .nth(1)
            .and_then(|r| r.split('"').next());
        match wants {
            Some(w) if w != a.identifier => bad("does not satisfy its designated Requirement"),
            _ => ok("", ""),
        }
    }
}

fn judge(a: Artifact) -> SigningVerdict {
    judge_signing("/tmp/Screepub-macOS.dmg", "/V/Screepub.app", &artifact(a))
}

const DESKTOP_ID: &str = "com.darkwell.screepub.desktop";

fn v060() -> SigningVerdict {
    judge(Artifact {
        dmg_signed: true,
        app_signed: true,
        identifier: DESKTOP_ID,
        dmg_notarized: true,
    })
}

#[test]
fn a_correctly_signed_0_6_0_artifact_signed_and_the_updater_refuses_it() {
    let v = v060();
    assert!(v.dmg_signed && v.app_chain_signed);
    assert_eq!(v.app_identifier.as_deref(), Some(DESKTOP_ID));
    assert!(!v.updater_would_install);
}

#[test]
fn a_correctly_signed_0_6_1_artifact_signed_and_the_updater_accepts_it() {
    let v = judge(Artifact {
        dmg_signed: true,
        app_signed: true,
        identifier: SWIFT_BUNDLE_ID,
        dmg_notarized: true,
    });
    assert!(v.dmg_signed && v.app_chain_signed);
    assert_eq!(v.app_identifier.as_deref(), Some(SWIFT_BUNDLE_ID));
    assert!(v.updater_would_install);
}

#[test]
fn an_unsigned_dmg_is_caught_even_when_the_app_inside_is_fine() {
    let v = judge_signing(
        "/tmp/x.dmg",
        "/V/Screepub.app",
        &artifact(Artifact {
            dmg_signed: false,
            app_signed: true,
            identifier: SWIFT_BUNDLE_ID,
            dmg_notarized: true,
        }),
    );
    assert!(!v.dmg_signed);
    assert!(v.updater_would_install);
}

#[test]
fn the_v0_6_0_defect_signed_and_stapled_app_unnotarized_image() {
    let v = judge(Artifact {
        dmg_signed: true,
        app_signed: true,
        identifier: DESKTOP_ID,
        dmg_notarized: false,
    });
    assert!(v.dmg_signed);
    assert!(!v.updater_would_install);
    assert!(!v.dmg_stapled);
    assert_eq!(v.dmg_assessment, Notarization::Unnotarized);
}

#[test]
fn a_correct_artifact_reports_both_notarization_signals_agreeing() {
    let v = v060();
    assert!(v.dmg_stapled);
    assert_eq!(v.dmg_assessment, Notarization::Notarized);
}

// ── the expectation each release is held to ────────────────────────────

fn signed060() -> SigningVerdict {
    SigningVerdict {
        dmg_signed: true,
        dmg_stapled: true,
        dmg_assessment: Notarization::Notarized,
        app_chain_signed: true,
        app_identifier: Some(DESKTOP_ID.into()),
        updater_would_install: false,
    }
}

fn taken() -> SigningVerdict {
    SigningVerdict {
        app_identifier: Some(SWIFT_BUNDLE_ID.into()),
        updater_would_install: true,
        ..signed060()
    }
}

#[test]
fn coexist_passes_when_signing_works_and_the_identifier_still_differs() {
    let (ok, lines) = describe_verdict(&signed060(), Expectation::Coexist);
    assert!(ok);
    assert!(lines.join(" ").contains("refuse"));
}

#[test]
fn coexist_fails_if_the_updater_would_install_it() {
    let (ok, lines) = describe_verdict(&taken(), Expectation::Coexist);
    assert!(!ok);
    assert!(lines.join(" ").contains("v0.6.1"));
}

#[test]
fn coexist_fails_on_an_unsigned_dmg() {
    let v = SigningVerdict {
        dmg_signed: false,
        ..signed060()
    };
    assert!(!describe_verdict(&v, Expectation::Coexist).0);
}

#[test]
fn coexist_fails_on_an_unsigned_app_even_if_the_dmg_is_signed() {
    let v = SigningVerdict {
        app_chain_signed: false,
        ..signed060()
    };
    assert!(!describe_verdict(&v, Expectation::Coexist).0);
}

#[test]
fn handover_passes_only_when_the_updater_would_actually_install_it() {
    assert!(describe_verdict(&taken(), Expectation::Handover).0);
    assert!(!describe_verdict(&signed060(), Expectation::Handover).0);
}

fn unnotarized(v: SigningVerdict) -> SigningVerdict {
    SigningVerdict {
        dmg_stapled: false,
        dmg_assessment: Notarization::Unnotarized,
        ..v
    }
}

#[test]
fn an_unnotarized_image_fails_in_both_modes() {
    assert!(!describe_verdict(&unnotarized(signed060()), Expectation::Coexist).0);
    assert!(!describe_verdict(&unnotarized(taken()), Expectation::Handover).0);
}

#[test]
fn the_notarization_failure_says_it_is_about_the_person_not_the_updater() {
    let (_, lines) = describe_verdict(&unnotarized(signed060()), Expectation::Coexist);
    let text = lines.join("\n");
    assert!(text.to_lowercase().contains("notariz"));
    assert!(
        ["Gatekeeper", "download", "double-click"]
            .iter()
            .any(|w| text.contains(w))
    );
    // And it must not claim the updater is affected, because it is not.
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("ok") && l.contains("refuse"))
    );
}

#[test]
fn a_disagreement_between_the_two_signals_fails_rather_than_picking_one() {
    let stapler_only = SigningVerdict {
        dmg_assessment: Notarization::Unknown,
        ..signed060()
    };
    assert!(!describe_verdict(&stapler_only, Expectation::Coexist).0);
    let spctl_only = SigningVerdict {
        dmg_stapled: false,
        ..signed060()
    };
    assert!(!describe_verdict(&spctl_only, Expectation::Coexist).0);
}

#[test]
fn every_failure_says_which_artifact_and_which_requirement_never_just_failed() {
    let v = SigningVerdict {
        dmg_signed: false,
        app_chain_signed: false,
        ..signed060()
    };
    let (ok, lines) = describe_verdict(&v, Expectation::Coexist);
    assert!(!ok);
    let text = lines.join("\n");
    assert!(text.contains("DMG") && text.contains(".app") && text.contains(TEAM_ID));
}

// ── the command (new beside the port: main through a fake runner) ──────

#[test]
fn a_usage_error_or_another_os_is_exit_2_and_runs_nothing() {
    let run = |_: &[String]| -> RunResult { panic!("nothing may run") };
    let main = xtask::cli_build::signing::main_with;
    assert_eq!(main(&strings(&["--dmg", "x.dmg"]), "macos", &run), 2);
    assert_eq!(
        main(
            &strings(&["--dmg", "x.dmg", "--expect", "later"]),
            "macos",
            &run
        ),
        2
    );
    assert_eq!(
        main(
            &strings(&["--dmg", "x.dmg", "--expect", "coexist", "--x"]),
            "macos",
            &run
        ),
        2
    );
    assert_eq!(
        main(
            &strings(&["--dmg", "x.dmg", "--expect", "coexist"]),
            "linux",
            &run
        ),
        2
    );
}

#[test]
fn a_full_run_mounts_judges_and_detaches_on_a_good_artifact_and_on_a_failed_mount() {
    use std::cell::RefCell;
    let seen: RefCell<Vec<Vec<String>>> = RefCell::new(Vec::new());
    let judge = artifact(Artifact {
        dmg_signed: true,
        app_signed: true,
        identifier: DESKTOP_ID,
        dmg_notarized: true,
    });
    let run = |argv: &[String]| -> RunResult {
        seen.borrow_mut().push(argv.to_vec());
        if argv[0] == "hdiutil" && argv[1] == "attach" {
            // Stand in for the image: one app at the mount point.
            let mount = argv.last().unwrap();
            std::fs::create_dir_all(std::path::Path::new(mount).join("Screepub.app")).unwrap();
            return ok("", "");
        }
        if argv[0] == "hdiutil" {
            return ok("", "");
        }
        judge(argv)
    };
    let main = xtask::cli_build::signing::main_with;
    assert_eq!(
        main(
            &strings(&["--dmg", "/tmp/r.dmg", "--expect", "coexist"]),
            "macos",
            &run
        ),
        0
    );
    assert_eq!(
        main(
            &strings(&["--dmg", "/tmp/r.dmg", "--expect", "handover"]),
            "macos",
            &run
        ),
        1
    );
    let detaches = seen
        .borrow()
        .iter()
        .filter(|a| a[0] == "hdiutil" && a[1] == "detach")
        .count();
    assert_eq!(detaches, 2);

    let refuse = |argv: &[String]| -> RunResult {
        assert_eq!(argv[0], "hdiutil");
        bad("hdiutil: attach failed - no such file")
    };
    assert_eq!(
        main(
            &strings(&["--dmg", "/tmp/none.dmg", "--expect", "coexist"]),
            "macos",
            &refuse
        ),
        1
    );
}

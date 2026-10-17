//! `cargo xtask verify-signing`: verify a PUBLISHED macOS DMG against the
//! requirements the frozen Swift updater pins (was tools/verify-signing.ts).
//!
//! ```text
//! cargo xtask verify-signing --dmg <path> --expect coexist
//!     # v0.6.0 to 0.7.x: signed, and the old app must REFUSE it
//! cargo xtask verify-signing --dmg Method-Reader-macOS.dmg --expect handover
//!     # the identifier release: signed, and the old app must TAKE it
//! ```
//!
//! Runs against a DOWNLOADED release asset, never a local build: the thing
//! being tested is what CI produced and published.
//!
//! The requirement strings are not the plan's shorthand
//! (`anchor apple generic and certificate leaf[subject.OU] = "XSRB3D643J"`),
//! which is strictly WEAKER than what the Swift app's installer pins: it
//! drops both Developer ID chain clauses, and for the app the bundle
//! identifier. The Swift source left the repository at piece F3; the
//! installed apps that run it did not, so these strings are a contract with
//! apps in the wild, pinned literally by tests/verify_signing.rs.
//!
//! Notarization is a separate axis. v0.6.0 shipped a DMG that passed every
//! updater check and was still wrong: tauri-bundler notarizes the .app and
//! only signs the image, so the ticket ended up on the app inside an image
//! that carried none. The frozen updater does not care; a person who
//! downloads it and double-clicks it meets Gatekeeper.

use std::path::Path;

use super::{FlagKind, RunResult, Runner, TempDir, parse_flags, real_run};
use crate::dmg::mount_dmg_app;

/// UpdateInstaller.teamID.
pub const TEAM_ID: &str = "XSRB3D643J";
/// UpdateInstaller.bundleID: the identifier the Tauri app TAKES at v0.6.1.
pub const SWIFT_BUNDLE_ID: &str = "com.darkwell.screepub";

/// The two Developer ID chain clauses, shared by both requirements.
/// `...6.2.6` is the Developer ID intermediate CA; `...6.1.13` a Developer ID
/// Application leaf. Together they stop an ad-hoc or self-signed artifact
/// satisfying "anchor apple generic".
pub const CHAIN_CLAUSES: [&str; 2] = [
    " and certificate 1[field.1.2.840.113635.100.6.2.6] exists",
    " and certificate leaf[field.1.2.840.113635.100.6.1.13] exists",
];

fn team_clause() -> String {
    format!(" and certificate leaf[subject.OU] = \"{TEAM_ID}\"")
}

/// UpdateInstaller.appRequirement, for whichever identifier is asked about.
/// The frozen app always asks about [`SWIFT_BUNDLE_ID`]; asking about the
/// app's own identifier is how the chain is checked apart from the refusal.
pub fn app_requirement(bundle_id: &str) -> String {
    format!(
        "anchor apple generic and identifier \"{bundle_id}\"{}{}",
        CHAIN_CLAUSES.concat(),
        team_clause()
    )
}

/// UpdateInstaller.dmgRequirement. No identifier: a codesigned DMG's
/// identifier is its filename stem, so pinning one would fail every image.
pub fn dmg_requirement() -> String {
    format!(
        "anchor apple generic{}{}",
        CHAIN_CLAUSES.concat(),
        team_clause()
    )
}

fn owned(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// Exactly UpdateInstaller.verify's invocation. The `=` prefix is
/// load-bearing: without it codesign reads the argument as a PATH to a
/// requirement file.
pub fn codesign_verify_argv(target: &str, requirement: &str) -> Vec<String> {
    owned(&[
        "codesign",
        "--verify",
        "--deep",
        "--strict",
        "--test-requirement",
        &format!("={requirement}"),
        target,
    ])
}

/// `-dv --verbose=4`, which is what prints `Identifier=`.
pub fn codesign_display_argv(target: &str) -> Vec<String> {
    owned(&["codesign", "-dv", "--verbose=4", target])
}

/// `-t open`, not `-t exec`: the thing assessed is a disk image a person
/// double-clicks. Without the primary-signature context an
/// unsigned-but-stapled image reports "no usable signature", a red flag
/// that is not real.
pub fn spctl_assess_argv(target: &str) -> Vec<String> {
    owned(&[
        "spctl",
        "-a",
        "-t",
        "open",
        "--context",
        "context:primary-signature",
        "-v",
        target,
    ])
}

/// validate, never staple: a verifier that repaired what it was checking
/// could never fail.
pub fn stapler_validate_argv(target: &str) -> Vec<String> {
    owned(&["xcrun", "stapler", "validate", target])
}

/// What spctl says about an image's notarization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notarization {
    Notarized,
    Unnotarized,
    Unknown,
}

impl Notarization {
    pub fn name(self) -> &'static str {
        match self {
            Self::Notarized => "notarized",
            Self::Unnotarized => "unnotarized",
            Self::Unknown => "unknown",
        }
    }
}

fn line_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.strip_prefix(key))
        .map(str::trim)
}

/// spctl's own word, read off `source=`, matched whole: a contains-check for
/// "Notarized" matches "Unnotarized" too. Anything unrecognised is
/// `Unknown`, so a future spctl wording fails closed.
pub fn parse_notarization(result: &RunResult) -> Notarization {
    let text = format!("{}\n{}", result.stderr, result.stdout);
    match line_value(&text, "source=") {
        Some("Notarized Developer ID") => Notarization::Notarized,
        Some("Unnotarized Developer ID") => Notarization::Unnotarized,
        _ => Notarization::Unknown,
    }
}

/// codesign -dv writes to STDERR; both streams are searched.
pub fn parse_identifier(result: &RunResult) -> Option<String> {
    let text = format!("{}\n{}", result.stderr, result.stdout);
    line_value(&text, "Identifier=").map(str::to_string)
}

/// What the frozen updater, and Gatekeeper, would make of an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningVerdict {
    /// The container satisfies the requirement the installer checks first.
    pub dmg_signed: bool,
    /// A notarization ticket is stapled to the IMAGE (read off the file).
    pub dmg_stapled: bool,
    /// What the system would do with the image: a different question from
    /// whether a ticket is present. Both are kept because they can disagree.
    pub dmg_assessment: Notarization,
    /// The .app satisfies the Developer ID chain and team, whatever its
    /// identifier.
    pub app_chain_signed: bool,
    pub app_identifier: Option<String>,
    /// The one that matters: the FULL requirement the frozen UpdateInstaller
    /// pins, identifier included.
    pub updater_would_install: bool,
}

/// Ask codesign, stapler and spctl about an image and the app on it.
pub fn judge_signing(dmg_path: &str, app_path: &str, run: Runner) -> SigningVerdict {
    let passes = |target: &str, requirement: &str| {
        run(&codesign_verify_argv(target, requirement)).exit_code == 0
    };
    let app_identifier = parse_identifier(&run(&codesign_display_argv(app_path)));
    SigningVerdict {
        dmg_signed: passes(dmg_path, &dmg_requirement()),
        dmg_stapled: run(&stapler_validate_argv(dmg_path)).exit_code == 0,
        dmg_assessment: parse_notarization(&run(&spctl_assess_argv(dmg_path))),
        // Asked about the app's OWN identifier, so this isolates the chain.
        app_chain_signed: app_identifier
            .as_deref()
            .is_some_and(|id| passes(app_path, &app_requirement(id))),
        updater_would_install: passes(app_path, &app_requirement(SWIFT_BUNDLE_ID)),
        app_identifier,
    }
}

/// Which release is being verified, and therefore what "correct" means.
/// `Coexist` is v0.6.0: signing proven while the identifier still differs.
/// `Handover` is v0.6.1: existing installs are meant to upgrade themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expectation {
    Coexist,
    Handover,
}

impl Expectation {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "coexist" => Some(Self::Coexist),
            "handover" => Some(Self::Handover),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Coexist => "coexist",
            Self::Handover => "handover",
        }
    }
}

/// The verdict as lines (`ok    ...` / `FAIL  ...`), and whether all passed.
pub fn describe_verdict(v: &SigningVerdict, expectation: Expectation) -> (bool, Vec<String>) {
    let mut lines = Vec::new();
    let mut ok = true;
    let mut fail = |lines: &mut Vec<String>, line: String| {
        ok = false;
        lines.push(format!("FAIL  {line}"));
    };
    let pass = |lines: &mut Vec<String>, line: String| lines.push(format!("ok    {line}"));

    if v.dmg_signed {
        pass(
            &mut lines,
            format!("the DMG satisfies the Developer ID chain and team {TEAM_ID}"),
        );
    } else {
        fail(
            &mut lines,
            format!(
                "the DMG does NOT satisfy the requirement the installer checks first: Developer ID \
                 chain, team {TEAM_ID}. Signing did not work. Stop here."
            ),
        );
    }

    if v.app_chain_signed {
        pass(
            &mut lines,
            format!("the .app satisfies the Developer ID chain and team {TEAM_ID}"),
        );
    } else {
        let unsigned = if v.app_identifier.is_some() {
            ""
        } else {
            ", and carries no identifier at all (is it signed?)"
        };
        fail(
            &mut lines,
            format!(
                "the .app does NOT satisfy the Developer ID chain and team {TEAM_ID}{unsigned}. Stop here."
            ),
        );
    }

    let identifier = v.app_identifier.as_deref().unwrap_or("<none>");
    lines.push(format!("      the .app's identifier is {identifier}"));

    // Not scoped to an expectation: the frozen updater does not check
    // notarization, so an unnotarized image can satisfy every line above and
    // still stop a person who downloads it. Both signals must agree.
    if v.dmg_stapled && v.dmg_assessment == Notarization::Notarized {
        pass(
            &mut lines,
            "the DMG is notarized and carries its stapled ticket".to_string(),
        );
    } else if !v.dmg_stapled && v.dmg_assessment == Notarization::Unnotarized {
        fail(
            &mut lines,
            "the DMG is NOT notarized: no stapled ticket, and Gatekeeper says \"Unnotarized \
             Developer ID\". Signing is fine and the updater does not care, but a person who \
             downloads this and double-clicks it meets Gatekeeper, and a release image must not \
             do this to them. tauri-bundler notarizes the .app and never the image; release.yml's \
             notarize step must notarize and staple the image too."
                .to_string(),
        );
    } else {
        let stapler = if v.dmg_stapled {
            "found a ticket"
        } else {
            "found no ticket"
        };
        fail(
            &mut lines,
            format!(
                "the DMG's two notarization signals DISAGREE: stapler {stapler}, Gatekeeper said \
                 \"{}\". That is a state nobody designed, so it is not being read as a pass.",
                v.dmg_assessment.name()
            ),
        );
    }

    match expectation {
        Expectation::Coexist => {
            if v.updater_would_install {
                fail(
                    &mut lines,
                    format!(
                        "an installed Swift app WOULD install this. At v0.6.0 the identifier is \
                         supposed to still differ, so the pin makes that impossible. Something \
                         has already taken {SWIFT_BUNDLE_ID} early. Do not ship: this is v0.6.1 \
                         behaviour arriving a release too soon, with signing unproven."
                    ),
                );
            } else {
                pass(
                    &mut lines,
                    format!(
                        "an installed Swift app would DOWNLOAD this and then refuse it: the \
                         identifier is not {SWIFT_BUNDLE_ID}. That is the intended v0.6.0 \
                         behaviour, and nobody's app is replaced."
                    ),
                );
            }
        }
        Expectation::Handover => {
            if v.updater_would_install {
                pass(
                    &mut lines,
                    "an installed Swift app would download, verify and install this. That is the \
                     handover working."
                        .to_string(),
                );
            } else {
                fail(
                    &mut lines,
                    format!(
                        "an installed Swift app would download this and then REFUSE it. At \
                         v0.6.1 the identifier must be {SWIFT_BUNDLE_ID}; it is {identifier}. \
                         Every existing install would see a failing update, forever."
                    ),
                );
            }
        }
    }

    (ok, lines)
}

const USAGE: &str = "verify-signing: --dmg <downloaded .dmg> --expect coexist|handover\n  \
coexist  = v0.6.0: signed, and an installed Swift app must REFUSE it\n  \
handover = v0.6.1: signed, and an installed Swift app must INSTALL it\n\
Run it against the asset DOWNLOADED from the release, never a local build.";

/// `cargo xtask verify-signing`: 0 all passed, 1 a check failed (or the
/// image would not mount), 2 a usage error or not a Mac.
pub fn main(args: &[String]) -> u8 {
    main_with(args, std::env::consts::OS, &real_run)
}

/// [`main`] with the platform and the runner handed in.
pub fn main_with(args: &[String], os: &str, run: Runner) -> u8 {
    let parsed = parse_flags(
        "verify-signing",
        args,
        &[("dmg", FlagKind::Value), ("expect", FlagKind::Value)],
    );
    let flags = match parsed {
        Ok(f) => f,
        Err(message) => {
            eprintln!("{message}");
            return 2;
        }
    };
    let (Some(dmg), Some(expectation)) = (
        flags.value("dmg"),
        flags.value("expect").and_then(Expectation::parse),
    ) else {
        eprintln!("{USAGE}");
        return 2;
    };
    if os != "macos" {
        eprintln!(
            "verify-signing: needs codesign and hdiutil, which are macOS-only (this is {os})."
        );
        return 2;
    }
    let work = match TempDir::new("method-reader-verify-signing-") {
        Ok(w) => w,
        Err(e) => {
            eprintln!("verify-signing: no work folder: {e}");
            return 1;
        }
    };
    // The work folder is removed when `work` drops, and the image detached
    // first, on every way out: a failed check once left the folder behind
    // with the DMG still mounted inside it.
    let mounted = match mount_dmg_app(
        Path::new(dmg),
        work.path(),
        &mut |a: &[String]| run(a),
        "verify-signing",
    ) {
        Ok(m) => m,
        Err(message) => {
            eprintln!("{message}");
            return 1;
        }
    };
    let verdict = judge_signing(dmg, &mounted.app_path.display().to_string(), run);
    mounted.detach(&mut |a: &[String]| run(a));
    let (ok, lines) = describe_verdict(&verdict, expectation);
    println!("verify-signing: {dmg} (--expect {})", expectation.name());
    for line in &lines {
        println!("  {line}");
    }
    if ok {
        println!("\nverify-signing: all checks passed.");
        0
    } else {
        eprintln!(
            "\nverify-signing: the artifact does not match what this release is meant to be."
        );
        1
    }
}

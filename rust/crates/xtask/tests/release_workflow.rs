//! release.yml's shape: which jobs exist, what each waits for, what it
//! uploads and in what order (the structural half of
//! tests/release-artifacts.test.ts, rewritten for the Rust release). What
//! the bash blocks actually DO is executed, with stub executables, in
//! release_steps.rs.
//!
//! The published names are read from sp_release::names, the one place the
//! pipeline takes them from, so a renamed artifact fails here rather than
//! leaving a page or a gate naming a file the release does not carry.
//!
//! The suite's release standard (2026-10-16): a tag builds a DRAFT, a
//! person checks the .dmg and publishes it, and only then does
//! release-published.yml read the release back.
//! Nothing in any workflow makes the draft public.

mod repo_common;

use regex::Regex;
use repo_common::*;
use serde_yaml::Value;
use sp_release::names;

fn rel() -> Value {
    workflow("release.yml")
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn strs(v: &[&str]) -> Vec<String> {
    sorted(v.iter().map(|s| s.to_string()).collect())
}

/// The files a job's `gh release upload` commands name: comment lines
/// dropped, continuations joined, flags and "$TAG" skipped, paths reduced
/// to their file name. A name in a comment is not an upload.
fn uploaded_by(job: &Value) -> Vec<String> {
    let code = commands(&run_text(job)).replace("\\\n", " ");
    let re = Regex::new(r"gh release upload\s+([^\n]+)").unwrap();
    let mut files = vec![];
    for c in re.captures_iter(&code) {
        for raw in c[1].split_whitespace() {
            let tok = raw.trim_matches('"');
            if tok.is_empty() || tok.starts_with('-') || tok == "$TAG" {
                continue;
            }
            files.push(tok.rsplit('/').next().unwrap().to_string());
        }
    }
    files
}

#[test]
fn every_job_is_there_and_waits_for_what_it_must() {
    let rel = rel();
    let graph: Vec<(&str, Vec<&str>)> = vec![
        ("checks", vec![]),
        ("release", vec!["checks"]),
        ("cli", vec!["checks"]),
        ("cli-upload", vec!["cli", "release"]),
        ("app-bundles", vec!["checks"]),
        ("handover-gate", vec!["app-bundles"]),
        (
            "app-upload",
            vec!["app-bundles", "handover-gate", "release"],
        ),
        (
            "checksums",
            vec!["app-upload", "cli-upload", "handover-gate", "release"],
        ),
        (
            "draft-check",
            vec![
                "app-upload",
                "checksums",
                "cli-upload",
                "handover-gate",
                "release",
            ],
        ),
    ];
    let names: Vec<String> = jobs(&rel).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        sorted(names),
        sorted(graph.iter().map(|(n, _)| n.to_string()).collect())
    );
    for (name, wants) in graph {
        assert_eq!(
            sorted(needs(job(&rel, name))),
            strs(&wants),
            "{name} waits for the wrong jobs"
        );
    }
}

#[test]
fn everything_that_signs_or_builds_waits_for_the_checks() {
    // The point of the checks job: a wrong version or a missing note costs
    // a minute, not a failure after notarization with an image built.
    let rel = rel();
    for name in ["release", "cli", "app-bundles"] {
        assert_eq!(needs(job(&rel, name)), vec!["checks".to_string()]);
    }
    // And the checks themselves import no certificate.
    let checks = serde_yaml::to_string(job(&rel, "checks")).unwrap();
    assert!(!checks.contains("import-developer-id"));
    assert!(!checks.contains("secrets."));
}

#[test]
fn the_checks_run_the_release_crate_age_check_over_the_whole_lock_before_any_build() {
    // SUITE-STANDARDS.md section 6: no release ships a young crate unless it
    // is a listed security fix. --release, and never --only-changed: the
    // question is the whole lock the release builds from.
    let rel = rel();
    let checks = job(&rel, "checks");
    let ages = step_index(checks, |st| {
        s(st, "run").contains(r#"python3 scripts/check-crate-ages.py --lock "$lock" --release"#)
    })
    .expect("the crate-age step");
    let run = s(steps(checks)[ages], "run");
    assert!(!run.contains("--only-changed"));
    // Every lock the release compiles from: the workspace, and the two
    // update proofs the handover gate builds in their own workspaces.
    assert!(
        run.contains(
            "for lock in rust/Cargo.lock rust/handover-proof/Cargo.lock rust/update-proof-0.13/Cargo.lock"
        ),
        "{run}"
    );
    let first_build = step_index(checks, |st| s(st, "run").contains("cargo ")).unwrap();
    assert!(
        ages < first_build,
        "the crate-age check runs after a cargo step"
    );
    // Everything that builds or signs waits for the checks (and so for it).
    for name in ["release", "cli", "app-bundles"] {
        assert_eq!(needs(job(&rel, name)), vec!["checks".to_string()], "{name}");
    }
}

#[test]
fn the_checks_read_the_version_and_the_notes_from_the_tagged_commit() {
    let rel = rel();
    let text = run_text(job(&rel, "checks"));
    // ONE version file now: [workspace.package] in rust/Cargo.toml.
    assert!(
        text.contains(
            r#"git cat-file blob "$GITHUB_SHA:rust/Cargo.toml" > "$RUNNER_TEMP/Cargo.toml""#
        ),
        "{text}"
    );
    assert!(text.contains(
        r#"sp-release -- version-check --version "${TAG#v}" --cargo-toml "$RUNNER_TEMP/Cargo.toml""#
    ));
    assert!(text.contains(r#"git cat-file -e "$GITHUB_SHA:$F""#));
    // The retired version files are not read any more.
    for gone in ["package.json", "tauri.conf.json", "desktop/src-tauri"] {
        assert!(!commands(&text).contains(gone), "{gone}");
    }
    // The checks run the whole workspace's gates before anything ships.
    for gate in [
        "cargo fmt --all --check",
        "cargo clippy --workspace --all-targets --locked -- -D warnings",
        "cargo test --workspace --locked",
    ] {
        assert!(text.contains(gate), "{gate}");
    }
}

/// Every command line, in every workflow, that could make a draft public:
/// `--draft=false` on `gh release edit`, or a `draft` field sent to the
/// API.
fn undrafting_lines() -> Vec<String> {
    let re = Regex::new(r"--draft=false|draft=false|draft:\s*false|\x22draft\x22").unwrap();
    let mut found = vec![];
    for f in workflow_files() {
        let wf = workflow(&f);
        for (name, job) in jobs(&wf) {
            for line in commands(&run_text(job)).lines() {
                if re.is_match(line) {
                    found.push(format!("{f}: {name}: {}", line.trim()));
                }
            }
        }
    }
    found
}

#[test]
fn the_release_job_leaves_a_draft_and_nothing_in_any_workflow_makes_it_public() {
    // The suite's standard: a person publishes, after checking the .dmg on
    // an Apple silicon Mac. It is also what makes release-published.yml
    // fire at all: GitHub starts no run from an event raised with the
    // default token, so a workflow that published would leave the
    // read-back unrun and nothing red (a weekly guard's old trigger, v0.5.3).
    let rel = rel();
    let release = run_text(job(&rel, "release"));
    assert!(release.contains("--draft"));
    assert_eq!(undrafting_lines(), Vec::<String>::new());
    // The only release edits left are the release job's notes and title.
    for f in workflow_files() {
        let wf = workflow(&f);
        for (name, job) in jobs(&wf) {
            for line in commands(&run_text(job)).lines() {
                if line.contains("gh release edit") {
                    assert_eq!((f.as_str(), name.as_str()), ("release.yml", "release"));
                    assert!(!line.contains("draft"), "{line}");
                }
            }
        }
    }
    assert!(!has_job(&rel, "publish"));
}

#[test]
fn the_draft_check_runs_last_and_fails_loudly_when_any_job_did_not_succeed() {
    // It is what a person reads before publishing: a draft missing its Mac
    // image or its latest.json, published, breaks the site's download
    // buttons and every window's update check (v0.12.1). So it runs even
    // when a job upstream failed, and names that job.
    let rel = rel();
    let check = job(&rel, "draft-check");
    let cond = job_if(check);
    assert!(cond.contains("!cancelled()"), "{cond}");
    assert!(!Regex::new(r"\bsuccess\(\)").unwrap().is_match(&cond));
    let every = steps(check)
        .into_iter()
        .find(|st| s(st, "name") == "Every job before it succeeded")
        .expect("the job-results step");
    let env = serde_yaml::to_string(every.get("env").expect("its env")).unwrap();
    for need in needs(check) {
        assert!(
            env.contains(&format!("needs.{need}.result")),
            "{need} is not checked: {env}"
        );
    }
    let text = step_named(&rel, "draft-check", "Every job before it succeeded");
    assert!(text.contains("NOT ready to publish"), "{text}");
    // Nothing in it changes the release: it reads the draft (which only a
    // token that can write may see) and says what it found.
    assert!(!commands(&run_text(check)).contains("gh release edit"));
    assert!(!commands(&run_text(check)).contains("gh release upload"));
}

#[test]
fn the_draft_check_refuses_any_image_list_but_exactly_the_one_mac_image() {
    // The frozen Swift updater installs the FIRST .dmg on the page.
    let rel = rel();
    let text = run_text(job(&rel, "draft-check"));
    assert!(text.contains(r#"select(endswith(".dmg"))"#));
    assert!(text.contains(&format!(r#""{}") ;;"#, names::MAC_DMG)));
    assert!(
        Regex::new(r#"""\)\s*echo "::error::[^\n]*; exit 1 ;;"#)
            .unwrap()
            .is_match(&text)
    );
    assert!(
        Regex::new(
            r#"\*\)\s*echo "::error::[^\n]*\n\s*exit 1 ;;|\*\) echo "::error::[^\n]*; exit 1 ;;"#
        )
        .unwrap()
        .is_match(&text),
        "{text}"
    );
}

/// Every file a complete release carries, from sp_release::names: the
/// downloads and the one checksums file.
fn every_release_file() -> Vec<String> {
    let mut all = names::downloads(false);
    all.push(names::CHECKSUMS.to_string());
    all.push(names::CHECKSUMS_SIGNATURE.to_string());
    sorted(all)
}

/// The names on a step's `WANT=(` line.
fn want_line(text: &str) -> Vec<String> {
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("WANT=("))
        .expect("a WANT=( line");
    line.trim()
        .trim_start_matches("WANT=(")
        .trim_end_matches(')')
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

#[test]
fn the_draft_check_wants_every_file_the_uploads_put_there() {
    // The list it checks the draft against is every file the release's
    // jobs upload, no more and no fewer, so a renamed or dropped upload
    // fails here and not on the night.
    let rel = rel();
    let text = step_named(
        &rel,
        "draft-check",
        "The draft carries every file the release names",
    );
    let mut want = want_line(&text);
    // latest.json is added for a release, never for a prerelease.
    assert!(text.contains(&format!(
        r#"[[ "$VERSION" == *-* ]] || WANT+=({})"#,
        names::MANIFEST
    )));
    want.push(names::MANIFEST.to_string());
    assert_eq!(sorted(want), every_release_file());
    // And each of those is uploaded by some job (the globs by extension).
    let mut uploaded = vec![];
    for name in ["release", "cli-upload", "app-upload", "checksums"] {
        uploaded.extend(uploaded_by(job(&rel, name)));
    }
    for f in every_release_file() {
        let glob = format!("*.{}", f.rsplit_once('.').map(|x| x.1).unwrap_or(""));
        assert!(
            uploaded.contains(&f) || uploaded.contains(&glob),
            "{f} is wanted but nothing uploads it: {uploaded:?}"
        );
    }
}

#[test]
fn the_mac_release_job_uploads_exactly_the_one_cli_tarball_and_no_image() {
    let rel = rel();
    let release = job(&rel, "release");
    assert_eq!(uploaded_by(release), vec![names::CLI_MAC.to_string()]);
    let text = run_text(release);
    // Both slices, joined into one universal file before anything is signed.
    assert!(text.contains("for t in aarch64-apple-darwin x86_64-apple-darwin; do"));
    assert!(text.contains("--bin method-reader --bin screepub --target \"$t\""));
    assert!(text.contains("lipo -create -output \"target/universal-apple-darwin/release/$b\""));
    assert!(text.find("lipo -create").unwrap() < text.find("codesign --force").unwrap());
    // The Intel slice runs, under Rosetta, before the archive is made.
    assert!(text.contains(r#"arch -x86_64 "$BIN" --version"#));
    // The tool and its old name, each under its own identifier.
    assert!(text.contains(
        r#"codesign --force --options runtime --timestamp --identifier "${pair##*:}" --sign "$SIGN_IDENTITY" "$B""#
    ));
    assert!(text.contains(r#""$BIN:method-reader-cli-arm64" "$ALIAS:screepub-cli-arm64""#));
    assert!(text.contains(r#"cli-archive --binary "$BIN" --alias "$ALIAS" --kind macos"#));
    assert!(text.contains("xcrun notarytool submit"));
    // Signed and notarized, THEN packaged, so the tarball holds the signed bytes.
    let sign = text.find("codesign --force").unwrap();
    let notarize = text.find("xcrun notarytool submit").unwrap();
    let package = text.find("cli-archive").unwrap();
    assert!(sign < notarize && notarize < package);
    // The tarball goes up from the folder cli-archive wrote. Its checksum
    // is in SHA256SUMS, with every other download's, and no longer in the
    // notes: the Homebrew tap that read it there is retired.
    let out = "$RUNNER_TEMP/cli-mac";
    assert!(text.contains(&format!("{out}/{}", names::CLI_MAC)));
    // (The notes name the tools in a printf; nothing here RUNS one.)
    let runs_a_hash = Regex::new(r"(?m)^\s*(\(\s*cd [^)]*&&\s*)?(shasum|sha256sum)\b").unwrap();
    assert!(!runs_a_hash.is_match(&commands(&text)), "{text}");
}

#[test]
fn the_install_line_the_release_job_writes_says_apple_silicon_or_intel() {
    let rel = rel();
    let text = run_text(job(&rel, "release"));
    let mac = text
        .lines()
        .find(|l| l.contains("**Mac:**"))
        .expect("a Mac line");
    assert!(mac.contains(names::MAC_DMG));
    assert!(mac.contains("Apple silicon or Intel"));
    assert!(mac.contains("macOS 13 or later"));
    assert_eq!(names::MAC_MINIMUM, "13.0");
}

#[test]
fn every_mac_binary_is_universal_and_checked_to_be_exactly_both_slices() {
    let yml = read(".github/workflows/release.yml");
    // 0.9.0's Intel tarball is history; 1.0.0's universal one replaces it.
    assert!(!yml.contains("screepub-cli-macos-x64.tar.gz"));
    assert!(!yml.contains(names::CLI_MAC_ARM64));
    let code = commands(&yml);
    // The command-line tool, the window and the MTP helper, each from both
    // slices.
    assert_eq!(code.matches("lipo -create").count(), 3);
    assert_eq!(
        code.matches("rustup target add aarch64-apple-darwin x86_64-apple-darwin")
            .count(),
        2
    );
    // Each is checked to be exactly both, in lipo's order, before it is signed.
    let exact = code.matches(r#"= "x86_64 arm64" ]"#).count();
    assert_eq!(exact, 3, "{code}");
    assert_eq!(
        code.matches("lipo -archs").count(),
        6,
        "the check and its error message, three times"
    );
    assert!(!code.contains(r#"= arm64 ]"#));
}

#[test]
fn the_cli_legs_build_the_three_other_archives_and_cli_upload_ships_them() {
    let rel = rel();
    let cli = job(&rel, "cli");
    let rows: Vec<&Value> = cli
        .get("strategy")
        .and_then(|s| s.get("matrix"))
        .and_then(|m| m.get("include"))
        .and_then(Value::as_sequence)
        .expect("a matrix")
        .iter()
        .collect();
    // No Mac row: the Mac tool is signed and notarized by the release job,
    // and an unsigned one from here would replace it with a warning screen.
    for r in &rows {
        assert!(!s(r, "os").starts_with("macos"), "{r:?}");
        assert!(!s(r, "archive").contains("macos"), "{r:?}");
    }
    let archives: Vec<String> = rows.iter().map(|r| s(r, "archive").to_string()).collect();
    let others = strs(&[
        names::CLI_LINUX_X64,
        names::CLI_LINUX_ARM64,
        names::CLI_WINDOWS_X64,
    ]);
    assert_eq!(sorted(archives), others);
    // Each is run on its own OS before it is packaged: the version, two
    // conversions, the device listing.
    let text = run_text(cli);
    assert!(
        text.contains(r#""$BIN" --version | grep -qx "method-reader $VERSION (Method Reader)""#)
    );
    // The old name rides beside it, and behaves the same.
    assert!(text.contains(
        r#""$ALIAS" --version 2>/dev/null | grep -qx "method-reader $VERSION (Method Reader)""#
    ));
    assert!(text.contains(r#"cli-archive --binary "$BIN" --alias "$ALIAS""#));
    assert!(text.contains("for f in screenplay jpx; do"));
    let upload = (steps(cli).into_iter())
        .find(|st| s(st, "uses").contains("upload-artifact"))
        .expect("an upload-artifact step");
    assert_eq!(s(upload.get("with").unwrap(), "if-no-files-found"), "error");
    // And all three arrive on the page. Their checksums are the checksums
    // job's, so cli-upload writes none.
    let up = job(&rel, "cli-upload");
    assert_eq!(sorted(uploaded_by(up)), others);
    let run = run_text(up);
    assert!(!commands(&run).contains("sha256sum"), "{run}");
    assert!(!commands(&run).contains(names::CHECKSUMS), "{run}");
    assert!(run.contains("cd cli"));
    // No checkout in that job, so gh is told the repository.
    assert!(serde_yaml::to_string(up).unwrap().contains("GH_REPO"));
    // It never touches the Mac downloads the release job owns.
    for mac in [names::CLI_MAC, names::MAC_DMG] {
        assert!(!run.contains(mac));
    }
}

#[test]
fn app_bundles_builds_on_all_three_runners_after_the_checks() {
    let rel = rel();
    let job = job(&rel, "app-bundles");
    let rows: Vec<String> = job
        .get("strategy")
        .and_then(|s| s.get("matrix"))
        .and_then(|m| m.get("include"))
        .and_then(Value::as_sequence)
        .expect("a matrix")
        .iter()
        .map(|r| s(r, "os").to_string())
        .collect();
    assert_eq!(
        sorted(rows),
        strs(&["macos-15", "ubuntu-latest", "windows-latest"])
    );
    let text = run_text(job);
    assert!(text.contains(r#"VERSION="${TAG#v}""#));
    assert!(text.contains("cargo build --release --locked -p sp-app --target \"$t\""));
    assert!(
        text.contains(
            "lipo -create -output target/universal-apple-darwin/release/method-reader-app"
        )
    );
    assert!(text.contains(r#"arch -x86_64 "$WIN" --version"#));
    let upload = steps(job)
        .into_iter()
        .find(|st| s(st, "uses").contains("upload-artifact"))
        .expect("an upload-artifact step");
    assert_eq!(s(upload.get("with").unwrap(), "if-no-files-found"), "error");
}

#[test]
fn the_mac_app_carries_the_mtp_helper_universal_and_started_before_signing() {
    let rel = rel();
    let build = step_named(
        &rel,
        "app-bundles",
        "Build the window and the release tools",
    );
    // Built for both slices in the macOS branch only, and joined by lipo.
    let mac_branch = &build[build.find("if [ \"$RUNNER_OS\" = macOS ]").unwrap()
        ..build.find("\nelse\n").expect("the other runners' branch")];
    assert!(mac_branch.contains(
        "cargo build --release --locked -p sp-mtp --bin method-reader-mtp --target \"$t\""
    ));
    assert!(mac_branch.contains(
        "lipo -create -output target/universal-apple-darwin/release/method-reader-mtp \\\n    target/aarch64-apple-darwin/release/method-reader-mtp target/x86_64-apple-darwin/release/method-reader-mtp"
    ));
    let mac = step_named(&rel, "app-bundles", "macOS app, update archive and image");
    let helper = mac
        .find("MTP=rust/target/universal-apple-darwin/release/method-reader-mtp")
        .expect("the helper");
    let archs = mac
        .find(r#"[ "$(lipo -archs "$MTP")" = "x86_64 arm64" ]"#)
        .expect("the helper's arch check");
    let native = mac.find(r#""$MTP" --version >/dev/null"#).expect("a run");
    let intel = mac
        .find(r#"arch -x86_64 "$MTP" --version >/dev/null"#)
        .expect("a Rosetta run");
    let lay_out = mac
        .find(r#""$SP" mac-app --version "$VERSION" --window "$WIN" --helper "$MTP""#)
        .expect("mac-app gets the helper");
    let sign = mac.find("codesign").expect("signing");
    assert!(helper < archs && archs < native && native < intel && intel < lay_out);
    assert!(lay_out < sign);
    // Inside out: the loop signs every executable in Contents/MacOS, the
    // helper among them, before the bundle.
    assert!(mac.contains(r#"for exe in "$APP"/Contents/MacOS/*; do"#));
    // No other step or job builds, packs or names it: not the Windows or
    // Linux packages, not the CLI downloads. The bundle check reads it back
    // out of the image and the update archive, and that is all.
    for (name, job) in jobs(&rel) {
        for st in steps(job) {
            let text = serde_yaml::to_string(st).unwrap();
            if text.contains("sp-mtp") || text.contains("method-reader-mtp") {
                let at = (name.as_str(), s(st, "name"));
                assert!(
                    at == ("app-bundles", "Build the window and the release tools")
                        || at == ("app-bundles", "macOS app, update archive and image")
                        || at.1 == "Check each bundle",
                    "{at:?}"
                );
            }
        }
    }
    let check = step_named(&rel, "app-bundles", "Check each bundle");
    assert!(check.contains(
        r#""$M/Method Reader.app/Contents/MacOS/method-reader-mtp" --version | grep -q "\"version\":\"$VERSION\"""#
    ));
    assert!(check.contains(r#"test -x "$M/Method Reader.app/Contents/MacOS/method-reader-mtp""#));
}

#[test]
fn every_bundle_is_checked_on_the_os_that_built_it_before_any_upload() {
    let rel = rel();
    let job = job(&rel, "app-bundles");
    let build = step_index(job, |st| {
        s(st, "name") == "Build the window and the release tools"
    })
    .unwrap();
    let check = step_index(job, |st| s(st, "name") == "Check each bundle").unwrap();
    let upload = step_index(job, |st| s(st, "uses").contains("upload-artifact")).unwrap();
    assert!(build < check && check < upload);
    let text = step_named(&rel, "app-bundles", "Check each bundle");
    assert!(text.contains("NOTHING WAS CHECKED"));
    assert!(!text.contains("::notice::"));
    assert!(!text.lines().any(|l| l.trim() == "exit 0"));
}

#[test]
fn the_mac_only_steps_are_keyed_on_the_runner_os() {
    let rel = rel();
    for (name, os) in [
        ("macOS app, update archive and image", "macOS"),
        ("Linux packages", "Linux"),
        ("Windows installer", "Windows"),
        ("Linux build and packaging tools", "Linux"),
    ] {
        let st = steps(job(&rel, "app-bundles"))
            .into_iter()
            .find(|st| s(st, "name") == name)
            .unwrap_or_else(|| panic!("no step {name}"));
        assert_eq!(s(st, "if"), format!("runner.os == '{os}'"), "{name}");
    }
    assert!(
        !serde_yaml::to_string(job(&rel, "app-bundles"))
            .unwrap()
            .contains("matrix.arch ==")
    );
}

#[test]
fn the_mac_leg_refuses_to_start_without_the_update_key_and_signs_the_archive() {
    let rel = rel();
    let mac = step_named(&rel, "app-bundles", "macOS app, update archive and image");
    let refuse = mac
        .find(r#"[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]"#)
        .expect("a refusal");
    let build = mac.find("mac-app").unwrap();
    assert!(refuse < build);
    // The archive is made from the stapled bundle, then signed.
    let staple = mac.find(r#"xcrun stapler staple "$APP""#).unwrap();
    let archive = mac.find("update-archive").unwrap();
    let sign = mac.find("sign-update").unwrap();
    assert!(staple < archive && archive < sign);
}

#[test]
fn the_release_uses_only_the_secrets_the_repository_already_has() {
    let yml = read(".github/workflows/release.yml");
    let mut declared: Vec<String> = Regex::new(r"secrets\.[A-Z0-9_]+")
        .unwrap()
        .find_iter(&yml)
        .map(|m| m.as_str().to_string())
        .collect();
    declared.sort();
    declared.dedup();
    assert_eq!(
        declared,
        strs(&[
            "secrets.AC_API_ISSUER_ID",
            "secrets.AC_API_KEY_ID",
            "secrets.AC_API_KEY_P8_BASE64",
            "secrets.CERT_PASSWORD",
            "secrets.DEVELOPER_ID_CERT_P12_BASE64",
            "secrets.KEYCHAIN_PASSWORD",
            "secrets.TAURI_SIGNING_PRIVATE_KEY",
            "secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
        ])
    );
    // The update key reaches two steps only: the Mac step, which signs the
    // update archive, and the step that signs SHA256SUMS once the draft is
    // complete.
    let rel = rel();
    for (name, job) in jobs(&rel) {
        for st in steps(job) {
            let text = serde_yaml::to_string(st).unwrap();
            if text.contains("secrets.TAURI_SIGNING_PRIVATE_KEY") {
                let at = (name.as_str(), s(st, "name"));
                assert!(
                    at == ("app-bundles", "macOS app, update archive and image")
                        || at == ("checksums", "Sign SHA256SUMS with the update key"),
                    "{at:?}"
                );
            }
        }
    }
}

#[test]
fn the_windows_leg_is_deliberately_unsigned() {
    // Authenticode is a procurement problem, not an engineering one. This
    // keeps that a decision rather than a drift.
    let yml = read(".github/workflows/release.yml");
    assert!(!yml.contains("WINDOWS_CERTIFICATE"));
    assert!(!yml.contains("signtool"));
}

/// The bundle names app-upload's count globs, as extensions.
fn counted_extensions() -> Vec<String> {
    let rel = rel();
    let count = step_named(&rel, "app-upload", "Exactly the five installers arrived");
    let line = count
        .lines()
        .find(|l| l.contains("FOUND=\"$(ls -1 "))
        .expect("the ls line");
    line.split_whitespace()
        .filter_map(|w| w.strip_prefix('*'))
        .map(|w| w.trim_end_matches(['"', ')']).to_string())
        .collect()
}

/// The installers the release publishes, from sp_release::names.
pub fn app_bundles() -> Vec<&'static str> {
    vec![
        names::LINUX_DEB_AMD64,
        names::LINUX_RPM_X86_64,
        names::LINUX_APPIMAGE_X86_64,
        names::MAC_DMG,
        names::WINDOWS_SETUP,
    ]
}

#[test]
fn app_upload_counts_exactly_the_installers_the_release_names() {
    // One glob per installer, and the count app-upload demands is their
    // number: a sixth that arrives (a second .dmg the frozen Swift updater
    // would pick from) fails there.
    let mut exts: Vec<String> = app_bundles()
        .iter()
        .map(|n| n.rsplit_once('.').unwrap().1.to_string())
        .map(|e| format!(".{e}"))
        .collect();
    exts.sort();
    assert_eq!(sorted(counted_extensions()), exts);
    let rel = rel();
    let count = step_named(&rel, "app-upload", "Exactly the five installers arrived");
    assert!(count.contains(&format!(r#"[ "$LINES" -eq {} ]"#, app_bundles().len())));
}

#[test]
fn app_upload_counts_then_uploads_then_publishes_latest_json_last() {
    let rel = rel();
    let job = job(&rel, "app-upload");
    let download = steps(job)
        .into_iter()
        .find(|st| s(st, "uses").contains("download-artifact"))
        .unwrap();
    assert_eq!(s(download.get("with").unwrap(), "path"), "arrivals");
    let count = step_index(job, |st| {
        s(st, "name") == "Exactly the five installers arrived"
    })
    .unwrap();
    let upload = step_index(job, |st| s(st, "name") == "Upload the bundles").unwrap();
    let manifest = step_index(job, |st| s(st, "run").contains("sp-release manifest")).unwrap();
    assert!(count < upload && upload < manifest);
    assert!(s(steps(job)[count], "run").contains("cd arrivals"));
    let mut up = app_bundles()
        .iter()
        .map(|n| format!("*.{}", n.rsplit_once('.').unwrap().1))
        .collect::<Vec<_>>();
    up.extend([
        names::MAC_UPDATE.to_string(),
        format!("{}{}", names::MAC_UPDATE, names::SIGNATURE_EXT),
        names::MANIFEST.to_string(),
    ]);
    assert_eq!(sorted(uploaded_by(job)), sorted(up));
    // The manifest is verified before it goes up, and a prerelease gets none.
    let m = s(steps(job)[manifest], "run");
    let verified = m.find("verify-update").unwrap();
    let published = m.find("gh release upload").unwrap();
    assert!(verified < published);
    assert!(m.contains(r#"[[ "$VERSION" == *-* ]]"#));
    // It writes no checksums file: the checksums job writes the one.
    let text = commands(&run_text(job));
    assert!(!text.contains("SHA256SUMS"), "{text}");
    assert!(!text.contains("sha256sum"), "{text}");
}

// ---- checksums: ONE SHA256SUMS over every download ----------------------

#[test]
fn the_checksums_job_runs_after_every_upload_and_before_the_draft_check() {
    // It hashes what the draft serves, so it must wait for every upload,
    // latest.json included; draft-check then reads its result.
    let rel = rel();
    let sums = job(&rel, "checksums");
    assert_eq!(s(sums, "runs-on"), "ubuntu-latest");
    for upload in ["release", "cli-upload", "app-upload"] {
        assert!(needs(sums).contains(&upload.to_string()), "{upload}");
    }
    assert!(needs(job(&rel, "draft-check")).contains(&"checksums".to_string()));
    // gh is told the repository. The checkout is for building sp-release,
    // which signs SHA256SUMS.
    assert!(serde_yaml::to_string(sums).unwrap().contains("GH_REPO"));
}

#[test]
fn the_checksums_job_lists_every_download_and_uploads_only_sha256sums() {
    let rel = rel();
    let text = step_named(
        &rel,
        "checksums",
        "One SHA256SUMS over every download on the draft",
    );
    let mut want = want_line(&text);
    assert!(text.contains(&format!(
        r#"[[ "$VERSION" == *-* ]] || WANT+=({})"#,
        names::MANIFEST
    )));
    assert_eq!(sorted(want.clone()), sorted(names::downloads(true)));
    want.push(names::MANIFEST.to_string());
    assert_eq!(sorted(want), sorted(names::downloads(false)));
    assert_eq!(
        uploaded_by(job(&rel, "checksums")),
        vec![
            names::CHECKSUMS.to_string(),
            names::CHECKSUMS_SIGNATURE.to_string()
        ]
    );
    // Read from the draft, verified, then uploaded.
    let download = text.find("gh release download").unwrap();
    let verify = text.find("sha256sum -c SHA256SUMS").unwrap();
    let upload = text.find("gh release upload").unwrap();
    assert!(download < verify && verify < upload);
}

#[test]
fn only_the_checksums_job_writes_a_checksums_file_and_none_writes_the_retired_one() {
    let rel = rel();
    for (name, job) in jobs(&rel) {
        let code = commands(&run_text(job));
        if name != "checksums" {
            assert!(
                !uploaded_by(job).contains(&names::CHECKSUMS.to_string()),
                "{name} uploads {}",
                names::CHECKSUMS
            );
            assert!(!code.contains("> SHA256SUMS"), "{name}");
        }
        assert!(
            !uploaded_by(job).contains(&names::RETIRED_APP_CHECKSUMS.to_string()),
            "{name} uploads {}",
            names::RETIRED_APP_CHECKSUMS
        );
        assert!(!code.contains("> SHA256SUMS-app"), "{name}");
    }
    // The checksums job deletes an old run's; draft-check refuses one.
    let sums = step_named(
        &rel,
        "checksums",
        "One SHA256SUMS over every download on the draft",
    );
    assert!(sums.contains(&format!(
        r#"gh release delete-asset "$TAG" {} --yes"#,
        names::RETIRED_APP_CHECKSUMS
    )));
    let check = step_named(
        &rel,
        "draft-check",
        "The draft carries every file the release names",
    );
    assert!(check.contains(&format!("still carries {}", names::RETIRED_APP_CHECKSUMS)));
}

#[test]
fn the_handover_gate_installs_this_release_with_the_shipped_window_before_anything_offers_it() {
    let rel = rel();
    let gate = job(&rel, "handover-gate");
    assert!(s(gate, "runs-on").starts_with("macos-"));
    let text = run_text(gate);
    assert!(text.contains("handover-proof"));
    assert!(text.contains("--pubkey \"$PUB\""));
    assert!(text.contains("latest.json is NOT published"));
}

// ---- release-published.yml: what runs once a person publishes ----------

fn published() -> Value {
    workflow("release-published.yml")
}

#[test]
fn the_published_workflow_runs_when_a_release_is_published_and_at_no_other_time() {
    let wf = published();
    let on = wf.get("on").expect("an on: table");
    let keys: Vec<String> = on
        .as_mapping()
        .expect("on: is a table")
        .keys()
        .map(|k| k.as_str().unwrap().to_string())
        .collect();
    assert_eq!(keys, vec!["release".to_string()]);
    let types: Vec<&str> = on
        .get("release")
        .and_then(|r| r.get("types"))
        .and_then(Value::as_sequence)
        .expect("release: types:")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(types, vec!["published"]);
}

#[test]
fn the_read_back_check_moved_out_of_the_tag_run() {
    let rel = rel();
    assert!(
        !has_job(&rel, "latest-check"),
        "release.yml still has latest-check"
    );
    let wf = published();
    let names: Vec<String> = jobs(&wf).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, strs(&["latest-check"]));
    assert_eq!(needs(job(&wf, "latest-check")), Vec::<String>::new());
}

#[test]
fn no_prerelease_reaches_the_manifest_check() {
    // releases/latest never resolves to a prerelease.
    let wf = published();
    let cond = job_if(job(&wf, "latest-check"));
    assert!(cond.contains("!contains(github.ref_name, '-')"));
    assert!(cond.contains("!github.event.release.prerelease"));
}

#[test]
fn the_published_workflow_writes_nothing_and_holds_no_secret() {
    // Every job here only reads this repository.
    let wf = published();
    for (name, job) in jobs(&wf) {
        assert!(job.get("permissions").is_none(), "{name} asks for more");
        assert!(
            !commands(&run_text(job)).contains("gh release"),
            "{name} changes a release"
        );
    }
    let text = read(".github/workflows/release-published.yml");
    assert!(
        !Regex::new(r"secrets\.[A-Z]").unwrap().is_match(&text),
        "a secret reached it"
    );
}

#[test]
fn latest_check_reads_the_manifest_back_after_publish() {
    let wf = published();
    let text = run_text(job(&wf, "latest-check"));
    assert!(text.contains("sp-release check-latest --deep"));
    // No --version: the question is what the ENDPOINT serves.
    assert!(!text.contains("--version"));
}

#[test]
fn no_release_build_carries_the_handover_rehearsals_updater_values() {
    // Review item O10. Nothing in any workflow turns the rehearsal on.
    for f in workflow_files() {
        let text = read(&format!(".github/workflows/{f}"));
        assert!(!text.contains("--features rehearsal"), "{f}");
        assert!(!text.contains("features: rehearsal"), "{f}");
        for line in text.lines() {
            let l = line.trim();
            assert!(
                !l.starts_with("METHOD_READER_UPDATE_")
                    && !l.contains("METHOD_READER_UPDATE_ENDPOINT="),
                "{f} sets a rehearsal value: {l}"
            );
        }
    }
    // The build step refuses before cargo starts if one is set.
    let step = step_named(
        &rel(),
        "app-bundles",
        "Build the window and the release tools",
    );
    let guard_end = step
        .find("if [ \"$RUNNER_OS\" = macOS ]")
        .expect("the build");
    let guard = &step[..guard_end];
    assert!(guard.contains("METHOD_READER_UPDATE_INSECURE"));
    let dir = Scratch::new("rehearsal-guard");
    let ok = bash(guard, dir.path(), &[("PATH", "/usr/bin:/bin")]);
    assert!(ok.status.success(), "{}", said(&ok));
    for v in [
        "METHOD_READER_UPDATE_ENDPOINT",
        "METHOD_READER_UPDATE_PUBKEY",
        "METHOD_READER_UPDATE_INSECURE",
    ] {
        for value in ["http://127.0.0.1:8765/latest.json", ""] {
            let out = bash(guard, dir.path(), &[("PATH", "/usr/bin:/bin"), (v, value)]);
            assert!(!out.status.success(), "{v}={value:?} passed the guard");
            assert!(said(&out).contains(v), "{}", said(&out));
        }
    }
}

/// A step that brings a secret into the job: a `secrets.` value anywhere in
/// it, the token GitHub hands every job aside.
fn carries_a_secret(step: &Value) -> bool {
    let text = serde_yaml::to_string(step).expect("a step");
    text.replace("secrets.GITHUB_TOKEN", "")
        .contains("secrets.")
}

/// A step that compiles or runs code a build can pull in: cargo's build,
/// run, test, clippy and xtask (a crate's build script runs in each).
fn builds(step: &Value) -> bool {
    let re = Regex::new(r"\bcargo (build|run|test|clippy|xtask|install)\b").unwrap();
    re.is_match(&commands(s(step, "run")))
}

#[test]
fn no_build_runs_after_a_signing_secret_is_in_the_job() {
    // A build script, or any crate a build pulls in, runs arbitrary code.
    // So in every job the builds come first, and a secret (the Developer ID
    // certificate, the notary key, the update key) enters only after the
    // last of them.
    let wf = rel();
    let mut with_secrets = 0;
    for (name, job) in jobs(&wf) {
        let all = steps(job);
        let Some(first) = all.iter().position(|st| carries_a_secret(st)) else {
            continue;
        };
        with_secrets += 1;
        let late: Vec<String> = all[first..]
            .iter()
            .filter(|st| builds(st))
            .map(|st| s(st, "name").to_string())
            .collect();
        assert_eq!(late, Vec::<String>::new(), "{name} builds after a secret");
    }
    // The CLI's notarization and the window's signing, at least.
    assert!(with_secrets >= 2, "{with_secrets}");
}

const NOT_REHEARSAL: &str = "!inputs.rehearse_version";

#[test]
fn a_rehearsal_writes_to_no_release_and_makes_no_tag() {
    // Dispatched with rehearse_version, release.yml builds, signs,
    // notarizes, staples and checks everything, and every step or job that
    // would create, edit or upload to a release is skipped. A real tag run
    // leaves the input empty and is unchanged.
    let wf = rel();
    let input = &wf["on"]["workflow_dispatch"]["inputs"]["rehearse_version"];
    assert_eq!(s(input, "type"), "string");
    assert_eq!(s(input, "default"), "");
    let writes =
        Regex::new(r"\bgh release (create|edit|upload|delete)\b|\bgit (tag|push)\b").unwrap();
    let mut guarded = 0;
    for (name, job) in jobs(&wf) {
        let job_skips = job_if(job).contains(NOT_REHEARSAL);
        for st in steps(job) {
            let run = commands(s(st, "run"));
            if writes.is_match(&run) {
                assert!(
                    job_skips || s(st, "if").contains(NOT_REHEARSAL),
                    "{name}: {} writes to a release in a rehearsal",
                    s(st, "name")
                );
                guarded += 1;
            }
        }
    }
    assert!(guarded >= 4, "the scan found only {guarded} release writes");
    // One spelling of the tag everywhere: the rehearsal's version, or the
    // ref a real run was started from.
    let tag = "${{ inputs.rehearse_version && format('v{0}', inputs.rehearse_version) || github.ref_name }}";
    let text = read(".github/workflows/release.yml");
    assert!(!text.contains("TAG: ${{ github.ref_name }}"));
    assert!(text.matches(tag).count() >= 10);
}

#[test]
fn a_rehearsal_still_runs_every_build_and_signature() {
    let wf = rel();
    for name in ["checks", "release", "cli", "app-bundles", "handover-gate"] {
        assert!(
            !job_if(job(&wf, name)).contains("rehearse_version"),
            "{name} must run in a rehearsal"
        );
        for st in steps(job(&wf, name)) {
            if s(st, "if").contains(NOT_REHEARSAL) {
                assert_eq!(
                    (name, s(st, "name")),
                    ("release", "Write the draft release"),
                    "only the draft is skipped inside the building jobs"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn a_rehearsal_refuses_any_ref_but_main() {
    // The real signing secrets are in a rehearsal, so the keys meet only
    // reviewed code. The guard is the first step of checks, which every
    // other job needs.
    let wf = rel();
    let all = steps(job(&wf, "checks"));
    let i = all
        .iter()
        .position(|st| s(st, "name") == "A rehearsal runs only from main")
        .expect("the main-only guard");
    assert!(i <= 1, "the guard comes right after checkout");
    assert_eq!(s(all[i], "if"), "${{ inputs.rehearse_version }}");
    for (name, j) in jobs(&wf) {
        if name != "checks" {
            assert!(
                needs(j).iter().any(|n| n == "checks") || !needs(j).is_empty(),
                "{name} runs without waiting for the checks"
            );
        }
    }
    let dir = Scratch::new("rehearse-ref");
    let run = |r: &str| {
        bash(
            s(all[i], "run"),
            dir.path(),
            &[("REF", r), ("PATH", "/usr/bin:/bin")],
        )
        .status
        .success()
    };
    assert!(run("refs/heads/main"));
    assert!(!run("refs/heads/feature"));
    assert!(!run("refs/tags/v9.9.9"));
}

#[test]
fn the_v0_13_0_apps_own_updater_must_install_every_release() {
    // Every Mac on 0.13.0 gets the next release from 0.13.0's own updater.
    // The handover gate runs that updater, built from the v0.13.0 commit,
    // with its REAL key (no --pubkey: the throwaway-key flag is for a run on
    // a developer's Mac), on an old copy named Screepub Desktop.app, and
    // then the new build must rename itself on its first start.
    let wf = rel();
    let step = step_named(
        &wf,
        "handover-gate",
        "The v0.13.0 app's own updater installs this release",
    );
    let run = commands(&step);
    assert!(run.contains("update-proof-0-13"), "{run}");
    assert!(
        !run.contains("--pubkey"),
        "the release always uses 0.13.0's real key"
    );
    assert!(run.contains("--current 0.13.0"));
    assert!(run.contains("Screepub Desktop.app"));
    assert!(run.contains(r#"test -d "old013/Applications/Method Reader.app""#));
    let manifest = read("rust/update-proof-0.13/Cargo.toml");
    let rev = Regex::new(r#"sp-update = \{ git = "https://github.com/method-production-suite/method-reader", rev = "([0-9a-f]{40})" \}"#)
        .unwrap()
        .captures(&manifest)
        .expect("sp-update pinned to a full commit of this repository")[1]
        .to_string();
    assert_eq!(
        rev, "0b0544e1804b703de59554190fe245936f8fc02c",
        "the v0.13.0 tag's commit"
    );
}

#[test]
fn the_checksums_are_signed_with_the_update_key_only_after_the_tools_are_built() {
    let rel = rel();
    let sums = job(&rel, "checksums");
    let all = steps(sums);
    let build = all
        .iter()
        .position(|st| s(st, "name") == "Build the release tools")
        .expect("a build step");
    let sign = all
        .iter()
        .position(|st| s(st, "name") == "Sign SHA256SUMS with the update key")
        .expect("a signing step");
    assert!(build < sign);
    // The key is in the signing step alone, never the job.
    assert!(sums.get("env").is_none());
    for (i, st) in all.iter().enumerate() {
        let has_key = serde_yaml::to_string(st)
            .unwrap()
            .contains("TAURI_SIGNING_PRIVATE_KEY:");
        assert_eq!(has_key, i == sign, "step {i}");
    }
    let text = s(all[sign], "run").to_string();
    assert!(text.contains("sp-release sign-file --file downloads/SHA256SUMS"));
    assert!(text.contains("sp-release verify-file --file downloads/SHA256SUMS"));
    assert!(text.contains("gh release upload \"$TAG\" downloads/SHA256SUMS.minisig --clobber"));
    // The signer and the checker fall back to the key the app ships
    // (sp_update_config::PUBKEY). A --pubkey here could point them at
    // another key, and the signature would no longer be one the app checks.
    let mac_step = s(
        steps(job(&rel, "app-bundles"))
            .into_iter()
            .find(|st| s(st, "name") == "macOS app, update archive and image")
            .expect("the Mac step"),
        "run",
    )
    .to_string();
    for (where_, run) in [("checksums", &text), ("app-bundles", &mac_step)] {
        for line in run
            .lines()
            .filter(|l| l.contains("sign-file") || l.contains("verify-file"))
        {
            assert!(!line.contains("--pubkey"), "{where_}: {line}");
        }
    }
    // A re-run starts from the downloads alone.
    assert!(run_text(sums).contains("rm -f SHA256SUMS SHA256SUMS-app SHA256SUMS.minisig"));
    // And the draft is not complete without it.
    assert!(run_text(job(&rel, "draft-check")).contains(" SHA256SUMS SHA256SUMS.minisig "));
    // Every rehearsal proves the key makes one, in the leg that holds it.
    let mac = run_text(job(&rel, "app-bundles"));
    assert!(mac.contains(r#""$SP" sign-file --file "$RUNNER_TEMP/minisig-probe""#));
}

#[test]
fn an_intel_mac_on_0_9_0_is_proved_to_take_the_release_before_anything_is_published() {
    let rel = rel();
    let gate = job(&rel, "handover-gate");
    let proof = steps(gate)
        .into_iter()
        .find(|st| s(st, "name") == "The v0.9.0 window on an Intel Mac installs this release")
        .expect("the 0.9.0 proof");
    let text = s(proof, "run").to_string();
    // 0.9.0's own update file and key, and its updater built for Intel.
    assert!(text.contains("OLD=v0.9.0"));
    assert!(text.contains("--pattern Screepub-macOS.app.tar.gz"));
    assert!(text.contains(r#"git show "$OLD:desktop/src-tauri/tauri.conf.json""#));
    assert!(text.contains("cargo build --release --locked --target x86_64-apple-darwin"));
    // Run under Rosetta, so it asks for the Intel key as a real Intel Mac does.
    assert!(text.contains(
        "arch -x86_64 rust/handover-proof/target/x86_64-apple-darwin/release/handover-proof"
    ));
    assert!(text.contains("--current 0.9.0"));
    // Its update file lists the Intel key alone, so success proves the lookup.
    assert!(text.contains(
        r#"[ "$(jq -r '.platforms | keys | join(" ")' site090/latest.json)" = darwin-x86_64 ]"#
    ));
    // The installed app's Intel slice starts and renames the old bundle.
    assert!(
        text.contains(r#"arch -x86_64 "$INSTALLED/Contents/MacOS/method-reader-app" --version"#)
    );
    assert!(text.contains(r#"test -d "old090/Applications/Method Reader.app""#));
    // Nothing is published unless the gate passes.
    for later in ["app-upload", "checksums"] {
        let needs = serde_yaml::to_string(job(&rel, later).get("needs").unwrap()).unwrap();
        assert!(needs.contains("handover-gate"), "{later}");
    }
}

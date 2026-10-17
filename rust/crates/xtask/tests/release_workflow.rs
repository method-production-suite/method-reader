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
//! release-published.yml bump the Homebrew tap and read the release back.
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
            "draft-check",
            vec!["app-upload", "cli-upload", "handover-gate", "release"],
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
    // default token, so a workflow that published would leave the tap
    // un-bumped and nothing red (tap-freshness.yml's old trigger, v0.5.3).
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

/// Every file a complete release carries, from sp_release::names.
fn every_release_file() -> Vec<String> {
    let mut all: Vec<String> = [
        names::CLI_MAC_ARM64,
        names::CLI_LINUX_X64,
        names::CLI_LINUX_ARM64,
        names::CLI_WINDOWS_X64,
        "SHA256SUMS",
    ]
    .iter()
    .map(|n| n.to_string())
    .collect();
    all.extend(app_bundles().iter().map(|n| n.to_string()));
    all.extend([
        names::MAC_UPDATE.to_string(),
        format!("{}{}", names::MAC_UPDATE, names::SIGNATURE_EXT),
        "SHA256SUMS-app".to_string(),
        names::MANIFEST.to_string(),
    ]);
    sorted(all)
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
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("WANT=("))
        .expect("a WANT=( line");
    let mut want: Vec<String> = line
        .trim()
        .trim_start_matches("WANT=(")
        .trim_end_matches(')')
        .split_whitespace()
        .map(str::to_string)
        .collect();
    // latest.json is added for a release, never for a prerelease.
    assert!(text.contains(&format!(
        r#"[[ "$VERSION" == *-* ]] || WANT+=({})"#,
        names::MANIFEST
    )));
    want.push(names::MANIFEST.to_string());
    assert_eq!(sorted(want), every_release_file());
    // And each of those is uploaded by some job (the globs by extension).
    let mut uploaded = vec![];
    for name in ["release", "cli-upload", "app-upload"] {
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
    assert_eq!(uploaded_by(release), vec![names::CLI_MAC_ARM64.to_string()]);
    let text = run_text(release);
    assert!(text.contains("--target aarch64-apple-darwin"));
    // The tool and its old name, each under its own identifier.
    assert!(text.contains(
        r#"codesign --force --options runtime --timestamp --identifier "${pair##*:}" --sign "$SIGN_IDENTITY" "$B""#
    ));
    assert!(text.contains(r#""$BIN:method-reader-cli-arm64" "$ALIAS:screepub-cli-arm64""#));
    assert!(text.contains(r#"cli-archive --binary "$BIN" --alias "$ALIAS" --kind macos-arm64"#));
    assert!(text.contains("xcrun notarytool submit"));
    // Signed and notarized, THEN packaged, so the tarball holds the signed bytes.
    let sign = text.find("codesign --force").unwrap();
    let notarize = text.find("xcrun notarytool submit").unwrap();
    let package = text.find("cli-archive").unwrap();
    assert!(sign < notarize && notarize < package);
    // The notes' checksum, the upload and the crib sheet read one folder.
    let out = "$RUNNER_TEMP/cli-mac";
    assert!(text.contains(&format!(
        r#"cd "{out}" && shasum -a 256 {}"#,
        names::CLI_MAC_ARM64
    )));
    assert!(
        text.matches(&format!("{out}/{}", names::CLI_MAC_ARM64))
            .count()
            >= 2
    );
}

#[test]
fn the_install_line_the_release_job_writes_says_apple_silicon() {
    let rel = rel();
    let text = run_text(job(&rel, "release"));
    let mac = text
        .lines()
        .find(|l| l.contains("**Mac:**"))
        .expect("a Mac line");
    assert!(mac.contains(names::MAC_DMG));
    assert!(mac.contains("Apple silicon"));
    assert!(mac.contains("macOS 13 or later"));
    assert_eq!(names::MAC_MINIMUM, "13.0");
    assert!(!Regex::new(r"(?i)universal|intel").unwrap().is_match(mac));
}

#[test]
fn nothing_builds_hashes_or_uploads_an_intel_or_universal_mac_build() {
    let yml = read(".github/workflows/release.yml");
    assert!(!yml.contains("screepub-cli-macos-x64.tar.gz"));
    let code = commands(&yml);
    for gone in [
        "x86_64-apple-darwin",
        "universal-apple-darwin",
        "--universal",
        "lipo -create",
    ] {
        assert!(!code.contains(gone), "{gone}");
    }
    // Each Mac binary is checked to be arm64 alone before it is signed.
    assert_eq!(code.matches(r#"lipo -archs"#).count(), 2);
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
    // And all three arrive on the page with their checksums, verified after
    // the artifact round trip and before the upload.
    let up = job(&rel, "cli-upload");
    let mut want = others.clone();
    want.push("SHA256SUMS".into());
    assert_eq!(sorted(uploaded_by(up)), sorted(want));
    let run = run_text(up);
    let verify = run.find("sha256sum -c SHA256SUMS").expect("a -c pass");
    let gh = run.find("gh release upload").unwrap();
    assert!(verify < gh);
    assert!(run.contains("cd cli"));
    // No checkout in that job, so gh is told the repository.
    assert!(serde_yaml::to_string(up).unwrap().contains("GH_REPO"));
    // It never touches the Mac downloads the release job owns.
    for mac in [names::CLI_MAC_ARM64, names::MAC_DMG] {
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
    assert!(
        text.contains("cargo build --release --locked -p sp-app --target aarch64-apple-darwin")
    );
    let upload = steps(job)
        .into_iter()
        .find(|st| s(st, "uses").contains("upload-artifact"))
        .expect("an upload-artifact step");
    assert_eq!(s(upload.get("with").unwrap(), "if-no-files-found"), "error");
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
    // The update key reaches the Mac step only.
    let rel = rel();
    for (name, job) in jobs(&rel) {
        for st in steps(job) {
            let text = serde_yaml::to_string(st).unwrap();
            if text.contains("secrets.TAURI_SIGNING_PRIVATE_KEY") {
                assert_eq!(
                    (name.as_str(), s(st, "name")),
                    ("app-bundles", "macOS app, update archive and image")
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

/// The bundle names app-upload's checksum line globs, as extensions.
fn summed_extensions() -> Vec<String> {
    let rel = rel();
    let sums = step_named(
        &rel,
        "app-upload",
        "Checksums from what arrived, then the count",
    );
    let line = sums
        .lines()
        .find(|l| l.contains("> SHA256SUMS-app"))
        .expect("the sha256sum line");
    line.split_whitespace()
        .filter_map(|w| w.strip_prefix('*'))
        .map(str::to_string)
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
fn app_upload_sums_exactly_the_installers_the_release_names() {
    // One glob per installer, and the count app-upload demands is their
    // number: a sixth that arrives (a second .dmg the frozen Swift updater
    // would pick from) fails there.
    let mut exts: Vec<String> = app_bundles()
        .iter()
        .map(|n| n.rsplit_once('.').unwrap().1.to_string())
        .map(|e| format!(".{e}"))
        .collect();
    exts.sort();
    assert_eq!(sorted(summed_extensions()), exts);
    let rel = rel();
    let sums = step_named(
        &rel,
        "app-upload",
        "Checksums from what arrived, then the count",
    );
    assert!(sums.contains(&format!(r#"[ "$LINES" -eq {} ]"#, app_bundles().len())));
}

#[test]
fn app_upload_verifies_then_uploads_then_publishes_latest_json_last() {
    let rel = rel();
    let job = job(&rel, "app-upload");
    let download = steps(job)
        .into_iter()
        .find(|st| s(st, "uses").contains("download-artifact"))
        .unwrap();
    assert_eq!(s(download.get("with").unwrap(), "path"), "arrivals");
    let verify = step_index(job, |st| {
        s(st, "run").contains("sha256sum -c SHA256SUMS-app")
    })
    .unwrap();
    let upload = step_index(job, |st| s(st, "name") == "Upload the bundles").unwrap();
    let manifest = step_index(job, |st| s(st, "run").contains("sp-release manifest")).unwrap();
    assert!(verify < upload && upload < manifest);
    assert!(s(steps(job)[verify], "run").contains("cd arrivals"));
    let mut up = app_bundles()
        .iter()
        .map(|n| format!("*.{}", n.rsplit_once('.').unwrap().1))
        .collect::<Vec<_>>();
    up.extend([
        names::MAC_UPDATE.to_string(),
        format!("{}{}", names::MAC_UPDATE, names::SIGNATURE_EXT),
        "SHA256SUMS-app".to_string(),
        names::MANIFEST.to_string(),
    ]);
    assert_eq!(sorted(uploaded_by(job)), sorted(up));
    // The manifest is verified before it goes up, and a prerelease gets none.
    let m = s(steps(job)[manifest], "run");
    let verified = m.find("verify-update").unwrap();
    let published = m.find("gh release upload").unwrap();
    assert!(verified < published);
    assert!(m.contains(r#"[[ "$VERSION" == *-* ]]"#));
    // Its checksums file is its own: SHA256SUMS is the CLI's.
    let text = run_text(job);
    assert!(text.contains("SHA256SUMS-app"));
    assert!(!Regex::new(r"SHA256SUMS($|[^-])").unwrap().is_match(&text));
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

#[test]
fn the_release_run_leaves_a_crib_sheet_for_bumping_the_tap_by_hand() {
    let crib = step_named(&rel(), "release", "Tap bump crib sheet");
    assert!(crib.contains("cargo xtask bump-tap"));
    assert!(crib.contains(names::CLI_MAC_ARM64));
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
fn the_tap_and_the_read_back_checks_moved_out_of_the_tag_run() {
    let rel = rel();
    for gone in ["tap", "tap-check", "latest-check"] {
        assert!(!has_job(&rel, gone), "release.yml still has {gone}");
    }
    let wf = published();
    let graph: Vec<(&str, Vec<&str>)> = vec![
        ("tap", vec![]),
        ("tap-check", vec!["tap"]),
        ("latest-check", vec![]),
    ];
    let names: Vec<String> = jobs(&wf).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        sorted(names),
        sorted(graph.iter().map(|(n, _)| n.to_string()).collect())
    );
    for (name, wants) in graph {
        assert_eq!(
            sorted(needs(job(&wf, name))),
            strs(&wants),
            "{name} waits for the wrong jobs"
        );
    }
}

#[test]
fn no_prerelease_reaches_the_tap_or_the_manifest_check() {
    // A hyphen in the tag must never become what `brew install` hands out,
    // and releases/latest never resolves to a prerelease.
    let wf = published();
    for name in ["tap", "tap-check", "latest-check"] {
        let cond = job_if(job(&wf, name));
        assert!(cond.contains("!contains(github.ref_name, '-')"), "{name}");
        assert!(cond.contains("!github.event.release.prerelease"), "{name}");
    }
    // A job-level `if` with no status function gets an implicit success()
    // that also looks further up the chain; tap-check names its own.
    let cond = job_if(job(&wf, "tap-check"));
    assert!(cond.contains("!cancelled()"));
    assert!(cond.contains("needs.tap.result == 'success'"));
}

#[test]
fn the_published_workflow_writes_nothing_to_this_repository() {
    // The tap is pushed with TAP_TOKEN, which can reach the tap and nothing
    // else; every job here only reads this repository.
    let wf = published();
    for (name, job) in jobs(&wf) {
        assert!(job.get("permissions").is_none(), "{name} asks for more");
        assert!(
            !commands(&run_text(job)).contains("gh release"),
            "{name} changes a release"
        );
    }
}

#[test]
fn the_tap_token_reaches_only_the_tap_step() {
    let text = read(".github/workflows/release-published.yml");
    let mut declared: Vec<String> = Regex::new(r"secrets\.[A-Z0-9_]+")
        .unwrap()
        .find_iter(&text)
        .map(|m| m.as_str().to_string())
        .collect();
    declared.sort();
    declared.dedup();
    assert_eq!(declared, strs(&["secrets.TAP_TOKEN"]));
    let wf = published();
    let mut given = 0;
    for (name, job) in jobs(&wf) {
        for st in steps(job) {
            if serde_yaml::to_string(st).unwrap().contains("${{ secrets.") {
                given += 1;
                assert_eq!(
                    (name.as_str(), s(st, "name")),
                    ("tap", "Bump the Homebrew tap")
                );
            }
        }
    }
    assert_eq!(given, 1);
}

#[test]
fn the_tap_is_bumped_by_the_xtask_and_checked_after() {
    let wf = published();
    let tap = run_text(job(&wf, "tap"));
    assert!(tap.contains("cargo xtask bump-tap \"$V\""));
    // A missing token is a notice, not a failure: the crib sheet and the
    // hand path stay.
    assert!(tap.contains("TAP_TOKEN is not set"));
    assert!(run_text(job(&wf, "tap-check")).contains("xtask check-tap"));
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

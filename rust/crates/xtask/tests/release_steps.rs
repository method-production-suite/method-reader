//! release.yml's bash, executed (was tests/release-app-step.test.ts and
//! tests/release-version-gate.test.ts, rewritten for the Rust release).
//!
//! release_workflow.rs proves the steps are THERE and in order. That is
//! necessary but not sufficient: a `contains("NOTHING WAS CHECKED")` also
//! passes if the loop globbed a folder nothing writes to, and a gate that
//! reads the working tree instead of the tagged commit looks the same on
//! the page. A workflow cannot run here (it needs a tagged push on a
//! GitHub runner), but its `run:` blocks are just bash, so this file pulls
//! the ACTUAL step text out of the committed release.yml and runs it, with
//! stub executables first on PATH so the step's own command lines are
//! recorded rather than run. Every `${{ }}` in these steps sits in `env:`,
//! so the text executed here is the text a runner executes.
//!
//! Nothing real is signed, notarized, uploaded or built: the stubs stand
//! in for codesign, xcrun, gh, cargo and sp-release, and HOME is the
//! scratch folder. Unix only: these steps run under bash on every runner,
//! and /bin/bash on a Mac is the 3.2 the macOS legs get.
#![cfg(unix)]

mod repo_common;

use repo_common::*;
use sp_release::names;
use std::path::Path;
use std::process::{Command, Output};

const LOG: &str = "calls.log";

/// A stub at `rel` that appends `<name>\t<args...>` to the scratch's one
/// shared log, so the ORDER across tools is visible, then runs `then`.
fn tool(sc: &Scratch, rel: &str, then: &str) {
    let log = sc.join(LOG);
    sc.script(
        rel,
        &format!(
            "{{ printf '%s' \"$(basename \"$0\")\"; printf '\\t%s' \"$@\"; printf '\\n'; }} >> '{}'\n{then}\nexit 0\n",
            log.display()
        ),
    );
}

fn log(sc: &Scratch) -> Vec<String> {
    std::fs::read_to_string(sc.join(LOG))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The index of the first logged call starting with `prefix` (tab-joined).
fn at(calls: &[String], prefix: &str) -> usize {
    calls
        .iter()
        .position(|c| c.starts_with(prefix))
        .unwrap_or_else(|| panic!("no call starting {prefix:?} in {calls:#?}"))
}

/// PATH: the stubs first, then the system's own tools.
fn path_with(stubs: &Path) -> String {
    format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", stubs.display())
}

fn release() -> serde_yaml::Value {
    workflow("release.yml")
}

// ---- the checks job: notes and tag, read from the TAGGED commit --------

/// Everything up to the main-ancestry check, which needs a real `origin`.
fn notes_gate() -> String {
    let full = step_named(
        &release(),
        "checks",
        "Release notes and tag are publishable",
    );
    let marker = "# The tagged commit must be ON main.";
    let cut = full
        .find(marker)
        .expect("the ancestry check's marker: was the step reordered?");
    full[..cut].to_string()
}

/// A real, local-only git repository: the files given, committed once.
fn repo(sc: &Scratch, files: &[(&str, &[u8])]) -> String {
    for (rel, bytes) in files {
        sc.write(&format!("repo/{rel}"), bytes);
    }
    let git = |args: &[&str]| -> Output {
        let out = Command::new("git")
            .args(args)
            .current_dir(sc.join("repo"))
            .env("HOME", sc.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {}", said(&out));
        out
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "gate-test@example.invalid"]);
    git(&["config", "user.name", "Gate Test"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "gate fixture"]);
    String::from_utf8(git(&["rev-parse", "HEAD"]).stdout)
        .unwrap()
        .trim()
        .to_string()
}

fn run_notes_gate(sc: &Scratch, sha: &str, tag: &str) -> Output {
    let path = path_with(&sc.join("bin"));
    bash(
        &notes_gate(),
        &sc.join("repo"),
        &[
            ("PATH", &path),
            ("TAG", tag),
            ("GITHUB_SHA", sha),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
        ],
    )
}

#[test]
fn notes_gate_passes_a_tag_with_its_notes() {
    let sc = Scratch::new("notes-ok");
    let sha = repo(&sc, &[("docs/releases/0.12.0.md", b"# Notes\n\nBody.\n")]);
    let out = run_notes_gate(&sc, &sha, "v0.12.0");
    assert!(out.status.success(), "{}", said(&out));
}

#[test]
fn notes_gate_refuses_a_ref_that_is_not_a_version_tag() {
    let sc = Scratch::new("notes-tag");
    let sha = repo(&sc, &[("docs/releases/0.12.0.md", b"# Notes\n")]);
    for tag in ["main", "v0.12", "0.12.0x", "vnext"] {
        let out = run_notes_gate(&sc, &sha, tag);
        assert!(!out.status.success(), "{tag}");
        assert!(
            said(&out).contains("is not vMAJOR.MINOR.PATCH"),
            "{tag}: {}",
            said(&out)
        );
    }
    // A prerelease suffix is a version.
    sc.write("repo/docs/releases/0.12.0-rc1.md", "# rc\n");
    let sha = repo(&sc, &[]);
    assert!(run_notes_gate(&sc, &sha, "v0.12.0-rc1").status.success());
}

#[test]
fn notes_gate_reads_the_tagged_commit_not_the_working_tree() {
    // Present on disk but never committed: the tag would ship no notes.
    let sc = Scratch::new("notes-uncommitted");
    let sha = repo(&sc, &[("README.md", b"x\n")]);
    sc.write("repo/docs/releases/0.12.0.md", "# Notes\n");
    let out = run_notes_gate(&sc, &sha, "v0.12.0");
    assert!(!out.status.success());
    assert!(
        said(&out).contains("missing at the tagged commit"),
        "{}",
        said(&out)
    );
    // And the other way: committed, then deleted from disk, still passes.
    let sc = Scratch::new("notes-deleted");
    let sha = repo(&sc, &[("docs/releases/0.12.0.md", b"# Notes\n")]);
    std::fs::remove_file(sc.join("repo/docs/releases/0.12.0.md")).unwrap();
    assert!(run_notes_gate(&sc, &sha, "v0.12.0").status.success());
}

#[test]
fn notes_gate_refuses_empty_notes_and_notes_that_are_not_utf8() {
    let sc = Scratch::new("notes-empty");
    let sha = repo(&sc, &[("docs/releases/0.12.0.md", b"")]);
    let out = run_notes_gate(&sc, &sha, "v0.12.0");
    assert!(!out.status.success());
    assert!(said(&out).contains("is empty or exceeds"), "{}", said(&out));
    let sc = Scratch::new("notes-latin1");
    let sha = repo(&sc, &[("docs/releases/0.12.0.md", b"# Caf\xe9\n")]);
    let out = run_notes_gate(&sc, &sha, "v0.12.0");
    assert!(!out.status.success());
    assert!(said(&out).contains("not valid UTF-8"), "{}", said(&out));
}

#[test]
fn notes_gate_refuses_notes_that_carry_a_checksum() {
    // The workflow appends the real ones; two on one page both look official.
    for (tag, body) in [
        ("sha", "SHA-256 of the image: see below\n".to_string()),
        ("hex", format!("`{}`\n", "ab".repeat(32))),
    ] {
        let sc = Scratch::new(&format!("notes-{tag}"));
        let sha = repo(&sc, &[("docs/releases/0.12.0.md", body.as_bytes())]);
        let out = run_notes_gate(&sc, &sha, "v0.12.0");
        assert!(!out.status.success(), "{tag}");
        assert!(said(&out).contains("contains a checksum"), "{}", said(&out));
    }
}

// ---- the checks job: the one version file, from the TAGGED commit ------

const CARGO_TOML: &str = "[workspace]\nmembers = []\n\n[workspace.dependencies]\nfoo = { version = \"9.9.9\" }\n\n[workspace.package]\nedition = \"2024\"\nversion = \"0.12.0\"\n";

/// Run the version step with a stub cargo that keeps the file it was
/// handed; returns the step's output and the cargo call.
fn run_version_step(sc: &Scratch, sha: &str, tag: &str) -> (Output, Vec<String>) {
    tool(
        sc,
        "bin/cargo",
        &format!(
            "while [ $# -gt 0 ]; do [ \"$1\" = --cargo-toml ] && cp \"$2\" '{}'; shift; done",
            sc.join("handed.toml").display()
        ),
    );
    std::fs::create_dir_all(sc.join("temp")).unwrap();
    let path = path_with(&sc.join("bin"));
    let temp = sc.join("temp").display().to_string();
    let out = bash(
        &step_named(&release(), "checks", "The workspace version is the tag's"),
        &sc.join("repo/rust"),
        &[
            ("PATH", &path),
            ("TAG", tag),
            ("GITHUB_SHA", sha),
            ("RUNNER_TEMP", &temp),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
        ],
    );
    (out, log(sc))
}

#[test]
fn version_step_hands_sp_release_the_tagged_cargo_toml_and_the_bare_version() {
    let sc = Scratch::new("version-ok");
    let sha = repo(&sc, &[("rust/Cargo.toml", CARGO_TOML.as_bytes())]);
    // A stray edit after the tag: the gate must not see it.
    sc.write(
        "repo/rust/Cargo.toml",
        "[workspace.package]\nversion = \"9.9.9\"\n",
    );
    let (out, calls) = run_version_step(&sc, &sha, "v0.12.0");
    assert!(out.status.success(), "{}", said(&out));
    let temp = sc.join("temp/Cargo.toml").display().to_string();
    assert_eq!(
        calls,
        vec![format!(
            "cargo\trun\t--locked\t--quiet\t-p\tsp-release\t--\tversion-check\t--version\t0.12.0\t--cargo-toml\t{temp}"
        )]
    );
    // What sp-release then judges is the TAGGED file, and its rule is not
    // fooled by the dependency's version that comes first.
    let handed = std::fs::read_to_string(sc.join("handed.toml")).unwrap();
    assert_eq!(handed, CARGO_TOML);
    assert_eq!(
        sp_release::version::workspace_version(&handed).as_deref(),
        Some("0.12.0")
    );
}

#[test]
fn version_step_fails_before_sp_release_when_the_tag_has_no_cargo_toml() {
    let sc = Scratch::new("version-missing");
    let sha = repo(&sc, &[("rust/README.md", b"x\n")]);
    let (out, calls) = run_version_step(&sc, &sha, "v0.12.0");
    assert!(!out.status.success());
    assert_eq!(calls, Vec::<String>::new());
}

// ---- the release job: the draft, under the Mac's bash 3.2 --------------

fn run_draft_step(tag: &str, exists: bool) -> (Output, Vec<String>) {
    let sc = Scratch::new("draft");
    let version = tag.trim_start_matches('v');
    sc.write(&format!("docs/releases/{version}.md"), "# Notes\n");
    sc.write(&format!("temp/cli-mac/{}", names::CLI_MAC_ARM64), "tarball");
    tool(
        &sc,
        "bin/gh",
        &format!("[ \"$2\" = view ] && exit {}", if exists { 0 } else { 1 }),
    );
    let path = path_with(&sc.join("bin"));
    let temp = sc.join("temp").display().to_string();
    let out = bash(
        &step_named(&release(), "release", "Write the draft release"),
        sc.path(),
        &[
            ("PATH", &path),
            ("TAG", tag),
            ("GH_TOKEN", "t"),
            ("RUNNER_TEMP", &temp),
        ],
    );
    let calls = log(&sc);
    (out, calls)
}

#[test]
fn draft_step_creates_a_draft_with_no_prerelease_flag_for_an_ordinary_tag() {
    // v0.5.0 died here, after notarization: an empty PRE expanded bare
    // under set -u on bash 3.2. This runs the step under /bin/bash.
    let (out, calls) = run_draft_step("v0.12.0", false);
    assert!(out.status.success(), "{}", said(&out));
    let create = &calls[at(&calls, "gh\trelease\tcreate")];
    assert!(create.contains("\t--draft"), "{create}");
    assert!(!create.contains("--prerelease"), "{create}");
    assert!(create.contains("\tMethod Reader 0.12.0\t"), "{create}");
    let upload = at(&calls, "gh\trelease\tupload");
    assert!(calls[upload].contains(names::CLI_MAC_ARM64));
    assert!(at(&calls, "gh\trelease\tcreate") < upload);
}

#[test]
fn draft_step_marks_a_prerelease_and_edits_an_existing_draft() {
    let (out, calls) = run_draft_step("v0.12.0-rc1", false);
    assert!(out.status.success(), "{}", said(&out));
    assert!(calls[at(&calls, "gh\trelease\tcreate")].ends_with("\t--prerelease"));
    let (out, calls) = run_draft_step("v0.12.0", true);
    assert!(out.status.success(), "{}", said(&out));
    assert!(!calls.iter().any(|c| c.starts_with("gh\trelease\tcreate")));
    assert!(!calls[at(&calls, "gh\trelease\tedit")].contains("--draft=false"));
}

#[test]
fn draft_step_appends_the_cli_checksum_to_the_notes() {
    let sc = Scratch::new("draft-notes");
    sc.write("docs/releases/0.12.0.md", "# Notes\n");
    sc.write(&format!("temp/cli-mac/{}", names::CLI_MAC_ARM64), "tarball");
    tool(&sc, "bin/gh", "[ \"$2\" = view ] && exit 1");
    let path = path_with(&sc.join("bin"));
    let temp = sc.join("temp").display().to_string();
    let out = bash(
        &step_named(&release(), "release", "Write the draft release"),
        sc.path(),
        &[("PATH", &path), ("TAG", "v0.12.0"), ("RUNNER_TEMP", &temp)],
    );
    assert!(out.status.success(), "{}", said(&out));
    let notes = std::fs::read_to_string(sc.join("temp/notes.md")).unwrap();
    assert!(notes.starts_with("# Notes\n"));
    assert!(notes.contains(names::MAC_DMG));
    assert!(notes.contains(&format!("  {}", names::CLI_MAC_ARM64)));
    assert!(regex::Regex::new(r"[0-9a-f]{64}").unwrap().is_match(&notes));
}

// ---- app-bundles: the Mac app, signed, notarized, stapled, in order ----

fn mac_step(
    sc: &Scratch,
    env_key: bool,
    archs: &str,
    validate_fails: &str,
) -> (Output, Vec<String>) {
    // sp-release writes the bundle mac-app is asked for; nothing else.
    tool(
        sc,
        "rust/target/release/sp-release",
        "if [ \"$1\" = mac-app ]; then while [ $# -gt 0 ]; do [ \"$1\" = --out ] && mkdir -p \"$2/Method Reader.app/Contents/MacOS\" && : > \"$2/Method Reader.app/Contents/MacOS/method-reader-app\"; shift; done; fi",
    );
    tool(sc, "bin/lipo", &format!("echo '{archs}'"));
    tool(
        sc,
        "bin/codesign",
        "[ \"$1\" = -d ] && echo 'designated => identifier \"com.methodreader.desktop\" and anchor apple generic' >&2",
    );
    tool(
        sc,
        "bin/xcrun",
        &format!("[ \"$1 $2 $3\" = 'stapler validate {validate_fails}' ] && exit 1"),
    );
    for t in ["ditto", "spctl", "hdiutil"] {
        tool(sc, &format!("bin/{t}"), "");
    }
    std::fs::create_dir_all(sc.join("temp")).unwrap();
    let path = path_with(&sc.join("bin"));
    let temp = sc.join("temp").display().to_string();
    let key = if env_key { "a-key" } else { "" };
    let out = bash(
        &step_named(
            &release(),
            "app-bundles",
            "macOS app, update archive and image",
        ),
        sc.path(),
        &[
            ("PATH", &path),
            ("TAG", "v0.12.0"),
            ("RUNNER_TEMP", &temp),
            ("SIGN_IDENTITY", "Developer ID Application: Test"),
            ("AC_API_KEY_PATH", "/keys/ac.p8"),
            ("AC_API_KEY_ID", "KEYID"),
            ("AC_API_ISSUER_ID", "ISSUER"),
            ("TAURI_SIGNING_PRIVATE_KEY", key),
            ("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", ""),
        ],
    );
    (out, log(sc))
}

#[test]
fn mac_step_refuses_to_start_without_the_update_key() {
    let sc = Scratch::new("mac-nokey");
    let (out, calls) = mac_step(&sc, false, "arm64", "none");
    assert!(!out.status.success());
    assert!(said(&out).contains("TAURI_SIGNING_PRIVATE_KEY is not set"));
    assert_eq!(calls, Vec::<String>::new());
}

#[test]
fn mac_step_refuses_a_window_that_is_not_apple_silicon_alone() {
    let sc = Scratch::new("mac-fat");
    let (out, calls) = mac_step(&sc, true, "x86_64 arm64", "none");
    assert!(!out.status.success());
    assert!(said(&out).contains("not arm64 only"));
    assert!(!calls.iter().any(|c| c.starts_with("sp-release\tmac-app")));
}

#[test]
fn mac_step_signs_inside_out_notarizes_staples_and_validates_app_then_image() {
    let sc = Scratch::new("mac-ok");
    let (out, calls) = mac_step(&sc, true, "arm64", "none");
    assert!(out.status.success(), "{}", said(&out));
    let app = sc.join("temp/app/Method Reader.app").display().to_string();
    let exe = format!("{app}/Contents/MacOS/method-reader-app");
    let dmg = "bundles/Method-Reader-macOS.dmg";
    let order = [
        "sp-release\tmac-app\t--version\t0.12.0".to_string(),
        format!(
            "codesign\t--force\t--options\truntime\t--timestamp\t--sign\tDeveloper ID Application: Test\t{exe}"
        ),
        format!(
            "codesign\t--force\t--options\truntime\t--timestamp\t--sign\tDeveloper ID Application: Test\t{app}"
        ),
        format!("codesign\t--verify\t--deep\t--strict\t--verbose=2\t{app}"),
        "xcrun\tnotarytool\tsubmit".to_string(),
        format!("xcrun\tstapler\tstaple\t{app}"),
        format!("xcrun\tstapler\tvalidate\t{app}"),
        format!("spctl\t-a\t-t\texec\t-vv\t{app}"),
        "sp-release\tupdate-archive".to_string(),
        "sp-release\tsign-update\t--file\tbundles/Method-Reader-macOS.app.tar.gz\t--version\t0.12.0"
            .to_string(),
        "hdiutil\tcreate".to_string(),
        format!("codesign\t--force\t--timestamp\t--sign\tDeveloper ID Application: Test\t{dmg}"),
        format!(
            "xcrun\tnotarytool\tsubmit\t{dmg}\t--key\t/keys/ac.p8\t--key-id\tKEYID\t--issuer\tISSUER\t--wait"
        ),
        format!("xcrun\tstapler\tstaple\t{dmg}"),
        format!("xcrun\tstapler\tvalidate\t{dmg}"),
        format!("spctl\t-a\t-t\topen\t--context\tcontext:primary-signature\t-v\t{dmg}"),
    ];
    let mut last = 0;
    for want in order {
        let found = calls[last..]
            .iter()
            .position(|c| c.starts_with(&want))
            .unwrap_or_else(|| panic!("{want:?} not after call {last} in {calls:#?}"));
        last += found + 1;
    }
    // The app's ticket is validated before the archive is made from it.
    assert!(
        at(&calls, &format!("xcrun\tstapler\tvalidate\t{app}"))
            < at(&calls, "sp-release\tupdate-archive")
    );
}

#[test]
fn mac_step_fails_when_a_staple_leaves_no_ticket() {
    // `stapler staple` can exit 0 and leave nothing: v0.6.0 shipped an
    // image Gatekeeper rejects. The validate is asserted, not trusted.
    let sc = Scratch::new("mac-dmg-ticket");
    let (out, calls) = mac_step(&sc, true, "arm64", "bundles/Method-Reader-macOS.dmg");
    assert!(!out.status.success());
    assert!(!calls.iter().any(|c| c.starts_with("spctl\t-a\t-t\topen")));
    let sc = Scratch::new("mac-app-ticket");
    let app = sc.join("temp/app/Method Reader.app").display().to_string();
    let (out, calls) = mac_step(&sc, true, "arm64", &app);
    assert!(!out.status.success());
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("sp-release\tupdate-archive"))
    );
}

// ---- app-bundles: every bundle opened on the OS that built it ----------

fn check_step(sc: &Scratch, files: &[&str]) -> Output {
    std::fs::create_dir_all(sc.join("bundles")).unwrap();
    for f in files {
        sc.write(&format!("bundles/{f}"), "x");
    }
    std::fs::create_dir_all(sc.join("local")).unwrap();
    let path = path_with(&sc.join("bin"));
    let local = sc.join("local").display().to_string();
    let ws = sc.path().display().to_string();
    bash(
        &step_named(&release(), "app-bundles", "Check each bundle"),
        sc.path(),
        &[
            ("PATH", &path),
            ("TAG", "v0.12.0"),
            ("LOCALAPPDATA", &local),
            ("GITHUB_WORKSPACE", &ws),
        ],
    )
}

/// The Requires an .rpm sp-release built declares (what the check reads).
const RPM_REQUIRES: &str = "libasound.so.2()(64bit)\nlibfontconfig.so.1()(64bit)\nlibc.so.6()(64bit)\nlibxkbcommon.so.0()(64bit)\nlibxkbcommon-x11.so.0()(64bit)\nlibwayland-client.so.0()(64bit)\nlibwayland-egl.so.1()(64bit)\nlibX11.so.6()(64bit)\nlibX11-xcb.so.1()(64bit)\nlibXcursor.so.1()(64bit)\nlibXi.so.6()(64bit)\nlibEGL.so.1()(64bit)";

fn rpm_stub(sc: &Scratch, version: &str) {
    rpm_stub_requiring(sc, version, RPM_REQUIRES);
}

fn rpm_stub_requiring(sc: &Scratch, version: &str, requires: &str) {
    tool(
        sc,
        "bin/rpm",
        &format!(
            "case \"$1\" in -qip) echo 'Version     : {version}' ;; -qlp) echo /usr/bin/method-reader-app ;; -qp) if [ \"$2\" = --obsoletes ]; then echo 'screepub < {version}'; else printf '%s\\n' '{}'; fi ;; esac",
            requires.replace('\n', "' '")
        ),
    );
}

#[test]
fn check_step_fails_loudly_on_an_empty_bundles_folder() {
    let sc = Scratch::new("check-empty");
    let out = check_step(&sc, &[]);
    assert!(!out.status.success());
    assert!(said(&out).contains("NOTHING WAS CHECKED"), "{}", said(&out));
}

#[test]
fn check_step_opens_each_bundle_and_counts_what_it_checked() {
    let sc = Scratch::new("check-rpm");
    rpm_stub(&sc, "0.12.0");
    let out = check_step(
        &sc,
        &[
            names::LINUX_RPM_X86_64,
            "Method-Reader-macOS.app.tar.gz.sig",
        ],
    );
    assert!(out.status.success(), "{}", said(&out));
    assert!(said(&out).contains("1 bundle(s) checked"));
    let calls = log(&sc);
    assert_eq!(calls.len(), 4, "{calls:#?}");
    assert!(calls[0].starts_with("rpm\t-qip\tbundles/Method-Reader-linux-x86_64.rpm"));
    assert!(calls[1].starts_with("rpm\t-qp\t--requires\tbundles/Method-Reader-linux-x86_64.rpm"));
}

#[test]
fn check_step_fails_an_rpm_that_does_not_require_a_library_the_window_opens() {
    for gone in ["libxkbcommon.so.0()(64bit)", "libasound.so.2()(64bit)"] {
        let sc = Scratch::new("check-requires");
        let requires: Vec<&str> = RPM_REQUIRES.lines().filter(|l| *l != gone).collect();
        rpm_stub_requiring(&sc, "0.12.0", &requires.join("\n"));
        let out = check_step(&sc, &[names::LINUX_RPM_X86_64]);
        assert!(!out.status.success(), "{gone}: {}", said(&out));
    }
}

#[test]
fn check_step_fails_a_bundle_that_claims_another_version() {
    let sc = Scratch::new("check-version");
    rpm_stub(&sc, "0.11.9");
    let out = check_step(&sc, &[names::LINUX_RPM_X86_64]);
    assert!(!out.status.success());
}

#[test]
fn check_step_refuses_a_file_it_does_not_know() {
    let sc = Scratch::new("check-unknown");
    let out = check_step(&sc, &["Method-Reader-linux-arm64.snap"]);
    assert!(!out.status.success());
    assert!(said(&out).contains("unexpected file"), "{}", said(&out));
}

#[test]
fn check_step_installs_and_uninstalls_the_windows_installer() {
    let sc = Scratch::new("check-exe");
    std::fs::create_dir_all(sc.join("bundles")).unwrap();
    // The installer, as the step runs it: silently, into LOCALAPPDATA.
    sc.script(
        &format!("bundles/{}", names::WINDOWS_SETUP),
        "mkdir -p \"$LOCALAPPDATA/Method Reader\"\n: > \"$LOCALAPPDATA/Method Reader/method-reader-app.exe\"\nprintf '#!/bin/sh\\nrm -rf \"$LOCALAPPDATA/Method Reader\"\\n' > \"$LOCALAPPDATA/Method Reader/uninstall.exe\"\nchmod +x \"$LOCALAPPDATA/Method Reader/uninstall.exe\"\n",
    );
    let out = check_step(&sc, &[]);
    assert!(out.status.success(), "{}", said(&out));
    assert!(!sc.join("local/Method Reader").exists(), "uninstalled");
    // One that installs nothing fails.
    let sc = Scratch::new("check-exe-empty");
    std::fs::create_dir_all(sc.join("bundles")).unwrap();
    sc.script(&format!("bundles/{}", names::WINDOWS_SETUP), "");
    assert!(!check_step(&sc, &[]).status.success());
}

// ---- app-upload: the checksums, from what arrived ----------------------

fn has_sha256sum() -> bool {
    [
        "/usr/bin/sha256sum",
        "/bin/sha256sum",
        "/sbin/sha256sum",
        "/usr/sbin/sha256sum",
    ]
    .iter()
    .any(|p| Path::new(p).exists())
}

fn sums_step(sc: &Scratch, files: &[&str], script: Option<String>) -> Output {
    for f in files {
        sc.write(&format!("arrivals/{f}"), format!("contents of {f}"));
    }
    let script = script.unwrap_or_else(|| {
        step_named(
            &release(),
            "app-upload",
            "Checksums from what arrived, then the count",
        )
    });
    bash(
        &script,
        sc.path(),
        &[("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")],
    )
}

fn five() -> Vec<&'static str> {
    vec![
        names::LINUX_DEB_AMD64,
        names::LINUX_RPM_X86_64,
        names::LINUX_APPIMAGE_X86_64,
        names::MAC_DMG,
        names::WINDOWS_SETUP,
    ]
}

#[test]
fn sums_step_rebuilds_the_checksums_over_what_arrived_and_verifies_them() {
    if !has_sha256sum() {
        eprintln!("skipped: no sha256sum on this machine");
        return;
    }
    // Including over a stale per-leg file merge-multiple kept.
    let sc = Scratch::new("sums-ok");
    let mut files = five();
    files.push("SHA256SUMS-app");
    let out = sums_step(&sc, &files, None);
    assert!(out.status.success(), "{}", said(&out));
    let sums = std::fs::read_to_string(sc.join("arrivals/SHA256SUMS-app")).unwrap();
    for n in five() {
        assert!(sums.contains(n), "{n}");
    }
    // Bare names, so `sha256sum -c` works in any download folder.
    assert!(!sums.contains('/'));
    assert_eq!(sums.trim().lines().count(), 5);
}

#[test]
fn sums_step_fails_a_leg_that_did_not_arrive() {
    if !has_sha256sum() {
        eprintln!("skipped: no sha256sum on this machine");
        return;
    }
    let sc = Scratch::new("sums-short");
    let files: Vec<&str> = five()
        .into_iter()
        .filter(|n| !n.ends_with(".exe"))
        .collect();
    let out = sums_step(&sc, &files, None);
    assert!(!out.status.success());
    assert!(said(&out).contains(".exe"), "{}", said(&out));
}

#[test]
fn sums_step_fails_a_second_mac_image() {
    // Every glob matches and sha256sum succeeds, but the frozen Swift
    // updater takes the FIRST .dmg on a release and cannot choose.
    if !has_sha256sum() {
        eprintln!("skipped: no sha256sum on this machine");
        return;
    }
    let sc = Scratch::new("sums-two-dmg");
    let mut files = five();
    files.push("Method-Reader-macOS-x64.dmg");
    let out = sums_step(&sc, &files, None);
    assert!(!out.status.success());
    assert!(
        said(&out).contains("expected 5 app bundles"),
        "{}",
        said(&out)
    );
}

#[test]
fn sums_step_fails_a_file_changed_after_it_was_hashed() {
    // Proof the -c pass is not a tautology: change a file between the two.
    if !has_sha256sum() {
        eprintln!("skipped: no sha256sum on this machine");
        return;
    }
    let sc = Scratch::new("sums-corrupt");
    let step = step_named(
        &release(),
        "app-upload",
        "Checksums from what arrived, then the count",
    );
    let script = step.replace(
        "sha256sum -c SHA256SUMS-app",
        &format!(
            "echo tampered > {}\nsha256sum -c SHA256SUMS-app",
            names::WINDOWS_SETUP
        ),
    );
    assert!(script.contains("echo tampered"));
    let out = sums_step(&sc, &five(), Some(script));
    assert!(!out.status.success());
    assert!(said(&out).contains("FAILED"), "{}", said(&out));
}

// ---- app-upload: latest.json, built, verified, published last ----------

fn manifest_step(sc: &Scratch, tag: &str, refuse: &str) -> (Output, Vec<String>) {
    tool(sc, "bin/cargo", "");
    tool(sc, "bin/gh", "");
    tool(
        sc,
        "rust/target/release/sp-release",
        &format!("[ \"$1\" = '{refuse}' ] && {{ echo refused >&2; exit 1; }}"),
    );
    std::fs::create_dir_all(sc.join("arrivals")).unwrap();
    let path = path_with(&sc.join("bin"));
    let out = bash(
        &step_named(
            &release(),
            "app-upload",
            "Build latest.json from what arrived, verify it, publish it last",
        ),
        &sc.join("rust"),
        &[
            ("PATH", &path),
            ("TAG", tag),
            ("GH_TOKEN", "t"),
            ("GH_REPO", "o/r"),
        ],
    );
    (out, log(sc))
}

#[test]
fn manifest_step_builds_verifies_then_uploads_latest_json() {
    let sc = Scratch::new("manifest-ok");
    let (out, calls) = manifest_step(&sc, "v0.12.0", "none");
    assert!(out.status.success(), "{}", said(&out));
    assert_eq!(
        calls,
        vec![
            "cargo\tbuild\t--release\t--locked\t-p\tsp-release".to_string(),
            "sp-release\tmanifest\t--dir\t../arrivals\t--version\t0.12.0\t--out\t../arrivals/latest.json".to_string(),
            "sp-release\tverify-update\t--dir\t../arrivals\t--manifest\t../arrivals/latest.json".to_string(),
            "gh\trelease\tupload\tv0.12.0\t../arrivals/latest.json\t--clobber".to_string(),
        ]
    );
}

#[test]
fn manifest_step_publishes_nothing_for_a_prerelease_and_says_so() {
    let sc = Scratch::new("manifest-pre");
    let (out, calls) = manifest_step(&sc, "v0.12.0-rc1", "none");
    assert!(out.status.success(), "{}", said(&out));
    assert!(said(&out).contains("::notice::"));
    assert_eq!(calls, Vec::<String>::new());
}

#[test]
fn manifest_step_uploads_nothing_when_the_manifest_or_its_check_refuses() {
    for refuse in ["manifest", "verify-update"] {
        let sc = Scratch::new(&format!("manifest-{refuse}"));
        let (out, calls) = manifest_step(&sc, "v0.12.0", refuse);
        assert!(!out.status.success(), "{refuse}");
        assert!(
            !calls.iter().any(|c| c.starts_with("gh\t")),
            "{refuse}: {calls:#?}"
        );
    }
}

// ---- draft-check: the draft is complete, and a person publishes it ------

/// Every file a complete release carries, one per line, as the fake
/// release's asset list answers it.
fn complete_assets(with_manifest: bool) -> String {
    let mut v = vec![
        names::CLI_MAC_ARM64.to_string(),
        names::CLI_LINUX_X64.to_string(),
        names::CLI_LINUX_ARM64.to_string(),
        names::CLI_WINDOWS_X64.to_string(),
        "SHA256SUMS".to_string(),
        names::LINUX_DEB_AMD64.to_string(),
        names::LINUX_RPM_X86_64.to_string(),
        names::LINUX_APPIMAGE_X86_64.to_string(),
        names::MAC_DMG.to_string(),
        names::WINDOWS_SETUP.to_string(),
        names::MAC_UPDATE.to_string(),
        format!("{}{}", names::MAC_UPDATE, names::SIGNATURE_EXT),
        "SHA256SUMS-app".to_string(),
    ];
    if with_manifest {
        v.push(names::MANIFEST.to_string());
    }
    v.join("\n")
}

/// The asset step against a fake release whose .dmg query answers `dmgs`
/// and whose plain asset list answers `assets`, one name per line.
fn draft_check(tag: &str, dmgs: &str, assets: &str) -> (Output, Vec<String>, String) {
    let sc = Scratch::new("draft-check");
    tool(
        &sc,
        "bin/gh",
        "if [ \"$2\" = view ]; then case \"$*\" in *endswith*) printf '%s\\n' \"$FAKE_DMGS\" ;; *) printf '%s\\n' \"$FAKE_ASSETS\" ;; esac; fi",
    );
    let path = path_with(&sc.join("bin"));
    let summary = sc.join("summary.md").display().to_string();
    let out = bash(
        &step_named(
            &release(),
            "draft-check",
            "The draft carries every file the release names",
        ),
        sc.path(),
        &[
            ("PATH", &path),
            ("TAG", tag),
            ("GH_REPO", "method-production-suite/method-reader"),
            ("FAKE_DMGS", dmgs),
            ("FAKE_ASSETS", assets),
            ("GITHUB_STEP_SUMMARY", &summary),
        ],
    );
    let calls = log(&sc);
    let said_summary = std::fs::read_to_string(&summary).unwrap_or_default();
    (out, calls, said_summary)
}

fn changes_the_release(calls: &[String]) -> bool {
    calls
        .iter()
        .any(|c| c.starts_with("gh\trelease\tedit") || c.starts_with("gh\trelease\tupload"))
}

#[test]
fn draft_check_passes_a_complete_draft_and_leaves_it_a_draft() {
    let (out, calls, summary) = draft_check("v0.13.0", names::MAC_DMG, &complete_assets(true));
    assert!(out.status.success(), "{}", said(&out));
    assert!(!changes_the_release(&calls), "{calls:#?}");
    // What the person does next, in the run's summary.
    assert!(summary.contains(names::MAC_DMG), "{summary}");
    assert!(summary.contains("Apple silicon"), "{summary}");
    assert!(summary.contains("publish"), "{summary}");
}

#[test]
fn draft_check_fails_a_draft_with_no_mac_image() {
    // v0.12.1 went public with no app attached and broke the site's download
    // buttons and every window's update check until a re-run.
    let assets = complete_assets(true).replace(&format!("{}\n", names::MAC_DMG), "");
    let (out, calls, _) = draft_check("v0.13.0", "", &assets);
    assert!(!out.status.success());
    assert!(said(&out).contains("no Mac image"), "{}", said(&out));
    assert!(!changes_the_release(&calls));
}

#[test]
fn draft_check_fails_a_draft_with_no_update_manifest_and_names_it() {
    let (out, calls, _) = draft_check("v0.13.0", names::MAC_DMG, &complete_assets(false));
    assert!(!out.status.success());
    assert!(said(&out).contains(names::MANIFEST), "{}", said(&out));
    assert!(said(&out).contains("Do not publish"), "{}", said(&out));
    assert!(!changes_the_release(&calls));
}

#[test]
fn draft_check_names_every_file_that_did_not_arrive() {
    let assets = complete_assets(true)
        .replace(&format!("{}\n", names::CLI_WINDOWS_X64), "")
        .replace(&format!("{}\n", names::LINUX_DEB_AMD64), "");
    let (out, _, _) = draft_check("v0.13.0", names::MAC_DMG, &assets);
    assert!(!out.status.success());
    let s = said(&out);
    assert!(s.contains(names::CLI_WINDOWS_X64), "{s}");
    assert!(s.contains(names::LINUX_DEB_AMD64), "{s}");
}

#[test]
fn draft_check_fails_a_draft_with_two_images() {
    let (out, calls, _) = draft_check(
        "v0.13.0",
        "Method-Reader-macOS.dmg Method-Reader-macOS-x64.dmg",
        &complete_assets(true),
    );
    assert!(!out.status.success());
    assert!(said(&out).contains("must carry exactly one .dmg"));
    assert!(!changes_the_release(&calls));
}

#[test]
fn draft_check_asks_no_update_manifest_of_a_prerelease_under_the_macs_bash() {
    // app-upload publishes no latest.json for a prerelease, on purpose
    // (releases/latest never resolves to one), so the check must not want
    // it: the old publish job did, and would have kept every prerelease a
    // draft.
    let (out, calls, _) = draft_check("v0.13.0-rc1", names::MAC_DMG, &complete_assets(false));
    assert!(out.status.success(), "{}", said(&out));
    assert!(!changes_the_release(&calls));
}

#[test]
fn draft_check_says_which_job_did_not_succeed() {
    let text = step_named(&release(), "draft-check", "Every job before it succeeded");
    let sc = Scratch::new("draft-check-jobs");
    let path = path_with(&sc.join("bin"));
    let ok = [
        ("PATH", path.as_str()),
        ("RELEASE", "success"),
        ("CLI_UPLOAD", "success"),
        ("APP_UPLOAD", "success"),
        ("HANDOVER_GATE", "success"),
    ];
    let out = bash(&text, sc.path(), &ok);
    assert!(out.status.success(), "{}", said(&out));
    let mut bad = ok;
    bad[3] = ("APP_UPLOAD", "failure");
    bad[4] = ("HANDOVER_GATE", "skipped");
    let out = bash(&text, sc.path(), &bad);
    assert!(!out.status.success());
    let s = said(&out);
    assert!(s.contains("app-upload (failure)"), "{s}");
    assert!(s.contains("handover-gate (skipped)"), "{s}");
    assert!(!s.contains("cli-upload"), "{s}");
}

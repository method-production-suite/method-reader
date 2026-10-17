//! Open a built bundle WITHOUT installing it and check what it carries:
//! was tools/smoke-bundle.ts, with the half of tools/dmg.ts it used.
//!
//! ```text
//! cargo xtask smoke-bundle --bundle dist/Method-Reader-linux-amd64.deb --expect-version 0.12.0
//! cargo xtask smoke-bundle --built --expect-version 0.12.0
//! ```
//!
//! The window's binary comes out of the bundle and is RUN headless (sp-app's
//! src/headless.rs), as the Tauri bundle's engine sidecar was until the Rust
//! cutover: `--version --json` must report the version the bundle claims,
//! and `--smoke` of the committed fixture (tests/fixtures/field-station.pdf)
//! must write an EPUB. Both runs get a scratch METHOD_READER_CONFIG_DIR and
//! METHOD_READER_LIBRARY under the work folder and a Calibre config path nothing
//! can be made at, so a smoke never reads or writes the real settings,
//! library or Calibre folder. Before that, the binary must be present,
//! non-empty and executable, and the version the package declares (the
//! .deb's control file, the .rpm's header, the .app's Info.plist) must be
//! the one expected.
//!
//! A .deb or .rpm needs Linux, a .dmg needs hdiutil (macOS) and an NSIS .exe
//! needs 7z (Windows): each is REFUSED off its own OS rather than skipped,
//! because a check that prints nothing and exits 0 reads like a check that
//! ran.
//!
//! The DMG half is [`crate::dmg`], the one copy verify-signing uses too.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::archive::{bundle_entries, deb_control_field, find_entry, rpm_header_string};
use super::bundle::{BundleDirs, os_for_platform};
use super::kinds::{BundleOs, bundle_dir_for, discover_artifact, kinds_for_os, verify_bundle_file};
use super::native::{NATIVE_APP, package_version, plist_string};
use super::util::{
    FlagSpec, RunResult, Runner, host_platform, parse_flags, real_run, real_run_env, strip_v,
};
use crate::dmg::MountedApp;
use crate::paths::repo_root;

/// Runs the window: its argv and the environment it is given on top of
/// this process's. A test passes a closure; the tool passes
/// [`real_run_env`].
pub type WindowRunner<'a> = dyn FnMut(&[String], &[(String, String)]) -> RunResult + 'a;

/// The committed fixture a smoke converts.
pub fn default_fixture() -> PathBuf {
    repo_root()
        .join("tests")
        .join("fixtures")
        .join("field-station.pdf")
}

fn trimmed(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

/// The --json contract is ONE parseable object on stdout.
pub fn sole_json(stdout: &str, what: &str) -> Result<serde_json::Map<String, Value>, String> {
    let lines: Vec<&str> = stdout
        .split('\n')
        .filter(|l| !l.trim().is_empty())
        .collect();
    if lines.len() != 1 {
        return Err(format!(
            "smoke: {what} printed {} lines on stdout under --json; the contract is exactly one",
            lines.len()
        ));
    }
    match serde_json::from_str::<Value>(lines[0]) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(other) => Ok([("value".to_string(), other)].into_iter().collect()),
        Err(_) => Err(format!(
            "smoke: {what} did not print parseable JSON: {}",
            lines[0].chars().take(200).collect::<String>()
        )),
    }
}

/// The bundled window's `--version --json`, against the bundle's version.
pub fn check_window_version(result: &RunResult, expected: &str) -> Result<(), String> {
    if result.exit_code != 0 {
        return Err(format!(
            "smoke-bundle: the window inside the bundle exited {}: {}",
            result.exit_code,
            trimmed(&result.stderr, 500)
        ));
    }
    let json = sole_json(&result.stdout, "the bundled window")?;
    if json.get("ok") != Some(&Value::Bool(true)) {
        return Err(format!(
            "smoke-bundle: the bundled window reported {}",
            Value::Object(json)
        ));
    }
    if json.get("version").and_then(Value::as_str) != Some(expected) {
        let said = json
            .get("version")
            .map_or("undefined".to_string(), |v| v.to_string());
        return Err(format!(
            "smoke-bundle: the window inside this bundle says {said} but the bundle is {expected}. An installer whose About line contradicts its own filename lies about itself in every bug report it appears in."
        ));
    }
    Ok(())
}

/// smoke-cli's conversion check, applied to the bundled window's `--smoke`.
pub fn check_convert_result(result: &RunResult, epub: &Path) -> Result<(), String> {
    if result.exit_code != 0 {
        return Err(format!(
            "smoke: convert exited {}: {}",
            result.exit_code,
            trimmed(&result.stderr, 500)
        ));
    }
    let json = sole_json(&result.stdout, "convert")?;
    if json.get("ok") != Some(&Value::Bool(true)) {
        return Err(format!("smoke: convert reported {}", Value::Object(json)));
    }
    let wrote = json
        .get("epubPath")
        .and_then(Value::as_str)
        .unwrap_or("undefined");
    if Path::new(wrote) != epub {
        return Err(format!(
            "smoke: convert wrote {wrote}, expected {}",
            epub.display()
        ));
    }
    match json.get("pages").and_then(Value::as_f64) {
        Some(p) if p > 0.0 => Ok(()),
        _ => Err(format!(
            "smoke: convert reported {} pages; an empty book is not a conversion",
            json.get("pages")
                .map_or("undefined".to_string(), |v| v.to_string())
        )),
    }
}

/// An EPUB is a zip container.
pub fn check_epub_bytes(head: &[u8]) -> Result<(), String> {
    if head.len() >= 4 && head[..4] == [0x50, 0x4b, 0x03, 0x04] {
        Ok(())
    } else {
        Err("smoke: the output is not a zip container; no EPUB was written".into())
    }
}

/// Which bundles this machine can open (and run), and why not. None means
/// go ahead; Some is the refusal to print.
pub fn platform_refusal(bundle: &str, platform: &str) -> Option<String> {
    let wrong_os = |os: &str, tool: &str| {
        format!(
            "smoke-bundle: {bundle} can only be opened ({tool}) and its window run on {os}, and this machine is {platform}. NOTHING WAS CHECKED -- run this step on a {os} runner. Reporting success here would mean reporting success for a window no one has run."
        )
    };
    if bundle.ends_with(".deb") || bundle.ends_with(".rpm") {
        return (platform != "linux").then(|| wrong_os("Linux", "no external tool"));
    }
    if bundle.ends_with(".dmg") {
        return (platform != "darwin").then(|| wrong_os("macOS", "hdiutil"));
    }
    if bundle.ends_with(".exe") {
        return (platform != "win32").then(|| wrong_os("Windows", "7z"));
    }
    Some(format!(
        "smoke-bundle: {bundle} is not a .deb, .rpm, .dmg or .exe"
    ))
}

/// The environment every window run gets: settings and a library of its
/// own under `work`, and a Calibre config folder that cannot be made (it
/// sits under a FILE, which works as a guard on every OS). Never the real
/// ones.
pub fn window_env(work: &Path) -> Result<Vec<(String, String)>, String> {
    let guard = work.join("calibre-guard");
    std::fs::write(&guard, "a file, so nothing can be made under it")
        .map_err(|e| format!("smoke-bundle: cannot write {}: {e}", guard.display()))?;
    let s = |p: PathBuf| p.to_string_lossy().into_owned();
    Ok(vec![
        ("METHOD_READER_CONFIG_DIR".into(), s(work.join("config"))),
        ("METHOD_READER_LIBRARY".into(), s(work.join("library"))),
        ("CALIBRE_CONFIG_DIRECTORY".into(), s(guard.join("config"))),
    ])
}

/// Two runs of the window that came out of the bundle, headless: its
/// version, then a real conversion of `fixture`, whose EPUB is then read
/// back.
pub fn smoke_window(
    window: &Path,
    fixture: &Path,
    work: &Path,
    expected: &str,
    run_window: &mut WindowRunner<'_>,
) -> Result<(), String> {
    if !fixture.exists() {
        return Err(format!("smoke-bundle: no fixture at {}", fixture.display()));
    }
    let env = window_env(work)?;
    let w = window.to_string_lossy().into_owned();
    check_window_version(
        &run_window(&[w.clone(), "--version".into(), "--json".into()], &env),
        expected,
    )?;
    let epub = work.join("smoke.epub");
    let r = run_window(
        &[
            w,
            "--smoke".into(),
            fixture.to_string_lossy().into_owned(),
            "--out".into(),
            epub.to_string_lossy().into_owned(),
        ],
        &env,
    );
    check_convert_result(&r, &epub)?;
    let Ok(bytes) = std::fs::read(&epub) else {
        return Err(format!(
            "smoke-bundle: the bundled window reported a successful conversion but {} is not there. A success object is not a book.",
            epub.display()
        ));
    };
    check_epub_bytes(&bytes[..bytes.len().min(8)])
}

/// Writes an executable file (the archive's mode does not come along).
fn write_executable(dest: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(dest, data)
        .map_err(|e| format!("smoke-bundle: cannot write {}: {e}", dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Where a .deb or an .rpm installs the window.
pub fn window_in_archive() -> String {
    format!("usr/bin/{}", NATIVE_APP.linux_executable)
}

/// The window out of a .deb or an .rpm, checked (non-empty, executable)
/// and written executable under `work`.
pub fn extract_archive_window(bundle: &Path, work: &Path) -> Result<PathBuf, String> {
    let entries = bundle_entries(bundle)?;
    let name = window_in_archive();
    let bin = find_entry(&entries, &name)?;
    if bin.data.is_empty() || bin.mode & 0o111 == 0 {
        return Err(format!(
            "smoke-bundle: {} in {} is {} bytes with mode {:o}; the window must be a non-empty executable",
            bin.name,
            bundle.display(),
            bin.data.len(),
            bin.mode
        ));
    }
    let dest = work.join(&name);
    write_executable(&dest, &bin.data)?;
    Ok(dest)
}

/// An attached image: where it is mounted and the one app on it.
pub type Mounted = MountedApp;

/// [`crate::dmg::mount_dmg_app`] through this module's runner (no working
/// folder for hdiutil).
pub fn mount_dmg_app(
    dmg: &Path,
    work: &Path,
    run: &mut Runner<'_>,
    tool: &str,
) -> Result<Mounted, String> {
    crate::dmg::mount_dmg_app(dmg, work, &mut |a: &[String]| run(a, None), tool)
}

/// Detach an image through this module's runner.
pub fn detach(m: &Mounted, run: &mut Runner<'_>) {
    m.detach(&mut |a: &[String]| run(a, None));
}

/// The window inside a .dmg's app. The caller detaches.
pub fn extract_dmg_window(
    bundle: &Path,
    work: &Path,
    run: &mut Runner<'_>,
) -> Result<(PathBuf, Mounted), String> {
    let m = mount_dmg_app(bundle, work, run, "smoke-bundle")?;
    let exe = m
        .app_path
        .join("Contents")
        .join("MacOS")
        .join(NATIVE_APP.mac_executable);
    Ok((exe, m))
}

fn listing(dir: &Path) -> String {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    if names.is_empty() {
        "<nothing>".into()
    } else {
        names.join(", ")
    }
}

/// Unpacks an NSIS installer with 7z (on GitHub's Windows image) and finds
/// the window in what came out.
pub fn extract_exe_window(
    bundle: &Path,
    work: &Path,
    run: &mut Runner<'_>,
) -> Result<PathBuf, String> {
    let file = NATIVE_APP.windows_executable;
    let r = run(
        &[
            "7z".into(),
            "x".into(),
            bundle.to_string_lossy().into_owned(),
            format!("-o{}", work.display()),
            "-y".into(),
        ],
        None,
    );
    if r.exit_code != 0 {
        return Err(format!(
            "smoke-bundle: 7z could not unpack {}: {}",
            bundle.display(),
            trimmed(&r.stderr, 500)
        ));
    }
    let dest = work.join(file);
    if !dest.exists() {
        return Err(format!(
            "smoke-bundle: no {file} under {} after unpacking {} (it holds: {})",
            work.display(),
            bundle.display(),
            listing(work)
        ));
    }
    if std::fs::metadata(&dest).map(|x| x.len()).unwrap_or(0) == 0 {
        return Err(format!("smoke-bundle: {} is empty", dest.display()));
    }
    Ok(dest)
}

fn ends(path: &Path, ext: &str) -> bool {
    path.to_string_lossy().ends_with(ext)
}

/// Smoke one bundle: the window is in it and declares the expected version,
/// then it runs headless (see [`smoke_window`]).
pub fn smoke_bundle(
    bundle: &Path,
    fixture: &Path,
    work: &Path,
    expected: &str,
    run: &mut Runner<'_>,
    run_window: &mut WindowRunner<'_>,
) -> Result<(), String> {
    if !bundle.exists() {
        return Err(format!("smoke-bundle: no bundle at {}", bundle.display()));
    }
    let a = NATIVE_APP;
    let pkg_version = package_version(expected);
    let mismatch = |said: Option<String>, want: &str, what: &str| -> Result<(), String> {
        if said.as_deref() == Some(want) {
            return Ok(());
        }
        Err(format!(
            "smoke-bundle: {} says it is {} in its {what}, but the bundle is {expected}. An installer whose About line contradicts its own filename lies about itself in every bug report it appears in.",
            bundle.display(),
            said.as_deref().unwrap_or("<nothing>")
        ))
    };
    if ends(bundle, ".deb") || ends(bundle, ".rpm") {
        let window = extract_archive_window(bundle, work)?;
        if ends(bundle, ".deb") {
            mismatch(
                deb_control_field(bundle, "Version")?,
                &pkg_version,
                "control file",
            )?;
        } else {
            let bytes = std::fs::read(bundle).map_err(|e| e.to_string())?;
            mismatch(rpm_header_string(&bytes, 1001)?, &pkg_version, "rpm header")?;
        }
        smoke_window(&window, fixture, work, expected, run_window)
    } else if ends(bundle, ".dmg") {
        let (exe, m) = extract_dmg_window(bundle, work, run)?;
        let result = (|| {
            let size = std::fs::metadata(&exe).map(|x| x.len()).unwrap_or(0);
            if size == 0 {
                return Err(format!(
                    "smoke-bundle: no window binary at {} on the image",
                    exe.display()
                ));
            }
            let plist = std::fs::read_to_string(m.app_path.join("Contents").join("Info.plist"))
                .unwrap_or_default();
            mismatch(plist_string(&plist, "CFBundleIdentifier"), a.identifier, "Info.plist identifier")
                .map_err(|_| {
                    format!(
                        "smoke-bundle: the app on {} is not {}: the updater and macOS would treat it as another app",
                        bundle.display(),
                        a.identifier
                    )
                })?;
            mismatch(
                plist_string(&plist, "CFBundleShortVersionString"),
                expected,
                "Info.plist",
            )?;
            smoke_window(&exe, fixture, work, expected, run_window)
        })();
        detach(&m, run);
        result
    } else if ends(bundle, ".exe") {
        let exe = extract_exe_window(bundle, work, run)?;
        smoke_window(&exe, fixture, work, expected, run_window)
    } else {
        Err(format!(
            "smoke-bundle: {} is not a .deb, .rpm, .dmg or .exe",
            bundle.display()
        ))
    }
}

/// What [`smoke_built_bundles`] is asked: which OS's bundles, found under
/// `root` (rust/), on which host.
#[derive(Clone, Copy, Debug)]
pub struct BuiltSmoke<'a> {
    pub os: BundleOs,
    pub expected: &'a str,
    pub fixture: &'a Path,
    pub work_root: &'a Path,
    pub root: &'a Path,
    pub host: &'a str,
}

/// Verify and smoke every bundle this runner just built, found under
/// `root` (rust/). An OS with no kinds, or a kind with no bundle, is a
/// failure.
pub fn smoke_built_bundles(
    s: &BuiltSmoke<'_>,
    run: &mut Runner<'_>,
    run_window: &mut WindowRunner<'_>,
) -> Result<Vec<PathBuf>, String> {
    let BuiltSmoke {
        os,
        expected,
        fixture,
        work_root,
        root,
        host,
    } = *s;
    let kinds = kinds_for_os(os);
    if kinds.is_empty() {
        return Err(format!(
            "smoke-bundle: no bundle kinds are defined for {}. NOTHING WAS CHECKED.",
            os.name()
        ));
    }
    let mut smoked = Vec::new();
    for kind in kinds {
        let path = discover_artifact(&bundle_dir_for(kind.dir, None, root), &kind)?;
        verify_bundle_file(&path, &kind)?;
        if let Some(refusal) = platform_refusal(&path.to_string_lossy(), host) {
            return Err(refusal);
        }
        let work = work_root.join(kind.id);
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        smoke_bundle(&path, fixture, &work, expected, run, run_window)?;
        println!("smoke-bundle: {} ok -- {}", kind.id, path.display());
        smoked.push(path);
    }
    Ok(smoked)
}

/// `cargo xtask smoke-bundle`.
pub fn main(args: &[String]) -> u8 {
    if super::util::wants_help(
        args,
        "usage: cargo xtask smoke-bundle --expect-version <x.y.z> (--bundle <file> | --built) [--fixture <pdf>]\n\nRuns the window inside the bundle headless (--version --json, then --smoke of the fixture, tests/fixtures/field-station.pdf by default) with scratch settings and library.",
    ) {
        return 0;
    }
    let parsed = parse_flags(
        args,
        &[
            FlagSpec {
                name: "bundle",
                value: true,
            },
            FlagSpec {
                name: "built",
                value: false,
            },
            FlagSpec {
                name: "expect-version",
                value: true,
            },
            FlagSpec {
                name: "fixture",
                value: true,
            },
            FlagSpec {
                name: "app",
                value: true,
            },
        ],
    );
    let mut work: Option<PathBuf> = None;
    let result = (|| {
        let f = parsed?;
        let expected =
            strip_v(f.get("expect-version").map(String::as_str).unwrap_or("")).to_string();
        if expected.is_empty() {
            return Err("smoke-bundle: --expect-version <version> is required".to_string());
        }
        // `--app native` is still taken, so a command written before the
        // cutover keeps working; it names the only window there is.
        match f.get("app").map(String::as_str) {
            None | Some("native") => {}
            Some("tauri") => {
                return Err("smoke-bundle: the Tauri window is gone (the Rust cutover); the native window is the only one, so drop --app tauri.".to_string());
            }
            Some(other) => {
                return Err(format!(
                    "smoke-bundle: --app is native, the only window, not \"{other}\""
                ));
            }
        }
        let fixture = f
            .get("fixture")
            .map(PathBuf::from)
            .unwrap_or_else(default_fixture);
        let dirs = BundleDirs::for_repo(&repo_root());
        let dir =
            std::env::temp_dir().join(format!("method-reader-bundle-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        work = Some(dir.clone());
        let mut run = real_run;
        let mut run_window = |argv: &[String], env: &[(String, String)]| real_run_env(argv, env);
        if f.contains_key("built") {
            let os = os_for_platform(host_platform())?;
            let smoked = smoke_built_bundles(
                &BuiltSmoke {
                    os,
                    expected: &expected,
                    fixture: &fixture,
                    work_root: &dir,
                    root: &dirs.rust_dir,
                    host: host_platform(),
                },
                &mut run,
                &mut run_window,
            )?;
            println!(
                "smoke-bundle: {} bundle(s) report {expected} and convert the fixture",
                smoked.len()
            );
        } else {
            let Some(bundle) = f.get("bundle") else {
                return Err("smoke-bundle: pass --bundle <path> or --built".to_string());
            };
            if let Some(refusal) = platform_refusal(bundle, host_platform()) {
                return Err(refusal);
            }
            smoke_bundle(
                Path::new(bundle),
                &fixture,
                &dir,
                &expected,
                &mut run,
                &mut run_window,
            )?;
            println!(
                "smoke-bundle: {bundle} reports {expected} and its window converts the fixture"
            );
        }
        Ok(())
    })();
    if let Some(dir) = work {
        let _ = std::fs::remove_dir_all(dir);
    }
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

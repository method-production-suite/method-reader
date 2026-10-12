//! Open a built bundle WITHOUT installing it and check what it carries:
//! was tools/smoke-bundle.ts, with the half of tools/dmg.ts it used.
//!
//! ```text
//! cargo xtask smoke-bundle --bundle dist/Screepub-linux-arm64.deb --expect-version 0.6.0
//! cargo xtask smoke-bundle --built --expect-version 0.6.0
//! ```
//!
//! For the Tauri window the engine sidecar comes out of the bundle and is
//! RUN: `--version --json` must report the version the bundle claims, and a
//! conversion of the committed fixture must produce a zip. The native
//! window has no sidecar and no headless mode yet, so for it the window's
//! binary must be present, non-empty and executable, and the version the
//! package declares (the .deb's control file, the .rpm's header, the .app's
//! Info.plist) must be the one expected. A .dmg needs hdiutil (macOS) and
//! an NSIS .exe needs 7z (Windows): either is REFUSED off its own OS rather
//! than skipped, because a check that prints nothing and exits 0 reads like
//! a check that ran.
//!
//! The DMG half is this module's own copy ([`mount_dmg_app`]); cli_build's
//! verify-signing carries another, to be unified by the owner.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::archive::{bundle_entries, deb_control_field, find_entry, rpm_header_string};
use super::bundle::{BundleDirs, os_for_platform, resolve_app};
use super::kinds::{
    AppSource, BundleOs, bundle_dir_for, discover_artifact, for_app, kinds_for_os,
    verify_bundle_file,
};
use super::native::{NATIVE_APP, package_version, plist_string};
use super::util::{FlagSpec, RunResult, Runner, host_platform, parse_flags, real_run, strip_v};
use crate::paths::repo_root;

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

/// The bundled engine's `--version --json`, against the bundle's version.
pub fn check_engine_version(result: &RunResult, expected: &str) -> Result<(), String> {
    if result.exit_code != 0 {
        return Err(format!(
            "smoke-bundle: the engine inside the bundle exited {}: {}",
            result.exit_code,
            trimmed(&result.stderr, 500)
        ));
    }
    let json = sole_json(&result.stdout, "the bundled engine")?;
    if json.get("ok") != Some(&Value::Bool(true)) {
        return Err(format!(
            "smoke-bundle: the bundled engine reported {}",
            Value::Object(json)
        ));
    }
    if json.get("version").and_then(Value::as_str) != Some(expected) {
        let said = json
            .get("version")
            .map_or("undefined".to_string(), |v| v.to_string());
        return Err(format!(
            "smoke-bundle: the engine inside this bundle says {said} but the bundle is {expected}. An installer whose About line contradicts its own filename lies about itself in every bug report it appears in."
        ));
    }
    Ok(())
}

/// smoke-cli's conversion check, applied to the bundled engine.
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
            "smoke-bundle: {bundle} can only be opened ({tool}) and its engine run on {os}, and this machine is {platform}. NOTHING WAS CHECKED -- run this step on a {os} runner. Reporting success here would mean reporting success for a sidecar no one has executed."
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

const ENGINE_IN_ARCHIVE: &str = "usr/bin/screepub-engine";
const ENGINE_IN_EXE: &str = "screepub-engine.exe";

fn engine_in_app() -> PathBuf {
    Path::new("Contents").join("MacOS").join("screepub-engine")
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

/// The engine out of a .deb or an .rpm, written executable under `work`.
pub fn extract_archive_engine(bundle: &Path, work: &Path) -> Result<PathBuf, String> {
    let entries = bundle_entries(bundle)?;
    let entry = find_entry(&entries, ENGINE_IN_ARCHIVE)?;
    let dest = work.join(ENGINE_IN_ARCHIVE);
    write_executable(&dest, &entry.data)?;
    Ok(dest)
}

/// An attached image: where it is mounted and the one app on it.
#[derive(Debug)]
pub struct Mounted {
    pub app_path: PathBuf,
    pub mount: PathBuf,
}

impl Mounted {
    /// Must run whatever happens: an attached image outlives the job.
    pub fn detach(&self, run: &mut Runner<'_>) {
        run(
            &[
                "hdiutil".into(),
                "detach".into(),
                self.mount.to_string_lossy().into_owned(),
                "-force".into(),
            ],
            None,
        );
    }
}

/// Mounts `dmg` read-only under `work` and finds the one .app on it, by
/// suffix ("Screepub Desktop.app" until the identifier release,
/// "Screepub.app" since). Detaches before any error.
pub fn mount_dmg_app(
    dmg: &Path,
    work: &Path,
    run: &mut Runner<'_>,
    tool: &str,
) -> Result<Mounted, String> {
    let mount = work.join("mnt");
    std::fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    let attach = run(
        &[
            "hdiutil".into(),
            "attach".into(),
            dmg.to_string_lossy().into_owned(),
            "-readonly".into(),
            "-nobrowse".into(),
            "-mountpoint".into(),
            mount.to_string_lossy().into_owned(),
        ],
        None,
    );
    if attach.exit_code != 0 {
        return Err(format!(
            "{tool}: hdiutil attach failed on {}: {}",
            dmg.display(),
            trimmed(&attach.stderr, 500)
        ));
    }
    let mut apps: Vec<String> = std::fs::read_dir(&mount)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".app"))
                .collect()
        })
        .unwrap_or_default();
    apps.sort();
    if apps.len() != 1 {
        Mounted {
            app_path: PathBuf::new(),
            mount,
        }
        .detach(run);
        let listed = if apps.is_empty() {
            String::new()
        } else {
            format!(" ({})", apps.join(", "))
        };
        return Err(format!(
            "{tool}: expected exactly one .app on the mounted image, found {}{listed}",
            apps.len()
        ));
    }
    Ok(Mounted {
        app_path: mount.join(&apps[0]),
        mount,
    })
}

/// The engine inside a .dmg's app. The caller detaches.
pub fn extract_dmg_engine(
    bundle: &Path,
    work: &Path,
    run: &mut Runner<'_>,
) -> Result<(PathBuf, Mounted), String> {
    let m = mount_dmg_app(bundle, work, run, "smoke-bundle")?;
    Ok((m.app_path.join(engine_in_app()), m))
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
/// `file` in what came out.
fn unpack_exe(
    bundle: &Path,
    work: &Path,
    run: &mut Runner<'_>,
    file: &str,
) -> Result<PathBuf, String> {
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
    Ok(dest)
}

pub fn extract_exe_engine(
    bundle: &Path,
    work: &Path,
    run: &mut Runner<'_>,
) -> Result<PathBuf, String> {
    unpack_exe(bundle, work, run, ENGINE_IN_EXE)
}

fn ends(path: &Path, ext: &str) -> bool {
    path.to_string_lossy().ends_with(ext)
}

/// Two runs of the engine that came out of the bundle: its version, then a
/// real conversion of the fixture, whose EPUB is then read back.
pub fn smoke_bundle(
    bundle: &Path,
    fixture: &Path,
    work: &Path,
    expected: &str,
    run: &mut Runner<'_>,
) -> Result<(), String> {
    if !bundle.exists() {
        return Err(format!("smoke-bundle: no bundle at {}", bundle.display()));
    }
    if !fixture.exists() {
        return Err(format!("smoke-bundle: no fixture at {}", fixture.display()));
    }
    let (engine, mounted) = if ends(bundle, ".deb") || ends(bundle, ".rpm") {
        (extract_archive_engine(bundle, work)?, None)
    } else if ends(bundle, ".dmg") {
        let (e, m) = extract_dmg_engine(bundle, work, run)?;
        (e, Some(m))
    } else if ends(bundle, ".exe") {
        (extract_exe_engine(bundle, work, run)?, None)
    } else {
        return Err(format!(
            "smoke-bundle: {} is not a .deb, .rpm, .dmg or .exe",
            bundle.display()
        ));
    };
    let result = (|| {
        let e = engine.to_string_lossy().into_owned();
        check_engine_version(
            &run(&[e.clone(), "--version".into(), "--json".into()], None),
            expected,
        )?;
        let epub = work.join("smoke.epub");
        let r = run(
            &[
                e,
                fixture.to_string_lossy().into_owned(),
                "-o".into(),
                epub.to_string_lossy().into_owned(),
                "--no-fountain".into(),
                "--json".into(),
            ],
            None,
        );
        check_convert_result(&r, &epub)?;
        let Ok(bytes) = std::fs::read(&epub) else {
            return Err(format!(
                "smoke-bundle: the bundled engine reported a successful conversion but {} is not there. A success object is not a book.",
                epub.display()
            ));
        };
        check_epub_bytes(&bytes[..bytes.len().min(8)])
    })();
    if let Some(m) = mounted {
        m.detach(run);
    }
    result
}

/// The native window's half: its binary is in the bundle, non-empty and
/// executable, and the package declares the expected version.
pub fn smoke_native_bundle(
    bundle: &Path,
    work: &Path,
    expected: &str,
    run: &mut Runner<'_>,
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
        let entries = bundle_entries(bundle)?;
        let bin = find_entry(&entries, &format!("usr/bin/{}", a.linux_executable))?;
        if bin.data.is_empty() || bin.mode & 0o111 == 0 {
            return Err(format!(
                "smoke-bundle: {} in {} is {} bytes with mode {:o}; the window must be a non-empty executable",
                bin.name,
                bundle.display(),
                bin.data.len(),
                bin.mode
            ));
        }
        if ends(bundle, ".deb") {
            mismatch(
                deb_control_field(bundle, "Version")?,
                &pkg_version,
                "control file",
            )
        } else {
            let bytes = std::fs::read(bundle).map_err(|e| e.to_string())?;
            mismatch(rpm_header_string(&bytes, 1001)?, &pkg_version, "rpm header")
        }
    } else if ends(bundle, ".dmg") {
        let m = mount_dmg_app(bundle, work, run, "smoke-bundle")?;
        let result = (|| {
            let exe = m
                .app_path
                .join("Contents")
                .join("MacOS")
                .join(a.mac_executable);
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
            )
        })();
        m.detach(run);
        result
    } else if ends(bundle, ".exe") {
        let exe = unpack_exe(bundle, work, run, a.windows_executable)?;
        if std::fs::metadata(&exe).map(|x| x.len()).unwrap_or(0) == 0 {
            return Err(format!("smoke-bundle: {} is empty", exe.display()));
        }
        Ok(())
    } else {
        Err(format!(
            "smoke-bundle: {} is not a .deb, .rpm, .dmg or .exe",
            bundle.display()
        ))
    }
}

/// What [`smoke_built_bundles`] is asked: which OS's bundles, found under
/// `root` (desktop/src-tauri for the Tauri window, rust/ for the native one),
/// on which host, for which window.
#[derive(Clone, Copy, Debug)]
pub struct BuiltSmoke<'a> {
    pub os: BundleOs,
    pub expected: &'a str,
    pub fixture: &'a Path,
    pub work_root: &'a Path,
    pub root: &'a Path,
    pub host: &'a str,
    pub app: AppSource,
}

/// Verify and smoke every bundle this runner just built, found under
/// `root` (desktop/src-tauri for the Tauri window, rust/ for the native
/// one). An OS with no kinds, or a kind with no bundle, is a failure.
pub fn smoke_built_bundles(
    s: &BuiltSmoke<'_>,
    run: &mut Runner<'_>,
) -> Result<Vec<PathBuf>, String> {
    let BuiltSmoke {
        os,
        expected,
        fixture,
        work_root,
        root,
        host,
        app,
    } = *s;
    let kinds = kinds_for_os(os);
    if kinds.is_empty() {
        return Err(format!(
            "smoke-bundle: no bundle kinds are defined for {}. NOTHING WAS CHECKED.",
            os.name()
        ));
    }
    let mut smoked = Vec::new();
    for kind in kinds.into_iter().map(|k| for_app(k, app)) {
        let path = discover_artifact(&bundle_dir_for(kind.dir, None, root), &kind)?;
        verify_bundle_file(&path, &kind)?;
        if let Some(refusal) = platform_refusal(&path.to_string_lossy(), host) {
            return Err(refusal);
        }
        let work = work_root.join(kind.id);
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        match app {
            AppSource::Tauri => smoke_bundle(&path, fixture, &work, expected, run)?,
            AppSource::Native => smoke_native_bundle(&path, &work, expected, run)?,
        }
        println!("smoke-bundle: {} ok -- {}", kind.id, path.display());
        smoked.push(path);
    }
    Ok(smoked)
}

/// `cargo xtask smoke-bundle`.
pub fn main(args: &[String]) -> u8 {
    if super::util::wants_help(
        args,
        "usage: cargo xtask smoke-bundle --expect-version <x.y.z> (--bundle <file> | --built) [--fixture <pdf>] [--app tauri|native]",
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
        let repo = repo_root();
        let fixture = f
            .get("fixture")
            .map(PathBuf::from)
            .unwrap_or_else(|| repo.join("tests").join("fixtures").join("screenplay.pdf"));
        let dirs = BundleDirs::for_repo(&repo);
        let app = resolve_app(
            match f.get("app").map(String::as_str) {
                None => None,
                Some("tauri") => Some(AppSource::Tauri),
                Some("native") => Some(AppSource::Native),
                Some(other) => {
                    return Err(format!(
                        "smoke-bundle: --app is tauri or native, not \"{other}\""
                    ));
                }
            },
            &dirs,
        );
        let dir =
            std::env::temp_dir().join(format!("screepub-bundle-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        work = Some(dir.clone());
        let mut run = real_run;
        if f.contains_key("built") {
            let os = os_for_platform(host_platform())?;
            let root = if app == AppSource::Tauri {
                dirs.desktop_dir
            } else {
                dirs.rust_dir
            };
            let smoked = smoke_built_bundles(
                &BuiltSmoke {
                    os,
                    expected: &expected,
                    fixture: &fixture,
                    work_root: &dir,
                    root: &root,
                    host: host_platform(),
                    app,
                },
                &mut run,
            )?;
            println!(
                "smoke-bundle: {} bundle(s) report {expected}{}",
                smoked.len(),
                if app == AppSource::Tauri {
                    " and convert the fixture"
                } else {
                    ""
                }
            );
        } else {
            let Some(bundle) = f.get("bundle") else {
                return Err("smoke-bundle: pass --bundle <path> or --built".to_string());
            };
            if let Some(refusal) = platform_refusal(bundle, host_platform()) {
                return Err(refusal);
            }
            match app {
                AppSource::Tauri => {
                    smoke_bundle(Path::new(bundle), &fixture, &dir, &expected, &mut run)?
                }
                AppSource::Native => {
                    smoke_native_bundle(Path::new(bundle), &dir, &expected, &mut run)?
                }
            }
            println!(
                "smoke-bundle: {bundle} reports {expected}{}",
                if app == AppSource::Tauri {
                    " and its engine converts the fixture"
                } else {
                    ""
                }
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

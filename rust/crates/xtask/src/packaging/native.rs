//! Bundling the native window (rust/crates/sp-app's `method-reader-app`) for
//! `cargo xtask build-app-bundle`.
//!
//! Every file is written by sp-release, the library release.yml runs: the
//! `.app` ([`sp_release::mac`]), the update archive
//! ([`sp_release::archive::app_archive`]) and its signature
//! ([`sp_release::sign`]), the .deb and the .rpm ([`sp_release::linux`]) and
//! the NSIS script ([`sp_release::nsis`]). This module only orders the
//! steps a laptop or the /release skill runs, and drives the outside tools
//! through the [`Runner`] seam, each overridable by environment variable
//! (see [`NativeTools`]): `cargo`, and on a Mac `codesign`, `ditto`,
//! `xcrun notarytool`, `xcrun stapler` and `hdiutil`; on Windows
//! `makensis`.
//!
//! The names live in sp-release's names.rs: the product name, the .app's
//! name, the executable name (`method-reader-app`), the Linux package name
//! and the Windows install folder, each with the 0.12.x (Screepub) name it
//! replaces beside it. The bundle identifier is Method Reader's own from
//! 0.13 (com.methodreader.desktop; it was com.darkwell.screepub), which
//! no updater compares. A 0.11.x or
//! 0.12.x app's updater downloads `Method-Reader-macOS.app.tar.gz`, checks
//! its signature against the key it was built with, strips the archive's
//! first path component and moves what is left over its own
//! `Screepub.app`, whatever the archive's top folder is called.

use std::path::{Path, PathBuf};

use sp_release::names;

use super::bundle::{BundleArgs, BundleDirs, Env};
use super::kinds::{BundleArch, BundleOs, SIGNATURE_EXT, bundle_dir_for};
use super::signature::{sign_file, verify_file};
use super::util::Runner;

/// What the native packages say about the app, from sp-release's names.
#[derive(Clone, Copy, Debug)]
pub struct NativeApp {
    pub product_name: &'static str,
    /// CFBundleIdentifier: the Tauri window's, unchanged by the rename, so
    /// macOS sees one app.
    pub identifier: &'static str,
    /// The .deb and .rpm package name, which replaces the old `screepub`
    /// package ([`sp_release::linux`]).
    pub package: &'static str,
    pub crate_name: &'static str,
    /// What cargo builds.
    pub binary: &'static str,
    /// The installed executable on every platform (`.exe` on Windows).
    pub mac_executable: &'static str,
    pub linux_executable: &'static str,
    pub windows_executable: &'static str,
    pub publisher: &'static str,
    pub short_description: &'static str,
    pub long_description: &'static str,
    pub minimum_macos: &'static str,
    /// (file in the repo, name in the bundle)
    pub resources: &'static [(&'static str, &'static str)],
    /// Where the icons are looked for, first match wins: sp-app's own
    /// folder, where they moved from desktop/src-tauri/icons at the cutover.
    pub icon_dirs: &'static [&'static str],
}

pub const NATIVE_APP: NativeApp = NativeApp {
    product_name: names::PRODUCT,
    identifier: sp_update::IDENTIFIER,
    package: sp_release::linux::PACKAGE,
    crate_name: "sp-app",
    binary: names::WINDOW_EXE,
    mac_executable: names::WINDOW_EXE,
    linux_executable: names::WINDOW_EXE,
    windows_executable: "method-reader-app.exe",
    publisher: names::PUBLISHER,
    short_description: names::SHORT_DESCRIPTION,
    long_description: names::LONG_DESCRIPTION,
    minimum_macos: names::MAC_MINIMUM,
    resources: &[
        ("LICENSE-MIT", "LICENSE-MIT"),
        ("LICENSE-APACHE", "LICENSE-APACHE"),
        ("rust/crates/sp-azw3/LICENSE-GPL-3.0", "LICENSE-GPL-3.0"),
        ("THIRD-PARTY-NOTICES.md", "THIRD-PARTY-NOTICES.md"),
    ],
    icon_dirs: &["rust/crates/sp-app/icons"],
};

/// The outside programs, each overridable: METHOD_READER_CARGO, METHOD_READER_CODESIGN,
/// METHOD_READER_DITTO, METHOD_READER_XCRUN, METHOD_READER_HDIUTIL, METHOD_READER_MAKENSIS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTools {
    pub cargo: String,
    pub codesign: String,
    pub ditto: String,
    pub xcrun: String,
    pub hdiutil: String,
    pub makensis: String,
}

impl NativeTools {
    pub fn from_env(env: &Env) -> Self {
        let pick =
            |var: &str, default: &str| env.get(var).cloned().unwrap_or_else(|| default.into());
        NativeTools {
            cargo: pick("METHOD_READER_CARGO", "cargo"),
            codesign: pick("METHOD_READER_CODESIGN", "codesign"),
            ditto: pick("METHOD_READER_DITTO", "ditto"),
            xcrun: pick("METHOD_READER_XCRUN", "xcrun"),
            hdiutil: pick("METHOD_READER_HDIUTIL", "hdiutil"),
            makensis: pick("METHOD_READER_MAKENSIS", "makensis"),
        }
    }
}

fn io<T>(what: &str, r: std::io::Result<T>) -> Result<T, String> {
    r.map_err(|e| format!("build-app-bundle: {what}: {e}"))
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

fn p(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Runs one outside step, failing with its stderr.
fn step(
    run: &mut Runner<'_>,
    argv: Vec<String>,
    cwd: Option<&Path>,
    what: &str,
) -> Result<String, String> {
    let mut shown = argv.clone();
    for i in 1..shown.len() {
        if shown[i - 1] == "--password" {
            shown[i] = "***".into();
        }
    }
    println!("── {}", shown.join(" "));
    let r = run(&argv, cwd);
    if r.exit_code != 0 {
        return Err(format!(
            "build-app-bundle: {what} failed (exit {})\n{}",
            r.exit_code,
            r.stderr.trim()
        ));
    }
    Ok(r.stdout)
}

/// The cargo invocation that builds the window.
pub fn cargo_argv(tools: &NativeTools, target: Option<&str>) -> Vec<String> {
    let mut argv = vec![
        tools.cargo.clone(),
        "build".into(),
        "--release".into(),
        "--locked".into(),
        "--package".into(),
        NATIVE_APP.crate_name.into(),
        "--bin".into(),
        NATIVE_APP.binary.into(),
    ];
    if let Some(t) = target {
        argv.extend(["--target".into(), t.to_string()]);
    }
    argv
}

/// Where cargo leaves the binary.
pub fn binary_path(rust_dir: &Path, target: Option<&str>, os: BundleOs) -> PathBuf {
    let mut dir = rust_dir.join("target");
    if let Some(t) = target {
        dir.push(t);
    }
    let exe = if os == BundleOs::Windows {
        format!("{}.exe", NATIVE_APP.binary)
    } else {
        NATIVE_APP.binary.to_string()
    };
    dir.join("release").join(exe)
}

/// A version every package format accepts ([`sp_release::linux::package_version`]).
pub fn package_version(version: &str) -> String {
    sp_release::linux::package_version(version)
}

/// The .app's Info.plist ([`sp_release::mac::info_plist`]).
pub fn info_plist(version: &str) -> String {
    sp_release::mac::info_plist(version, NATIVE_APP.identifier)
}

/// One string value of an Info.plist, as [`info_plist`] writes them.
pub fn plist_string(plist: &str, key: &str) -> Option<String> {
    let at = plist.find(&format!("<key>{key}</key>"))?;
    let rest = &plist[at..];
    let open = rest.find("<string>")? + "<string>".len();
    let close = rest[open..].find("</string>")? + open;
    Some(
        rest[open..close]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&amp;", "&"),
    )
}

fn find_icons(repo: &Path, needed: &[&str]) -> Result<PathBuf, String> {
    for dir in NATIVE_APP.icon_dirs {
        let d = repo.join(dir);
        if needed.iter().all(|n| d.join(n).is_file()) {
            return Ok(d);
        }
    }
    Err(format!(
        "build-app-bundle: no icon folder holds {} (looked in {})",
        needed.join(", "),
        NATIVE_APP.icon_dirs.join(", ")
    ))
}

/// The resources every bundle carries, as paths in the repository.
fn resources(repo: &Path) -> Result<Vec<PathBuf>, String> {
    NATIVE_APP
        .resources
        .iter()
        .map(|(from, _)| {
            let p = repo.join(from);
            if p.is_file() {
                Ok(p)
            } else {
                Err(format!("build-app-bundle: cannot read {from}"))
            }
        })
        .collect()
}

/// The Linux icons as sp-release takes them: (hicolor folder, PNG).
fn linux_icons(repo: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let icons = find_icons(repo, &["32x32.png", "128x128.png", "128x128@2x.png"])?;
    Ok([
        ("32x32", "32x32.png"),
        ("128x128", "128x128.png"),
        ("256x256@2", "128x128@2x.png"),
    ]
    .into_iter()
    .map(|(size, file)| (size.to_string(), icons.join(file)))
    .collect())
}

/// Lays out `<dir>/Method Reader.app` around `binary` with [`sp_release::mac::build_app`].
pub fn assemble_app(
    dir: &Path,
    binary: &Path,
    version: &str,
    repo: &Path,
) -> Result<PathBuf, String> {
    let icons = find_icons(repo, &["icon.icns"])?;
    let resources = resources(repo)?;
    io(
        "cannot lay out the .app",
        sp_release::mac::build_app(
            &sp_release::mac::MacApp {
                version,
                identifier: NATIVE_APP.identifier,
                window: binary,
                helpers: &[],
                icon: &icons.join("icon.icns"),
                resources: &resources,
            },
            dir,
        ),
    )
}

fn fresh_dir(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        io(
            &format!("cannot clear {}", dir.display()),
            std::fs::remove_dir_all(dir),
        )?;
    }
    io(
        &format!("cannot create {}", dir.display()),
        std::fs::create_dir_all(dir),
    )
}

fn write_file(path: &Path, bytes: &[u8], executable: bool) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        io(
            &format!("cannot create {}", parent.display()),
            std::fs::create_dir_all(parent),
        )?;
    }
    io(
        &format!("cannot write {}", path.display()),
        std::fs::write(path, bytes),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        io(
            &format!("cannot set the mode of {}", path.display()),
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)),
        )?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    Ok(())
}

/// The public key the archive must verify against: `--pubkey` (a path or
/// the text), else [`sp_update::PUBKEY`], the one home of the key the
/// shipped app trusts.
pub fn trusted_pubkey(args: &BundleArgs) -> Result<String, String> {
    super::signature::trusted_pubkey(args.pubkey.as_deref()).map(|t| t.trim().to_string())
}

fn source_date(env: &Env) -> u64 {
    env.get("SOURCE_DATE_EPOCH")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        })
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    io(
        &format!("cannot create {}", to.display()),
        std::fs::create_dir_all(to),
    )?;
    for e in io("list", std::fs::read_dir(from))?.filter_map(Result::ok) {
        let src = e.path();
        let dst = to.join(e.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            io(
                &format!("cannot copy {}", src.display()),
                std::fs::copy(&src, &dst),
            )?;
        }
    }
    Ok(())
}

/// The notarytool credentials tauri-bundler read: an App Store Connect API
/// key, or an Apple ID with an app-specific password.
pub fn notary_credentials(env: &Env) -> Option<Vec<String>> {
    let get = |k: &str| env.get(k).filter(|v| !v.is_empty()).cloned();
    if let (Some(key), Some(issuer), Some(path)) = (
        get("APPLE_API_KEY"),
        get("APPLE_API_ISSUER"),
        get("APPLE_API_KEY_PATH"),
    ) {
        return Some(vec![
            "--key".into(),
            path,
            "--key-id".into(),
            key,
            "--issuer".into(),
            issuer,
        ]);
    }
    if let (Some(id), Some(pw), Some(team)) =
        (get("APPLE_ID"), get("APPLE_PASSWORD"), get("APPLE_TEAM_ID"))
    {
        return Some(vec![
            "--apple-id".into(),
            id,
            "--password".into(),
            pw,
            "--team-id".into(),
            team,
        ]);
    }
    None
}

fn build_mac(
    args: &BundleArgs,
    run: &mut Runner<'_>,
    dirs: &BundleDirs,
    env: &Env,
    tools: &NativeTools,
    binary: &Path,
) -> Result<(), String> {
    let target = args.target.as_deref();
    let macos = bundle_dir_for("macos", target, &dirs.rust_dir);
    fresh_dir(&macos)?;
    let app = assemble_app(&macos, binary, &args.version, &dirs.repo_dir)?;
    let identity = env
        .get("APPLE_SIGNING_IDENTITY")
        .filter(|v| !v.is_empty())
        .cloned();

    // Hardened runtime and a timestamp with a real identity, which
    // notarization requires; ad hoc otherwise, so a local build still opens.
    let sign = match &identity {
        Some(id) => vec![
            tools.codesign.clone(),
            "--force".into(),
            "--options".into(),
            "runtime".into(),
            "--timestamp".into(),
            "--sign".into(),
            id.clone(),
            p(&app),
        ],
        None => vec![
            tools.codesign.clone(),
            "--force".into(),
            "--sign".into(),
            "-".into(),
            p(&app),
        ],
    };
    step(run, sign, None, "codesign")?;

    if let (Some(_), Some(creds)) = (&identity, notary_credentials(env)) {
        let zip = macos.join(format!("{}.zip", NATIVE_APP.product_name));
        step(
            run,
            vec![
                tools.ditto.clone(),
                "-c".into(),
                "-k".into(),
                "--keepParent".into(),
                p(&app),
                p(&zip),
            ],
            None,
            "ditto",
        )?;
        let mut submit = vec![
            tools.xcrun.clone(),
            "notarytool".into(),
            "submit".into(),
            p(&zip),
        ];
        submit.extend(creds);
        submit.extend(strings(&["--wait", "--output-format", "json"]));
        let out = step(run, submit, None, "notarytool submit")?;
        let status = serde_json::from_str::<serde_json::Value>(out.trim())
            .ok()
            .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(str::to_string));
        if status.as_deref() != Some("Accepted") {
            return Err(format!(
                "build-app-bundle: notarization did not accept {} (status {}). `xcrun notarytool log <id>` says why.\n{}",
                p(&app),
                status.as_deref().unwrap_or("<none>"),
                out.trim()
            ));
        }
        let _ = std::fs::remove_file(&zip);
        step(
            run,
            vec![
                tools.xcrun.clone(),
                "stapler".into(),
                "staple".into(),
                p(&app),
            ],
            None,
            "stapler staple",
        )?;
    }

    if args.updater {
        let pubkey = trusted_pubkey(args)?;
        let tar_path = macos.join(names::MAC_UPDATE);
        let sig_path = PathBuf::from(format!("{}{SIGNATURE_EXT}", tar_path.display()));
        let _ = std::fs::remove_file(&sig_path);
        let archive = io(
            "cannot pack the update archive",
            sp_release::archive::app_archive(&app, source_date(env)),
        )?;
        write_file(&tar_path, &archive, false)?;
        let key = env
            .get("TAURI_SIGNING_PRIVATE_KEY")
            .map(String::as_str)
            .unwrap_or("");
        let password = env
            .get("TAURI_SIGNING_PRIVATE_KEY_PASSWORD")
            .cloned()
            .unwrap_or_default();
        let sig = sign_file(&tar_path, key, &password, &args.version, source_date(env))?;
        // Stricter than tauri-cli, which only warned: an archive the shipped
        // app's key does not verify would fail every install.
        verify_file(&tar_path, &sig, &pubkey, &args.version)?;
        write_file(&sig_path, sig.as_bytes(), false)?;
        println!("   signed {} for the updater", p(&tar_path));
    }

    let dmg_dir = bundle_dir_for("dmg", target, &dirs.rust_dir);
    fresh_dir(&dmg_dir)?;
    let root = macos.join("dmg-root");
    fresh_dir(&root)?;
    copy_dir(&app, &root.join(app.file_name().unwrap_or_default()))?;
    #[cfg(unix)]
    io(
        "cannot link /Applications",
        std::os::unix::fs::symlink("/Applications", root.join("Applications")),
    )?;
    let arch = if args.arch == BundleArch::Arm64 {
        "aarch64"
    } else {
        "x64"
    };
    let dmg = dmg_dir.join(format!(
        "{}_{}_{arch}.dmg",
        NATIVE_APP.product_name, args.version
    ));
    step(
        run,
        vec![
            tools.hdiutil.clone(),
            "create".into(),
            "-volname".into(),
            NATIVE_APP.product_name.into(),
            "-srcfolder".into(),
            p(&root),
            "-ov".into(),
            "-format".into(),
            "UDZO".into(),
            p(&dmg),
        ],
        None,
        "hdiutil create",
    )?;
    let _ = std::fs::remove_dir_all(&root);
    if let Some(id) = identity {
        step(
            run,
            vec![
                tools.codesign.clone(),
                "--force".into(),
                "--sign".into(),
                id,
                p(&dmg),
            ],
            None,
            "codesign the image",
        )?;
    }
    Ok(())
}

fn build_linux(
    args: &BundleArgs,
    dirs: &BundleDirs,
    env: &Env,
    binary: &Path,
) -> Result<(), String> {
    let target = args.target.as_deref();
    let icons = linux_icons(&dirs.repo_dir)?;
    let resources = resources(&dirs.repo_dir)?;
    // No Depends: here. release.yml measures it on the built binary with
    // dpkg-shlibdeps and passes it to `sp-release deb --depends`; a laptop
    // build leaves it empty (rust/PARITY.md, Decisions).
    let app = sp_release::linux::LinuxApp {
        version: &args.version,
        window: binary,
        icons: &icons,
        resources: &resources,
        depends: "",
    };
    let mtime = source_date(env);
    let ver = package_version(&args.version);
    let (deb_arch, rpm_arch) = if args.arch == BundleArch::X64 {
        ("amd64", "x86_64")
    } else {
        ("arm64", "aarch64")
    };
    let deb_dir = bundle_dir_for("deb", target, &dirs.rust_dir);
    fresh_dir(&deb_dir)?;
    let deb = deb_dir.join(format!("{}_{ver}_{deb_arch}.deb", NATIVE_APP.package));
    let deb_bytes = io(
        "cannot write the .deb",
        sp_release::linux::build_deb(&app, deb_arch, mtime),
    )?;
    write_file(&deb, &deb_bytes, false)?;
    let rpm_dir = bundle_dir_for("rpm", target, &dirs.rust_dir);
    fresh_dir(&rpm_dir)?;
    let rpm = rpm_dir.join(format!("{}-{ver}-1.{rpm_arch}.rpm", NATIVE_APP.package));
    let rpm_bytes = sp_release::linux::build_rpm(&app, rpm_arch, mtime as u32)
        .map_err(|e| format!("build-app-bundle: cannot write the .rpm: {e}"))?;
    write_file(&rpm, &rpm_bytes, false)?;
    println!("   wrote {} and {}", p(&deb), p(&rpm));
    Ok(())
}

fn build_windows(
    args: &BundleArgs,
    run: &mut Runner<'_>,
    dirs: &BundleDirs,
    tools: &NativeTools,
    binary: &Path,
) -> Result<(), String> {
    let target = args.target.as_deref();
    let stage = bundle_dir_for("nsis-stage", target, &dirs.rust_dir);
    fresh_dir(&stage)?;
    let window = io("cannot read the window", std::fs::read(binary))?;
    write_file(&stage.join(NATIVE_APP.windows_executable), &window, true)?;
    for r in resources(&dirs.repo_dir)? {
        let bytes = io("cannot read a resource", std::fs::read(&r))?;
        write_file(
            &stage.join(r.file_name().unwrap_or_default()),
            &bytes,
            false,
        )?;
    }
    let icons = find_icons(&dirs.repo_dir, &["icon.ico"])?;
    let ico = io(
        "cannot read icon.ico",
        std::fs::read(icons.join("icon.ico")),
    )?;
    write_file(&stage.join("icon.ico"), &ico, false)?;
    let nsis_dir = bundle_dir_for("nsis", target, &dirs.rust_dir);
    fresh_dir(&nsis_dir)?;
    let out = nsis_dir.join(format!(
        "{}_{}_x64-setup.exe",
        NATIVE_APP.product_name, args.version
    ));
    let script = stage.join("installer.nsi");
    write_file(
        &script,
        sp_release::nsis::script(&args.version, &p(&stage), &p(&out)).as_bytes(),
        false,
    )?;
    step(
        run,
        vec![tools.makensis.clone(), p(&script)],
        Some(&stage),
        "makensis",
    )?;
    Ok(())
}

/// Builds the native window and leaves its bundles where the shared half of
/// build-app-bundle looks: rust/target/[<triple>/]release/bundle/<kind>/.
pub fn build_native(
    args: &BundleArgs,
    run: &mut Runner<'_>,
    dirs: &BundleDirs,
    env: &Env,
) -> Result<(), String> {
    let tools = NativeTools::from_env(env);
    let target = args.target.as_deref();
    step(
        run,
        cargo_argv(&tools, target),
        Some(&dirs.rust_dir),
        "cargo build of the native window",
    )?;
    let bin = binary_path(&dirs.rust_dir, target, args.os);
    if !bin.is_file() {
        return Err(format!(
            "build-app-bundle: cargo reported success and there is no {}",
            bin.display()
        ));
    }
    match args.os {
        BundleOs::Macos => build_mac(args, run, dirs, env, &tools, &bin),
        BundleOs::Linux => build_linux(args, dirs, env, &bin),
        BundleOs::Windows => build_windows(args, run, dirs, &tools, &bin),
    }
}

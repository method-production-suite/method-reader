//! Build, name and VERIFY the installable app bundles: was
//! tools/build-app-bundle.ts.
//!
//! ```text
//! cargo xtask build-app-bundle --version 0.12.0 --out dist/
//! cargo xtask build-app-bundle --version 0.12.0 --out dist/ --arch arm64 --updater
//! ```
//!
//! The window is the native one: `cargo build -p sp-app` in rust/, then
//! [`super::native`] assembles the .app, .dmg, .deb, .rpm and NSIS
//! installer and signs the updater archive, calling the outside tools
//! (codesign, ditto, xcrun, hdiutil, makensis) through the [`Runner`] seam.
//! (Until the Rust cutover `--app tauri` bundled the Tauri window through
//! `cargo tauri build`, with its engine sidecar; that went with the window.)
//!
//! The files land in `rust/target/[<triple>/]release/bundle/`, and then:
//! discover each kind's one file, verify its container and size, copy it
//! out under its published name, verify again, and write SHA256SUMS-app. A
//! Mac bundle is Apple silicon ONLY.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::kinds::{
    BundleArch, BundleOs, MAC_ARCH, MAC_TARGET, SIGNATURE_EXT, bundle_dir_for, discover_artifact,
    kinds_for_os, released_name, updater_kinds_for_os, verify_bundle_file,
};
use super::native;
use super::signature::parse_signature_box;
use super::util::{
    FlagSpec, Runner, absolute, host_arch, host_platform, is_release_version, parse_flags,
    real_build, real_run, strip_v, write_checksums,
};
use crate::paths::repo_root;

pub type Env = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleArgs {
    pub version: String,
    pub out_dir: PathBuf,
    pub os: BundleOs,
    pub arch: BundleArch,
    pub target: Option<String>,
    /// Also produce the updater archive and its signature.
    pub updater: bool,
    /// The public key the updater archive must verify against (a path or
    /// the base64 text); defaults to [`sp_update::PUBKEY`], the key the
    /// shipped app trusts.
    pub pubkey: Option<String>,
}

impl BundleArgs {
    /// A run with nothing optional set.
    pub fn new(version: &str, out_dir: &Path, os: BundleOs, arch: BundleArch) -> Self {
        BundleArgs {
            version: version.to_string(),
            out_dir: out_dir.to_path_buf(),
            os,
            arch,
            target: None,
            updater: false,
            pubkey: None,
        }
    }
}

/// Where a run reads the repo and builds: parameters so a test can point a
/// whole run at a scratch tree.
#[derive(Clone, Debug)]
pub struct BundleDirs {
    pub repo_dir: PathBuf,
    /// rust/, the window's workspace.
    pub rust_dir: PathBuf,
}

impl BundleDirs {
    pub fn for_repo(repo: &Path) -> Self {
        BundleDirs {
            repo_dir: repo.to_path_buf(),
            rust_dir: repo.join("rust"),
        }
    }
}

/// The `version` key of the `[package]` TABLE and nothing else: a missing
/// key answers None rather than a dependency's version.
pub fn cargo_package_version(text: &str) -> Option<String> {
    crate::cli_build::toml_string(text, "package", "version")
}

/// `[workspace.package]`'s version, which every crate inherits:
/// [`sp_release::version::workspace_version`], the reader release.yml's
/// version gate uses.
pub fn workspace_package_version(text: &str) -> Option<String> {
    sp_release::version::workspace_version(text)
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("build-app-bundle: cannot read {}: {e}", path.display()))
}

/// The window's one version: rust/Cargo.toml's
/// `[workspace.package]`, which sp-app must inherit rather than override.
pub fn assert_native_versions(version: &str, rust_dir: &Path) -> Result<(), String> {
    let workspace = workspace_package_version(&read(&rust_dir.join("Cargo.toml"))?);
    if workspace.as_deref() != Some(version) {
        return Err(format!(
            "build-app-bundle: rust/Cargo.toml's [workspace.package] says {} but the version being built is {version}. The window's About line, its Info.plist and its installer all read that one number.",
            workspace.as_deref().unwrap_or("<nothing>")
        ));
    }
    let app = read(&rust_dir.join("crates/sp-app/Cargo.toml"))?;
    if cargo_package_version(&app).is_some() || !app.contains("version.workspace = true") {
        return Err("build-app-bundle: rust/crates/sp-app/Cargo.toml must say `version.workspace = true`, so the window reports the workspace's version and nothing else.".into());
    }
    Ok(())
}

pub fn os_for_platform(platform: &str) -> Result<BundleOs, String> {
    match platform {
        "linux" => Ok(BundleOs::Linux),
        "darwin" => Ok(BundleOs::Macos),
        "win32" => Ok(BundleOs::Windows),
        other => Err(format!(
            "build-app-bundle: no app bundle is defined for platform \"{other}\""
        )),
    }
}

/// A Mac bundle is Apple silicon, built for the Apple silicon triple, or
/// it is refused. No-op off macOS.
pub fn assert_apple_silicon_mac(
    os: BundleOs,
    arch: BundleArch,
    target: Option<&str>,
) -> Result<(), String> {
    if os != BundleOs::Macos || (arch == MAC_ARCH && target == Some(MAC_TARGET)) {
        return Ok(());
    }
    Err(format!(
        "build-app-bundle: a Mac bundle is built for Apple silicon only (--arch {}, target {MAC_TARGET}), and this asked for arch {}, target {}. Intel Macs are not supported after 0.9.0 (docs/superpowers/plans/2026-10-01-f2-handover-amendment.md), and an Intel or universal image would be published under the one Mac name, Method-Reader-macOS.dmg. An Intel Mac can still cross-build the Apple silicon bundle: rustup target add aarch64-apple-darwin, then --arch arm64.",
        MAC_ARCH.name(),
        arch.name(),
        target.unwrap_or("<the host>")
    ))
}

pub fn parse_bundle_args(
    argv: &[String],
    platform: &str,
    host: &str,
) -> Result<BundleArgs, String> {
    let f = parse_flags(
        argv,
        &[
            FlagSpec {
                name: "version",
                value: true,
            },
            FlagSpec {
                name: "out",
                value: true,
            },
            FlagSpec {
                name: "target",
                value: true,
            },
            FlagSpec {
                name: "arch",
                value: true,
            },
            FlagSpec {
                name: "updater",
                value: false,
            },
            FlagSpec {
                name: "app",
                value: true,
            },
            FlagSpec {
                name: "pubkey",
                value: true,
            },
        ],
    )?;
    let given = f.get("version").map(String::as_str);
    let version = strip_v(given.unwrap_or("")).to_string();
    if !is_release_version(&version) {
        return Err(format!(
            "build-app-bundle: --version must be MAJOR.MINOR.PATCH (got {}). release.yml can be dispatched against a branch, and a branch name must never reach a bundle filename.",
            given.unwrap_or("<missing>")
        ));
    }
    let Some(out) = f.get("out") else {
        return Err("build-app-bundle: --out <dir> is required. There is no default: each bundle is ~44 MB and a Linux run writes two of them plus a checksums file.".into());
    };
    let arch_name = f.get("arch").map(String::as_str).unwrap_or(host);
    if arch_name == "universal" {
        return Err("build-app-bundle: \"universal\" is not an architecture any more. 0.9.0 was the last universal Mac image; a Mac bundle is Apple silicon only (--arch arm64). See docs/superpowers/plans/2026-10-01-f2-handover-amendment.md.".into());
    }
    let arch = match arch_name {
        "x64" => BundleArch::X64,
        "arm64" => BundleArch::Arm64,
        other => {
            return Err(format!(
                "build-app-bundle: no app bundle is defined for architecture \"{other}\""
            ));
        }
    };
    let os = os_for_platform(platform)?;
    let target = f
        .get("target")
        .cloned()
        .or_else(|| (os == BundleOs::Macos).then(|| MAC_TARGET.to_string()));
    assert_apple_silicon_mac(os, arch, target.as_deref())?;
    // `--app native` is still taken, so a command written before the
    // cutover keeps working; it names the only window there is.
    match f.get("app").map(String::as_str) {
        None | Some("native") => {}
        Some("tauri") => {
            return Err("build-app-bundle: the Tauri window is gone (the Rust cutover); the native window is the only one, so drop --app tauri.".into());
        }
        Some(other) => {
            return Err(format!(
                "build-app-bundle: --app is native, the only window, not \"{other}\""
            ));
        }
    }
    Ok(BundleArgs {
        version,
        out_dir: absolute(out),
        os,
        arch,
        target,
        updater: f.contains_key("updater"),
        pubkey: f.get("pubkey").cloned(),
    })
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to).map(|_| ()).map_err(|e| {
        format!(
            "build-app-bundle: cannot copy {} to {}: {e}",
            from.display(),
            to.display()
        )
    })
}

/// One build per OS, then discover, verify, rename and checksum. Returns
/// the paths written to `out_dir`.
pub fn build_bundles(
    args: &BundleArgs,
    run: &mut Runner<'_>,
    dirs: &BundleDirs,
    env: &Env,
) -> Result<Vec<PathBuf>, String> {
    assert_apple_silicon_mac(args.os, args.arch, args.target.as_deref())?;
    assert_native_versions(&args.version, &dirs.rust_dir)?;
    if args.updater
        && env
            .get("TAURI_SIGNING_PRIVATE_KEY")
            .is_none_or(String::is_empty)
    {
        return Err("build-app-bundle: --updater needs TAURI_SIGNING_PRIVATE_KEY in the environment, and it is not set. See docs/release-secrets.md §4.".into());
    }
    if args.updater && !env.contains_key("TAURI_SIGNING_PRIVATE_KEY_PASSWORD") {
        return Err("build-app-bundle: --updater needs TAURI_SIGNING_PRIVATE_KEY_PASSWORD SET in the environment, empty if the key has no password (TAURI_SIGNING_PRIVATE_KEY_PASSWORD=\"\"), the contract release.yml's secrets keep. Unset, a missing secret would read as no password and fail later, after the build.".into());
    }
    std::fs::create_dir_all(&args.out_dir).map_err(|e| {
        format!(
            "build-app-bundle: cannot create {}: {e}",
            args.out_dir.display()
        )
    })?;

    native::build_native(args, run, dirs, env)?;
    let root = dirs.rust_dir.clone();

    let mut names = Vec::new();
    for kind in kinds_for_os(args.os) {
        let built = discover_artifact(
            &bundle_dir_for(kind.dir, args.target.as_deref(), &root),
            &kind,
        )?;
        verify_bundle_file(&built, &kind)?;
        let name = released_name(&kind, &args.version, args.arch);
        let dest = args.out_dir.join(&name);
        copy(&built, &dest)?;
        // Again AFTER the copy: a full disk truncates silently.
        verify_bundle_file(&dest, &kind)?;
        let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        println!("   {} → {name}  {size} bytes  ok", name_of(&built));
        names.push(name);
    }

    // The updater archive and its signature, only when asked, and NOT in
    // SHA256SUMS-app: the archive is proven by its signature.
    let mut extras = Vec::new();
    if args.updater {
        for kind in updater_kinds_for_os(args.os) {
            let built = discover_artifact(
                &bundle_dir_for(kind.dir, args.target.as_deref(), &root),
                &kind,
            )?;
            verify_bundle_file(&built, &kind)?;
            let sig = PathBuf::from(format!("{}{SIGNATURE_EXT}", built.display()));
            if !sig.exists() {
                return Err(format!(
                    "build-app-bundle: {}: {} has no {} beside it. The archive is signed right after it is packed, so a missing one means signing did not run.",
                    kind.id,
                    name_of(&built),
                    name_of(&sig)
                ));
            }
            let sig_text = std::fs::read_to_string(&sig).map_err(|e| {
                format!(
                    "build-app-bundle: {}: cannot read {}: {e}",
                    kind.id,
                    name_of(&sig)
                )
            })?;
            parse_signature_box(&sig_text).map_err(|e| {
                format!(
                    "build-app-bundle: {}: {} is not a minisign signature: {e}",
                    kind.id,
                    name_of(&sig)
                )
            })?;
            let name = released_name(&kind, &args.version, args.arch);
            let dest = args.out_dir.join(&name);
            copy(&built, &dest)?;
            verify_bundle_file(&dest, &kind)?;
            let sig_dest = PathBuf::from(format!("{}{SIGNATURE_EXT}", dest.display()));
            copy(&sig, &sig_dest)?;
            if std::fs::read_to_string(&sig_dest).ok().as_deref() != Some(sig_text.as_str()) {
                return Err(format!(
                    "build-app-bundle: {}: {} did not copy intact",
                    kind.id,
                    name_of(&sig_dest)
                ));
            }
            let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            println!(
                "   {} → {name} (+{SIGNATURE_EXT})  {size} bytes  ok",
                name_of(&built)
            );
            extras.push(name.clone());
            extras.push(format!("{name}{SIGNATURE_EXT}"));
        }
    }

    // NOT "SHA256SUMS": the CLI archives' file has that name on the same
    // release.
    write_checksums(&args.out_dir, &names, "SHA256SUMS-app")?;
    Ok(names
        .iter()
        .chain(extras.iter())
        .map(|n| args.out_dir.join(n))
        .collect())
}

/// `cargo xtask build-app-bundle`.
pub fn main(argv: &[String]) -> u8 {
    if super::util::wants_help(
        argv,
        "usage: cargo xtask build-app-bundle --version <x.y.z> --out <dir> [--arch x64|arm64] [--target <triple>] [--updater] [--pubkey <file or base64>]\n\n--updater also makes the signed updater archive (TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD must be set), checked against the key the shipped app trusts (sp_update::PUBKEY) unless --pubkey names another.\nThe outside tools can be redirected with METHOD_READER_CARGO, METHOD_READER_CODESIGN, METHOD_READER_DITTO, METHOD_READER_XCRUN, METHOD_READER_HDIUTIL and METHOD_READER_MAKENSIS.",
    ) {
        return 0;
    }
    let result = parse_bundle_args(argv, host_platform(), host_arch()).and_then(|args| {
        let env: Env = std::env::vars().collect();
        // cargo's progress goes to the terminal; every other tool's answer
        // is read (notarytool's JSON, hdiutil's errors).
        let mut run = |argv: &[String], cwd: Option<&Path>| {
            let is_cargo = argv
                .first()
                .is_some_and(|a| Path::new(a).file_stem().is_some_and(|s| s == "cargo"));
            if is_cargo {
                real_build(argv, cwd)
            } else {
                real_run(argv, cwd)
            }
        };
        let made = build_bundles(&args, &mut run, &BundleDirs::for_repo(&repo_root()), &env)?;
        println!(
            "\n{} bundle(s) + SHA256SUMS-app in {}",
            made.len(),
            args.out_dir.display()
        );
        Ok(())
    });
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

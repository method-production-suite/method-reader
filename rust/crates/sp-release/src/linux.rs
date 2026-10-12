//! The Linux packages: a .deb written by hand (an `ar` of three members),
//! an .rpm through the pure-Rust `rpm` crate, and the AppDir appimagetool
//! turns into an AppImage.
//!
//! All three install the same files the Tauri .deb and .rpm installed,
//! under the same package name, so a package manager upgrades the Tauri
//! window in place and drops its engine sidecar (`/usr/bin/screepub-engine`
//! is simply not in the new file list):
//!
//! ```text
//! /usr/bin/screepub-desktop
//! /usr/share/applications/Screepub.desktop      Categories=Office (row W9)
//! /usr/share/icons/hicolor/<size>/apps/screepub-desktop.png
//! /usr/lib/Screepub/LICENSE
//! /usr/lib/Screepub/THIRD-PARTY-NOTICES.md
//! ```

use std::path::{Path, PathBuf};

use crate::archive::{Entry, gzip, tar};
use crate::names::{LONG_DESCRIPTION, PRODUCT, PUBLISHER, REPO, SHORT_DESCRIPTION, WINDOW_EXE};

/// The package name the Tauri bundler derived from productName.
pub const PACKAGE: &str = "screepub";

/// What goes into a Linux package.
pub struct LinuxApp<'a> {
    pub version: &'a str,
    /// The built window binary (`screepub-window`), installed as
    /// `/usr/bin/screepub-desktop`.
    pub window: &'a Path,
    /// (hicolor folder name, PNG): `32x32`, `128x128`, `256x256@2`.
    pub icons: &'a [(String, PathBuf)],
    pub resources: &'a [PathBuf],
    /// The Debian `Depends:` line, measured on the built binary in CI
    /// (`dpkg-shlibdeps`), never guessed here.
    pub depends: &'a str,
}

/// A version every package format accepts: a prerelease's `-` becomes
/// `~`, which sorts before the release in both dpkg and rpm (rpm refuses a
/// `-` in a version outright).
pub fn package_version(version: &str) -> String {
    version.replacen('-', "~", 1)
}

/// The launcher entry, as tauri-bundler wrote it, with Categories=Office.
pub fn desktop_entry() -> String {
    format!(
        "[Desktop Entry]\nCategories=Office;\nComment={SHORT_DESCRIPTION}\nExec={WINDOW_EXE}\nStartupWMClass={WINDOW_EXE}\nIcon={WINDOW_EXE}\nName={PRODUCT}\nTerminal=false\nType=Application\n"
    )
}

/// (absolute install path, bytes, executable) for every file.
pub fn payload(app: &LinuxApp) -> std::io::Result<Vec<(String, Vec<u8>, bool)>> {
    let mut files = vec![
        (
            format!("/usr/bin/{WINDOW_EXE}"),
            std::fs::read(app.window)?,
            true,
        ),
        (
            format!("/usr/share/applications/{PRODUCT}.desktop"),
            desktop_entry().into_bytes(),
            false,
        ),
    ];
    for (size, png) in app.icons {
        files.push((
            format!("/usr/share/icons/hicolor/{size}/apps/{WINDOW_EXE}.png"),
            std::fs::read(png)?,
            false,
        ));
    }
    for r in app.resources {
        let name = r.file_name().unwrap_or_default().to_string_lossy();
        files.push((
            format!("/usr/lib/{PRODUCT}/{name}"),
            std::fs::read(r)?,
            false,
        ));
    }
    Ok(files)
}

/// `./usr/`, `./usr/bin/`, ... for every folder above every file.
fn tree(files: &[(String, Vec<u8>, bool)]) -> Vec<Entry> {
    let mut dirs: std::collections::BTreeSet<String> = Default::default();
    for (path, _, _) in files {
        let mut acc = String::from(".");
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        for part in &parts[..parts.len() - 1] {
            acc.push('/');
            acc.push_str(part);
            dirs.insert(acc.clone());
        }
    }
    let mut entries = vec![Entry::dir(".")];
    entries.extend(dirs.iter().map(|d| Entry::dir(d)));
    let mut sorted: Vec<&(String, Vec<u8>, bool)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, data, exe) in sorted {
        entries.push(Entry::file(&format!(".{path}"), data.clone(), *exe));
    }
    entries
}

/// The control file.
pub fn control(version: &str, arch: &str, installed_kib: u64, depends: &str) -> String {
    let mut c = format!(
        "Package: {PACKAGE}\nVersion: {}\nArchitecture: {arch}\nInstalled-Size: {installed_kib}\nMaintainer: {PUBLISHER}\nPriority: optional\nSection: office\nHomepage: https://github.com/{REPO}\n",
        package_version(version)
    );
    if !depends.trim().is_empty() {
        c += &format!("Depends: {}\n", depends.trim());
    }
    c += &format!("Description: {SHORT_DESCRIPTION}\n {LONG_DESCRIPTION}\n");
    c
}

/// One `ar` member header and body, padded to an even length.
fn ar_member(out: &mut Vec<u8>, name: &str, data: &[u8], mtime: u64) {
    let header = format!(
        "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
        name,
        mtime,
        0,
        0,
        "100644",
        data.len()
    );
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(b'\n');
    }
}

/// The .deb: `debian-binary`, `control.tar.gz`, `data.tar.gz`.
pub fn build_deb(app: &LinuxApp, arch: &str, mtime: u64) -> std::io::Result<Vec<u8>> {
    let files = payload(app)?;
    let installed: u64 = files.iter().map(|(_, d, _)| d.len() as u64).sum();
    let control_text = control(app.version, arch, installed.div_ceil(1024), app.depends);
    let control_tar = gzip(&tar(
        &[
            Entry::dir("."),
            Entry::file("./control", control_text.into_bytes(), false),
        ],
        mtime,
    )?)?;
    let data_tar = gzip(&tar(&tree(&files), mtime)?)?;
    let mut out = b"!<arch>\n".to_vec();
    ar_member(&mut out, "debian-binary", b"2.0\n", mtime);
    ar_member(&mut out, "control.tar.gz", &control_tar, mtime);
    ar_member(&mut out, "data.tar.gz", &data_tar, mtime);
    Ok(out)
}

/// The .rpm, gzip payload, unsigned like the Tauri one.
pub fn build_rpm(app: &LinuxApp, arch: &str, mtime: u32) -> Result<Vec<u8>, String> {
    let config = rpm::BuildConfig::default()
        .compression(rpm::CompressionType::Gzip)
        .source_date(mtime);
    let version = package_version(app.version);
    let mut b = rpm::PackageBuilder::new(
        PACKAGE,
        &version,
        "AGPL-3.0-or-later",
        arch,
        SHORT_DESCRIPTION,
    );
    b.using_config(config)
        .release("1")
        .description(LONG_DESCRIPTION)
        .url(format!("https://github.com/{REPO}"))
        .vendor(PUBLISHER);
    for (path, data, exe) in payload(app).map_err(|e| e.to_string())? {
        let mode = if exe { 0o755 } else { 0o644 };
        b.with_file_contents(data, rpm::FileOptions::new(path).permissions(mode))
            .map_err(|e| e.to_string())?;
    }
    let pkg = b.build().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    pkg.write(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// The AppDir appimagetool packs: the same files under `usr/`, plus the
/// three things an AppImage needs at its root (AppRun, the .desktop file,
/// the icon). The AppImage has no installer, so it carries no Depends:
/// what the binary loads must be on every desktop it claims (glibc, the
/// GPU driver), which CI checks by running it.
pub fn build_appdir(app: &LinuxApp, out: &Path) -> std::io::Result<PathBuf> {
    let dir = out.join(format!("{PRODUCT}.AppDir"));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    for (path, data, exe) in payload(app)? {
        let to = dir.join(path.trim_start_matches('/'));
        std::fs::create_dir_all(to.parent().unwrap_or(&dir))?;
        std::fs::write(&to, data)?;
        set_mode(&to, exe)?;
    }
    std::fs::write(dir.join(format!("{WINDOW_EXE}.desktop")), desktop_entry())?;
    if let Some((_, png)) = app.icons.iter().max_by_key(|(s, _)| s.clone()) {
        std::fs::copy(png, dir.join(format!("{WINDOW_EXE}.png")))?;
    }
    let apprun = dir.join("AppRun");
    std::fs::write(
        &apprun,
        format!(
            "#!/bin/sh\nHERE=\"$(dirname \"$(readlink -f \"$0\")\")\"\nexec \"$HERE/usr/bin/{WINDOW_EXE}\" \"$@\"\n"
        ),
    )?;
    set_mode(&apprun, true)?;
    Ok(dir)
}

#[cfg(unix)]
fn set_mode(p: &Path, exe: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        p,
        std::fs::Permissions::from_mode(if exe { 0o755 } else { 0o644 }),
    )
}

#[cfg(not(unix))]
fn set_mode(_p: &Path, _exe: bool) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(dir: &Path) -> (PathBuf, Vec<(String, PathBuf)>, Vec<PathBuf>) {
        let bin = dir.join("screepub-window");
        std::fs::write(&bin, b"\x7fELF fake").unwrap();
        let png = dir.join("32x32.png");
        std::fs::write(&png, b"png").unwrap();
        let lic = dir.join("LICENSE");
        std::fs::write(&lic, b"AGPL").unwrap();
        (bin, vec![("32x32".into(), png)], vec![lic])
    }

    #[test]
    fn the_deb_is_an_ar_of_three_members_with_the_tauri_paths() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, icons, res) = app(dir.path());
        let a = LinuxApp {
            version: "0.12.0",
            window: &bin,
            icons: &icons,
            resources: &res,
            depends: "libc6 (>= 2.31)",
        };
        let deb = build_deb(&a, "amd64", 7).unwrap();
        assert!(deb.starts_with(b"!<arch>\ndebian-binary   "));
        assert_eq!(deb, build_deb(&a, "amd64", 7).unwrap(), "reproducible");
        // Find data.tar.gz and list it.
        let at = deb.windows(11).position(|w| w == b"data.tar.gz").unwrap();
        let size: usize = std::str::from_utf8(&deb[at + 48..at + 58])
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let body = &deb[at + 60..at + 60 + size];
        let mut t = tar::Archive::new(flate2::read::GzDecoder::new(body));
        let names: Vec<String> = t
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
            .filter(|n| !n.ends_with('/'))
            .collect();
        assert_eq!(
            names,
            [
                "./usr/bin/screepub-desktop",
                "./usr/lib/Screepub/LICENSE",
                "./usr/share/applications/Screepub.desktop",
                "./usr/share/icons/hicolor/32x32/apps/screepub-desktop.png",
            ]
        );
    }

    #[test]
    fn the_control_file_names_the_tauri_package() {
        let c = control("0.12.0", "amd64", 9, "libc6");
        assert!(c.starts_with("Package: screepub\nVersion: 0.12.0\nArchitecture: amd64\n"));
        assert!(c.contains("Depends: libc6\n"));
        assert!(c.ends_with(&format!(" {LONG_DESCRIPTION}\n")));
    }

    #[test]
    fn a_prerelease_sorts_before_its_release_in_both_formats() {
        assert_eq!(package_version("0.12.0-rc1"), "0.12.0~rc1");
        assert_eq!(package_version("0.12.0"), "0.12.0");
        assert!(control("0.12.0-rc1", "amd64", 1, "").contains("\nVersion: 0.12.0~rc1\n"));
    }

    #[test]
    fn the_launcher_says_office() {
        assert!(desktop_entry().contains("\nCategories=Office;\n"));
        assert!(desktop_entry().contains("\nExec=screepub-desktop\n"));
    }

    #[test]
    fn the_rpm_builds_and_has_the_rpm_magic() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, icons, res) = app(dir.path());
        let a = LinuxApp {
            version: "0.12.0",
            window: &bin,
            icons: &icons,
            resources: &res,
            depends: "",
        };
        let rpm = build_rpm(&a, "x86_64", 7).unwrap();
        assert_eq!(&rpm[..4], &[0xed, 0xab, 0xee, 0xdb]);
    }

    #[cfg(unix)]
    #[test]
    fn the_appdir_has_its_three_root_files() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, icons, res) = app(dir.path());
        let a = LinuxApp {
            version: "0.12.0",
            window: &bin,
            icons: &icons,
            resources: &res,
            depends: "",
        };
        let d = build_appdir(&a, dir.path()).unwrap();
        for f in [
            "AppRun",
            "screepub-desktop.desktop",
            "screepub-desktop.png",
            "usr/bin/screepub-desktop",
        ] {
            assert!(d.join(f).exists(), "{f}");
        }
    }
}

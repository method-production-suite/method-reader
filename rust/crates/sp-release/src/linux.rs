//! The Linux packages: a .deb written by hand (an `ar` of three members),
//! an .rpm through the pure-Rust `rpm` crate, and the AppDir appimagetool
//! turns into an AppImage.
//!
//! The packages are called `method-reader`. Up to 0.12.x they were
//! `screepub` (the Tauri bundler's name, kept by the Rust 0.12 packages),
//! so both formats say the new one takes the old one's place: the .deb
//! `Replaces:`, `Conflicts:` and `Provides:` screepub, the .rpm
//! `Obsoletes:` and `Provides:` it, each bounded below this version.
//! Installing the new package then removes the old one (its window, its
//! launcher, its notices, and the Tauri engine sidecar if it was that
//! old) instead of leaving two apps side by side:
//!
//! ```text
//! /usr/bin/method-reader-app
//! /usr/share/applications/method-reader.desktop      Categories=Office (row W9)
//! /usr/share/icons/hicolor/<size>/apps/method-reader-app.png
//! /usr/lib/method-reader/LICENSE-MIT ...
//! /usr/lib/method-reader/THIRD-PARTY-NOTICES.md
//! ```

use std::path::{Path, PathBuf};

use crate::archive::{Entry, gzip, tar};
pub use crate::names::{LEGACY_PACKAGE, PACKAGE};
use crate::names::{LONG_DESCRIPTION, PRODUCT, PUBLISHER, REPO, SHORT_DESCRIPTION, WINDOW_EXE};

/// What goes into a Linux package.
pub struct LinuxApp<'a> {
    pub version: &'a str,
    /// The built window binary (cargo's `method-reader-app`), installed as
    /// `/usr/bin/method-reader-app`.
    pub window: &'a Path,
    /// (hicolor folder name, PNG): `32x32`, `128x128`, `256x256@2`.
    pub icons: &'a [(String, PathBuf)],
    pub resources: &'a [PathBuf],
    /// The Debian `Depends:` line, measured on the built binary in CI
    /// (`dpkg-shlibdeps`), never guessed here. The .deb adds the libraries
    /// the window opens at run time ([`LOADED_AT_RUN_TIME`]); the .rpm
    /// reads the binary's own DT_NEEDED list instead ([`rpm_requires`]).
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
            format!("/usr/share/applications/{PACKAGE}.desktop"),
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
            format!("/usr/lib/{PACKAGE}/{name}"),
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

/// The libraries the window OPENS at run time (dlopen) rather than links,
/// as (soname, Debian package). `dpkg-shlibdeps` and rpm's own dependency
/// finder only read the binary's DT_NEEDED list, which holds ALSA,
/// fontconfig and the C runtime; these never appear there, so they are
/// declared here, from the crates that open them:
///
/// - xkbcommon-dl: libxkbcommon (Wayland and X11) and libxkbcommon-x11;
/// - wayland-sys (built with `dlopen`): libwayland-client, and
///   libwayland-egl for wgpu's GL path on Wayland;
/// - winit's X11 backend through x11-dl: Xlib, Xlib-xcb, Xcursor, XInput2;
/// - wgpu's GL path through khronos-egl: libEGL (Vulkan, tried first, is
///   optional: without it wgpu falls back to EGL).
///
/// GTK is not among them: the file dialog (rfd) asks the XDG desktop
/// portal over D-Bus, and the only GTK user in the lock file (muda) is
/// built for macOS alone.
pub const LOADED_AT_RUN_TIME: &[(&str, &str)] = &[
    ("libxkbcommon.so.0", "libxkbcommon0"),
    ("libxkbcommon-x11.so.0", "libxkbcommon-x11-0"),
    ("libwayland-client.so.0", "libwayland-client0"),
    ("libwayland-egl.so.1", "libwayland-egl1"),
    ("libX11.so.6", "libx11-6"),
    ("libX11-xcb.so.1", "libx11-xcb1"),
    ("libXcursor.so.1", "libxcursor1"),
    ("libXi.so.6", "libxi6"),
    ("libEGL.so.1", "libegl1"),
];

/// The .deb's `Depends:`: the line measured on the binary (dpkg-shlibdeps,
/// in CI), then each run-time library's package it does not already name.
///
/// A measured package with the 64-bit time name (`libasound2t64`, from
/// Debian's t64 transition, which Ubuntu 24.04 and Debian 13 made) also
/// accepts its old name (`libasound2`), the only one Debian 12 and
/// Ubuntu 22.04 know, with the same version bound.
pub fn deb_depends(measured: &str) -> String {
    let mut parts: Vec<String> = measured
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(with_pre_t64_names)
        .collect();
    let named = |parts: &[String], pkg: &str| {
        parts.iter().any(|p| {
            p.split('|')
                .any(|alt| alt.split_whitespace().next() == Some(pkg))
        })
    };
    for (_, pkg) in LOADED_AT_RUN_TIME {
        if !named(&parts, pkg) {
            parts.push((*pkg).to_string());
        }
    }
    parts.join(", ")
}

/// One `Depends:` entry (`a (>= 1) | b`), each `...t64` alternative
/// followed by its old name, unless the entry already offers that name.
fn with_pre_t64_names(entry: &str) -> String {
    let alts: Vec<&str> = entry.split('|').map(str::trim).collect();
    let names: Vec<&str> = alts
        .iter()
        .filter_map(|a| a.split_whitespace().next())
        .collect();
    let mut out: Vec<String> = Vec::new();
    for alt in &alts {
        out.push((*alt).to_string());
        let name = alt.split_whitespace().next().unwrap_or("");
        if let Some(old) = name.strip_suffix("t64")
            && !old.is_empty()
            && !names.contains(&old)
        {
            out.push(format!("{old}{}", &alt[name.len()..]));
        }
    }
    out.join(" | ")
}

/// The DT_NEEDED sonames of a 64-bit little-endian ELF file, in order:
/// what the dynamic linker loads before `main`. Anything else (another
/// format, a 32-bit or damaged file) has none.
pub fn elf_needed(elf: &[u8]) -> Vec<String> {
    let u16_at = |o: usize| -> Option<u64> {
        Some(u64::from(u16::from_le_bytes(
            elf.get(o..o + 2)?.try_into().ok()?,
        )))
    };
    let u32_at = |o: usize| -> Option<u64> {
        Some(u64::from(u32::from_le_bytes(
            elf.get(o..o + 4)?.try_into().ok()?,
        )))
    };
    let u64_at =
        |o: usize| -> Option<u64> { Some(u64::from_le_bytes(elf.get(o..o + 8)?.try_into().ok()?)) };
    let needed = || -> Option<Vec<String>> {
        // ELFCLASS64, little-endian.
        if elf.get(..6)? != b"\x7fELF\x02\x01" {
            return None;
        }
        let shoff = usize::try_from(u64_at(0x28)?).ok()?;
        let shentsize = usize::try_from(u16_at(0x3a)?).ok()?;
        let shnum = usize::try_from(u16_at(0x3c)?).ok()?;
        let section = |i: usize| -> Option<(u64, usize, usize, usize)> {
            let h = shoff.checked_add(i.checked_mul(shentsize)?)?;
            Some((
                u32_at(h + 4)?,                           // sh_type
                usize::try_from(u64_at(h + 0x18)?).ok()?, // sh_offset
                usize::try_from(u64_at(h + 0x20)?).ok()?, // sh_size
                usize::try_from(u32_at(h + 0x28)?).ok()?, // sh_link
            ))
        };
        const SHT_DYNAMIC: u64 = 6;
        const DT_NEEDED: u64 = 1;
        let (_, dyn_off, dyn_size, strtab) = (0..shnum)
            .filter_map(section)
            .find(|s| s.0 == SHT_DYNAMIC)?;
        let (_, str_off, str_size, _) = section(strtab)?;
        let strings = elf.get(str_off..str_off.checked_add(str_size)?)?;
        let mut out = Vec::new();
        for entry in (dyn_off..dyn_off.checked_add(dyn_size)?).step_by(16) {
            let (tag, val) = (u64_at(entry)?, u64_at(entry + 8)?);
            if tag == 0 {
                break;
            }
            if tag == DT_NEEDED {
                let from = usize::try_from(val).ok()?;
                let rest = strings.get(from..)?;
                let end = rest.iter().position(|b| *b == 0)?;
                out.push(String::from_utf8_lossy(&rest[..end]).into_owned());
            }
        }
        Some(out)
    };
    needed().unwrap_or_default()
}

/// The .rpm's `Requires:`, as rpm's own dependency finder would write them
/// for a 64-bit binary (`libfoo.so.1()(64bit)`, which every rpm system's
/// library package provides): the binary's DT_NEEDED list, measured on the
/// binary itself, then the run-time libraries.
pub fn rpm_requires(window: &[u8], arch: &str) -> Vec<String> {
    let suffix = if matches!(arch, "x86_64" | "aarch64") {
        "()(64bit)"
    } else {
        ""
    };
    let mut out: Vec<String> = Vec::new();
    let sonames = elf_needed(window);
    for soname in sonames
        .iter()
        .map(String::as_str)
        .chain(LOADED_AT_RUN_TIME.iter().map(|(s, _)| *s))
    {
        let req = format!("{soname}{suffix}");
        if !out.contains(&req) {
            out.push(req);
        }
    }
    out
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
    // The old package name: this one takes its place and its files.
    let v = package_version(version);
    c += &format!(
        "Replaces: {LEGACY_PACKAGE} (<< {v})\nConflicts: {LEGACY_PACKAGE} (<< {v})\nProvides: {LEGACY_PACKAGE} (= {v})\n"
    );
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
    let control_text = control(
        app.version,
        arch,
        installed.div_ceil(1024),
        &deb_depends(app.depends),
    );
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

/// The .rpm, gzip payload, unsigned like the Tauri one, with its
/// `Requires:` ([`rpm_requires`]).
pub fn build_rpm(app: &LinuxApp, arch: &str, mtime: u32) -> Result<Vec<u8>, String> {
    let config = rpm::BuildConfig::default()
        .compression(rpm::CompressionType::Gzip)
        .source_date(mtime);
    let version = package_version(app.version);
    let mut b = rpm::PackageBuilder::new(
        PACKAGE,
        &version,
        "(MIT OR Apache-2.0) AND GPL-3.0-only",
        arch,
        SHORT_DESCRIPTION,
    );
    b.using_config(config)
        .release("1")
        .description(LONG_DESCRIPTION)
        .url(format!("https://github.com/{REPO}"))
        .vendor(PUBLISHER);
    let window = std::fs::read(app.window).map_err(|e| e.to_string())?;
    for req in rpm_requires(&window, arch) {
        b.requires(rpm::Dependency::any(req));
    }
    // The old package name: an upgrade swaps it for this one.
    b.obsoletes(rpm::Dependency::less(LEGACY_PACKAGE, &version));
    b.provides(rpm::Dependency::eq(LEGACY_PACKAGE, &version));
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
    let dir = out.join(format!("{PACKAGE}.AppDir"));
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
        let bin = dir.join("method-reader-app");
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
                "./usr/bin/method-reader-app",
                "./usr/lib/method-reader/LICENSE",
                "./usr/share/applications/method-reader.desktop",
                "./usr/share/icons/hicolor/32x32/apps/method-reader-app.png",
            ]
        );
    }

    #[test]
    fn the_control_file_names_the_package_and_replaces_the_old_one() {
        let c = control("0.13.0", "amd64", 9, "libc6");
        assert!(c.starts_with("Package: method-reader\nVersion: 0.13.0\nArchitecture: amd64\n"));
        assert!(c.contains("Depends: libc6\n"));
        assert!(c.contains("\nReplaces: screepub (<< 0.13.0)\n"), "{c}");
        assert!(c.contains("\nConflicts: screepub (<< 0.13.0)\n"), "{c}");
        assert!(c.contains("\nProvides: screepub (= 0.13.0)\n"), "{c}");
        assert!(c.contains(
            "Maintainer: Method Production Suite Team and the Method Reader contributors\n"
        ));
        assert!(c.contains("Homepage: https://github.com/method-production-suite/method-reader\n"));
        assert!(c.ends_with(&format!(" {LONG_DESCRIPTION}\n")));
    }

    #[test]
    fn a_prerelease_sorts_before_its_release_in_both_formats() {
        assert_eq!(package_version("0.12.0-rc1"), "0.12.0~rc1");
        assert_eq!(package_version("0.12.0"), "0.12.0");
        assert!(control("0.12.0-rc1", "amd64", 1, "").contains("\nVersion: 0.12.0~rc1\n"));
    }

    #[test]
    fn the_deb_depends_on_what_was_measured_and_every_run_time_library_once() {
        let measured = "libasound2 (>= 1.0.16), libc6 (>= 2.34), libfontconfig1 (>= 2.12.6), libxkbcommon0 (>= 0.5.0)";
        let d = deb_depends(measured);
        assert!(d.starts_with(measured), "{d}");
        for (_, pkg) in LOADED_AT_RUN_TIME {
            let n = d
                .split(',')
                .filter(|p| p.split_whitespace().next() == Some(*pkg))
                .count();
            assert_eq!(n, 1, "{pkg}: {d}");
        }
        assert!(
            d.contains("libxkbcommon0 (>= 0.5.0)"),
            "the measured version stays"
        );
        // Nothing measured (a laptop build): the run-time libraries alone.
        assert_eq!(
            deb_depends(""),
            LOADED_AT_RUN_TIME
                .iter()
                .map(|(_, p)| *p)
                .collect::<Vec<_>>()
                .join(", ")
        );
        let c = control("0.12.0", "amd64", 9, &deb_depends(measured));
        assert!(c.contains("\nDepends: libasound2 (>= 1.0.16), "), "{c}");
        assert!(c.contains(", libegl1\n"), "{c}");
    }

    #[test]
    fn a_t64_package_measured_on_a_new_system_also_accepts_its_old_name() {
        // dpkg-shlibdeps on Ubuntu 24.04 names ALSA by its 64-bit time
        // name; Debian 12 and Ubuntu 22.04 only know libasound2.
        let measured =
            "libasound2t64 (>= 1.1.0), libc6 (>= 2.39), libglib2.0-0t64 (>= 2.75.3) | libfoo1";
        let d = deb_depends(measured);
        assert!(
            d.starts_with(
                "libasound2t64 (>= 1.1.0) | libasound2 (>= 1.1.0), libc6 (>= 2.39), \
                 libglib2.0-0t64 (>= 2.75.3) | libglib2.0-0 (>= 2.75.3) | libfoo1, "
            ),
            "{d}"
        );
        // Already offering the old name: nothing is added twice.
        let both = "libasound2t64 (>= 1.1.0) | libasound2 (>= 1.1.0)";
        assert!(deb_depends(both).starts_with(&format!("{both}, ")));
        // An old system's line is left as it was.
        assert!(deb_depends("libasound2 (>= 1.0.16)").starts_with("libasound2 (>= 1.0.16), "));
    }

    /// A minimal 64-bit little-endian ELF with a dynamic section naming
    /// `needed`.
    fn elf(needed: &[&str]) -> Vec<u8> {
        let mut strtab = vec![0u8];
        let mut offsets = Vec::new();
        for n in needed {
            offsets.push(strtab.len() as u64);
            strtab.extend_from_slice(n.as_bytes());
            strtab.push(0);
        }
        let str_off = 64usize;
        let dyn_off = (str_off + strtab.len()).next_multiple_of(8);
        let mut dynamic = Vec::new();
        dynamic.extend_from_slice(&5u64.to_le_bytes()); // DT_STRTAB, not NEEDED
        dynamic.extend_from_slice(&0u64.to_le_bytes());
        for o in &offsets {
            dynamic.extend_from_slice(&1u64.to_le_bytes());
            dynamic.extend_from_slice(&o.to_le_bytes());
        }
        dynamic.extend_from_slice(&[0u8; 16]);
        let sh_off = dyn_off + dynamic.len();
        let mut out = vec![0u8; sh_off + 3 * 64];
        out[..6].copy_from_slice(b"\x7fELF\x02\x01");
        out[0x28..0x30].copy_from_slice(&(sh_off as u64).to_le_bytes());
        out[0x3a..0x3c].copy_from_slice(&64u16.to_le_bytes());
        out[0x3c..0x3e].copy_from_slice(&3u16.to_le_bytes());
        out[str_off..str_off + strtab.len()].copy_from_slice(&strtab);
        out[dyn_off..dyn_off + dynamic.len()].copy_from_slice(&dynamic);
        let mut header = |i: usize, kind: u32, off: usize, size: usize, link: u32| {
            let h = sh_off + i * 64;
            out[h + 4..h + 8].copy_from_slice(&kind.to_le_bytes());
            out[h + 0x18..h + 0x20].copy_from_slice(&(off as u64).to_le_bytes());
            out[h + 0x20..h + 0x28].copy_from_slice(&(size as u64).to_le_bytes());
            out[h + 0x28..h + 0x2c].copy_from_slice(&link.to_le_bytes());
        };
        header(1, 3, str_off, strtab.len(), 0);
        header(2, 6, dyn_off, dynamic.len(), 1);
        out
    }

    #[test]
    fn the_needed_list_is_read_out_of_the_elf() {
        let bin = elf(&["libasound.so.2", "libfontconfig.so.1", "libc.so.6"]);
        assert_eq!(
            elf_needed(&bin),
            ["libasound.so.2", "libfontconfig.so.1", "libc.so.6"]
        );
        assert!(elf_needed(b"\x7fELF fake").is_empty());
        assert!(
            elf_needed(&bin[..100]).is_empty(),
            "a cut-short file has none"
        );
        assert!(elf_needed(b"").is_empty());
    }

    #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
    #[test]
    fn this_test_binary_needs_the_c_library() {
        let me = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        assert!(elf_needed(&me).iter().any(|n| n == "libc.so.6"));
    }

    #[test]
    fn the_rpm_requires_what_the_binary_links_and_what_it_opens() {
        let bin = elf(&["libasound.so.2", "libc.so.6", "libasound.so.2"]);
        let r = rpm_requires(&bin, "x86_64");
        assert_eq!(r[..2], ["libasound.so.2()(64bit)", "libc.so.6()(64bit)"]);
        assert_eq!(r.len(), 2 + LOADED_AT_RUN_TIME.len(), "once each: {r:?}");
        assert!(r.contains(&"libxkbcommon.so.0()(64bit)".to_string()));
        assert!(!r.iter().any(|x| x.contains("gtk")));

        let dir = tempfile::tempdir().unwrap();
        let (_, icons, res) = app(dir.path());
        let window = dir.path().join("method-reader-app");
        std::fs::write(&window, &bin).unwrap();
        let a = LinuxApp {
            version: "0.12.0",
            window: &window,
            icons: &icons,
            resources: &res,
            depends: "",
        };
        let bytes = build_rpm(&a, "x86_64", 7).unwrap();
        let pkg = rpm::Package::parse(&mut std::io::Cursor::new(bytes)).unwrap();
        let names: Vec<String> = pkg
            .metadata
            .get_requires()
            .unwrap()
            .into_iter()
            .map(|d| d.name)
            .collect();
        for want in &r {
            assert!(names.contains(want), "{want}: {names:?}");
        }
    }

    #[test]
    fn the_launcher_says_office() {
        assert!(desktop_entry().contains("\nCategories=Office;\n"));
        assert!(desktop_entry().contains("\nExec=method-reader-app\n"));
        assert!(desktop_entry().contains("\nName=Method Reader\n"));
        // Row W9: no file association, and no path handed in by the launcher.
        assert!(!desktop_entry().contains("MimeType"));
        assert!(!desktop_entry().contains('%'));
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
        let pkg = rpm::Package::parse(&mut std::io::Cursor::new(rpm)).unwrap();
        assert_eq!(pkg.metadata.get_name().unwrap(), "method-reader");
        let obsoletes = pkg.metadata.get_obsoletes().unwrap();
        assert!(
            obsoletes
                .iter()
                .any(|d| d.name == "screepub" && d.version == "0.12.0"),
            "{obsoletes:?}"
        );
        let provides = pkg.metadata.get_provides().unwrap();
        assert!(
            provides
                .iter()
                .any(|d| d.name == "screepub" && d.version == "0.12.0"),
            "{provides:?}"
        );
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
            "method-reader-app.desktop",
            "method-reader-app.png",
            "usr/bin/method-reader-app",
        ] {
            assert!(d.join(f).exists(), "{f}");
        }
    }
}

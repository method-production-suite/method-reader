//! Every name a release publishes, in one place: the download pages, the
//! README, the Homebrew cask and formula and the apps' updater
//! (latest.json's keys) all depend on these. Method Reader was called
//! Screepub up to 0.12.x; the `LEGACY_*` names below are that release's,
//! kept only where something must recognise or replace an old install.

/// The product, the bundle folder and the window's installed executable.
/// On a Mac nothing depends on the executable's name: the restart reads
/// CFBundleExecutable from the NEW Info.plist.
pub const PRODUCT: &str = "Method Reader";
pub const MAC_BUNDLE: &str = "Method Reader.app";
pub const WINDOW_EXE: &str = "method-reader-app";
/// The command-line tool's name inside every CLI archive and on PATH.
pub const CLI_EXE: &str = "method-reader";
/// The CLI's old name, shipped beside [`CLI_EXE`] in every CLI archive for
/// a few releases: it says once on stderr that the tool is now called
/// method-reader, then behaves exactly like it.
pub const CLI_ALIAS_EXE: &str = "screepub";
/// The Linux package, and the folder its notices install under.
pub const PACKAGE: &str = "method-reader";
pub const REPO: &str = "method-production-suite/method-reader";
/// Who publishes it: installers, bundle metadata, package maintainer.
pub const PUBLISHER: &str = "Method Production Suite Team and the Method Reader contributors";
/// The copyright line, wherever a copyright is stated (LICENSE-MIT says
/// the same).
pub const COPYRIGHT: &str =
    "Copyright (c) 2026 Method Production Suite Team and the Method Reader contributors";
pub const SHORT_DESCRIPTION: &str = "Turn a screenplay PDF into a reflowable e-book.";
pub const LONG_DESCRIPTION: &str = "Method Reader converts a screenplay PDF into an EPUB or MOBI that reflows properly on an e-reader, keeping cues with their dialogue and scenes with their headings, and copies the finished book onto a connected reader over USB.";
pub const MAC_MINIMUM: &str = "13.0";

/// The 0.12.x names, for the places that must recognise an old install:
/// the Mac bundle the 0.12 updater swaps the new build into, the Windows
/// install (folder and uninstall key, both named after the product) the
/// new installer removes, and the Linux package the new one replaces.
pub const LEGACY_PRODUCT: &str = "Screepub";
pub const LEGACY_MAC_BUNDLE: &str = "Screepub.app";
pub const LEGACY_WINDOW_EXE: &str = "screepub-desktop";
pub const LEGACY_PACKAGE: &str = "screepub";
pub const LEGACY_REPO: &str = "ssandweiss/screepub";

/// The installers. No version in a name, on purpose: pages link to
/// releases/latest/download/<name>.
pub const MAC_DMG: &str = "Method-Reader-macOS.dmg";
pub const LINUX_DEB_AMD64: &str = "Method-Reader-linux-amd64.deb";
pub const LINUX_RPM_X86_64: &str = "Method-Reader-linux-x86_64.rpm";
pub const LINUX_APPIMAGE_X86_64: &str = "Method-Reader-linux-x86_64.AppImage";
pub const WINDOWS_SETUP: &str = "Method-Reader-windows-x64-setup.exe";
/// What an installer for another architecture is called: `Method-Reader-linux-<arch>.<ext>`.
pub const INSTALLER_STEM: &str = "Method-Reader";

/// The command-line archives (the Homebrew formula serves the first).
pub const CLI_MAC_ARM64: &str = "method-reader-cli-macos-arm64.tar.gz";
pub const CLI_LINUX_X64: &str = "method-reader-cli-linux-x64.tar.gz";
pub const CLI_LINUX_ARM64: &str = "method-reader-cli-linux-arm64.tar.gz";
pub const CLI_WINDOWS_X64: &str = "method-reader-cli-windows-x64.zip";

/// The update archive and its signature; latest.json points at these.
pub const MAC_UPDATE: &str = "Method-Reader-macOS.app.tar.gz";
pub const SIGNATURE_EXT: &str = ".sig";
pub const MANIFEST: &str = "latest.json";

/// One signed update artifact and the `platforms` keys it answers.
#[derive(Clone, Copy, Debug)]
pub struct UpdaterKind {
    pub name: &'static str,
    pub platform_keys: &'static [&'static str],
}

/// Apple silicon only, as since 0.10.0: an Intel Mac finds no key and is
/// told why (sp-app, `is_intel_mac_retired`). Linux and Windows are not
/// offered updates (inventory row U1); adding one is a row here and a
/// signing step on that leg.
pub const UPDATER_KINDS: &[UpdaterKind] = &[UpdaterKind {
    name: MAC_UPDATE,
    platform_keys: &["darwin-aarch64"],
}];

/// A release asset's stable address.
pub fn asset_url(repo: &str, version: &str, name: &str) -> String {
    format!("https://github.com/{repo}/releases/download/v{version}/{name}")
}

pub fn release_page_url(repo: &str, version: &str) -> String {
    format!("https://github.com/{repo}/releases/tag/v{version}")
}

/// MAJOR.MINOR.PATCH with an optional prerelease suffix, as release.yml's
/// gate and tools/build-cli.ts's VERSION_RE accept.
pub fn is_release_version(v: &str) -> bool {
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && pre.is_none_or(|p| {
            !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_as_the_release_gate_reads_them() {
        for ok in ["0.12.0", "1.0.0-rc1", "0.12.0-beta.2"] {
            assert!(is_release_version(ok), "{ok}");
        }
        for bad in [
            "v0.12.0",
            "0.12",
            "main",
            "0.12.0-",
            "0.12.0+meta",
            "0.12.0-1-gabc",
            "",
        ] {
            assert!(!is_release_version(bad), "{bad}");
        }
    }

    #[test]
    fn urls() {
        assert_eq!(
            asset_url(REPO, "0.12.0", MAC_UPDATE),
            "https://github.com/method-production-suite/method-reader/releases/download/v0.12.0/Method-Reader-macOS.app.tar.gz"
        );
    }
}

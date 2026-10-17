//! The macOS app bundle: `Method Reader.app` laid out the way tauri-bundler
//! 2.9.4 laid out the Tauri window's (macos/app.rs). From 0.13 it carries
//! Method Reader's own identifier, com.methodreader.desktop
//! ([`sp_update::IDENTIFIER`]), where up to 0.12.x it was
//! com.darkwell.screepub ([`sp_update::LEGACY_IDENTIFIER`]): macOS sees a
//! new app, so its permission grants (removable volumes, Automation),
//! which are keyed to the identifier and the team, are asked for again
//! once. The folder and the executable were renamed with the product too
//! (from `Screepub.app` and `screepub-desktop`,
//! [`crate::names::LEGACY_MAC_BUNDLE`]).
//!
//! ```text
//! Method Reader.app/Contents/Info.plist
//! Method Reader.app/Contents/MacOS/method-reader-app     the egui window
//! Method Reader.app/Contents/MacOS/<helpers>             method-reader-mtp, when shipped
//! Method Reader.app/Contents/Resources/icon.icns
//! Method Reader.app/Contents/Resources/LICENSE-MIT ...
//! Method Reader.app/Contents/Resources/THIRD-PARTY-NOTICES.md
//! ```
//!
//! This only lays files out. Signing (codesign, hardened runtime, no
//! entitlements: nothing in the Rust app JITs) and notarization are
//! release.yml's, with Apple's own tools.

use std::path::{Path, PathBuf};

use crate::names::{MAC_BUNDLE, MAC_MINIMUM, PRODUCT, WINDOW_EXE};

pub struct MacApp<'a> {
    pub version: &'a str,
    pub identifier: &'a str,
    /// The built window binary (cargo's `method-reader-app`), copied in as
    /// [`WINDOW_EXE`].
    pub window: &'a Path,
    /// Other executables that ride in Contents/MacOS, under their own names.
    pub helpers: &'a [PathBuf],
    pub icon: &'a Path,
    pub resources: &'a [PathBuf],
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The Info.plist tauri-bundler wrote for the window, key for key (no URL
/// schemes, no document types, no copyright were configured).
pub fn info_plist(version: &str, identifier: &str) -> String {
    let s = |k: &str, v: &str| {
        format!(
            "\t<key>{}</key>\n\t<string>{}</string>\n",
            escape(k),
            escape(v)
        )
    };
    let t = |k: &str| format!("\t<key>{k}</key>\n\t<true/>\n");
    let mut body = String::new();
    body += &s("CFBundleDevelopmentRegion", "English");
    body += &s("CFBundleDisplayName", PRODUCT);
    body += &s("CFBundleExecutable", WINDOW_EXE);
    body += &s("CFBundleIconFile", "icon.icns");
    body += &s("CFBundleIdentifier", identifier);
    body += &s("CFBundleInfoDictionaryVersion", "6.0");
    body += &s("CFBundleName", PRODUCT);
    body += &s("CFBundlePackageType", "APPL");
    body += &s("CFBundleShortVersionString", version);
    body += &s("CFBundleVersion", version);
    body += &t("CSResourcesFileMapped");
    body += &s(
        "LSApplicationCategoryType",
        "public.app-category.productivity",
    );
    body += &s("LSMinimumSystemVersion", MAC_MINIMUM);
    body += &t("LSRequiresCarbon");
    body += &t("NSHighResolutionCapable");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n{body}</dict>\n</plist>\n"
    )
}

#[cfg(unix)]
fn make_executable(p: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_p: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Lay the bundle out under `out_dir`, replacing one already there.
pub fn build_app(app: &MacApp, out_dir: &Path) -> std::io::Result<PathBuf> {
    let bundle = out_dir.join(MAC_BUNDLE);
    if bundle.exists() {
        std::fs::remove_dir_all(&bundle)?;
    }
    let contents = bundle.join("Contents");
    let macos = contents.join("MacOS");
    let res = contents.join("Resources");
    std::fs::create_dir_all(&macos)?;
    std::fs::create_dir_all(&res)?;
    std::fs::write(
        contents.join("Info.plist"),
        info_plist(app.version, app.identifier),
    )?;
    std::fs::copy(app.window, macos.join(WINDOW_EXE))?;
    make_executable(&macos.join(WINDOW_EXE))?;
    for h in app.helpers {
        let name = h
            .file_name()
            .ok_or_else(|| std::io::Error::other("a helper has no file name"))?;
        std::fs::copy(h, macos.join(name))?;
        make_executable(&macos.join(name))?;
    }
    std::fs::copy(app.icon, res.join("icon.icns"))?;
    for r in app.resources {
        let name = r
            .file_name()
            .ok_or_else(|| std::io::Error::other("a resource has no file name"))?;
        std::fs::copy(r, res.join(name))?;
    }
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plist_reads_back_with_the_keys_the_updater_and_finder_use() {
        let text = info_plist("0.13.0", sp_update::IDENTIFIER);
        let dir = tempfile::tempdir().unwrap();
        // Read back the way the swap and the restart read it.
        let app = dir.path().join("X.app/Contents");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("Info.plist"), &text).unwrap();
        assert_eq!(
            sp_update::install::bundle_identifier(&dir.path().join("X.app")).as_deref(),
            Some("com.methodreader.desktop")
        );
        assert!(
            text.contains("<key>CFBundleExecutable</key>\n\t<string>method-reader-app</string>")
        );
        assert!(text.contains("<key>CFBundleName</key>\n\t<string>Method Reader</string>"));
        assert!(text.contains("<key>LSMinimumSystemVersion</key>\n\t<string>13.0</string>"));
        assert!(text.contains("<key>CFBundleShortVersionString</key>\n\t<string>0.13.0</string>"));
    }
}

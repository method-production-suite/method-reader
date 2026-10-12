//! Putting a verified update in place: the macOS bundle swap
//! tauri-plugin-updater 2.12.0 does (updater.rs lines 1286 to 1388), the
//! AppImage replacement, and the Windows installer hand-off.
//!
//! The Mac swap keeps the plugin's shape: unpack the `.app.tar.gz` with its
//! top folder dropped (so the archive's own `.app` name never matters),
//! move the running bundle aside, move the new one into the SAME path,
//! touch it. Three things are stricter than the plugin, none of which a
//! valid archive can notice:
//!
//! - the staging and backup folders sit beside the bundle, not in the
//!   system temp folder, so both renames stay on one volume (the plugin's
//!   rename into /var/folders fails for an app kept on another disk);
//! - an entry that would land outside the bundle (`..`, an absolute path)
//!   is refused, and the unpacked bundle must carry an Info.plist whose
//!   CFBundleExecutable exists before anything is moved;
//! - a failure after the running bundle moved puts it back.
//!
//! A backup that cannot be deleted afterwards is reported, not failed: the
//! new bundle is in place (inventory row U7).

use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;

#[derive(Debug)]
pub enum InstallError {
    Io(String),
    /// The archive is not a bundle Screepub would run.
    Archive(String),
    /// The bundle's folder is not writable and the person declined (or no
    /// administrator could be asked).
    NeedsAdmin(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(s) | Self::Archive(s) | Self::NeedsAdmin(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for InstallError {}

fn io(what: &str, path: &Path, e: std::io::Error) -> InstallError {
    InstallError::Io(format!("Could not {what} {}: {e}", path.display()))
}

/// What an install left behind.
#[derive(Debug, PartialEq, Eq)]
pub struct Installed {
    pub bundle: PathBuf,
    /// The old bundle, when deleting it failed. The update still counts.
    pub leftover: Option<PathBuf>,
}

/// The `.app` a running executable lives in
/// (`/Applications/Screepub.app/Contents/MacOS/screepub-desktop` gives
/// `/Applications/Screepub.app`), or None outside a bundle, which is how a
/// development build knows it cannot update itself.
pub fn bundle_of_exe(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app")
        .then(|| bundle.to_path_buf())
}

/// Unpack a `.app.tar.gz` into `dest`, dropping the first component of
/// every entry as the plugin does (`.iter().skip(1)`).
pub fn unpack_app_archive(bytes: &[u8], dest: &Path) -> Result<(), InstallError> {
    if bytes.len() < 2 || bytes[0] != 0x1f || bytes[1] != 0x8b {
        return Err(InstallError::Archive(
            "The update is not a gzip archive.".into(),
        ));
    }
    std::fs::create_dir_all(dest).map_err(|e| io("create", dest, e))?;
    let mut archive = tar::Archive::new(GzDecoder::new(Cursor::new(bytes)));
    let entries = archive
        .entries()
        .map_err(|e| InstallError::Archive(format!("The update archive is unreadable: {e}")))?;
    for entry in entries {
        let mut entry = entry
            .map_err(|e| InstallError::Archive(format!("The update archive is unreadable: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| InstallError::Archive(format!("A name in the update is unreadable: {e}")))?
            .into_owned();
        let mut rest = PathBuf::new();
        for c in path.components().skip(1) {
            match c {
                Component::Normal(p) => rest.push(p),
                Component::CurDir => {}
                _ => {
                    return Err(InstallError::Archive(format!(
                        "The update names a file outside its bundle: {}",
                        path.display()
                    )));
                }
            }
        }
        if path
            .components()
            .next()
            .is_some_and(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(InstallError::Archive(format!(
                "The update names a file outside its bundle: {}",
                path.display()
            )));
        }
        let target = dest.join(&rest);
        if rest.as_os_str().is_empty() {
            continue;
        }
        // Files, folders, and links that stay inside the bundle. A link out
        // of it, with a file "under" it later in the archive, would be
        // written through (tar's unpack follows links), and a hard link's
        // name resolves against the working folder, not the bundle. The
        // plugin unpacked all of these; a signature already vouches for the
        // archive, so this only refuses what a bundle never holds.
        match entry.header().entry_type() {
            tar::EntryType::Regular | tar::EntryType::Directory => {}
            tar::EntryType::Symlink => {
                let link = entry
                    .link_name()
                    .ok()
                    .flatten()
                    .map(|l| l.into_owned())
                    .unwrap_or_default();
                if !link_stays_inside(&rest, &link) {
                    return Err(InstallError::Archive(format!(
                        "The update names a file outside its bundle: {} -> {}",
                        path.display(),
                        link.display()
                    )));
                }
            }
            other => {
                return Err(InstallError::Archive(format!(
                    "The update holds an entry a bundle never has ({other:?}): {}",
                    path.display()
                )));
            }
        }
        // Nothing is written THROUGH a link already unpacked: the lexical
        // check above cannot see a chain of links that loop back on
        // themselves (`L -> .`, then `L/L/L/x -> ../../..`).
        let mut walked = dest.to_path_buf();
        for c in rest.parent().into_iter().flat_map(Path::components) {
            walked.push(c);
            if walked
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err(InstallError::Archive(format!(
                    "The update names a file outside its bundle: {}",
                    path.display()
                )));
            }
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io("create", parent, e))?;
        }
        entry
            .unpack(&target)
            .map_err(|e| io("unpack", &target, e))?;
    }
    Ok(())
}

/// Whether a symlink at `at` (relative to the bundle) pointing at `link`
/// resolves inside the bundle: relative, its `..`s all leading and never
/// climbing above the root, then only names. A `..` after a name is
/// refused, because that name may itself be a link and `..` is taken from
/// where it points (`L -> ..`, then `X -> L/../..`). Every link checked
/// this way resolves inside, so descending through one stays inside.
fn link_stays_inside(at: &Path, link: &Path) -> bool {
    let mut depth = at.components().count().saturating_sub(1) as i64;
    if link.as_os_str().is_empty() {
        return false;
    }
    let mut named = false;
    for c in link.components() {
        match c {
            Component::Normal(_) => named = true,
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if named || depth < 0 {
                    return false;
                }
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

/// The executable an unpacked (or installed) bundle names, checked to
/// exist: what tauri's restart reads (`CFBundleExecutable`).
pub fn bundle_executable(bundle: &Path) -> Result<PathBuf, InstallError> {
    let info = bundle.join("Contents").join("Info.plist");
    let dict: plist::Dictionary = plist::from_file(&info).map_err(|e| {
        InstallError::Archive(format!("The update has no readable Info.plist: {e}"))
    })?;
    let name = dict
        .get("CFBundleExecutable")
        .and_then(plist::Value::as_string)
        .ok_or_else(|| {
            InstallError::Archive("The update's Info.plist names no executable.".into())
        })?;
    let exe = bundle.join("Contents").join("MacOS").join(name);
    if !exe.is_file() {
        return Err(InstallError::Archive(format!(
            "The update's executable {name} is missing."
        )));
    }
    Ok(exe)
}

/// The bundle's `CFBundleIdentifier`, if it has one.
pub fn bundle_identifier(bundle: &Path) -> Option<String> {
    let dict: plist::Dictionary =
        plist::from_file(bundle.join("Contents").join("Info.plist")).ok()?;
    dict.get("CFBundleIdentifier")?
        .as_string()
        .map(str::to_string)
}

fn sibling(bundle: &Path, tag: &str) -> PathBuf {
    let name = bundle
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Screepub.app".into());
    bundle.with_file_name(format!(".{name}.{tag}-{}", std::process::id()))
}

fn permission(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::PermissionDenied
}

/// Swap `bundle` for the one in `bytes`. A folder the person cannot write
/// (an /Applications owned by another account) needs an administrator:
/// `admin` is asked to run the swap (on a Mac, [`admin_swap`] through
/// osascript, as the plugin's AppleScript did).
pub fn install_app(
    bytes: &[u8],
    bundle: &Path,
    admin: &dyn Fn(&Path, &Path) -> Result<(), InstallError>,
) -> Result<Installed, InstallError> {
    let staging = sibling(bundle, "update");
    let _ = std::fs::remove_dir_all(&staging);
    match std::fs::create_dir(&staging) {
        Ok(()) => {}
        Err(e) if permission(&e) => return install_with_admin(bytes, bundle, admin),
        Err(e) => return Err(io("create", &staging, e)),
    }
    let staged = (|| {
        unpack_app_archive(bytes, &staging)?;
        bundle_executable(&staging)
    })();
    if let Err(e) = staged {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    let backup = sibling(bundle, "old");
    let _ = std::fs::remove_dir_all(&backup);
    let had_old = bundle.exists();
    if had_old && let Err(e) = std::fs::rename(bundle, &backup) {
        let _ = std::fs::remove_dir_all(&staging);
        if permission(&e) {
            return install_with_admin(bytes, bundle, admin);
        }
        return Err(io("move aside", bundle, e));
    }
    if let Err(e) = std::fs::rename(&staging, bundle) {
        if had_old {
            let _ = std::fs::rename(&backup, bundle);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(io("move the new bundle into", bundle, e));
    }
    let leftover = (had_old && std::fs::remove_dir_all(&backup).is_err()).then_some(backup);
    touch(bundle);
    Ok(Installed {
        bundle: bundle.to_path_buf(),
        leftover,
    })
}

fn install_with_admin(
    bytes: &[u8],
    bundle: &Path,
    admin: &dyn Fn(&Path, &Path) -> Result<(), InstallError>,
) -> Result<Installed, InstallError> {
    // The copy an administrator's shell moves into place is staged in a
    // folder made fresh for it, readable by this account alone: a
    // predictable name in a shared temp folder could be made first by
    // another account, and its contents swapped between the check below
    // and the root `mv`. (The plugin used tempfile's tempdir the same way.)
    let private = private_temp_dir()?;
    let temp = private.join("Screepub.app");
    let result = (|| {
        unpack_app_archive(bytes, &temp)?;
        bundle_executable(&temp)?;
        admin(bundle, &temp)
    })();
    let _ = std::fs::remove_dir_all(&private);
    result?;
    touch(bundle);
    Ok(Installed {
        bundle: bundle.to_path_buf(),
        leftover: None,
    })
}

/// A new folder in the temp folder that did not exist before this call,
/// mode 0700 where the platform has modes.
fn private_temp_dir() -> Result<PathBuf, InstallError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir();
    for _ in 0..64 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let name = format!(
            "screepub-update-{}-{}-{nanos:08x}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let dir = base.join(name);
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        // create, not create_all: an existing folder is never reused.
        match builder.create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io("create", &dir, e)),
        }
    }
    Err(InstallError::Archive(
        "Could not make a private folder to stage the update in.".into(),
    ))
}

/// Bump the bundle's modification time, as the plugin's `touch` does, so
/// Finder and Launch Services read the new Info.plist.
pub fn touch(path: &Path) {
    if let Ok(f) = std::fs::File::open(path) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
}

/// The shell command [`admin_swap_script`] hands an administrator: the old
/// app moved aside, the new one moved in, and the old one put back if that
/// move fails (the plugin's `rm -rf` first left no app at all when the move
/// failed after it), then the old one removed.
pub fn admin_swap_shell(bundle: &Path, new: &Path) -> String {
    let src = shell_quote(&bundle.to_string_lossy());
    let bak = shell_quote(&sibling(bundle, "old").to_string_lossy());
    let new = shell_quote(&new.to_string_lossy());
    format!(
        "rm -rf {bak} && {{ [ ! -e {src} ] || mv -f {src} {bak}; }} && \
         {{ mv -f {new} {src} || {{ [ ! -e {bak} ] || mv -f {bak} {src}; false; }}; }} && \
         rm -rf {bak}"
    )
}

/// One shell word, single-quoted.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The AppleScript the plugin runs when the bundle's folder needs an
/// administrator, with the paths quoted for the shell and then for
/// AppleScript (the plugin quoted neither, so a name with an apostrophe
/// broke it).
pub fn admin_swap_script(bundle: &Path, new: &Path) -> String {
    let shell = admin_swap_shell(bundle, new);
    let escaped = shell.replace('\\', "\\\\").replace('"', "\\\"");
    format!("do shell script \"{escaped}\" with administrator privileges")
}

/// Run [`admin_swap_script`] through /usr/bin/osascript: the system asks
/// for an administrator's name and password, never this app.
pub fn admin_swap(bundle: &Path, new: &Path) -> Result<(), InstallError> {
    let status = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(admin_swap_script(bundle, new))
        .status()
        .map_err(|e| {
            InstallError::NeedsAdmin(format!("Could not ask for an administrator: {e}"))
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(InstallError::NeedsAdmin(
            "Failed to move the new app into place".into(),
        ))
    }
}

/// An AppImage update: either the AppImage itself or a `.tar.gz` holding
/// one, written beside the running file and renamed over it.
#[cfg(unix)]
pub fn install_appimage(bytes: &[u8], appimage: &Path) -> Result<Installed, InstallError> {
    use std::os::unix::fs::PermissionsExt;
    let image = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut archive = tar::Archive::new(GzDecoder::new(Cursor::new(bytes)));
        let mut found = None;
        for entry in archive
            .entries()
            .map_err(|e| InstallError::Archive(e.to_string()))?
        {
            let mut entry = entry.map_err(|e| InstallError::Archive(e.to_string()))?;
            let is_image = entry
                .path()
                .ok()
                .and_then(|p| p.extension().map(|e| e == "AppImage"))
                .unwrap_or(false);
            if is_image {
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|e| InstallError::Archive(e.to_string()))?;
                found = Some(buf);
                break;
            }
        }
        found.ok_or_else(|| {
            InstallError::Archive("binary for the current target not found in the archive".into())
        })?
    } else {
        bytes.to_vec()
    };
    if !image.starts_with(b"\x7fELF") {
        return Err(InstallError::Archive(
            "invalid updater binary format".into(),
        ));
    }
    let staged = sibling(appimage, "update");
    std::fs::write(&staged, &image).map_err(|e| io("write", &staged, e))?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| io("mark executable", &staged, e))?;
    std::fs::rename(&staged, appimage).map_err(|e| {
        let _ = std::fs::remove_file(&staged);
        io("replace", appimage, e)
    })?;
    Ok(Installed {
        bundle: appimage.to_path_buf(),
        leftover: None,
    })
}

/// The arguments Screepub's own NSIS installer takes for an update: silent,
/// then relaunch (packaging/windows/screepub.nsi defines `/R`).
pub fn nsis_update_args() -> [&'static str; 2] {
    ["/S", "/R"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_is_found_from_its_executable() {
        assert_eq!(
            bundle_of_exe(Path::new(
                "/Applications/Screepub.app/Contents/MacOS/screepub-desktop"
            )),
            Some(PathBuf::from("/Applications/Screepub.app"))
        );
        assert_eq!(
            bundle_of_exe(Path::new("/Users/x/rust/target/release/screepub-window")),
            None
        );
    }

    #[test]
    fn the_admin_script_survives_an_apostrophe_and_a_quote() {
        let bundle = Path::new("/Applications/Sam's \"Screepub\".app");
        let s = admin_swap_script(bundle, Path::new("/tmp/new"));
        let shell = admin_swap_shell(bundle, Path::new("/tmp/new"));
        assert!(
            shell.contains(r#"'/Applications/Sam'\''s "Screepub".app'"#),
            "{shell}"
        );
        assert_eq!(
            s,
            format!(
                "do shell script \"{}\" with administrator privileges",
                shell.replace('\\', "\\\\").replace('"', "\\\"")
            )
        );
        // The shell form runs for real in tests/hostile_archive.rs.
    }

    #[test]
    fn not_gzip_is_refused_before_anything_moves() {
        let dir = tempfile::tempdir().unwrap();
        let err = unpack_app_archive(b"PK\x03\x04", dir.path()).unwrap_err();
        assert_eq!(err.to_string(), "The update is not a gzip archive.");
    }
}

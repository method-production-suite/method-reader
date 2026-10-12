//! Reproducible archives: the update's `.app.tar.gz`, the command-line
//! tarballs and zip, and the tar members of a .deb.
//!
//! Entries are sorted, owned by 0:0 with no user or group names, dated
//! with one time (`SOURCE_DATE_EPOCH`, else now), and folders are 0755,
//! files 0755 when executable and 0644 otherwise, so the same tree always
//! gives the same bytes. Symlinks stay symlinks. The gzip header carries no
//! time and no name.

use std::io::Write;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File(Vec<u8>),
    Symlink(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Relative, `/`-separated; folders without a trailing slash.
    pub path: String,
    pub kind: Kind,
    pub mode: u32,
}

impl Entry {
    pub fn dir(path: &str) -> Self {
        Self {
            path: path.into(),
            kind: Kind::Dir,
            mode: 0o755,
        }
    }
    pub fn file(path: &str, data: Vec<u8>, executable: bool) -> Self {
        Self {
            path: path.into(),
            kind: Kind::File(data),
            mode: if executable { 0o755 } else { 0o644 },
        }
    }
}

/// The time every entry carries: `SOURCE_DATE_EPOCH` when set (a rebuild
/// of a tag gives the same bytes), else now.
pub fn build_time() -> u64 {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        })
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    false
}

/// Every entry of the folder `root`, under `top` (`Screepub.app`, ...),
/// `top` itself first, then depth-first in name order.
pub fn tree_entries(root: &Path, top: &str) -> std::io::Result<Vec<Entry>> {
    let mut out = vec![Entry::dir(top)];
    walk(root, top, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<Entry>) -> std::io::Result<()> {
    let mut names: Vec<_> = std::fs::read_dir(dir)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<Result<_, _>>()?;
    names.sort();
    for name in names {
        let name_str = name.to_string_lossy();
        if name_str == ".DS_Store" || name_str.starts_with("._") {
            continue;
        }
        let path = dir.join(&name);
        let rel = format!("{prefix}/{name_str}");
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(&path)?;
            out.push(Entry {
                path: rel,
                kind: Kind::Symlink(target.to_string_lossy().replace('\\', "/")),
                mode: 0o755,
            });
        } else if meta.is_dir() {
            out.push(Entry::dir(&rel));
            walk(&path, &rel, out)?;
        } else {
            out.push(Entry::file(
                &rel,
                std::fs::read(&path)?,
                is_executable(&meta),
            ));
        }
    }
    Ok(())
}

fn raw_name(h: &mut tar::Header, name: &str) -> std::io::Result<()> {
    let field = &mut h
        .as_gnu_mut()
        .ok_or_else(|| std::io::Error::other("not a GNU header"))?
        .name;
    if name.len() >= field.len() {
        return Err(std::io::Error::other(format!("{name} is too long")));
    }
    field.fill(0);
    field[..name.len()].copy_from_slice(name.as_bytes());
    h.set_cksum();
    Ok(())
}

/// A tar of `entries`, in the order given.
pub fn tar(entries: &[Entry], mtime: u64) -> std::io::Result<Vec<u8>> {
    let mut b = tar::Builder::new(Vec::new());
    b.mode(tar::HeaderMode::Deterministic);
    for e in entries {
        let mut h = tar::Header::new_gnu();
        h.set_mtime(mtime);
        h.set_uid(0);
        h.set_gid(0);
        h.set_mode(e.mode);
        // dpkg-deb writes `./usr/...`; the tar crate's path setter drops a
        // `./`, so such names (all short) go into the header as written.
        let dotted = e.path == "." || e.path.starts_with("./");
        match &e.kind {
            Kind::Dir if dotted => {
                h.set_entry_type(tar::EntryType::Directory);
                h.set_size(0);
                raw_name(&mut h, &format!("{}/", e.path))?;
                b.append(&h, std::io::empty())?;
            }
            Kind::File(data) if dotted => {
                h.set_entry_type(tar::EntryType::Regular);
                h.set_size(data.len() as u64);
                raw_name(&mut h, &e.path)?;
                b.append(&h, data.as_slice())?;
            }
            Kind::Dir => {
                h.set_entry_type(tar::EntryType::Directory);
                h.set_size(0);
                b.append_data(&mut h, format!("{}/", e.path), std::io::empty())?;
            }
            Kind::File(data) => {
                h.set_entry_type(tar::EntryType::Regular);
                h.set_size(data.len() as u64);
                b.append_data(&mut h, &e.path, data.as_slice())?;
            }
            Kind::Symlink(target) => {
                h.set_entry_type(tar::EntryType::Symlink);
                h.set_size(0);
                b.append_link(&mut h, &e.path, target)?;
            }
        }
    }
    b.into_inner()
}

/// gzip with no time and no name in its header.
pub fn gzip(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut gz = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::best());
    gz.write_all(bytes)?;
    gz.finish()
}

pub fn tar_gz(entries: &[Entry], mtime: u64) -> std::io::Result<Vec<u8>> {
    gzip(&tar(entries, mtime)?)
}

/// The update archive of a finished (signed, stapled) bundle: the folder
/// under `Screepub.app/`, whatever the folder is called on disk, since the
/// installer drops the top component anyway. Every file in Contents/MacOS
/// is 0755 whatever the host's file system says (a Windows host has no
/// mode bits to read), so the installed window always runs.
pub fn app_archive(bundle: &Path, mtime: u64) -> std::io::Result<Vec<u8>> {
    let top = crate::names::MAC_BUNDLE;
    let macos = format!("{top}/Contents/MacOS/");
    let mut entries = tree_entries(bundle, top)?;
    for e in &mut entries {
        if matches!(e.kind, Kind::File(_)) && e.path.starts_with(&macos) {
            e.mode = 0o755;
        }
    }
    tar_gz(&entries, mtime)
}

/// A command-line tarball: one executable file named `screepub`, as the
/// Homebrew formula's `bin.install "screepub"` expects.
pub fn cli_tarball(binary: &Path, mtime: u64) -> std::io::Result<Vec<u8>> {
    tar_gz(
        &[Entry::file(
            crate::names::CLI_EXE,
            std::fs::read(binary)?,
            true,
        )],
        mtime,
    )
}

/// The Windows command-line zip: one `screepub.exe`.
pub fn cli_zip(binary: &Path) -> std::io::Result<Vec<u8>> {
    use zip::write::SimpleFileOptions;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o755);
    w.start_file(format!("{}.exe", crate::names::CLI_EXE), options)
        .map_err(std::io::Error::other)?;
    w.write_all(&std::fs::read(binary)?)?;
    Ok(w.finish().map_err(std::io::Error::other)?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_tree_gives_the_same_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let b = dir.path().join("Built.app/Contents/MacOS");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join("x"), "x").unwrap();
        std::fs::write(dir.path().join("Built.app/Contents/._x"), "junk").unwrap();
        let one = app_archive(&dir.path().join("Built.app"), 7).unwrap();
        let two = app_archive(&dir.path().join("Built.app"), 7).unwrap();
        assert_eq!(one, two);
        let mut names = Vec::new();
        let mut a = tar::Archive::new(flate2::read::GzDecoder::new(one.as_slice()));
        for e in a.entries().unwrap() {
            let e = e.unwrap();
            names.push(e.path().unwrap().to_string_lossy().into_owned());
            assert_eq!(e.header().mtime().unwrap(), 7);
            assert_eq!(e.header().uid().unwrap(), 0);
        }
        assert_eq!(
            names,
            [
                "Screepub.app/",
                "Screepub.app/Contents/",
                "Screepub.app/Contents/MacOS/",
                "Screepub.app/Contents/MacOS/x"
            ]
        );
    }

    #[test]
    fn a_cli_zip_holds_one_exe() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("screepub.exe");
        std::fs::write(&bin, "MZ").unwrap();
        let z = cli_zip(&bin).unwrap();
        let mut r = zip::ZipArchive::new(std::io::Cursor::new(z)).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r.by_index(0).unwrap().name().unwrap(), "screepub.exe");
    }
}

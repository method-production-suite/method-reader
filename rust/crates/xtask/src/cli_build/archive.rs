//! Reading the CLI's download archives back: tar.gz for Linux and the Mac,
//! zip for Windows, what each platform's users can open with nothing
//! installed. Writing them is sp-release's ([`sp_release::archive`]), the
//! one writer release.yml uses.
//!
//! The tar is walked here rather than by a `tar` program:
//! `tar -tzvf` answers in a listing format that differs between GNU tar and
//! bsdtar, and macOS's bsdtar adds a hidden `._name` member beside each file
//! unless told not to (the TypeScript set COPYFILE_DISABLE for that). A
//! ustar header is 512 fixed bytes.

use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;

/// One member of an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    /// Unix mode as recorded in the archive; 0 when the format carries none.
    pub mode: u32,
    pub data: Vec<u8>,
}

fn tar_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn tar_octal(bytes: &[u8]) -> u64 {
    let text = tar_string(bytes);
    u64::from_str_radix(text.trim(), 8).unwrap_or(0)
}

/// Walk a tar held in memory. Regular files only, and only the 100-byte
/// `name` field: the ustar `prefix` and GNU long-name records are not
/// joined, because nothing packaged here reaches a 100-byte path.
pub fn tar_entries(tar: &[u8]) -> Vec<ArchiveEntry> {
    let mut entries = Vec::new();
    let mut off = 0usize;
    while off + 512 <= tar.len() {
        let header = &tar[off..off + 512];
        if header.iter().all(|&b| b == 0) {
            break; // end-of-archive marker
        }
        let name = tar_string(&header[0..100]);
        let mode = tar_octal(&header[100..108]) as u32;
        let size = tar_octal(&header[124..136]);
        let typeflag = header[156];
        let start = off + 512;
        if typeflag == b'0' || typeflag == 0 {
            let end = (start as u64 + size).min(tar.len() as u64) as usize;
            entries.push(ArchiveEntry {
                name,
                size,
                mode,
                data: tar[start..end].to_vec(),
            });
        }
        off = start + (size as usize).div_ceil(512) * 512;
    }
    entries
}

/// Every member of a .tar.gz.
pub fn tar_gz_entries(path: &Path) -> Result<Vec<ArchiveEntry>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut tar = Vec::new();
    GzDecoder::new(bytes.as_slice())
        .read_to_end(&mut tar)
        .map_err(|e| format!("{} is not a gzip stream: {e}", path.display()))?;
    Ok(tar_entries(&tar))
}

/// Every file member of a .zip, with its Unix mode when the zip records one.
pub fn zip_entries(path: &Path) -> Result<Vec<ArchiveEntry>, String> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("{} is not a zip: {e}", path.display()))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut member = zip
            .by_index(i)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if member.is_dir() {
            continue;
        }
        let mut data = Vec::new();
        member
            .read_to_end(&mut data)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        out.push(ArchiveEntry {
            name: member
                .name()
                .map_err(|e| format!("{}: {e}", path.display()))?
                .into_owned(),
            size: data.len() as u64,
            mode: member.unix_mode().unwrap_or(0),
            data,
        });
    }
    Ok(out)
}

/// Dispatches on the extension, and refuses anything else.
pub fn archive_entries(path: &Path) -> Result<Vec<ArchiveEntry>, String> {
    let name = path.to_string_lossy();
    if name.ends_with(".tar.gz") {
        tar_gz_entries(path)
    } else if name.ends_with(".zip") {
        zip_entries(path)
    } else {
        Err(format!(
            "build-cli: {} is neither a .tar.gz nor a .zip",
            path.display()
        ))
    }
}

/// A .tar.gz holding exactly these members, each with its mode as given,
/// written by sp-release's archive writer (the one release.yml's
/// `cli-archive` uses). For hand-built archives in tests; the CLI's own
/// archive is [`sp_release::archive::cli_tarball`].
pub fn write_tar_gz(path: &Path, members: &[(&str, &[u8], u32)]) -> Result<(), String> {
    let entries: Vec<sp_release::archive::Entry> = members
        .iter()
        .map(|(name, data, mode)| sp_release::archive::Entry {
            path: (*name).to_string(),
            kind: sp_release::archive::Kind::File(data.to_vec()),
            mode: *mode,
        })
        .collect();
    let bytes = sp_release::archive::tar_gz(&entries, sp_release::archive::build_time())
        .map_err(|e| format!("build-cli: cannot write {}: {e}", path.display()))?;
    std::fs::write(path, bytes)
        .map_err(|e| format!("build-cli: cannot write {}: {e}", path.display()))
}

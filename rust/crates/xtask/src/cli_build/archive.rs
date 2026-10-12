//! Writing and reading the CLI's download archives: tar.gz for Linux and
//! the Mac, zip for Windows, what each platform's users can open with
//! nothing installed.
//!
//! The tar is written and walked here rather than by a `tar` program:
//! `tar -tzvf` answers in a listing format that differs between GNU tar and
//! bsdtar, and macOS's bsdtar adds a hidden `._name` member beside each file
//! unless told not to (the TypeScript set COPYFILE_DISABLE for that). A
//! ustar header is 512 fixed bytes.

use std::io::{Read, Write};
use std::path::Path;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

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

fn octal_field(field: &mut [u8], value: u64) {
    // Zero-padded octal digits filling all but the last byte, then NUL.
    let digits = field.len() - 1;
    let text = format!("{value:0digits$o}");
    field[..digits].copy_from_slice(&text.as_bytes()[text.len() - digits..]);
    field[digits] = 0;
}

/// A ustar header for one regular file.
fn tar_header(name: &str, size: u64, mode: u32, mtime: u64) -> Result<[u8; 512], String> {
    if name.len() > 100 {
        return Err(format!("build-cli: {name} is longer than a tar name field"));
    }
    let mut h = [0u8; 512];
    h[..name.len()].copy_from_slice(name.as_bytes());
    octal_field(&mut h[100..108], u64::from(mode));
    octal_field(&mut h[108..116], 0); // uid
    octal_field(&mut h[116..124], 0); // gid
    octal_field(&mut h[124..136], size);
    octal_field(&mut h[136..148], mtime);
    h[156] = b'0';
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    // The checksum is summed with its own field as eight spaces.
    h[148..156].copy_from_slice(b"        ");
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    let text = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(text.as_bytes());
    Ok(h)
}

/// A .tar.gz holding exactly these members, each with its mode as given.
pub fn write_tar_gz(path: &Path, members: &[(&str, &[u8], u32)]) -> Result<(), String> {
    let mtime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut tar = Vec::new();
    for (name, data, mode) in members {
        tar.extend_from_slice(&tar_header(name, data.len() as u64, *mode, mtime)?);
        tar.extend_from_slice(data);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.extend_from_slice(&[0u8; 1024]); // two zero blocks end the archive
    let file = std::fs::File::create(path)
        .map_err(|e| format!("build-cli: cannot write {}: {e}", path.display()))?;
    let mut gz = GzEncoder::new(file, Compression::best());
    gz.write_all(&tar)
        .and_then(|()| gz.finish().map(drop))
        .map_err(|e| format!("build-cli: cannot write {}: {e}", path.display()))
}

/// A deflated .zip holding exactly these members, each with its Unix mode
/// written into the central directory.
pub fn write_zip(path: &Path, members: &[(&str, &[u8], u32)]) -> Result<(), String> {
    let fail =
        |e: &dyn std::fmt::Display| format!("build-cli: cannot write {}: {e}", path.display());
    let file = std::fs::File::create(path).map_err(|e| fail(&e))?;
    let mut zip = zip::ZipWriter::new(file);
    for (name, data, mode) in members {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(*mode);
        zip.start_file(*name, options).map_err(|e| fail(&e))?;
        zip.write_all(data).map_err(|e| fail(&e))?;
    }
    zip.finish().map_err(|e| fail(&e))?;
    Ok(())
}

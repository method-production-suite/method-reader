//! Opening a .deb or an .rpm and reading the files inside it with no outside
//! tool, on any platform: was tools/bundle-archive.ts (and the tar walker
//! it borrowed from tools/build-cli.ts).
//!
//! rpm2cpio, dpkg-deb and 7z are not on every runner, and a check that
//! shells out to a missing tool is a check that skips. Both containers are
//! fixed headers around a gzip stream, so the readers are short. They are
//! written by hand rather than with the `tar` crate so they keep the
//! TypeScript's refusals word for word; the native bundle WRITERS
//! (native.rs) are checked by reading their output back through these.
//!
//! `cargo xtask bundle-archive <file.deb|file.rpm>` lists what a package
//! holds: mode, size and path, one per line.

use std::io::Read;
use std::path::Path;

/// One regular file inside an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    /// Unix mode as recorded in the archive; 0 when the format has none.
    pub mode: u32,
    pub data: Vec<u8>,
}

const AR_MAGIC: &[u8] = b"!<arch>\n";

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn json(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

/// JavaScript's parseInt over a leading run of digits in `radix`.
fn parse_int(text: &str, radix: u32) -> Option<u64> {
    let t = text.trim_start();
    let digits: String = t.chars().take_while(|c| c.is_digit(radix)).collect();
    if digits.is_empty() {
        return None;
    }
    u64::from_str_radix(&digits, radix).ok()
}

/// Bun.gunzipSync.
pub fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    flate2::read::MultiGzDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// The members of an `ar` archive, in order, by name. GNU long names (the
/// `//` table, referenced as `/<offset>`) are resolved; the symbol tables
/// are skipped; a cut-off archive is refused.
pub fn ar_members(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let head = &bytes[..bytes.len().min(8)];
    if head != AR_MAGIC {
        return Err(format!(
            "bundle-archive: not an ar archive (expected the {} magic, got {})",
            json("!<arch>\n"),
            json(&latin1(head))
        ));
    }
    let mut members: Vec<(String, Vec<u8>)> = Vec::new();
    let mut long_names = String::new();
    let mut off = 8usize;
    while off + 60 <= bytes.len() {
        let header = &bytes[off..off + 60];
        if header[58] != 0x60 || header[59] != 0x0a {
            return Err(format!(
                "bundle-archive: ar member header at offset {off} has no 0x60 0x0a magic"
            ));
        }
        let raw_name = latin1(&header[..16]).trim().to_string();
        let Some(size) = parse_int(latin1(&header[48..58]).trim(), 10) else {
            return Err(format!(
                "bundle-archive: ar member \"{raw_name}\" declares an unreadable size"
            ));
        };
        let size = size as usize;
        let start = off + 60;
        if start + size > bytes.len() {
            return Err(format!(
                "bundle-archive: ar member \"{raw_name}\" claims {size} bytes but the archive is truncated {} bytes short",
                start + size - bytes.len()
            ));
        }
        let data = &bytes[start..start + size];
        if raw_name == "//" {
            long_names = latin1(data);
        } else if raw_name != "/" && raw_name != "/SYM64/" {
            let mut name = raw_name.clone();
            if let Some(digits) = raw_name.strip_prefix('/')
                && !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
            {
                let at: usize = digits.parse().unwrap_or(usize::MAX);
                let end = long_names
                    .get(at..)
                    .and_then(|rest| rest.find("/\n"))
                    .map(|e| at + e);
                let Some(end) = end else {
                    return Err(format!(
                        "bundle-archive: ar member \"{raw_name}\" points outside the // long-name table"
                    ));
                };
                name = long_names[at..end].to_string();
            }
            let name = name.strip_suffix('/').unwrap_or(&name).to_string();
            // A Map keeps the first insertion's place and the last value.
            if let Some(slot) = members.iter_mut().find(|(n, _)| *n == name) {
                slot.1 = data.to_vec();
            } else {
                members.push((name, data.to_vec()));
            }
        }
        off = start + size + (size % 2);
    }
    if off != bytes.len() {
        return Err(format!(
            "bundle-archive: ar archive is truncated -- {} bytes left over after the last complete member, too few for a 60-byte header",
            bytes.len() as isize - off as isize
        ));
    }
    Ok(members)
}

/// Every regular file in a `newc`/`crc` cpio stream, which must end at its
/// TRAILER!!! entry.
pub fn cpio_entries(bytes: &[u8]) -> Result<Vec<ArchiveEntry>, String> {
    let mut entries = Vec::new();
    let mut off = 0usize;
    loop {
        let magic = latin1(&bytes[off.min(bytes.len())..(off + 6).min(bytes.len())]);
        if magic != "070701" && magic != "070702" {
            return Err(format!(
                "bundle-archive: cpio entry at offset {off} has magic {}, expected 070701 or 070702",
                json(&magic)
            ));
        }
        if off + 110 > bytes.len() {
            return Err(format!(
                "bundle-archive: cpio header at offset {off} is cut off mid-header"
            ));
        }
        let field = |i: usize| parse_int(&latin1(&bytes[off + 6 + i * 8..off + 14 + i * 8]), 16);
        let (Some(mode), Some(file_size), Some(name_size)) = (field(1), field(6), field(11)) else {
            return Err(format!(
                "bundle-archive: cpio entry at offset {off} has unreadable size fields"
            ));
        };
        if name_size < 1 {
            return Err(format!(
                "bundle-archive: cpio entry at offset {off} has unreadable size fields"
            ));
        }
        let (file_size, name_size) = (file_size as usize, name_size as usize);
        let name_start = off + 110;
        let name_end = (name_start + name_size - 1).min(bytes.len());
        let name = latin1(&bytes[name_start.min(bytes.len())..name_end]);
        if name == "TRAILER!!!" {
            return Ok(entries);
        }
        let data_start = name_start + name_size + (4 - (110 + name_size) % 4) % 4;
        if data_start + file_size > bytes.len() {
            return Err(format!(
                "bundle-archive: cpio entry \"{name}\" claims {file_size} bytes but the payload ends first"
            ));
        }
        if mode & 0o170000 == 0o100000 {
            entries.push(ArchiveEntry {
                name,
                size: file_size as u64,
                mode: (mode & 0o7777) as u32,
                data: bytes[data_start..data_start + file_size].to_vec(),
            });
        }
        off = data_start + file_size + (4 - file_size % 4) % 4;
        if off >= bytes.len() {
            return Err(format!(
                "bundle-archive: the cpio payload ended after {} files with no TRAILER!!! entry, so it is truncated and this file list is incomplete",
                entries.len()
            ));
        }
    }
}

fn be32(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
}

/// Where an rpm header that starts at `start` ends.
fn rpm_header_end(bytes: &[u8], start: usize) -> Result<usize, String> {
    if start + 16 > bytes.len() {
        return Err(format!(
            "bundle-archive: rpm ends before the header at offset {start} begins"
        ));
    }
    if bytes[start..start + 3] != [0x8e, 0xad, 0xe8] {
        return Err(format!(
            "bundle-archive: rpm header at offset {start} has no 8e ad e8 magic"
        ));
    }
    let nindex = be32(bytes, start + 8);
    let hsize = be32(bytes, start + 12);
    let end = start + 16 + nindex * 16 + hsize;
    if end > bytes.len() {
        return Err(format!(
            "bundle-archive: rpm header at offset {start} declares {} bytes, past the end of a {}-byte file",
            end - start,
            bytes.len()
        ));
    }
    Ok(end)
}

/// Where the signature header ends and the main header starts (the
/// signature header alone is padded to 8 bytes).
fn rpm_main_header_start(bytes: &[u8]) -> Result<usize, String> {
    if bytes.len() < 4 || bytes[..4] != [0xed, 0xab, 0xee, 0xdb] {
        return Err("bundle-archive: not an rpm (the ed ab ee db lead magic is missing)".into());
    }
    let sig_end = rpm_header_end(bytes, 96)?;
    Ok(sig_end + (8 - sig_end % 8) % 8)
}

/// The compressed payload of an .rpm: everything after the 96-byte lead,
/// the signature header and the header.
pub fn rpm_payload(bytes: &[u8]) -> Result<&[u8], String> {
    let hdr = rpm_main_header_start(bytes)?;
    Ok(&bytes[rpm_header_end(bytes, hdr)?..])
}

/// One string tag of an rpm's main header (NAME is 1000, VERSION 1001,
/// RELEASE 1002, ARCH 1022), for the checks that read what a package says
/// it is.
pub fn rpm_header_string(bytes: &[u8], tag: u32) -> Result<Option<String>, String> {
    let start = rpm_main_header_start(bytes)?;
    rpm_header_end(bytes, start)?;
    let nindex = be32(bytes, start + 8);
    let store = start + 16 + nindex * 16;
    for i in 0..nindex {
        let e = start + 16 + i * 16;
        if be32(bytes, e) as u32 == tag {
            let off = store + be32(bytes, e + 8);
            let end = bytes[off..].iter().position(|&b| b == 0).map(|p| off + p);
            return Ok(end.map(|end| String::from_utf8_lossy(&bytes[off..end]).into_owned()));
        }
    }
    Ok(None)
}

fn tar_string(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    match text.find('\0') {
        Some(nul) => text[..nul].to_string(),
        None => text.into_owned(),
    }
}

fn tar_octal(bytes: &[u8]) -> u64 {
    let text = tar_string(bytes);
    let t = text.trim();
    if t.is_empty() {
        0
    } else {
        parse_int(t, 8).unwrap_or(0)
    }
}

/// The regular files of a tar held in memory: the 100-byte name field only
/// (nothing packaged here reaches a longer path).
pub fn tar_entries(tar: &[u8]) -> Vec<ArchiveEntry> {
    let mut entries = Vec::new();
    let mut off = 0usize;
    while off + 512 <= tar.len() {
        let header = &tar[off..off + 512];
        if header.iter().all(|&b| b == 0) {
            break;
        }
        let name = tar_string(&header[..100]);
        let mode = tar_octal(&header[100..108]) as u32;
        let size = tar_octal(&header[124..136]);
        let typeflag = header[156];
        let start = off + 512;
        if typeflag == b'0' || typeflag == 0 {
            let end = (start + size as usize).min(tar.len());
            entries.push(ArchiveEntry {
                name,
                size,
                mode,
                data: tar[start.min(end)..end].to_vec(),
            });
        }
        off = start + (size as usize).div_ceil(512) * 512;
    }
    entries
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("bundle-archive: cannot read {}: {e}", path.display()))
}

/// The files of a .deb's data.tar.gz.
pub fn deb_entries(path: &Path) -> Result<Vec<ArchiveEntry>, String> {
    let members = ar_members(&read(path)?)?;
    let Some((_, data)) = members.iter().find(|(n, _)| n == "data.tar.gz") else {
        let held: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();
        return Err(format!(
            "bundle-archive: {} holds no data.tar.gz (it holds: {})",
            path.display(),
            if held.is_empty() {
                "<nothing>".to_string()
            } else {
                held.join(", ")
            }
        ));
    };
    Ok(tar_entries(&gunzip(data).map_err(|e| {
        format!("bundle-archive: {}: {e}", path.display())
    })?))
}

/// One field of a .deb's control file (`Version`, `Package`, ...).
pub fn deb_control_field(path: &Path, field: &str) -> Result<Option<String>, String> {
    let members = ar_members(&read(path)?)?;
    let Some((_, control)) = members.iter().find(|(n, _)| n == "control.tar.gz") else {
        return Err(format!(
            "bundle-archive: {} holds no control.tar.gz",
            path.display()
        ));
    };
    let entries = tar_entries(&gunzip(control)?);
    let Ok(file) = find_entry(&entries, "control") else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&file.data);
    let prefix = format!("{field}:");
    Ok(text
        .lines()
        .find_map(|l| l.strip_prefix(&prefix).map(|v| v.trim().to_string())))
}

/// The files of an .rpm's gzip cpio payload.
pub fn rpm_entries(path: &Path) -> Result<Vec<ArchiveEntry>, String> {
    let bytes = read(path)?;
    let payload = rpm_payload(&bytes)?;
    let cpio = gunzip(payload).map_err(|e| {
        format!(
            "bundle-archive: {}'s payload did not gunzip ({e}). tauri-bundler wrote a gzip cpio payload when this reader was written; a different PAYLOADCOMPRESSOR (zstd, xz) needs a new branch here.",
            path.display()
        )
    })?;
    cpio_entries(&cpio)
}

/// A .deb or an .rpm, by extension.
pub fn bundle_entries(path: &Path) -> Result<Vec<ArchiveEntry>, String> {
    let s = path.to_string_lossy();
    if s.ends_with(".deb") {
        return deb_entries(path);
    }
    if s.ends_with(".rpm") {
        return rpm_entries(path);
    }
    Err(format!(
        "bundle-archive: {s} is neither a .deb nor an .rpm. A .dmg needs hdiutil and an NSIS .exe needs 7z; those are opened on the OS that has them."
    ))
}

fn strip_dot_slash(p: &str) -> &str {
    p.strip_prefix("./")
        .or_else(|| p.strip_prefix('/'))
        .unwrap_or(p)
}

/// One entry by path, across the `./` prefix a .deb's tar and an .rpm's
/// cpio disagree about, and never by a bare suffix.
pub fn find_entry<'a>(entries: &'a [ArchiveEntry], path: &str) -> Result<&'a ArchiveEntry, String> {
    let want = strip_dot_slash(path);
    entries
        .iter()
        .find(|e| strip_dot_slash(&e.name) == want)
        .ok_or_else(|| {
            let held: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            format!(
                "bundle-archive: no {path} inside the bundle (it holds: {})",
                if held.is_empty() {
                    "<nothing>".to_string()
                } else {
                    held.join(", ")
                }
            )
        })
}

/// `cargo xtask bundle-archive <file>`: list a package's files.
pub fn main(args: &[String]) -> u8 {
    let [path] = args else {
        eprintln!(
            "usage: cargo xtask bundle-archive <file.deb|file.rpm>\n\nLists the files inside a built Linux package: mode, size and path."
        );
        return 2;
    };
    if path == "-h" || path == "--help" {
        println!(
            "usage: cargo xtask bundle-archive <file.deb|file.rpm>\n\nLists the files inside a built Linux package: mode, size and path."
        );
        return 0;
    }
    match bundle_entries(Path::new(path)) {
        Ok(entries) => {
            for e in entries {
                println!("{:04o} {:>12} {}", e.mode, e.size, e.name);
            }
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

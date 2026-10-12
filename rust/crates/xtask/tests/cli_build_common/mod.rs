//! Shared by the cli_build tests: scratch folders and hand-built executable
//! headers. A header carries the exact bytes a real executable of that shape
//! carries, and zeros elsewhere, rather than a 100 MB artifact in the repo.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A fresh folder under the system temp folder, removed on drop.
pub struct Scratch(PathBuf);

static NEXT: AtomicU32 = AtomicU32::new(0);

impl Scratch {
    pub fn new(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("screepub-xtask-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("make scratch folder");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, p: &str) -> PathBuf {
        self.0.join(p)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// A 64-bit little-endian ELF header with this e_machine.
pub fn elf_header(machine: u16) -> Vec<u8> {
    let mut b = vec![0u8; 256];
    b[..8].copy_from_slice(&[0x7f, 0x45, 0x4c, 0x46, 2, 1, 1, 0]);
    b[0x12..0x14].copy_from_slice(&machine.to_le_bytes());
    b
}

/// An MZ stub pointing at a PE header at `pe_offset` with this machine.
pub fn pe_header(machine: u16, pe_offset: u32) -> Vec<u8> {
    let off = pe_offset as usize;
    let size = if off >= 256 { 256 } else { 256.max(off + 6) };
    let mut b = vec![0u8; size];
    b[..2].copy_from_slice(b"MZ");
    b[0x3c..0x40].copy_from_slice(&pe_offset.to_le_bytes());
    if off + 6 <= size {
        b[off..off + 4].copy_from_slice(b"PE\0\0");
        b[off + 4..off + 6].copy_from_slice(&machine.to_le_bytes());
    }
    b
}

/// A thin 64-bit Mach-O header with this cputype.
pub fn macho_header(cputype: u32) -> Vec<u8> {
    let mut b = vec![0u8; 256];
    b[..4].copy_from_slice(&0xfeed_facf_u32.to_le_bytes());
    b[4..8].copy_from_slice(&cputype.to_le_bytes());
    b
}

/// A fat (universal) Mach-O header: big-endian magic, a count, then one
/// fat_arch record per slice (20 bytes, or 32 for FAT_MAGIC_64).
pub fn fat_header(cputypes: &[u32], magic: u32) -> Vec<u8> {
    let mut b = vec![0u8; 256];
    b[..4].copy_from_slice(&magic.to_be_bytes());
    b[4..8].copy_from_slice(&(cputypes.len() as u32).to_be_bytes());
    let stride = if magic == 0xcafe_babf { 32 } else { 20 };
    for (i, cpu) in cputypes.iter().enumerate() {
        let off = 8 + i * stride;
        b[off..off + 4].copy_from_slice(&cpu.to_be_bytes());
    }
    b
}

/// An ELF of `bytes` bytes, not compressible to nothing.
pub fn elf_payload(bytes: usize, machine: u16) -> Vec<u8> {
    let mut b = vec![0u8; bytes];
    let h = elf_header(machine);
    let n = h.len().min(bytes);
    b[..n].copy_from_slice(&h[..n]);
    for (i, x) in b.iter_mut().enumerate().skip(64) {
        *x = (i % 251) as u8;
    }
    b
}

/// A payload of `bytes` bytes that reads as `format` (one of the formats a
/// target names), not compressible to nothing.
pub fn payload_for(format: xtask::cli_build::binfmt::BinaryFormat, bytes: usize) -> Vec<u8> {
    use xtask::cli_build::binfmt::BinaryFormat as F;
    let head = match format {
        F::ElfX8664 => elf_header(0x3e),
        F::ElfAarch64 => elf_header(0xb7),
        F::PeX8664 => pe_header(0x8664, 0x78),
        F::MachoArm64 => macho_header(0x0100_000c),
        F::MachoX8664 => macho_header(0x0100_0007),
        F::MachoUniversal => fat_header(&[0x0100_0007, 0x0100_000c], 0xcafe_babe),
        F::Unknown => vec![0u8; 256],
    };
    let mut b = vec![0u8; bytes.max(head.len())];
    b[..head.len()].copy_from_slice(&head);
    for (i, x) in b.iter_mut().enumerate().skip(head.len()) {
        *x = (i % 251) as u8;
    }
    b
}

/// A program on PATH, or None.
pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidates = [dir.join(program), dir.join(format!("{program}.exe"))];
        candidates.into_iter().find(|c| c.is_file())
    })
}

/// Writes an executable shell script (Unix only; callers skip elsewhere).
#[cfg(unix)]
pub fn write_script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, format!("#!/bin/sh\n{body}")).expect("write script");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod script");
}

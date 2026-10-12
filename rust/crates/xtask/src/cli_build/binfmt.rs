//! Naming an executable's format from its header (was build-cli.ts's
//! detectBinaryFormat).
//!
//! Shelling out to `file` is not usable: it does not exist on a Windows
//! runner, is not guaranteed in a container, and answers in prose. These
//! offsets are the on-disk ABI.

use std::fmt;
use std::io::Read;
use std::path::Path;

/// How many leading bytes are enough to name every format below.
pub const HEAD_BYTES: usize = 4096;

/// An executable format, as build-cli checks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryFormat {
    ElfX8664,
    ElfAarch64,
    PeX8664,
    MachoArm64,
    MachoX8664,
    MachoUniversal,
    Unknown,
}

impl BinaryFormat {
    /// The name every message uses (the TypeScript's string values).
    pub fn name(self) -> &'static str {
        match self {
            Self::ElfX8664 => "elf-x86-64",
            Self::ElfAarch64 => "elf-aarch64",
            Self::PeX8664 => "pe-x86-64",
            Self::MachoArm64 => "macho-arm64",
            Self::MachoX8664 => "macho-x86-64",
            Self::MachoUniversal => "macho-universal",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for BinaryFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

fn u16_le(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

fn u32_le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn u32_be(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

const CPU_X86_64: u32 = 0x0100_0007;
const CPU_ARM64: u32 = 0x0100_000c;

/// Name the executable format from its header. Never panics: empty, short
/// and garbage input is `Unknown`.
pub fn detect_binary_format(head: &[u8]) -> BinaryFormat {
    // ELF: 7f 'E' 'L' 'F', EI_CLASS=2 (64-bit), EI_DATA=1 (little endian),
    // then e_machine as a u16 at 0x12.
    if head.starts_with(&[0x7f, b'E', b'L', b'F']) {
        if head.get(4) != Some(&2) || head.get(5) != Some(&1) {
            return BinaryFormat::Unknown;
        }
        return match u16_le(head, 0x12) {
            Some(0x3e) => BinaryFormat::ElfX8664,
            Some(0xb7) => BinaryFormat::ElfAarch64,
            _ => BinaryFormat::Unknown,
        };
    }

    // PE: 'MZ', a u32 at 0x3c pointing at 'PE\0\0', machine as a u16 after it.
    if head.starts_with(b"MZ") {
        let Some(off) = u32_le(head, 0x3c).map(|o| o as usize) else {
            return BinaryFormat::Unknown;
        };
        if off.checked_add(6).is_none_or(|end| end > head.len()) {
            return BinaryFormat::Unknown;
        }
        if &head[off..off + 4] != b"PE\0\0" {
            return BinaryFormat::Unknown;
        }
        return if u16_le(head, off + 4) == Some(0x8664) {
            BinaryFormat::PeX8664
        } else {
            BinaryFormat::Unknown
        };
    }

    // Mach-O 64-bit, little endian: magic 0xfeedfacf, cputype at 0x04.
    if u32_le(head, 0) == Some(0xfeed_facf) {
        match u32_le(head, 4) {
            Some(CPU_ARM64) => return BinaryFormat::MachoArm64,
            Some(CPU_X86_64) => return BinaryFormat::MachoX8664,
            _ => {}
        }
    }

    // Universal (fat) Mach-O: the header is big-endian always. FAT_MAGIC
    // carries 20-byte fat_arch records, FAT_MAGIC_64 32-byte ones. Both
    // arches are required: a one-slice fat file is a thin binary wearing a
    // universal wrapper. Nothing is built universal any more; the detector
    // keeps the format so a wrong binary is refused naming what it is.
    if let Some(magic @ (0xcafe_babe | 0xcafe_babf)) = u32_be(head, 0) {
        let stride = if magic == 0xcafe_babf { 32 } else { 20 };
        let Some(count) = u32_be(head, 4) else {
            return BinaryFormat::Unknown;
        };
        // A sane bound, which also stops a Java class file (same magic) from
        // being read as a thousand arches.
        if !(1..=16).contains(&count) {
            return BinaryFormat::Unknown;
        }
        let mut cpus = Vec::new();
        for i in 0..count as usize {
            let Some(cpu) = u32_be(head, 8 + i * stride) else {
                return BinaryFormat::Unknown;
            };
            cpus.push(cpu);
        }
        if cpus.contains(&CPU_X86_64) && cpus.contains(&CPU_ARM64) {
            return BinaryFormat::MachoUniversal;
        }
        return BinaryFormat::Unknown;
    }

    BinaryFormat::Unknown
}

/// Read only the header, never the whole file.
pub fn read_binary_format(path: &Path) -> std::io::Result<BinaryFormat> {
    let mut head = Vec::with_capacity(HEAD_BYTES);
    std::fs::File::open(path)?
        .take(HEAD_BYTES as u64)
        .read_to_end(&mut head)?;
    Ok(detect_binary_format(&head))
}

//! The CLI's release targets (was build-cli.ts's TARGETS and MAC_TARGETS;
//! the Rust triples were tools/sidecar-targets.ts's, which retired with the
//! sidecar).
//!
//! Two tables, never mixed. [`TARGETS`] is the Linux and Windows set.
//! [`MAC_TARGETS`] is the Apple silicon tarball the Homebrew formula serves,
//! reachable ONLY with `--mac`, which also demands `--sign` or an explicit
//! `--unsigned`: an unsigned Mach-O is a Gatekeeper warning for everyone
//! who downloads it, so a build that never passes `--mac` cannot produce one
//! by accident. Intel Macs are not supported after 0.9.0, so there is no
//! x86_64-apple-darwin row.

use std::path::{Path, PathBuf};

use super::binfmt::BinaryFormat;

/// How a target's binary is packed for download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Packaging {
    TarGz,
    Zip,
}

/// One release target.
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    pub id: &'static str,
    /// The value for `cargo build --target`.
    pub rust_triple: &'static str,
    /// What the compiled output must be. Checked, never assumed.
    pub format: BinaryFormat,
    pub packaging: Packaging,
    /// The name the binary has, cargo's and inside the archive alike, so
    /// `tar -xzf` and `Expand-Archive` both drop a runnable `method-reader`
    /// in the current folder (what the Homebrew formula's `bin.install`
    /// wants).
    pub binary_name: &'static str,
    /// The old name, `screepub`: sp-cli's second binary, which says once on
    /// stderr that the tool was renamed and then does exactly what
    /// method-reader does. It rides in every archive beside the tool for a
    /// few releases, so scripts that call `screepub` keep working.
    pub alias_name: &'static str,
    pub archive_name: &'static str,
}

/// The Linux and Windows set. No darwin row, ever: see [`MAC_TARGETS`].
pub const TARGETS: &[Target] = &[
    Target {
        id: "linux-x64",
        rust_triple: "x86_64-unknown-linux-gnu",
        format: BinaryFormat::ElfX8664,
        packaging: Packaging::TarGz,
        binary_name: "method-reader",
        alias_name: "screepub",
        archive_name: "method-reader-cli-linux-x64.tar.gz",
    },
    Target {
        // Asahi, Raspberry Pi, ARM servers.
        id: "linux-arm64",
        rust_triple: "aarch64-unknown-linux-gnu",
        format: BinaryFormat::ElfAarch64,
        packaging: Packaging::TarGz,
        binary_name: "method-reader",
        alias_name: "screepub",
        archive_name: "method-reader-cli-linux-arm64.tar.gz",
    },
    Target {
        id: "windows-x64",
        rust_triple: "x86_64-pc-windows-msvc",
        format: BinaryFormat::PeX8664,
        packaging: Packaging::Zip,
        binary_name: "method-reader.exe",
        alias_name: "screepub.exe",
        archive_name: "method-reader-cli-windows-x64.zip",
    },
];

/// The Homebrew formula's one download: Apple silicon. The archive name is
/// the tap's (tools/bump-tap.sh), not ours to choose.
pub const MAC_TARGETS: &[Target] = &[Target {
    id: "macos-arm64",
    rust_triple: "aarch64-apple-darwin",
    format: BinaryFormat::MachoArm64,
    packaging: Packaging::TarGz,
    binary_name: "method-reader",
    alias_name: "screepub",
    archive_name: "method-reader-cli-macos-arm64.tar.gz",
}];

/// Whether a target is in the Mac table.
pub fn is_mac_target(target: &Target) -> bool {
    MAC_TARGETS.iter().any(|t| t.id == target.id)
}

/// A target of either table, by id.
pub fn find_target(id: &str) -> Option<&'static Target> {
    TARGETS.iter().chain(MAC_TARGETS).find(|t| t.id == id)
}

/// Per-target build folder: every target would otherwise land on the same
/// `method-reader` and overwrite the others.
pub fn build_dir(target: &Target, out_dir: &Path) -> PathBuf {
    out_dir.join(target.id)
}

/// Where the built binary sits for packaging.
pub fn binary_path(target: &Target, out_dir: &Path) -> PathBuf {
    build_dir(target, out_dir).join(target.binary_name)
}

/// Where the built alias (`screepub`) sits for packaging, beside it.
pub fn alias_path(target: &Target, out_dir: &Path) -> PathBuf {
    build_dir(target, out_dir).join(target.alias_name)
}

/// Archives sit flat in the output folder, so SHA256SUMS names them without
/// a path.
pub fn archive_path(target: &Target, out_dir: &Path) -> PathBuf {
    out_dir.join(target.archive_name)
}

/// Which cross target, if any, this machine can execute, from Rust's
/// `std::env::consts::{OS, ARCH}` spelling. A Mac answers `None` on purpose:
/// the cross table has no macOS row; see [`mac_target_id_for_host`].
pub fn target_id_for_host(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("linux-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("windows", "x86_64") => Some("windows-x64"),
        _ => None,
    }
}

/// The cross target this machine is, if any.
pub fn host_target() -> Option<&'static Target> {
    let id = target_id_for_host(std::env::consts::OS, std::env::consts::ARCH)?;
    TARGETS.iter().find(|t| t.id == id)
}

/// The Mac table's own host mapping. An Intel Mac answers nothing: nothing
/// it could build ships.
pub fn mac_target_id_for_host(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("macos-arm64"),
        _ => None,
    }
}

/// The Mac target this machine is, if any.
pub fn mac_host_target() -> Option<&'static Target> {
    let id = mac_target_id_for_host(std::env::consts::OS, std::env::consts::ARCH)?;
    MAC_TARGETS.iter().find(|t| t.id == id)
}

//! The artifacts a release publishes, as data: was the tables at the top of
//! tools/build-app-bundle.ts.
//!
//! Every installer kind (deb, rpm, dmg, nsis) and the one updater kind
//! (the macOS `.app.tar.gz`) with the container magic it must carry, the
//! size floor it must clear, the bundler folder it is found in and the
//! stable name it is published under, for the native egui window (sp-app).
//! (Until the Rust cutover the same rows served the Tauri window too, with
//! a 10 MB floor for the Bun engine it carried.)

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleOs {
    Linux,
    Macos,
    Windows,
}

impl BundleOs {
    pub fn name(self) -> &'static str {
        match self {
            BundleOs::Linux => "linux",
            BundleOs::Macos => "macos",
            BundleOs::Windows => "windows",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleArch {
    X64,
    Arm64,
}

impl BundleArch {
    pub fn name(self) -> &'static str {
        match self {
            BundleArch::X64 => "x64",
            BundleArch::Arm64 => "arm64",
        }
    }
}

/// Every bundle carries the window: egui, wgpu and the linked engine in
/// one binary (rust/crates/sp-app's `method-reader-app`). Not measured yet
/// (no release build of sp-app has been published); 2 MB is far under any
/// egui+wgpu binary and far over a truncated write. Raise it once the first
/// native bundle is measured.
pub const NATIVE_FLOOR: u64 = 2_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MagicAt {
    Head,
    /// A UDIF image's `koly` block is the LAST 512 bytes of the file.
    UdifTrailer,
}

/// One artifact kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kind {
    pub id: &'static str,
    pub os: BundleOs,
    /// The folder under `target/[<triple>/]release/bundle/`.
    pub dir: &'static str,
    pub ext: &'static str,
    pub magic: &'static [u8],
    pub magic_at: MagicAt,
    pub floor_bytes: u64,
}

const ARCH_MAGIC: &[u8] = b"!<arch>\n";
const RPM_MAGIC: &[u8] = &[0xed, 0xab, 0xee, 0xdb];
const MZ_MAGIC: &[u8] = b"MZ";
const KOLY_MAGIC: &[u8] = b"koly";
const GZIP_MAGIC: &[u8] = &[0x1f, 0x8b];

/// The one architecture a Mac bundle is built for, and its cargo triple.
pub const MAC_ARCH: BundleArch = BundleArch::Arm64;
pub const MAC_TARGET: &str = "aarch64-apple-darwin";

/// The installers. No AppImage here (the Bun-era table had none because
/// linuxdeploy corrupted the Bun engine; release.yml's sp-release makes the
/// Rust one), no MSI (NSIS installs per user with no administrator prompt).
pub const BUNDLE_KINDS: [Kind; 4] = [
    Kind {
        id: "deb",
        os: BundleOs::Linux,
        dir: "deb",
        ext: ".deb",
        magic: ARCH_MAGIC,
        magic_at: MagicAt::Head,
        floor_bytes: NATIVE_FLOOR,
    },
    Kind {
        id: "rpm",
        os: BundleOs::Linux,
        dir: "rpm",
        ext: ".rpm",
        magic: RPM_MAGIC,
        magic_at: MagicAt::Head,
        floor_bytes: NATIVE_FLOOR,
    },
    Kind {
        id: "dmg",
        os: BundleOs::Macos,
        dir: "dmg",
        ext: ".dmg",
        magic: KOLY_MAGIC,
        magic_at: MagicAt::UdifTrailer,
        floor_bytes: NATIVE_FLOOR,
    },
    Kind {
        id: "nsis",
        os: BundleOs::Windows,
        dir: "nsis",
        ext: ".exe",
        magic: MZ_MAGIC,
        magic_at: MagicAt::Head,
        floor_bytes: NATIVE_FLOOR,
    },
];

/// What the in-app updater downloads: the macOS `.app.tar.gz`, signed.
pub const UPDATER_KINDS: [Kind; 1] = [Kind {
    id: "app-tar",
    os: BundleOs::Macos,
    dir: "macos",
    ext: ".app.tar.gz",
    magic: GZIP_MAGIC,
    magic_at: MagicAt::Head,
    floor_bytes: NATIVE_FLOOR,
}];

pub const SIGNATURE_EXT: &str = ".sig";

/// The published name of `kind`: sp-release's names.rs, the one table
/// release.yml publishes by. No version, ever: pages link to
/// releases/latest/download/<name>. The arch is in the name only where more
/// than one is built (sp-release publishes x64 Linux packages only; the
/// arm64 names are this tool's, for a laptop build).
pub fn released_name(kind: &Kind, _version: &str, arch: BundleArch) -> String {
    use sp_release::names;
    let x64 = arch == BundleArch::X64;
    match kind.id {
        "deb" if x64 => names::LINUX_DEB_AMD64.into(),
        "deb" => "Method-Reader-linux-arm64.deb".into(),
        "rpm" if x64 => names::LINUX_RPM_X86_64.into(),
        "rpm" => "Method-Reader-linux-aarch64.rpm".into(),
        "dmg" => names::MAC_DMG.into(),
        "nsis" => names::WINDOWS_SETUP.into(),
        "app-tar" => names::MAC_UPDATE.into(),
        other => panic!("no published name for kind {other}"),
    }
}

/// The inverse of [`released_name`] for an updater kind: the arch a
/// published name serves, or None when it is not this kind's.
pub fn arch_of(kind: &Kind, released: &str) -> Option<BundleArch> {
    (kind.id == "app-tar" && released == sp_release::names::MAC_UPDATE).then_some(MAC_ARCH)
}

/// The `<os>-<arch>` keys of latest.json an updater kind answers for:
/// sp-release's UPDATER_KINDS row for its published name, the table the
/// manifest writer reads.
pub fn platform_keys(kind: &Kind, arch: BundleArch) -> Vec<&'static str> {
    if kind.id != "app-tar" {
        return Vec::new();
    }
    let name = released_name(kind, "", arch);
    sp_release::names::UPDATER_KINDS
        .iter()
        .find(|k| k.name == name)
        .map(|k| k.platform_keys.to_vec())
        .unwrap_or_default()
}

pub fn kinds_for_os(os: BundleOs) -> Vec<Kind> {
    BUNDLE_KINDS
        .iter()
        .copied()
        .filter(|k| k.os == os)
        .collect()
}

pub fn updater_kinds_for_os(os: BundleOs) -> Vec<Kind> {
    UPDATER_KINDS
        .iter()
        .copied()
        .filter(|k| k.os == os)
        .collect()
}

/// `<root>/target/[<triple>/]release/bundle/<dir>`, root being rust/.
pub fn bundle_dir_for(dir: &str, target: Option<&str>, root: &Path) -> PathBuf {
    let mut p = root.join("target");
    if let Some(t) = target {
        p.push(t);
    }
    p.join("release").join("bundle").join(dir)
}

/// The one file the bundler wrote, found by extension, never by
/// reconstructing its name. `rw.*` is bundle_dmg's crash leftover.
pub fn discover_artifact(dir: &Path, kind: &Kind) -> Result<PathBuf, String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Err(format!(
            "build-app-bundle: {}: no bundle directory at {}. The build reported success and produced nothing there.",
            kind.id,
            dir.display()
        ));
    };
    let mut found: Vec<String> = read
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(kind.ext) && !n.starts_with("rw."))
        .collect();
    found.sort();
    if found.len() != 1 {
        let listed = if found.is_empty() {
            String::new()
        } else {
            format!(" ({})", found.join(", "))
        };
        return Err(format!(
            "build-app-bundle: {}: expected exactly one {} in {}, found {}{listed}. A leftover from an earlier version is the usual cause; clear the directory and rebuild.",
            kind.id,
            kind.ext,
            dir.display(),
            found.len()
        ));
    }
    Ok(dir.join(&found[0]))
}

fn bytes_at(path: &Path, n: usize, position: u64) -> Vec<u8> {
    let Ok(mut f) = File::open(path) else {
        return Vec::new();
    };
    if f.seek(SeekFrom::Start(position)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::with_capacity(n);
    let _ = f.take(n as u64).read_to_end(&mut buf);
    buf
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Checks what was PRODUCED: present, over the floor, and carrying its
/// container's magic where that container keeps it.
pub fn verify_bundle_file(path: &Path, kind: &Kind) -> Result<(), String> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Err(format!(
            "build-app-bundle: {}: nothing at {}",
            kind.id,
            path.display()
        ));
    };
    let bytes = meta.len();
    if bytes < kind.floor_bytes {
        return Err(format!(
            "build-app-bundle: {}: {} is {bytes} bytes, under the {}-byte floor. Every bundle carries the whole window and its engine in one binary.",
            kind.id,
            file_name(path),
            kind.floor_bytes,
        ));
    }
    let window = match kind.magic_at {
        MagicAt::UdifTrailer => bytes_at(path, 512, bytes.saturating_sub(512)),
        MagicAt::Head => bytes_at(path, kind.magic.len(), 0),
    };
    let ok = window.len() >= kind.magic.len() && window[..kind.magic.len()] == *kind.magic;
    if !ok {
        let got: Vec<String> = window
            .iter()
            .take(kind.magic.len())
            .map(|b| format!("{b:02x}"))
            .collect();
        return Err(format!(
            "build-app-bundle: {}: {} does not carry the {} container magic at the {} (saw {}). A renamed file of another format is the usual cause.",
            kind.id,
            file_name(path),
            kind.id,
            if kind.magic_at == MagicAt::Head {
                "start"
            } else {
                "UDIF trailer"
            },
            got.join(" ")
        ));
    }
    Ok(())
}

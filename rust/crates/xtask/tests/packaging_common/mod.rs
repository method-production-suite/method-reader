//! Helpers the packaging tests share (each test file includes the ones it
//! uses; the rest are allowed to go unused there).
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// A scratch folder under the system temp dir, removed when dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "screepub-xtask-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, p: &str) -> PathBuf {
        self.0.join(p)
    }

    /// A new empty subfolder.
    pub fn fresh(&self, tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = self
            .0
            .join(format!("{tag}-{}", N.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&d).unwrap();
        d
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub const FAKE_KEY_ID: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

/// The key id as tauri and minisign print it.
pub fn key_id_hex(id: [u8; 8]) -> String {
    format!("{:X}", u64::from_le_bytes(id))
}

/// A `.sig` shaped exactly as tauri-cli writes one, with bytes that are not
/// a real signature (tests/signature-box.ts).
pub fn fake_signature_box(file_name: &str, key_id: [u8; 8]) -> String {
    let mut sig = b"ED".to_vec();
    sig.extend_from_slice(&key_id);
    sig.extend_from_slice(&[7u8; 64]);
    let text = format!(
        "untrusted comment: signature from tauri secret key\n{}\ntrusted comment: timestamp:1758470000\tfile:{file_name}\n{}\n",
        STANDARD.encode(&sig),
        STANDARD.encode([9u8; 64])
    );
    STANDARD.encode(text)
}

pub fn fake_public_key_box(key_id: [u8; 8]) -> String {
    let mut key = b"Ed".to_vec();
    key.extend_from_slice(&key_id);
    key.extend_from_slice(&[3u8; 32]);
    let text = format!(
        "untrusted comment: minisign public key: {}\n{}\n",
        key_id_hex(key_id),
        STANDARD.encode(&key)
    );
    STANDARD.encode(text)
}

pub fn b64(text: &str) -> String {
    STANDARD.encode(text)
}

pub fn unb64(text: &str) -> String {
    String::from_utf8(STANDARD.decode(text).unwrap()).unwrap()
}

/// The built `cargo xtask` binary, run with these arguments.
pub fn xtask(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Asserts `result` is an Err whose message contains `needle`.
#[track_caller]
pub fn err_contains<T: std::fmt::Debug>(result: Result<T, String>, needle: &str) -> String {
    match result {
        Ok(v) => panic!("expected an error containing {needle:?}, got Ok({v:?})"),
        Err(e) => {
            assert!(
                e.contains(needle),
                "error {e:?} does not contain {needle:?}"
            );
            e
        }
    }
}

// ---- hand-built containers (tests/bundle-archive.test.ts's builders) ----

pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(bytes).unwrap();
    enc.finish().unwrap()
}

/// A tar member: 512-byte header, octal mode and size, data padded to 512.
pub fn tar_member(name: &str, data: &[u8], mode: u32) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    h[..name.len()].copy_from_slice(name.as_bytes());
    let m = format!("{mode:07o}\0");
    h[100..108].copy_from_slice(m.as_bytes());
    let s = format!("{:011o}\0", data.len());
    h[124..136].copy_from_slice(s.as_bytes());
    h[148..156].copy_from_slice(b"        ");
    h[156] = b'0';
    h.extend_from_slice(data);
    h.resize(h.len() + (512 - data.len() % 512) % 512, 0);
    h
}

pub fn ar_member(name: &str, data: &[u8]) -> Vec<u8> {
    let mut h = vec![b' '; 60];
    h[..name.len()].copy_from_slice(name.as_bytes());
    let size = data.len().to_string();
    h[48..48 + size.len()].copy_from_slice(size.as_bytes());
    h[58] = 0x60;
    h[59] = 0x0a;
    h.extend_from_slice(data);
    if data.len() % 2 == 1 {
        h.push(b'\n');
    }
    h
}

pub fn cpio_member(name: &str, data: &[u8], mode: u32) -> Vec<u8> {
    let mut name_bytes = name.as_bytes().to_vec();
    name_bytes.push(0);
    let fields = [
        1,
        mode as usize,
        0,
        0,
        1,
        0,
        data.len(),
        0,
        0,
        0,
        0,
        name_bytes.len(),
        0,
    ];
    let mut out = b"070701".to_vec();
    for f in fields {
        out.extend_from_slice(format!("{f:08x}").as_bytes());
    }
    let name_pad = (4 - (out.len() + name_bytes.len()) % 4) % 4;
    out.extend_from_slice(&name_bytes);
    out.resize(out.len() + name_pad, 0);
    out.extend_from_slice(data);
    out.resize(out.len() + (4 - data.len() % 4) % 4, 0);
    out
}

pub fn cpio_trailer() -> Vec<u8> {
    cpio_member("TRAILER!!!", &[], 0)
}

pub fn rpm_header(nindex: usize, hsize: usize) -> Vec<u8> {
    let mut h = vec![0u8; 16 + nindex * 16 + hsize];
    h[..8].copy_from_slice(&[0x8e, 0xad, 0xe8, 0x01, 0, 0, 0, 0]);
    h[8..12].copy_from_slice(&(nindex as u32).to_be_bytes());
    h[12..16].copy_from_slice(&(hsize as u32).to_be_bytes());
    h
}

/// A 96-byte lead, a signature header padded to 8 (53 bytes, so the pad is
/// exercised), a 35-byte header, then the payload.
pub fn fake_rpm(payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 96];
    out[..8].copy_from_slice(&[0xed, 0xab, 0xee, 0xdb, 0x03, 0, 0, 0]);
    let sig = rpm_header(2, 5);
    let pad = (8 - sig.len() % 8) % 8;
    out.extend_from_slice(&sig);
    out.resize(out.len() + pad, 0);
    out.extend_from_slice(&rpm_header(1, 3));
    out.extend_from_slice(payload);
    out
}

pub fn concat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

/// One .deb holding one named executable file (smoke-bundle's fakeDeb).
pub fn fake_deb(path: &Path, name: &str, contents: &[u8]) {
    let tar = concat(&[&tar_member(name, contents, 0o755), &[0u8; 1024]]);
    fs::write(
        path,
        concat(&[
            b"!<arch>\n",
            &ar_member("debian-binary/  ", b"2.0\n"),
            &ar_member("control.tar.gz/ ", &gzip(&[0u8; 1024])),
            &ar_member("data.tar.gz/    ", &gzip(&tar)),
        ]),
    )
    .unwrap();
}

/// Bytes nothing compresses: a seeded xorshift stream.
pub fn noise(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    let mut out = Vec::with_capacity(n + 8);
    while out.len() < n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(n);
    out
}

/// tauri-cli 2.11.4's own test key (helpers/updater_signature.rs), published
/// in its source and encrypted with an empty password. Not a credential.
pub const TAURI_TEST_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5ClJXUlRZMEl5dkpDN09RZm5GeVAzc2RuYlNzWVVJelJRQnNIV2JUcGVXZUplWXZXYXpqUUFBQkFBQUFBQUFBQUFBQUlBQUFBQTZrN2RnWGh5dURxSzZiL1ZQSDdNcktiaHRxczQwMXdQelRHbjRNcGVlY1BLMTBxR2dpa3I3dDE1UTVDRDE4MXR4WlQwa1BQaXdxKy9UU2J2QmVSNXhOQWFDeG1GSVllbUNpTGJQRkhhTnROR3I5RmdUZi90OGtvaGhJS1ZTcjdZU0NyYzhQWlQ5cGM9Cg==";

/// The public half of [`TAURI_TEST_KEY`], as tauri.conf.json would carry it.
pub fn tauri_test_public_key() -> String {
    let sk = minisign::SecretKeyBox::from_string(&unb64(TAURI_TEST_KEY))
        .unwrap()
        .into_secret_key(Some(String::new()))
        .unwrap();
    let pk = minisign::PublicKey::from_secret_key(&sk).unwrap();
    STANDARD.encode(pk.to_box().unwrap().to_string())
}

// ---- the native window's repository and outside tools, faked ----

use xtask::packaging::bundle::{BundleArgs, BundleDirs, Env};
use xtask::packaging::kinds::{
    BUNDLE_KINDS, BundleArch, BundleOs, Kind, MAC_TARGET, MagicAt, NATIVE_FLOOR,
};
use xtask::packaging::native::binary_path;
use xtask::packaging::util::RunResult;

/// The version the native tests bundle.
pub const NATIVE_VERSION: &str = "0.12.0";

/// A repository with what build-app-bundle reads: the workspace version,
/// sp-app inheriting it, the icons and the resources. No desktop/.
pub fn native_repo(s: &Scratch, version: &str) -> BundleDirs {
    let repo = s.fresh("repo");
    fs::write(repo.join("LICENSE-MIT"), "MIT License").unwrap();
    fs::write(repo.join("LICENSE-APACHE"), "Apache License").unwrap();
    fs::create_dir_all(repo.join("rust/crates/sp-azw3")).unwrap();
    fs::write(
        repo.join("rust/crates/sp-azw3/LICENSE-GPL-3.0"),
        "GNU GENERAL PUBLIC LICENSE",
    )
    .unwrap();
    fs::write(repo.join("THIRD-PARTY-NOTICES.md"), "# Notices").unwrap();
    let rust = repo.join("rust");
    fs::create_dir_all(rust.join("crates/sp-app/icons")).unwrap();
    fs::write(
        rust.join("Cargo.toml"),
        format!("[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nedition = \"2024\"\nversion = \"{version}\"\n"),
    )
    .unwrap();
    fs::write(
        rust.join("crates/sp-app/Cargo.toml"),
        "[package]\nname = \"sp-app\"\nversion.workspace = true\n",
    )
    .unwrap();
    for icon in [
        "icon.icns",
        "icon.ico",
        "32x32.png",
        "128x128.png",
        "128x128@2x.png",
    ] {
        fs::write(
            rust.join("crates/sp-app/icons").join(icon),
            format!("icon bytes of {icon}"),
        )
        .unwrap();
    }
    BundleDirs::for_repo(&repo)
}

/// A row of the installer table.
pub fn native_kind(id: &str) -> Kind {
    *BUNDLE_KINDS.iter().find(|k| k.id == id).unwrap()
}

/// A file that passes `k`'s magic check and clears its floor.
pub fn plausible_file(path: &Path, k: &Kind) {
    use std::io::{Seek, SeekFrom, Write};
    let size = k.floor_bytes + 1024;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .unwrap();
    f.set_len(size).unwrap();
    f.seek(SeekFrom::Start(if k.magic_at == MagicAt::Head {
        0
    } else {
        size - 512
    }))
    .unwrap();
    f.write_all(k.magic).unwrap();
}

/// What the fake cargo builds: bytes over the floor nothing compresses.
pub fn the_binary() -> Vec<u8> {
    noise(NATIVE_FLOOR as usize + 65_536, 0x5EED)
}

/// Every outside tool the native path calls, faked: cargo writes the
/// binary, hdiutil and makensis write their containers, notarytool answers.
pub struct FakeTools {
    pub calls: Vec<Vec<String>>,
    pub rust: PathBuf,
    pub os: BundleOs,
    pub notary_status: &'static str,
    /// What hdiutil's image is: a plausible one, or this many zero bytes.
    pub dmg_bytes: Option<usize>,
}

impl FakeTools {
    pub fn new(dirs: &BundleDirs, os: BundleOs) -> Self {
        FakeTools {
            calls: vec![],
            rust: dirs.rust_dir.clone(),
            os,
            notary_status: "Accepted",
            dmg_bytes: None,
        }
    }

    pub fn run(&mut self, argv: &[String], _cwd: Option<&Path>) -> RunResult {
        self.calls.push(argv.to_vec());
        let tool = Path::new(&argv[0])
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        match tool.as_str() {
            "cargo" => {
                let target = argv
                    .iter()
                    .position(|a| a == "--target")
                    .map(|i| argv[i + 1].clone());
                let bin = binary_path(&self.rust, target.as_deref(), self.os);
                fs::create_dir_all(bin.parent().unwrap()).unwrap();
                fs::write(&bin, the_binary()).unwrap();
            }
            "hdiutil" => {
                let out = Path::new(argv.last().unwrap());
                match self.dmg_bytes {
                    Some(n) => fs::write(out, vec![0u8; n]).unwrap(),
                    None => plausible_file(out, &native_kind("dmg")),
                }
            }
            "makensis" => {
                let script = fs::read_to_string(&argv[1]).unwrap();
                let at = script.find("OutFile \"").unwrap() + 9;
                let out = &script[at..at + script[at..].find('"').unwrap()];
                plausible_file(Path::new(out), &native_kind("nsis"));
            }
            "xcrun" if argv[1] == "notarytool" => {
                return RunResult::ok(
                    &format!("{{\"id\":\"abc\",\"status\":\"{}\"}}", self.notary_status),
                    "",
                );
            }
            _ => {}
        }
        RunResult::ok("", "")
    }

    /// The tools called, in order (`xcrun <subcommand>` for xcrun).
    pub fn tools(&self) -> Vec<String> {
        self.calls
            .iter()
            .map(|c| {
                let t = Path::new(&c[0])
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                if t == "xcrun" {
                    format!("xcrun {}", c[1])
                } else {
                    t
                }
            })
            .collect()
    }
}

/// A run for `os` with nothing optional set (a Mac names its triple).
pub fn native_args(out: &Path, os: BundleOs, arch: BundleArch) -> BundleArgs {
    BundleArgs {
        target: (os == BundleOs::Macos).then(|| MAC_TARGET.to_string()),
        ..BundleArgs::new(NATIVE_VERSION, out, os, arch)
    }
}

pub fn env_of(pairs: &[(&str, &str)]) -> Env {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

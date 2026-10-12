//! Bundling the native window (rust/crates/sp-app's `screepub-window`):
//! what tauri-bundler did for the Tauri window, done here.
//!
//! The facts every package states live in one table, [`NATIVE_APP`], and
//! the ones the updater handover depends on are the Tauri window's own: the
//! bundle identifier, the product name, the .app's name, the Linux package
//! name and the Windows install folder. A 0.11.x window's updater downloads
//! `Screepub-macOS.app.tar.gz`, checks its signature against the key it
//! was built with, strips the archive's first path component and moves
//! what is left over its own `Screepub.app`; so the archive is one
//! `Screepub.app/` folder, signed with the same key.
//!
//! Pure Rust: the .app folder, Info.plist, the `.app.tar.gz` and its
//! signature, the .deb (ar + tar + gzip) and the .rpm (lead, headers, gzip
//! cpio). Outside tools, through the [`Runner`] seam and overridable by
//! environment variable (see [`NativeTools`]): `cargo`, and on a Mac
//! `codesign`, `ditto`, `xcrun notarytool`, `xcrun stapler` and `hdiutil`;
//! on Windows `makensis`.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::bundle::{BundleArgs, BundleDirs, Env};
use super::kinds::{BundleArch, BundleOs, SIGNATURE_EXT, bundle_dir_for};
use super::signature::{private_key_text, pubkey_from_config, sign_file, verify_file};
use super::util::{Runner, hex};

/// What every native package says about the app.
#[derive(Clone, Copy, Debug)]
pub struct NativeApp {
    pub product_name: &'static str,
    /// CFBundleIdentifier: the Tauri window's, so macOS sees one app.
    pub identifier: &'static str,
    /// The .deb and .rpm package name: the Tauri window's, so a package
    /// manager upgrades it in place.
    pub package: &'static str,
    pub crate_name: &'static str,
    /// What cargo builds.
    pub binary: &'static str,
    pub mac_executable: &'static str,
    pub linux_executable: &'static str,
    pub windows_executable: &'static str,
    pub publisher: &'static str,
    pub homepage: &'static str,
    pub license: &'static str,
    pub mac_category: &'static str,
    pub linux_categories: &'static str,
    pub short_description: &'static str,
    pub long_description: &'static str,
    pub minimum_macos: &'static str,
    /// (file in the repo, name in the bundle)
    pub resources: &'static [(&'static str, &'static str)],
    /// Where the icons are looked for, first match wins. desktop/ goes at
    /// cutover, so the icons must move first (see the report).
    pub icon_dirs: &'static [&'static str],
    /// Files the Tauri window installed on Windows that the native one does
    /// not replace, removed when the native installer runs over it.
    pub windows_leftovers: &'static [&'static str],
}

pub const NATIVE_APP: NativeApp = NativeApp {
    product_name: "Screepub",
    identifier: "com.darkwell.screepub",
    package: "screepub",
    crate_name: "sp-app",
    binary: "screepub-window",
    mac_executable: "Screepub",
    linux_executable: "screepub-window",
    windows_executable: "Screepub.exe",
    publisher: "Darkwell Entertainment LLC",
    homepage: "https://github.com/ssandweiss/screepub",
    license: "AGPL-3.0-or-later",
    mac_category: "public.app-category.productivity",
    linux_categories: "Office;",
    short_description: "Turn a screenplay PDF into a reflowable e-book.",
    long_description: "Screepub converts a screenplay PDF into an EPUB or MOBI that reflows properly on an e-reader, keeping cues with their dialogue and scenes with their headings, and copies the finished book onto a connected reader over USB.",
    minimum_macos: "13.0",
    resources: &[
        ("LICENSE", "LICENSE"),
        ("THIRD-PARTY-NOTICES.md", "THIRD-PARTY-NOTICES.md"),
    ],
    icon_dirs: &["rust/crates/sp-app/icons", "desktop/src-tauri/icons"],
    windows_leftovers: &["screepub-desktop.exe", "screepub-engine.exe"],
};

/// The outside programs, each overridable: SCREEPUB_CARGO, SCREEPUB_CODESIGN,
/// SCREEPUB_DITTO, SCREEPUB_XCRUN, SCREEPUB_HDIUTIL, SCREEPUB_MAKENSIS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTools {
    pub cargo: String,
    pub codesign: String,
    pub ditto: String,
    pub xcrun: String,
    pub hdiutil: String,
    pub makensis: String,
}

impl NativeTools {
    pub fn from_env(env: &Env) -> Self {
        let pick =
            |var: &str, default: &str| env.get(var).cloned().unwrap_or_else(|| default.into());
        NativeTools {
            cargo: pick("SCREEPUB_CARGO", "cargo"),
            codesign: pick("SCREEPUB_CODESIGN", "codesign"),
            ditto: pick("SCREEPUB_DITTO", "ditto"),
            xcrun: pick("SCREEPUB_XCRUN", "xcrun"),
            hdiutil: pick("SCREEPUB_HDIUTIL", "hdiutil"),
            makensis: pick("SCREEPUB_MAKENSIS", "makensis"),
        }
    }
}

fn io<T>(what: &str, r: std::io::Result<T>) -> Result<T, String> {
    r.map_err(|e| format!("build-app-bundle: {what}: {e}"))
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

fn p(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Runs one outside step, failing with its stderr.
fn step(
    run: &mut Runner<'_>,
    argv: Vec<String>,
    cwd: Option<&Path>,
    what: &str,
) -> Result<String, String> {
    let mut shown = argv.clone();
    for i in 1..shown.len() {
        if shown[i - 1] == "--password" {
            shown[i] = "***".into();
        }
    }
    println!("── {}", shown.join(" "));
    let r = run(&argv, cwd);
    if r.exit_code != 0 {
        return Err(format!(
            "build-app-bundle: {what} failed (exit {})\n{}",
            r.exit_code,
            r.stderr.trim()
        ));
    }
    Ok(r.stdout)
}

/// The cargo invocation that builds the window.
pub fn cargo_argv(tools: &NativeTools, target: Option<&str>) -> Vec<String> {
    let mut argv = vec![
        tools.cargo.clone(),
        "build".into(),
        "--release".into(),
        "--locked".into(),
        "--package".into(),
        NATIVE_APP.crate_name.into(),
        "--bin".into(),
        NATIVE_APP.binary.into(),
    ];
    if let Some(t) = target {
        argv.extend(["--target".into(), t.to_string()]);
    }
    argv
}

/// Where cargo leaves the binary.
pub fn binary_path(rust_dir: &Path, target: Option<&str>, os: BundleOs) -> PathBuf {
    let mut dir = rust_dir.join("target");
    if let Some(t) = target {
        dir.push(t);
    }
    let exe = if os == BundleOs::Windows {
        format!("{}.exe", NATIVE_APP.binary)
    } else {
        NATIVE_APP.binary.to_string()
    };
    dir.join("release").join(exe)
}

/// A version every package format accepts: a prerelease's `-` becomes `~`,
/// which sorts before the release in both dpkg and rpm.
pub fn package_version(version: &str) -> String {
    version.replacen('-', "~", 1)
}

/// The .app's Info.plist.
pub fn info_plist(version: &str) -> String {
    let a = NATIVE_APP;
    let pairs: [(&str, &str); 11] = [
        ("CFBundleDevelopmentRegion", "en"),
        ("CFBundleDisplayName", a.product_name),
        ("CFBundleExecutable", a.mac_executable),
        ("CFBundleIconFile", "icon.icns"),
        ("CFBundleIdentifier", a.identifier),
        ("CFBundleInfoDictionaryVersion", "6.0"),
        ("CFBundleName", a.product_name),
        ("CFBundlePackageType", "APPL"),
        ("CFBundleShortVersionString", version),
        ("CFBundleVersion", version),
        ("LSApplicationCategoryType", a.mac_category),
    ];
    let mut body = String::new();
    for (k, v) in pairs {
        body.push_str(&format!(
            "\t<key>{k}</key>\n\t<string>{}</string>\n",
            xml(v)
        ));
    }
    body.push_str(&format!(
        "\t<key>LSMinimumSystemVersion</key>\n\t<string>{}</string>\n",
        xml(a.minimum_macos)
    ));
    body.push_str("\t<key>NSHighResolutionCapable</key>\n\t<true/>\n");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n{body}</dict>\n</plist>\n"
    )
}

/// One string value of an Info.plist, as [`info_plist`] writes them.
pub fn plist_string(plist: &str, key: &str) -> Option<String> {
    let at = plist.find(&format!("<key>{key}</key>"))?;
    let rest = &plist[at..];
    let open = rest.find("<string>")? + "<string>".len();
    let close = rest[open..].find("</string>")? + open;
    Some(
        rest[open..close]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&amp;", "&"),
    )
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn find_icons(repo: &Path, needed: &[&str]) -> Result<PathBuf, String> {
    for dir in NATIVE_APP.icon_dirs {
        let d = repo.join(dir);
        if needed.iter().all(|n| d.join(n).is_file()) {
            return Ok(d);
        }
    }
    Err(format!(
        "build-app-bundle: no icon folder holds {} (looked in {})",
        needed.join(", "),
        NATIVE_APP.icon_dirs.join(", ")
    ))
}

fn resource_bytes(repo: &Path) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    NATIVE_APP
        .resources
        .iter()
        .map(|(from, to)| {
            io(
                &format!("cannot read {from}"),
                std::fs::read(repo.join(from)),
            )
            .map(|b| (*to, b))
        })
        .collect()
}

fn fresh_dir(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        io(
            &format!("cannot clear {}", dir.display()),
            std::fs::remove_dir_all(dir),
        )?;
    }
    io(
        &format!("cannot create {}", dir.display()),
        std::fs::create_dir_all(dir),
    )
}

fn write_file(path: &Path, bytes: &[u8], executable: bool) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        io(
            &format!("cannot create {}", parent.display()),
            std::fs::create_dir_all(parent),
        )?;
    }
    io(
        &format!("cannot write {}", path.display()),
        std::fs::write(path, bytes),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        io(
            &format!("cannot set the mode of {}", path.display()),
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)),
        )?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    Ok(())
}

/// Assembles `<dir>/Screepub.app` around `binary`.
pub fn assemble_app(
    dir: &Path,
    binary: &[u8],
    version: &str,
    repo: &Path,
) -> Result<PathBuf, String> {
    let icons = find_icons(repo, &["icon.icns"])?;
    let app = dir.join(format!("{}.app", NATIVE_APP.product_name));
    fresh_dir(&app)?;
    let contents = app.join("Contents");
    write_file(
        &contents.join("Info.plist"),
        info_plist(version).as_bytes(),
        false,
    )?;
    write_file(&contents.join("PkgInfo"), b"APPL????", false)?;
    write_file(
        &contents.join("MacOS").join(NATIVE_APP.mac_executable),
        binary,
        true,
    )?;
    let icns = io(
        "cannot read icon.icns",
        std::fs::read(icons.join("icon.icns")),
    )?;
    write_file(&contents.join("Resources").join("icon.icns"), &icns, false)?;
    for (name, bytes) in resource_bytes(repo)? {
        write_file(&contents.join("Resources").join(name), &bytes, false)?;
    }
    Ok(app)
}

/// One file a package installs: its absolute path without the leading `/`.
#[derive(Clone, Debug)]
pub struct PkgFile {
    pub path: String,
    pub mode: u32,
    pub data: Vec<u8>,
}

/// What the Linux packages install.
pub fn linux_files(binary: &[u8], repo: &Path) -> Result<Vec<PkgFile>, String> {
    let a = NATIVE_APP;
    let icons = find_icons(repo, &["32x32.png", "128x128.png", "128x128@2x.png"])?;
    let mut files = vec![PkgFile {
        path: format!("usr/bin/{}", a.linux_executable),
        mode: 0o755,
        data: binary.to_vec(),
    }];
    files.push(PkgFile {
        path: format!("usr/share/applications/{}.desktop", a.product_name),
        mode: 0o644,
        data: desktop_entry().into_bytes(),
    });
    for (file, size) in [
        ("32x32.png", "32x32"),
        ("128x128.png", "128x128"),
        ("128x128@2x.png", "256x256@2"),
    ] {
        files.push(PkgFile {
            path: format!("usr/share/icons/hicolor/{size}/apps/{}.png", a.package),
            mode: 0o644,
            data: io(
                &format!("cannot read {file}"),
                std::fs::read(icons.join(file)),
            )?,
        });
    }
    for (name, bytes) in resource_bytes(repo)? {
        files.push(PkgFile {
            path: format!("usr/lib/{}/{name}", a.product_name),
            mode: 0o644,
            data: bytes,
        });
    }
    Ok(files)
}

pub fn desktop_entry() -> String {
    let a = NATIVE_APP;
    format!(
        "[Desktop Entry]\nCategories={}\nComment={}\nExec={}\nStartupWMClass={}\nIcon={}\nName={}\nTerminal=false\nType=Application\n",
        a.linux_categories,
        a.short_description,
        a.linux_executable,
        a.product_name,
        a.package,
        a.product_name
    )
}

fn deb_arch(arch: BundleArch) -> &'static str {
    if arch == BundleArch::X64 {
        "amd64"
    } else {
        "arm64"
    }
}

fn rpm_arch(arch: BundleArch) -> &'static str {
    if arch == BundleArch::X64 {
        "x86_64"
    } else {
        "aarch64"
    }
}

/// The parent folders of every file, sorted, each once.
fn parent_dirs(files: &[PkgFile]) -> Vec<String> {
    let mut dirs = BTreeSet::new();
    for f in files {
        let mut acc = String::new();
        let parts: Vec<&str> = f.path.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            acc.push_str(part);
            acc.push('/');
            dirs.insert(acc.clone());
        }
    }
    dirs.into_iter().collect()
}

fn tar_header(
    path: &str,
    size: u64,
    mode: u32,
    mtime: u64,
    dir: bool,
) -> Result<tar::Header, String> {
    let mut h = tar::Header::new_gnu();
    h.set_path(path)
        .map_err(|e| format!("build-app-bundle: tar path {path}: {e}"))?;
    h.set_size(size);
    h.set_mode(mode);
    h.set_mtime(mtime);
    h.set_uid(0);
    h.set_gid(0);
    let _ = h.set_username("root");
    let _ = h.set_groupname("root");
    h.set_entry_type(if dir {
        tar::EntryType::Directory
    } else {
        tar::EntryType::Regular
    });
    h.set_cksum();
    Ok(h)
}

fn gzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    io("gzip", enc.write_all(bytes))?;
    io("gzip", enc.finish())
}

/// A gzip tar of `files` under `./`, folders first.
fn tar_gz(files: &[PkgFile], mtime: u64) -> Result<Vec<u8>, String> {
    let mut b = tar::Builder::new(Vec::new());
    io(
        "tar",
        b.append(&tar_header("./", 0, 0o755, mtime, true)?, &[][..]),
    )?;
    for d in parent_dirs(files) {
        io(
            "tar",
            b.append(
                &tar_header(&format!("./{d}"), 0, 0o755, mtime, true)?,
                &[][..],
            ),
        )?;
    }
    for f in files {
        let h = tar_header(
            &format!("./{}", f.path),
            f.data.len() as u64,
            f.mode,
            mtime,
            false,
        )?;
        io("tar", b.append(&h, f.data.as_slice()))?;
    }
    gzip(&io("tar", b.into_inner())?)
}

fn ar_member(name: &str, data: &[u8], mtime: u64) -> Vec<u8> {
    let mut out = format!(
        "{name:<16}{mtime:<12}{:<6}{:<6}{:<8}{:<10}`\n",
        0,
        0,
        "100644",
        data.len()
    )
    .into_bytes();
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(b'\n');
    }
    out
}

/// The .deb's control file.
pub fn deb_control(version: &str, arch: BundleArch, files: &[PkgFile]) -> String {
    let a = NATIVE_APP;
    let installed: u64 = files
        .iter()
        .map(|f| f.data.len() as u64)
        .sum::<u64>()
        .div_ceil(1024);
    format!(
        "Package: {}\nVersion: {}\nArchitecture: {}\nInstalled-Size: {installed}\nMaintainer: {}\nPriority: optional\nSection: text\nHomepage: {}\nDescription: {}\n {}\n",
        a.package,
        package_version(version),
        deb_arch(arch),
        a.publisher,
        a.homepage,
        a.short_description,
        a.long_description
    )
}

/// A .deb: `debian-binary`, `control.tar.gz` and `data.tar.gz` in an ar.
pub fn deb_bytes(
    version: &str,
    arch: BundleArch,
    files: &[PkgFile],
    mtime: u64,
) -> Result<Vec<u8>, String> {
    let control = PkgFile {
        path: "control".into(),
        mode: 0o644,
        data: deb_control(version, arch, files).into_bytes(),
    };
    let mut out = b"!<arch>\n".to_vec();
    out.extend(ar_member("debian-binary", b"2.0\n", mtime));
    out.extend(ar_member(
        "control.tar.gz",
        &tar_gz(&[control], mtime)?,
        mtime,
    ));
    out.extend(ar_member("data.tar.gz", &tar_gz(files, mtime)?, mtime));
    Ok(out)
}

// ---- rpm ----

enum Val {
    I16(Vec<u16>),
    I32(Vec<u32>),
    Str(String),
    StrArr(Vec<String>),
    I18n(String),
}

impl Val {
    fn kind(&self) -> u32 {
        match self {
            Val::I16(_) => 3,
            Val::I32(_) => 4,
            Val::Str(_) => 6,
            Val::StrArr(_) => 8,
            Val::I18n(_) => 9,
        }
    }
}

/// An rpm header with its region (62 for the signature header, 63 for the
/// main one): index sorted by tag, data aligned by type.
fn rpm_header(mut entries: Vec<(u32, Val)>, region: u32) -> Vec<u8> {
    entries.sort_by_key(|(t, _)| *t);
    let mut store: Vec<u8> = Vec::new();
    let mut index: Vec<[u32; 4]> = Vec::new();
    for (tag, val) in &entries {
        let align = match val {
            Val::I16(_) => 2,
            Val::I32(_) => 4,
            _ => 1,
        };
        while !store.len().is_multiple_of(align) {
            store.push(0);
        }
        let offset = store.len() as u32;
        let count = match val {
            Val::I16(v) => {
                v.iter()
                    .for_each(|x| store.extend_from_slice(&x.to_be_bytes()));
                v.len()
            }
            Val::I32(v) => {
                v.iter()
                    .for_each(|x| store.extend_from_slice(&x.to_be_bytes()));
                v.len()
            }
            Val::Str(s) | Val::I18n(s) => {
                store.extend_from_slice(s.as_bytes());
                store.push(0);
                1
            }
            Val::StrArr(v) => {
                for s in v {
                    store.extend_from_slice(s.as_bytes());
                    store.push(0);
                }
                v.len()
            }
        };
        index.push([*tag, val.kind(), offset, count as u32]);
    }
    // The region: an index entry first, pointing at a 16-byte trailer at the
    // end of the store whose offset is minus the size of the whole index.
    let nindex = index.len() as u32 + 1;
    let trailer_at = store.len() as u32;
    store.extend_from_slice(&region.to_be_bytes());
    store.extend_from_slice(&7u32.to_be_bytes());
    store.extend_from_slice(&(-((nindex * 16) as i32)).to_be_bytes());
    store.extend_from_slice(&16u32.to_be_bytes());
    index.insert(0, [region, 7, trailer_at, 16]);

    let mut out = vec![0x8e, 0xad, 0xe8, 0x01, 0, 0, 0, 0];
    out.extend_from_slice(&nindex.to_be_bytes());
    out.extend_from_slice(&(store.len() as u32).to_be_bytes());
    for e in &index {
        for x in e {
            out.extend_from_slice(&x.to_be_bytes());
        }
    }
    out.extend_from_slice(&store);
    out
}

fn cpio_newc(files: &[PkgFile], mtime: u64) -> Vec<u8> {
    let mut out = Vec::new();
    let entry = |out: &mut Vec<u8>, ino: usize, name: &str, mode: u32, data: &[u8]| {
        let name_size = name.len() + 1;
        let fields = [
            ino,
            mode as usize,
            0,
            0,
            1,
            mtime as usize,
            data.len(),
            0,
            0,
            0,
            0,
            name_size,
            0,
        ];
        out.extend_from_slice(b"070701");
        for f in fields {
            out.extend_from_slice(format!("{f:08x}").as_bytes());
        }
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        out.extend_from_slice(data);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    };
    for (i, f) in files.iter().enumerate() {
        entry(
            &mut out,
            i + 1,
            &format!("./{}", f.path),
            0o100000 | f.mode,
            &f.data,
        );
    }
    entry(&mut out, 0, "TRAILER!!!", 0, &[]);
    out
}

/// An .rpm (format 3 lead, v4 headers with regions, gzip cpio payload).
pub fn rpm_bytes(
    version: &str,
    arch: BundleArch,
    files: &[PkgFile],
    mtime: u64,
) -> Result<Vec<u8>, String> {
    let a = NATIVE_APP;
    let ver = package_version(version);
    let release = "1";
    // rpm looks files up by path with a binary search, so the header's file
    // list (and the payload, to match) is sorted, as rpmbuild sorts it.
    let mut sorted = files.to_vec();
    sorted.sort_by(|x, y| x.path.cmp(&y.path));
    let files = sorted.as_slice();
    let n = files.len();
    let mut dirnames: Vec<String> = Vec::new();
    let mut dirindexes = Vec::new();
    let mut basenames = Vec::new();
    for f in files {
        let (dir, base) = f.path.rsplit_once('/').unwrap_or(("", &f.path));
        let dir = format!("/{dir}/");
        let at = dirnames.iter().position(|d| *d == dir).unwrap_or_else(|| {
            dirnames.push(dir);
            dirnames.len() - 1
        });
        dirindexes.push(at as u32);
        basenames.push(base.to_string());
    }
    let digests: Vec<String> = files
        .iter()
        .map(|f| hex(&Sha256::digest(&f.data)))
        .collect();
    let total: u64 = files.iter().map(|f| f.data.len() as u64).sum();
    let rpmlib = [
        ("rpmlib(CompressedFileNames)", "3.0.4-1"),
        ("rpmlib(FileDigests)", "4.6.0-1"),
        ("rpmlib(PayloadFilesHavePrefix)", "4.0-1"),
    ];
    // RPMSENSE_LESS | RPMSENSE_EQUAL | RPMSENSE_RPMLIB.
    let rpmlib_flags = 0x02 | 0x08 | 0x0100_0000;
    let s = |x: &str| Val::Str(x.to_string());
    let arr = |v: Vec<String>| Val::StrArr(v);
    let main = rpm_header(
        vec![
            // HEADERI18NTABLE: the one locale the I18N strings are in.
            (100, arr(vec!["C".into()])),
            (1000, s(a.package)),
            (1001, s(&ver)),
            (1002, s(release)),
            (1004, Val::I18n(a.short_description.into())),
            (1005, Val::I18n(a.long_description.into())),
            (1006, Val::I32(vec![mtime as u32])),
            (1009, Val::I32(vec![total as u32])),
            (1014, s(a.license)),
            (1016, Val::I18n("Unspecified".into())),
            (1020, s(a.homepage)),
            (1021, s("linux")),
            (1022, s(rpm_arch(arch))),
            (
                1028,
                Val::I32(files.iter().map(|f| f.data.len() as u32).collect()),
            ),
            (
                1030,
                Val::I16(files.iter().map(|f| (0o100000 | f.mode) as u16).collect()),
            ),
            (1033, Val::I16(vec![0; n])),
            (1034, Val::I32(vec![mtime as u32; n])),
            (1035, arr(digests)),
            (1036, arr(vec![String::new(); n])),
            (1037, Val::I32(vec![0; n])),
            (1039, arr(vec!["root".into(); n])),
            (1040, arr(vec!["root".into(); n])),
            (
                1047,
                arr(vec![
                    a.package.into(),
                    format!(
                        "{}({})",
                        a.package,
                        if arch == BundleArch::X64 {
                            "x86-64"
                        } else {
                            "aarch-64"
                        }
                    ),
                ]),
            ),
            (1048, Val::I32(vec![rpmlib_flags; rpmlib.len()])),
            (
                1049,
                arr(rpmlib.iter().map(|(n, _)| n.to_string()).collect()),
            ),
            (
                1050,
                arr(rpmlib.iter().map(|(_, v)| v.to_string()).collect()),
            ),
            (1064, s("4.0")),
            (1095, Val::I32(vec![1; n])),
            (1096, Val::I32((1..=n as u32).collect())),
            (1097, arr(vec![String::new(); n])),
            (1112, Val::I32(vec![8, 8])),
            (
                1113,
                arr(vec![format!("{ver}-{release}"), format!("{ver}-{release}")]),
            ),
            (1116, Val::I32(dirindexes)),
            (1117, arr(basenames)),
            (1118, arr(dirnames)),
            (1124, s("cpio")),
            (1125, s("gzip")),
            (1126, s("9")),
            (5011, Val::I32(vec![8])),
        ],
        63,
    );
    let cpio = cpio_newc(files, mtime);
    let payload = gzip(&cpio)?;
    let sig = rpm_header(
        vec![
            (273, Val::Str(hex(&Sha256::digest(&main)))),
            (1000, Val::I32(vec![(main.len() + payload.len()) as u32])),
            (1007, Val::I32(vec![cpio.len() as u32])),
        ],
        62,
    );

    let mut lead = vec![0u8; 96];
    lead[..4].copy_from_slice(&[0xed, 0xab, 0xee, 0xdb]);
    lead[4] = 3; // major
    lead[8..10].copy_from_slice(&(if arch == BundleArch::X64 { 1u16 } else { 19 }).to_be_bytes());
    let name = format!("{}-{ver}-{release}", a.package);
    let nb = name.as_bytes();
    lead[10..10 + nb.len().min(65)].copy_from_slice(&nb[..nb.len().min(65)]);
    lead[76..78].copy_from_slice(&1u16.to_be_bytes()); // linux
    lead[78..80].copy_from_slice(&5u16.to_be_bytes()); // header-style signature

    let mut out = lead;
    out.extend_from_slice(&sig);
    while !out.len().is_multiple_of(8) {
        out.push(0);
    }
    out.extend_from_slice(&main);
    out.extend_from_slice(&payload);
    Ok(out)
}

// ---- Windows ----

/// NSIS's VIProductVersion wants four numbers.
fn four_part(version: &str) -> String {
    let core = version.split('-').next().unwrap_or(version);
    format!("{core}.0")
}

/// The installer script: per user, no administrator prompt, into the same
/// folder the Tauri window's NSIS installer used, over its files.
pub fn nsis_script(version: &str, stage: &Path, out_file: &Path) -> String {
    let a = NATIVE_APP;
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let mut files = format!("  File \"{}\"\n", s(&stage.join(a.windows_executable)));
    for (_, name) in a.resources {
        files.push_str(&format!("  File \"{}\"\n", s(&stage.join(name))));
    }
    let mut leftovers = String::new();
    for old in a.windows_leftovers {
        leftovers.push_str(&format!("  Delete \"$INSTDIR\\{old}\"\n"));
    }
    let mut removes = format!("  Delete \"$INSTDIR\\{}\"\n", a.windows_executable);
    for (_, name) in a.resources {
        removes.push_str(&format!("  Delete \"$INSTDIR\\{name}\"\n"));
    }
    format!(
        r#"; GENERATED by cargo xtask build-app-bundle. Do not edit.
Unicode true
!define PRODUCTNAME "{product}"
!define VERSION "{version}"
!define PUBLISHER "{publisher}"
!define IDENTIFIER "{identifier}"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${{PRODUCTNAME}}"
Name "${{PRODUCTNAME}}"
OutFile "{out}"
InstallDir "$LOCALAPPDATA\${{PRODUCTNAME}}"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "{icon}"
VIProductVersion "{four}"
VIAddVersionKey "ProductName" "${{PRODUCTNAME}}"
VIAddVersionKey "CompanyName" "${{PUBLISHER}}"
VIAddVersionKey "FileDescription" "{description}"
VIAddVersionKey "FileVersion" "${{VERSION}}"
VIAddVersionKey "ProductVersion" "${{VERSION}}"
Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  SetOutPath "$INSTDIR"
{leftovers}{files}  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\${{PRODUCTNAME}}.lnk" "$INSTDIR\{exe}"
  WriteRegStr HKCU "${{UNINSTKEY}}" "DisplayName" "${{PRODUCTNAME}}"
  WriteRegStr HKCU "${{UNINSTKEY}}" "DisplayVersion" "${{VERSION}}"
  WriteRegStr HKCU "${{UNINSTKEY}}" "Publisher" "${{PUBLISHER}}"
  WriteRegStr HKCU "${{UNINSTKEY}}" "DisplayIcon" "$INSTDIR\{exe}"
  WriteRegStr HKCU "${{UNINSTKEY}}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${{UNINSTKEY}}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegDWORD HKCU "${{UNINSTKEY}}" "NoModify" 1
  WriteRegDWORD HKCU "${{UNINSTKEY}}" "NoRepair" 1
SectionEnd

Section "Uninstall"
{removes}  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${{PRODUCTNAME}}.lnk"
  DeleteRegKey HKCU "${{UNINSTKEY}}"
SectionEnd
"#,
        product = a.product_name,
        publisher = a.publisher,
        identifier = a.identifier,
        out = s(out_file),
        icon = s(&stage.join("icon.ico")),
        four = four_part(version),
        description = a.short_description,
        exe = a.windows_executable,
    )
}

// ---- the updater archive ----

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries: Vec<PathBuf> = io(
        &format!("cannot list {}", dir.display()),
        std::fs::read_dir(dir),
    )?
    .filter_map(Result::ok)
    .map(|e| e.path())
    .collect();
    entries.sort();
    for e in entries {
        out.push(e.clone());
        if e.is_dir() {
            walk(&e, out)?;
        }
    }
    Ok(())
}

/// The `.app.tar.gz` the updater downloads: one `<name>.app/` folder, every
/// file under it 0644 except Contents/MacOS/*, which is 0755, whatever the
/// host's file system says (a Windows host has no mode bits to read).
pub fn app_tar_gz(app: &Path, mtime: u64) -> Result<Vec<u8>, String> {
    let root = app
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("build-app-bundle: the .app has no name")?;
    let mut paths = Vec::new();
    walk(app, &mut paths)?;
    let mut b = tar::Builder::new(Vec::new());
    io(
        "tar",
        b.append(
            &tar_header(&format!("{root}/"), 0, 0o755, mtime, true)?,
            &[][..],
        ),
    )?;
    for path in paths {
        let rel = path
            .strip_prefix(app)
            .map_err(|e| e.to_string())?
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let meta = io("stat", std::fs::symlink_metadata(&path))?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "build-app-bundle: {} is a symlink; the native .app is assembled without any",
                path.display()
            ));
        }
        if meta.is_dir() {
            io(
                "tar",
                b.append(
                    &tar_header(&format!("{root}/{rel}/"), 0, 0o755, mtime, true)?,
                    &[][..],
                ),
            )?;
        } else {
            let data = io("read", std::fs::read(&path))?;
            let mode = if rel.starts_with("Contents/MacOS/") {
                0o755
            } else {
                0o644
            };
            io(
                "tar",
                b.append(
                    &tar_header(
                        &format!("{root}/{rel}"),
                        data.len() as u64,
                        mode,
                        mtime,
                        false,
                    )?,
                    data.as_slice(),
                ),
            )?;
        }
    }
    gzip(&io("tar", b.into_inner())?)
}

/// The public key the shipped app trusts: `--pubkey` (a path or the text),
/// else tauri.conf.json's.
pub fn trusted_pubkey(args: &BundleArgs, dirs: &BundleDirs) -> Result<String, String> {
    if let Some(k) = &args.pubkey {
        return private_key_text(k).map(|t| t.trim().to_string());
    }
    let conf = dirs.desktop_dir.join("tauri.conf.json");
    if conf.is_file() {
        return pubkey_from_config(&conf);
    }
    Err("build-app-bundle: --updater needs the public key the shipped app trusts, to check the archive's signature before anything is published: pass --pubkey <file or base64> (desktop/src-tauri/tauri.conf.json, which carried it, is gone).".into())
}

fn source_date(env: &Env) -> u64 {
    env.get("SOURCE_DATE_EPOCH")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        })
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    io(
        &format!("cannot create {}", to.display()),
        std::fs::create_dir_all(to),
    )?;
    for e in io("list", std::fs::read_dir(from))?.filter_map(Result::ok) {
        let src = e.path();
        let dst = to.join(e.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            io(
                &format!("cannot copy {}", src.display()),
                std::fs::copy(&src, &dst),
            )?;
        }
    }
    Ok(())
}

/// The notarytool credentials tauri-bundler read: an App Store Connect API
/// key, or an Apple ID with an app-specific password.
pub fn notary_credentials(env: &Env) -> Option<Vec<String>> {
    let get = |k: &str| env.get(k).filter(|v| !v.is_empty()).cloned();
    if let (Some(key), Some(issuer), Some(path)) = (
        get("APPLE_API_KEY"),
        get("APPLE_API_ISSUER"),
        get("APPLE_API_KEY_PATH"),
    ) {
        return Some(vec![
            "--key".into(),
            path,
            "--key-id".into(),
            key,
            "--issuer".into(),
            issuer,
        ]);
    }
    if let (Some(id), Some(pw), Some(team)) =
        (get("APPLE_ID"), get("APPLE_PASSWORD"), get("APPLE_TEAM_ID"))
    {
        return Some(vec![
            "--apple-id".into(),
            id,
            "--password".into(),
            pw,
            "--team-id".into(),
            team,
        ]);
    }
    None
}

fn build_mac(
    args: &BundleArgs,
    run: &mut Runner<'_>,
    dirs: &BundleDirs,
    env: &Env,
    tools: &NativeTools,
    binary: &[u8],
) -> Result<(), String> {
    let target = args.target.as_deref();
    let macos = bundle_dir_for("macos", target, &dirs.rust_dir);
    let app = assemble_app(&macos, binary, &args.version, &dirs.repo_dir)?;
    let identity = env
        .get("APPLE_SIGNING_IDENTITY")
        .filter(|v| !v.is_empty())
        .cloned();

    // Hardened runtime and a timestamp with a real identity, which
    // notarization requires; ad hoc otherwise, so a local build still opens.
    let sign = match &identity {
        Some(id) => vec![
            tools.codesign.clone(),
            "--force".into(),
            "--options".into(),
            "runtime".into(),
            "--timestamp".into(),
            "--sign".into(),
            id.clone(),
            p(&app),
        ],
        None => vec![
            tools.codesign.clone(),
            "--force".into(),
            "--sign".into(),
            "-".into(),
            p(&app),
        ],
    };
    step(run, sign, None, "codesign")?;

    if let (Some(_), Some(creds)) = (&identity, notary_credentials(env)) {
        let zip = macos.join(format!("{}.zip", NATIVE_APP.product_name));
        step(
            run,
            vec![
                tools.ditto.clone(),
                "-c".into(),
                "-k".into(),
                "--keepParent".into(),
                p(&app),
                p(&zip),
            ],
            None,
            "ditto",
        )?;
        let mut submit = vec![
            tools.xcrun.clone(),
            "notarytool".into(),
            "submit".into(),
            p(&zip),
        ];
        submit.extend(creds);
        submit.extend(strings(&["--wait", "--output-format", "json"]));
        let out = step(run, submit, None, "notarytool submit")?;
        let status = serde_json::from_str::<serde_json::Value>(out.trim())
            .ok()
            .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(str::to_string));
        if status.as_deref() != Some("Accepted") {
            return Err(format!(
                "build-app-bundle: notarization did not accept {} (status {}). `xcrun notarytool log <id>` says why.\n{}",
                p(&app),
                status.as_deref().unwrap_or("<none>"),
                out.trim()
            ));
        }
        let _ = std::fs::remove_file(&zip);
        step(
            run,
            vec![
                tools.xcrun.clone(),
                "stapler".into(),
                "staple".into(),
                p(&app),
            ],
            None,
            "stapler staple",
        )?;
    }

    if args.updater {
        let pubkey = trusted_pubkey(args, dirs)?;
        let tar_path = macos.join(format!("{}.app.tar.gz", NATIVE_APP.product_name));
        let sig_path = PathBuf::from(format!("{}{SIGNATURE_EXT}", tar_path.display()));
        let _ = std::fs::remove_file(&sig_path);
        write_file(&tar_path, &app_tar_gz(&app, source_date(env))?, false)?;
        let key = private_key_text(
            env.get("TAURI_SIGNING_PRIVATE_KEY")
                .map(String::as_str)
                .unwrap_or(""),
        )?;
        let password = env
            .get("TAURI_SIGNING_PRIVATE_KEY_PASSWORD")
            .cloned()
            .unwrap_or_default();
        let sig = sign_file(&tar_path, &key, &password, source_date(env))?;
        // Stricter than tauri-cli, which only warned: an archive the shipped
        // app's key does not verify would fail every install.
        verify_file(&tar_path, &sig, &pubkey)?;
        write_file(&sig_path, sig.as_bytes(), false)?;
        println!("   signed {} for the updater", p(&tar_path));
    }

    let dmg_dir = bundle_dir_for("dmg", target, &dirs.rust_dir);
    fresh_dir(&dmg_dir)?;
    let root = macos.join("dmg-root");
    fresh_dir(&root)?;
    copy_dir(&app, &root.join(app.file_name().unwrap_or_default()))?;
    #[cfg(unix)]
    io(
        "cannot link /Applications",
        std::os::unix::fs::symlink("/Applications", root.join("Applications")),
    )?;
    let arch = if args.arch == BundleArch::Arm64 {
        "aarch64"
    } else {
        "x64"
    };
    let dmg = dmg_dir.join(format!(
        "{}_{}_{arch}.dmg",
        NATIVE_APP.product_name, args.version
    ));
    step(
        run,
        vec![
            tools.hdiutil.clone(),
            "create".into(),
            "-volname".into(),
            NATIVE_APP.product_name.into(),
            "-srcfolder".into(),
            p(&root),
            "-ov".into(),
            "-format".into(),
            "UDZO".into(),
            p(&dmg),
        ],
        None,
        "hdiutil create",
    )?;
    let _ = std::fs::remove_dir_all(&root);
    if let Some(id) = identity {
        step(
            run,
            vec![
                tools.codesign.clone(),
                "--force".into(),
                "--sign".into(),
                id,
                p(&dmg),
            ],
            None,
            "codesign the image",
        )?;
    }
    Ok(())
}

fn build_linux(
    args: &BundleArgs,
    dirs: &BundleDirs,
    env: &Env,
    binary: &[u8],
) -> Result<(), String> {
    let target = args.target.as_deref();
    let files = linux_files(binary, &dirs.repo_dir)?;
    let mtime = source_date(env);
    let ver = package_version(&args.version);
    let deb_dir = bundle_dir_for("deb", target, &dirs.rust_dir);
    fresh_dir(&deb_dir)?;
    let deb = deb_dir.join(format!(
        "{}_{ver}_{}.deb",
        NATIVE_APP.package,
        deb_arch(args.arch)
    ));
    write_file(
        &deb,
        &deb_bytes(&args.version, args.arch, &files, mtime)?,
        false,
    )?;
    let rpm_dir = bundle_dir_for("rpm", target, &dirs.rust_dir);
    fresh_dir(&rpm_dir)?;
    let rpm = rpm_dir.join(format!(
        "{}-{ver}-1.{}.rpm",
        NATIVE_APP.package,
        rpm_arch(args.arch)
    ));
    write_file(
        &rpm,
        &rpm_bytes(&args.version, args.arch, &files, mtime)?,
        false,
    )?;
    println!("   wrote {} and {}", p(&deb), p(&rpm));
    Ok(())
}

fn build_windows(
    args: &BundleArgs,
    run: &mut Runner<'_>,
    dirs: &BundleDirs,
    tools: &NativeTools,
    binary: &[u8],
) -> Result<(), String> {
    let target = args.target.as_deref();
    let stage = bundle_dir_for("nsis-stage", target, &dirs.rust_dir);
    fresh_dir(&stage)?;
    write_file(&stage.join(NATIVE_APP.windows_executable), binary, true)?;
    for (name, bytes) in resource_bytes(&dirs.repo_dir)? {
        write_file(&stage.join(name), &bytes, false)?;
    }
    let icons = find_icons(&dirs.repo_dir, &["icon.ico"])?;
    let ico = io(
        "cannot read icon.ico",
        std::fs::read(icons.join("icon.ico")),
    )?;
    write_file(&stage.join("icon.ico"), &ico, false)?;
    let nsis_dir = bundle_dir_for("nsis", target, &dirs.rust_dir);
    fresh_dir(&nsis_dir)?;
    let out = nsis_dir.join(format!(
        "{}_{}_x64-setup.exe",
        NATIVE_APP.product_name, args.version
    ));
    let script = stage.join("installer.nsi");
    write_file(
        &script,
        nsis_script(&args.version, &stage, &out).as_bytes(),
        false,
    )?;
    step(
        run,
        vec![tools.makensis.clone(), p(&script)],
        Some(&stage),
        "makensis",
    )?;
    Ok(())
}

/// Builds the native window and leaves its bundles where the shared half of
/// build-app-bundle looks: rust/target/[<triple>/]release/bundle/<kind>/.
pub fn build_native(
    args: &BundleArgs,
    run: &mut Runner<'_>,
    dirs: &BundleDirs,
    env: &Env,
) -> Result<(), String> {
    let tools = NativeTools::from_env(env);
    let target = args.target.as_deref();
    step(
        run,
        cargo_argv(&tools, target),
        Some(&dirs.rust_dir),
        "cargo build of the native window",
    )?;
    let bin = binary_path(&dirs.rust_dir, target, args.os);
    let binary = std::fs::read(&bin).map_err(|e| {
        format!(
            "build-app-bundle: cargo reported success and there is no {} ({e})",
            bin.display()
        )
    })?;
    match args.os {
        BundleOs::Macos => build_mac(args, run, dirs, env, &tools, &binary),
        BundleOs::Linux => build_linux(args, dirs, env, &binary),
        BundleOs::Windows => build_windows(args, run, dirs, &tools, &binary),
    }
}

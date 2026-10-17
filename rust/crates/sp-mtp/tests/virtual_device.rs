//! The real mtp-rs adapter (src/real.rs) and the copy (src/put.rs) against
//! mtp-rs's virtual device: a whole MTP session, PTP containers and all,
//! backed by a temp folder instead of a USB cable. Everything a Kindle
//! session does except the USB transport and the Kindle's own firmware.
//!
//! What it cannot say: how a real Kindle answers (rename support, folder
//! names, timing). That is docs/mtp-kindle-hardware-day.md's job.

use futures::executor::block_on;
use mtp_rs::{MtpDevice, VirtualDeviceConfig, VirtualStorageConfig};
use sp_mtp::paths::parse_device_path;
use sp_mtp::put::{Method, list_folder, put};
use sp_mtp::real::RealSession;
use sp_mtp::session::Session;
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "method-reader-mtp-{tag}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("device")).unwrap();
        Scratch(dir)
    }
    fn device(&self) -> PathBuf {
        self.0.join("device")
    }
    fn book(&self, name: &str, bytes: &[u8]) -> (PathBuf, u64) {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        (p, bytes.len() as u64)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn kindle(backing: &Path, supports_rename: bool) -> RealSession {
    let device = block_on(MtpDevice::builder().open_virtual(VirtualDeviceConfig {
        manufacturer: "Amazon".into(),
        model: "Virtual Kindle".into(),
        serial: "G000VIRTUAL".into(),
        storages: vec![VirtualStorageConfig {
            description: "Internal Storage".into(),
            capacity: 512 * 1024 * 1024,
            backing_dir: backing.to_path_buf(),
            read_only: false,
        }],
        supports_rename,
        // Nothing here changes the folder behind the session's back.
        watch_backing_dirs: false,
        event_poll_interval: Duration::from_millis(50),
        ..Default::default()
    }))
    .expect("the virtual device opens");
    RealSession::new(device)
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn a_book_lands_in_a_documents_folder_the_copy_makes() {
    let s = Scratch::new("new");
    let (book, size) = s.book("Title.kfx", b"KFX BYTES");
    let mut session = kindle(&s.device(), true);
    let out = put(
        &mut session,
        &book,
        size,
        &parse_device_path("documents/Title.kfx").unwrap(),
    )
    .unwrap();
    session.close().unwrap();
    assert_eq!(out.method, Method::Rename);
    assert_eq!(out.storage, "Internal Storage");
    assert_eq!(names(&s.device().join("documents")), vec!["Title.kfx"]);
    assert_eq!(
        std::fs::read(s.device().join("documents/Title.kfx")).unwrap(),
        b"KFX BYTES"
    );
}

#[test]
fn a_second_copy_replaces_the_first_and_leaves_no_temp_file() {
    let s = Scratch::new("replace");
    std::fs::create_dir_all(s.device().join("documents")).unwrap();
    std::fs::write(s.device().join("documents/Title.kfx"), b"OLD").unwrap();
    std::fs::write(s.device().join("documents/Other.azw3"), b"KEEP").unwrap();
    let (book, size) = s.book("Title.kfx", b"NEW AND LONGER");
    let mut session = kindle(&s.device(), true);
    let out = put(
        &mut session,
        &book,
        size,
        &parse_device_path("documents/Title.kfx").unwrap(),
    )
    .unwrap();
    session.close().unwrap();
    assert!(out.replaced);
    assert_eq!(
        names(&s.device().join("documents")),
        vec!["Other.azw3", "Title.kfx"]
    );
    assert_eq!(
        std::fs::read(s.device().join("documents/Title.kfx")).unwrap(),
        b"NEW AND LONGER"
    );
    assert_eq!(
        std::fs::read(s.device().join("documents/Other.azw3")).unwrap(),
        b"KEEP"
    );
}

#[test]
fn a_leftover_from_a_crashed_copy_is_cleared() {
    let s = Scratch::new("leftover");
    std::fs::create_dir_all(s.device().join("documents")).unwrap();
    std::fs::write(
        s.device().join("documents/.Title.kfx.method-reader-part"),
        b"HALF",
    )
    .unwrap();
    let (book, size) = s.book("Title.kfx", b"WHOLE");
    let mut session = kindle(&s.device(), true);
    put(
        &mut session,
        &book,
        size,
        &parse_device_path("documents/Title.kfx").unwrap(),
    )
    .unwrap();
    session.close().unwrap();
    assert_eq!(names(&s.device().join("documents")), vec!["Title.kfx"]);
}

#[test]
fn a_device_without_rename_gets_a_plain_replace() {
    let s = Scratch::new("norename");
    std::fs::create_dir_all(s.device().join("documents")).unwrap();
    std::fs::write(s.device().join("documents/Title.kfx"), b"OLD").unwrap();
    let (book, size) = s.book("Title.kfx", b"NEW");
    let mut session = kindle(&s.device(), false);
    assert!(!session.supports_rename());
    let out = put(
        &mut session,
        &book,
        size,
        &parse_device_path("documents/Title.kfx").unwrap(),
    )
    .unwrap();
    session.close().unwrap();
    assert_eq!(out.method, Method::Replace);
    assert_eq!(names(&s.device().join("documents")), vec!["Title.kfx"]);
    assert_eq!(
        std::fs::read(s.device().join("documents/Title.kfx")).unwrap(),
        b"NEW"
    );
}

#[test]
fn a_book_larger_than_one_chunk_arrives_whole() {
    let s = Scratch::new("large");
    let data: Vec<u8> = (0..(3 * 1024 * 1024 + 7))
        .map(|i| (i % 253) as u8)
        .collect();
    let (book, size) = s.book("Big.kfx", &data);
    let mut session = kindle(&s.device(), true);
    put(
        &mut session,
        &book,
        size,
        &parse_device_path("documents/Big.kfx").unwrap(),
    )
    .unwrap();
    session.close().unwrap();
    assert_eq!(
        std::fs::read(s.device().join("documents/Big.kfx")).unwrap(),
        data
    );
}

#[test]
fn ls_shows_the_book_and_no_temp_file_after_a_copy() {
    let s = Scratch::new("ls");
    let (book, size) = s.book("Title.kfx", b"KFX");
    let mut session = kindle(&s.device(), true);
    put(
        &mut session,
        &book,
        size,
        &parse_device_path("documents/Title.kfx").unwrap(),
    )
    .unwrap();
    let (storage, entries) = list_folder(&mut session, Some("documents")).unwrap();
    session.close().unwrap();
    assert_eq!(storage, "Internal Storage");
    assert_eq!(
        entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["Title.kfx"]
    );
}

#[test]
fn the_storage_reads_back_as_the_session_reports_it() {
    let s = Scratch::new("storage");
    let mut session = kindle(&s.device(), true);
    let storages = session.storages().unwrap();
    session.close().unwrap();
    assert_eq!(storages.len(), 1);
    assert_eq!(storages[0].description, "Internal Storage");
    assert!(storages[0].writable);
    assert!(storages[0].free_bytes > 0);
}

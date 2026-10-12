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

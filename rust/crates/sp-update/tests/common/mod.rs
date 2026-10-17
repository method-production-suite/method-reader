//! A throwaway signing key, a fake app bundle, its `.app.tar.gz`, and a
//! local HTTP server that answers like a GitHub release. Nothing here
//! reaches the network or the real key.
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};

use base64::Engine;

pub fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

pub struct TestKey {
    pub pk: minisign::PublicKey,
    pub sk: minisign::SecretKey,
}

impl TestKey {
    pub fn new() -> Self {
        let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        Self {
            pk: kp.pk,
            sk: kp.sk,
        }
    }

    /// tauri.conf.json's `pubkey` shape: base64 of the key box.
    pub fn pubkey_b64(&self) -> String {
        b64(&self.pk.to_box().unwrap().to_string())
    }

    /// A `.sig` file's content, as tauri-cli 2.11.4's `sign_file` writes it
    /// (`version` None) or as `sp-release sign-update` does (`Some`).
    pub fn sign(&self, bytes: &[u8], file: &str, version: Option<&str>) -> String {
        let mut trusted = format!("timestamp:1767225600\tfile:{file}");
        if let Some(v) = version {
            trusted.push_str(&format!("\tversion:{v}"));
        }
        let sig = minisign::sign(
            None,
            &self.sk,
            std::io::Cursor::new(bytes),
            Some(&trusted),
            Some("signature from tauri secret key"),
        )
        .unwrap();
        b64(&sig.to_string())
    }
}

/// An Info.plist with the keys the swap and the restart read, under the
/// identifier a bundle running `exe` shipped with: the 0.12.x one for the
/// old executables, Method Reader's own otherwise.
pub fn info_plist(exe: &str, version: &str) -> String {
    let id = if matches!(exe, "screepub-desktop" | "Screepub") {
        sp_update::LEGACY_IDENTIFIER
    } else {
        sp_update::IDENTIFIER
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>{exe}</string>
  <key>CFBundleIdentifier</key>
  <string>{id}</string>
  <key>CFBundleShortVersionString</key>
  <string>{version}</string>
</dict>
</plist>
"#
    )
}

/// A bundle on disk: Info.plist, one executable, one resource.
pub fn make_bundle(at: &Path, exe: &str, version: &str) {
    let contents = at.join("Contents");
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    std::fs::create_dir_all(contents.join("Resources")).unwrap();
    std::fs::write(contents.join("Info.plist"), info_plist(exe, version)).unwrap();
    std::fs::write(
        contents.join("MacOS").join(exe),
        format!("#!/bin/sh\necho {version}\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            contents.join("MacOS").join(exe),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    std::fs::write(contents.join("Resources").join("LICENSE"), version).unwrap();
}

/// `<top>/...` entries from (path, bytes, mode), gzipped.
pub fn tar_gz(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut b = tar::Builder::new(gz);
    for (path, data, mode) in entries {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(*mode);
        h.set_mtime(1_767_225_600);
        if path.ends_with('/') {
            h.set_entry_type(tar::EntryType::Directory);
            h.set_size(0);
            b.append_data(&mut h, path, std::io::empty()).unwrap();
        } else {
            // Raw name bytes: append_data would refuse a `..` and this
            // helper has to be able to build a hostile archive.
            h.as_gnu_mut().unwrap().name[..path.len()].copy_from_slice(path.as_bytes());
            h.set_cksum();
            b.append(&h, *data).unwrap();
        }
    }
    b.into_inner().unwrap().finish().unwrap()
}

/// The archive of a whole bundle folder, top folder `Method Reader.app`,
/// as `sp-release update-archive` lays it out (the Tauri bundler's had
/// `Screepub.app`; the swap drops the top folder, so it never matters).
pub fn archive_bundle(bundle: &Path) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut b = tar::Builder::new(gz);
    b.append_dir_all("Method Reader.app", bundle).unwrap();
    b.into_inner().unwrap().finish().unwrap()
}

/// One canned answer.
#[derive(Clone)]
pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: &'static str,
}

impl Answer {
    pub fn json(body: &str) -> Self {
        Self {
            status: 200,
            body: body.as_bytes().to_vec(),
            content_type: "application/json",
        }
    }
    pub fn bytes(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            body,
            content_type: "application/octet-stream",
        }
    }
    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
            content_type: "text/plain",
        }
    }
}

/// A local server: `routes` by path, every request recorded as
/// (path, Accept header).
pub struct Server {
    pub base: String,
    pub routes: Arc<Mutex<HashMap<String, Answer>>>,
    pub seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl Server {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let routes: Arc<Mutex<HashMap<String, Answer>>> = Arc::default();
        let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
        let (r, s) = (routes.clone(), seen.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let mut accept = String::new();
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("accept:") {
                        accept = v.trim().to_string();
                    }
                }
                s.lock().unwrap().push((path.clone(), accept));
                let answer = r
                    .lock()
                    .unwrap()
                    .get(&path)
                    .cloned()
                    .unwrap_or(Answer::status(404));
                let reason = match answer.status {
                    200 => "OK",
                    204 => "No Content",
                    404 => "Not Found",
                    _ => "Other",
                };
                let head = format!(
                    "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    answer.status,
                    answer.content_type,
                    answer.body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&answer.body);
            }
        });
        Self { base, routes, seen }
    }

    pub fn route(&self, path: &str, answer: Answer) {
        self.routes.lock().unwrap().insert(path.into(), answer);
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

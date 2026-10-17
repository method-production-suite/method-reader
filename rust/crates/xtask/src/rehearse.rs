//! `cargo xtask rehearse-handover`: watch a real Tauri 0.11.1 window update
//! itself into the Rust app, on this Mac, with nothing published and no
//! real key (was tools/rehearse-handover.sh).
//!
//! ```text
//! cargo xtask rehearse-handover prepare        build both apps, the archive, a signed latest.json
//! cargo xtask rehearse-handover prove <work>   the same with no window: the real updater code, headless
//! cargo xtask rehearse-handover run <work>     serve latest.json locally, open the Tauri window
//! cargo xtask rehearse-handover verify <work>  check what the update left behind
//! cargo xtask rehearse-handover clean <work>   remove the rehearsal's own folders
//! ```
//!
//! What it builds, all in a fresh folder under the temp folder:
//! - the Tauri window exactly as tagged v0.11.1 (`git archive` of the tag,
//!   built with that tag's own Bun engine), with a config overlay that
//!   changes ONLY the identifier (com.darkwell.screepub.rehearsal, so WebKit
//!   storage, caches and the Rust app's own storage stay away from the real
//!   app's), the updater's public key (a throwaway one) and its endpoint
//!   (http://127.0.0.1);
//! - the Rust app from this checkout, as version 0.12.0, under the
//!   rehearsal form of Method Reader's own identifier
//!   (com.methodreader.desktop.rehearsal: the shipped one changed from the
//!   Tauri window's at 0.13, and the app finds the old one's storage by
//!   that same rule), its updater baked with the same throwaway key and
//!   endpoint (`METHOD_READER_UPDATE_*` at compile time, read only because
//!   it builds with `--features rehearsal`);
//! - Method-Reader-macOS.app.tar.gz of it, signed with the throwaway key by
//!   `sp-release sign-update`, and latest.json pointing at the local server,
//!   checked by `sp-release verify-update` before anything runs.
//!
//! Both apps run with their settings and library folders inside the work
//! folder (the Tauri window's SCREEPUB_CONFIG_DIR and SCREEPUB_LIBRARY, and
//! the METHOD_READER_ names the Rust app reads first), inherited across
//! the restart, so the real settings and library are never read or
//! written. Nothing is code-signed: locally built
//! apps carry no quarantine, and the linker signs arm64 binaries ad hoc.
//!
//! The old app sits where every real install of it sits, at
//! `Applications/Screepub.app` (the name up to 0.12.x), and the update
//! lands at that same path, as the old updater always swaps in place:
//! what is checked is that the new build, whose archive holds
//! `Method Reader.app` running `method-reader-app`, works from there.
//!
//! Needs: macOS on Apple silicon, cargo (rustup), bun 1.3.14 (the TAGGED
//! window's engine is TypeScript; this checkout has none), the Tauri CLI
//! 2.11.4 (`cargo install tauri-cli --version 2.11.4 --locked`), git.
//!
//! `prepare` is the ONE place left in the workspace that runs bun (or any
//! JavaScript tool): it builds the old v0.11.1 window from its own tag,
//! whose engine was TypeScript, because the handover has to start from the
//! window people actually have. Nothing it builds ships, and no other
//! command, test or workflow needs bun; xtask's tests/no_js_runtime.rs and
//! tests/workflows.rs hold that.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::paths;

/// The Tauri window's identifier, as rehearsed: the 0.12.x one's.
const OLD_ID: &str = "com.darkwell.screepub.rehearsal";
/// The Rust app's: Method Reader's own, as rehearsed.
const NEW_ID: &str = "com.methodreader.desktop.rehearsal";
/// Where the old app is installed, and so where the update lands.
const OLD_BUNDLE: &str = sp_release::names::LEGACY_MAC_BUNDLE;
const OLD_EXE: &str = sp_release::names::LEGACY_WINDOW_EXE;
const NEW_EXE: &str = sp_release::names::WINDOW_EXE;
const ARCHIVE: &str = sp_release::names::MAC_UPDATE;
const TAG: &str = "v0.11.1";
const NEW: &str = "0.12.0";

const USAGE: &str = "\
usage: cargo xtask rehearse-handover <step> [work folder]

  prepare          build both apps, the archive, a signed latest.json
  prove <work>     the same with no window: the real updater code, headless
  run <work>       serve latest.json locally, open the Tauri window
  verify <work>    check what the update left behind
  clean <work>     remove the rehearsal's own folders

REHEARSAL_PORT sets the local server's port (default 8765).
";

fn port() -> String {
    std::env::var("REHEARSAL_PORT").unwrap_or_else(|_| "8765".into())
}

fn say(s: &str) -> String {
    format!("rehearse-handover: {s}")
}

/// Runs `cmd`, inheriting the terminal; an error names it.
fn sh(cmd: &mut Command) -> Result<(), String> {
    let what = format!("{cmd:?}");
    let status = cmd
        .status()
        .map_err(|e| say(&format!("could not start {what}: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(say(&format!("{what} failed ({status})")))
    }
}

/// Runs `cmd` and answers its stdout.
fn output(cmd: &mut Command) -> Result<String, String> {
    let what = format!("{cmd:?}");
    let out = cmd
        .output()
        .map_err(|e| say(&format!("could not start {what}: {e}")))?;
    if !out.status.success() {
        return Err(say(&format!(
            "{what} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn need(tool: &str, hint: &str) -> Result<(), String> {
    let found = Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok();
    if found {
        Ok(())
    } else {
        Err(say(&format!("{tool} is not installed. {hint}")))
    }
}

fn sp_release() -> PathBuf {
    paths::workspace_root().join("target/release/sp-release")
}

fn read(p: &Path) -> Result<String, String> {
    std::fs::read_to_string(p).map_err(|e| say(&format!("cannot read {}: {e}", p.display())))
}

fn mkdirs(dirs: &[PathBuf]) -> Result<(), String> {
    for d in dirs {
        std::fs::create_dir_all(d).map_err(|e| say(&format!("mkdir {}: {e}", d.display())))?;
    }
    Ok(())
}

/// `git archive <rev> | tar -x -C <into>`.
fn unpack(rev: &str, into: &Path) -> Result<(), String> {
    mkdirs(&[into.to_path_buf()])?;
    let mut git = Command::new("git")
        .args(["archive", rev])
        .current_dir(paths::repo_root())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| say(&format!("git archive: {e}")))?;
    let stdout = git.stdout.take().expect("piped");
    let tar = Command::new("tar")
        .arg("-x")
        .arg("-C")
        .arg(into)
        .stdin(stdout)
        .status()
        .map_err(|e| say(&format!("tar: {e}")))?;
    let git = git.wait().map_err(|e| say(&format!("git archive: {e}")))?;
    if git.success() && tar.success() {
        Ok(())
    } else {
        Err(say(&format!("could not unpack {rev}")))
    }
}

fn copy_app(from: &Path, into: &Path) -> Result<(), String> {
    sh(Command::new("cp").arg("-R").arg(from).arg(into))
}

/// The workspace version line, set to `version`.
fn set_workspace_version(cargo_toml: &Path, version: &str) -> Result<(), String> {
    let text = read(cargo_toml)?;
    let mut in_package = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let t = line.trim_end();
        if t.starts_with('[') {
            in_package = t == "[workspace.package]";
        }
        if in_package && t.starts_with("version = \"") {
            out.push_str(&format!("version = \"{version}\"\n"));
        } else {
            out.push_str(line);
        }
    }
    std::fs::write(cargo_toml, out).map_err(|e| say(&e.to_string()))
}

fn prepare() -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err(say(
            "the handover is a Mac's (latest.json lists darwin-aarch64 only)",
        ));
    }
    if !cfg!(target_arch = "aarch64") {
        return Err(say("Apple silicon only, like the release"));
    }
    need("cargo", "Install Rust with rustup.")?;
    need(
        "bun",
        "Install bun 1.3.14: the tagged window's engine needs it.",
    )?;
    let tauri = output(Command::new("cargo").args(["tauri", "--version"])).unwrap_or_default();
    if !tauri.contains("2.11.4") {
        return Err(say(
            "the Tauri CLI 2.11.4 is needed: cargo install tauri-cli --version 2.11.4 --locked",
        ));
    }

    let w =
        crate::scratch::TempDir::new("method-reader-handover.").map_err(|e| say(&e.to_string()))?;
    // Kept: the rehearsal's later steps read it.
    let w = {
        let p = w.path().to_path_buf();
        std::mem::forget(w);
        p
    };
    println!("work folder: {}", w.display());
    mkdirs(&[
        w.join("site"),
        w.join("Applications"),
        w.join("config"),
        w.join("library"),
        w.join("keys"),
    ])?;

    // 1. The release tools, from this checkout.
    sh(Command::new("cargo")
        .args(["build", "--release", "-p", "sp-release"])
        .current_dir(paths::workspace_root()))?;
    let sp = sp_release();
    sh(Command::new(&sp)
        .args(["keygen", "--out"])
        .arg(w.join("keys/rehearsal.key")))?;
    let public = read(&w.join("keys/rehearsal.key.pub"))?.trim().to_owned();
    let port = port();
    let endpoint = format!("http://127.0.0.1:{port}/latest.json");

    // 2. The Tauri window as tagged, with the rehearsal overlay.
    let tauri_src = w.join("tauri-src");
    unpack(TAG, &tauri_src)?;
    let overlay = serde_json::json!({
        "identifier": OLD_ID,
        "plugins": { "updater": {
            "pubkey": public,
            "endpoints": [endpoint],
            "dangerousInsecureTransportProtocol": true,
        }},
    });
    std::fs::write(
        w.join("rehearsal.conf.json"),
        serde_json::to_string_pretty(&overlay).expect("json"),
    )
    .map_err(|e| say(&e.to_string()))?;
    // The one bun call left in the workspace (module docs): the TAGGED
    // window's engine, built from the tag.
    sh(Command::new("bun")
        .args(["install", "--frozen-lockfile"])
        .current_dir(&tauri_src))?;
    sh(Command::new("bun")
        .args(["tools/build-sidecar.ts", "--host"])
        .current_dir(&tauri_src))?;
    sh(Command::new("cargo")
        .args(["tauri", "build", "--bundles", "app", "--config"])
        .arg(w.join("rehearsal.conf.json"))
        .current_dir(tauri_src.join("desktop/src-tauri")))?;
    copy_app(
        &tauri_src.join("desktop/src-tauri/target/release/bundle/macos/Screepub.app"),
        &w.join("Applications"),
    )?;

    // 3. The Rust app as 0.12.0, baked with the rehearsal key and endpoint.
    // The whole commit: the crates read files outside rust/ at build time
    // (format-defaults.json, the release notes).
    let rust_src = w.join("rust-src");
    unpack("HEAD", &rust_src)?;
    set_workspace_version(&rust_src.join("rust/Cargo.toml"), NEW)?;
    sh(Command::new("cargo")
        .args([
            "build",
            "--release",
            "-p",
            "sp-app",
            "--features",
            "rehearsal",
        ])
        .env("METHOD_READER_UPDATE_ENDPOINT", &endpoint)
        .env("METHOD_READER_UPDATE_PUBKEY", &public)
        .env("METHOD_READER_UPDATE_INSECURE", "1")
        .current_dir(rust_src.join("rust")))?;
    sh(Command::new(&sp)
        .args([
            "mac-app",
            "--version",
            NEW,
            "--identifier",
            NEW_ID,
            "--window",
        ])
        .arg(rust_src.join("rust/target/release").join(NEW_EXE))
        .arg("--icon")
        .arg(rust_src.join("rust/crates/sp-app/icons/icon.icns"))
        .arg("--resource")
        .arg(rust_src.join("LICENSE-MIT"))
        .arg("--resource")
        .arg(rust_src.join("LICENSE-APACHE"))
        .arg("--resource")
        .arg(rust_src.join("rust/crates/sp-azw3/LICENSE-GPL-3.0"))
        .arg("--resource")
        .arg(rust_src.join("THIRD-PARTY-NOTICES.md"))
        .arg("--out")
        .arg(w.join("build")))?;

    // 4. The update, signed and described, as release.yml would publish it.
    let archive = w.join("site").join(ARCHIVE);
    sh(Command::new(&sp)
        .args(["update-archive", "--app"])
        .arg(w.join("build").join(sp_release::names::MAC_BUNDLE))
        .arg("--out")
        .arg(&archive))?;
    sh(Command::new(&sp)
        .args(["sign-update", "--file"])
        .arg(&archive)
        .args(["--version", NEW])
        .env(
            "TAURI_SIGNING_PRIVATE_KEY",
            read(&w.join("keys/rehearsal.key"))?,
        )
        .env("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", ""))?;
    sh(Command::new(&sp)
        .args(["manifest", "--dir"])
        .arg(w.join("site"))
        .args(["--version", NEW, "--out"])
        .arg(w.join("site/latest.json"))
        .args(["--asset-base", &format!("http://127.0.0.1:{port}")]))?;
    sh(Command::new(&sp)
        .args(["verify-update", "--dir"])
        .arg(w.join("site"))
        .arg("--manifest")
        .arg(w.join("site/latest.json"))
        .args(["--pubkey", &public]))?;

    println!();
    println!(
        "Prepared. Next: cargo xtask rehearse-handover run {}",
        w.display()
    );
    Ok(())
}

fn prepared(w: &Path) -> Result<(), String> {
    if w.join("site/latest.json").is_file() {
        Ok(())
    } else {
        Err(say(&format!(
            "{} is not a prepared work folder",
            w.display()
        )))
    }
}

fn run(w: &Path) -> Result<(), String> {
    prepared(w)?;
    let port = port();
    let log = std::fs::File::create(w.join("serve.log")).map_err(|e| say(&e.to_string()))?;
    let server = Command::new(sp_release())
        .args(["serve", "--dir"])
        .arg(w.join("site"))
        .args(["--port", &port])
        .stderr(log)
        .spawn()
        .map_err(|e| say(&format!("sp-release serve: {e}")))?;
    std::fs::write(w.join("serve.pid"), server.id().to_string())
        .map_err(|e| say(&e.to_string()))?;
    std::thread::sleep(Duration::from_secs(1));
    println!(
        "serving {}/site on http://127.0.0.1:{port} (log: {}/serve.log)",
        w.display(),
        w.display()
    );
    println!();
    println!("Opening the Tauri 0.11.1 window. In it:");
    println!("  1. Answer the once-a-day question (either answer; 'yes' also tests that");
    println!("     the consent carries over), or open the release notes and press");
    println!("     Check for updates.");
    println!("  2. Press 'Update to {NEW}' (beside the stamp, or Install {NEW} in the notes).");
    println!("  3. Watch it download, install and restart. The window that comes back");
    println!("     is the Rust app: its release notes say {NEW}.");
    println!("Then: cargo xtask rehearse-handover verify {}", w.display());
    let app_log = std::fs::File::create(w.join("app.log")).map_err(|e| say(&e.to_string()))?;
    let app = Command::new(
        w.join("Applications")
            .join(OLD_BUNDLE)
            .join("Contents/MacOS")
            .join(OLD_EXE),
    )
    .env("SCREEPUB_CONFIG_DIR", w.join("config"))
    .env("SCREEPUB_LIBRARY", w.join("library"))
    .env("METHOD_READER_CONFIG_DIR", w.join("config"))
    .env("METHOD_READER_LIBRARY", w.join("library"))
    .stdout(app_log.try_clone().map_err(|e| say(&e.to_string()))?)
    .stderr(app_log)
    .spawn()
    .map_err(|e| say(&format!("the Tauri window: {e}")))?;
    std::fs::write(w.join("app.pid"), app.id().to_string()).map_err(|e| say(&e.to_string()))?;
    Ok(())
}

/// The same handover with no window and no click, so CI can run it too:
/// rust/handover-proof runs tauri-plugin-updater 2.12.0 itself (on tauri's
/// mock runtime, configured as tauri.conf.json configured every window)
/// against a COPY of the prepared Tauri bundle: check(), then
/// downloadAndInstall(), then reads what a relaunch would start.
fn prove(w: &Path) -> Result<(), String> {
    prepared(w)?;
    let proof_dir = paths::workspace_root().join("handover-proof");
    sh(Command::new("cargo")
        .args(["build", "--release", "--locked"])
        .current_dir(&proof_dir))?;
    let (proof, site) = (w.join("proof"), w.join("proof-site"));
    let _ = std::fs::remove_dir_all(&proof);
    let _ = std::fs::remove_dir_all(&site);
    mkdirs(&[proof.clone(), site.clone()])?;
    copy_app(&w.join("Applications").join(OLD_BUNDLE), &proof)?;
    for f in [ARCHIVE.to_string(), format!("{ARCHIVE}.sig")] {
        std::fs::copy(w.join("site").join(&f), site.join(&f)).map_err(|e| say(&e.to_string()))?;
    }
    // Any free port (a CI runner, or something else on this Mac, may hold
    // the rehearsal's): the server says which, and the manifest follows it.
    let log = std::fs::File::create(w.join("proof-serve.log")).map_err(|e| say(&e.to_string()))?;
    let mut server = Command::new(sp_release())
        .args(["serve", "--dir"])
        .arg(&site)
        .args(["--port", "0"])
        .stdout(Stdio::piped())
        .stderr(log)
        .spawn()
        .map_err(|e| say(&format!("sp-release serve: {e}")))?;
    let mut lines = BufReader::new(server.stdout.take().expect("piped")).lines();
    let started = Instant::now();
    let base = loop {
        if started.elapsed() > Duration::from_secs(5) {
            let _ = server.kill();
            return Err(say("the local server did not start"));
        }
        match lines.next() {
            Some(Ok(line)) => {
                if let Some(at) = line.rfind(" at http:") {
                    break line[at + 4..].trim().to_owned();
                }
            }
            _ => {
                let _ = server.kill();
                return Err(say("the local server did not start"));
            }
        }
    };
    let result = (|| {
        sh(Command::new(sp_release())
            .args(["manifest", "--dir"])
            .arg(&site)
            .args(["--version", NEW, "--out"])
            .arg(site.join("latest.json"))
            .args(["--asset-base", &base]))?;
        sh(
            Command::new(proof_dir.join("target/release/handover-proof"))
                .arg("--bundle")
                .arg(proof.join(OLD_BUNDLE))
                .args(["--endpoint", &format!("{base}/latest.json")])
                .args(["--pubkey", read(&w.join("keys/rehearsal.key.pub"))?.trim()]),
        )
    })();
    let _ = server.kill();
    let _ = server.wait();
    if let Ok(log) = std::fs::read_to_string(w.join("proof-serve.log")) {
        print!("{log}");
    }
    result.map_err(|e| {
        say(&format!(
            "the Tauri updater did not carry the bundle across ({e})"
        ))
    })?;
    if proof
        .join(OLD_BUNDLE)
        .join("Contents/MacOS/screepub-engine")
        .exists()
    {
        return Err(say("the old engine sidecar survived"));
    }
    let left: Vec<String> = std::fs::read_dir(&proof)
        .map_err(|e| say(&e.to_string()))?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    if left != [OLD_BUNDLE] {
        return Err(say(&format!(
            "something was left beside the bundle: {left:?}"
        )));
    }
    println!(
        "Proved: tauri-plugin-updater 2.12.0 installed the Rust {NEW} bundle over the Tauri 0.11.1 one."
    );
    Ok(())
}

fn plist_get(app: &Path, key: &str) -> String {
    Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print :{key}")])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn verify(w: &Path) -> Result<(), String> {
    let app = w.join("Applications").join(OLD_BUNDLE);
    let mut fail = false;
    let mut check = |what: &str, ok: bool| {
        println!("  {} {what}", if ok { "ok  " } else { "FAIL" });
        fail |= !ok;
    };
    println!("after the update:");
    check(
        &format!("the bundle at the same path says {NEW}"),
        plist_get(&app, "CFBundleShortVersionString") == NEW,
    );
    check(
        "with Method Reader's identifier",
        plist_get(&app, "CFBundleIdentifier") == NEW_ID,
    );
    check(
        &format!("and runs {NEW_EXE}, the Rust window"),
        plist_get(&app, "CFBundleExecutable") == NEW_EXE,
    );
    check(
        "the Tauri engine sidecar is gone",
        !app.join("Contents/MacOS/screepub-engine").exists(),
    );
    let beside: Vec<String> = std::fs::read_dir(w.join("Applications"))
        .map(|r| {
            r.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    check("nothing left beside the bundle", beside == [OLD_BUNDLE]);
    let serve_log = std::fs::read_to_string(w.join("serve.log")).unwrap_or_default();
    check(
        "the window fetched latest.json",
        serve_log.contains("GET /latest.json -> 200"),
    );
    check(
        "and the signed archive",
        serve_log.contains(&format!("GET /{ARCHIVE} -> 200")),
    );
    let running = Command::new("pgrep")
        .arg("-f")
        .arg(app.join("Contents/MacOS").join(NEW_EXE))
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    check("the Rust app is running from the bundle", running);
    // In the work folder's settings folder when the restart kept the
    // overrides, else in the rehearsal identifier's own app-data folder.
    let home = std::env::home_dir().unwrap_or_default();
    let store = [
        w.join("config/window-storage.json"),
        home.join(format!(
            "Library/Application Support/{NEW_ID}/window-storage.json"
        )),
    ]
    .into_iter()
    .find(|p| p.exists())
    .unwrap_or_else(|| w.join("config/window-storage.json"));
    match std::fs::read_to_string(&store) {
        Ok(text) => {
            println!("  ok   the Rust app wrote its storage: {}", text.trim());
            println!(
                "       (updateAsked/updateOptIn there are what the Tauri window had: compare with your answer)"
            );
        }
        Err(_) => {
            println!(
                "  FAIL the Rust app has not written {} yet",
                store.display()
            );
            fail = true;
        }
    }
    if fail {
        return Err(say("something did not hand over; see above"));
    }
    println!("The handover worked.");
    Ok(())
}

fn clean(w: &Path) -> Result<(), String> {
    for f in ["serve.pid", "app.pid"] {
        if let Ok(pid) = std::fs::read_to_string(w.join(f)) {
            let _ = Command::new("kill").arg(pid.trim()).status();
        }
    }
    let _ = Command::new("pkill")
        .arg("-f")
        .arg(
            w.join("Applications")
                .join(OLD_BUNDLE)
                .join("Contents/MacOS/"),
        )
        .status();
    // Only the rehearsal identifiers' folders, never the real app's.
    let home = std::env::home_dir().unwrap_or_default();
    for id in [OLD_ID, NEW_ID] {
        for p in [
            format!("Library/WebKit/{id}"),
            format!("Library/Caches/{id}"),
            format!("Library/Application Support/{id}"),
            format!("Library/Saved Application State/{id}.savedState"),
        ] {
            let _ = std::fs::remove_dir_all(home.join(p));
        }
    }
    let _ = std::fs::remove_dir_all(w);
    println!(
        "removed {} and the {OLD_ID} and {NEW_ID} folders",
        w.display()
    );
    let _ = std::io::stdout().flush();
    Ok(())
}

pub fn main(args: &[String]) -> u8 {
    let work = || -> Result<PathBuf, String> {
        args.get(1)
            .map(PathBuf::from)
            .ok_or_else(|| say(&format!("{} needs the work folder", args[0])))
    };
    let result = match args.first().map(String::as_str) {
        Some("prepare") => prepare(),
        Some("run") => work().and_then(|w| run(&w)),
        Some("prove") => work().and_then(|w| prove(&w)),
        Some("verify") => work().and_then(|w| verify(&w)),
        Some("clean") => work().and_then(|w| clean(&w)),
        Some("-h" | "--help") => {
            print!("{USAGE}");
            return 0;
        }
        _ => {
            eprint!("{USAGE}");
            return 2;
        }
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

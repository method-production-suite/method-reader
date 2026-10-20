//! Method Reader 0.13.0's own updater, run headless against a release's
//! update files: the code every 0.13.0 install runs to fetch the next
//! version, built from the v0.13.0 tag (Cargo.toml pins its commit).
//!
//!     update-proof-0-13 --bundle <old .app> --endpoint <latest.json URL> --current 0.13.0
//!         [--pubkey <base64>]
//!
//! It checks the endpoint with 0.13.0's REAL public key (sp_update::PUBKEY
//! as tagged) and signed versions required, as the shipped app does; only
//! the endpoint is the local one, so plain http is allowed. `--pubkey`
//! replaces the key for a run on files signed with a throwaway key (a run
//! on a developer's Mac, which has no release key); the release workflow
//! never passes it. Then it
//! downloads (the signature is verified inside download), and installs into
//! the bundle with 0.13.0's install_app. Exit 0 and one line on stdout when
//! all three worked; 1 and the reason on stderr otherwise. No restart: the
//! caller checks the bundle on disk.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use semver::Version;
use sp_update::{Config, check, download};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn run(args: &[String]) -> Result<String, String> {
    let bundle = PathBuf::from(arg(args, "--bundle").ok_or("--bundle is required")?);
    let endpoint = arg(args, "--endpoint").ok_or("--endpoint is required")?;
    let current = arg(args, "--current").ok_or("--current is required")?;
    let current = Version::parse(&current).map_err(|e| format!("--current: {e}"))?;
    let config = Config {
        endpoints: vec![endpoint],
        pubkey: arg(args, "--pubkey").unwrap_or_else(|| sp_update::PUBKEY.to_string()),
        allow_insecure: true,
        require_signed_version: true,
        timeout: Some(Duration::from_secs(600)),
        installer: None,
        targets: None,
    };
    let update = check(&config, &current)
        .map_err(|e| format!("check: {e}"))?
        .ok_or_else(|| format!("check: nothing newer than {current} was offered"))?;
    let bytes = download(&config, &update, |_, _| {}, || {}).map_err(|e| format!("download: {e}"))?;
    let refuse_admin = |_: &std::path::Path, _: &std::path::Path| {
        Err(sp_update::install::InstallError::NeedsAdmin(
            "the proof never asks for an administrator".into(),
        ))
    };
    let installed = sp_update::install::install_app(&bytes, &bundle, &refuse_admin)
        .map_err(|e| format!("install: {e}"))?;
    Ok(format!(
        "0.13.0's updater installed {} ({} bytes, signature verified) at {}",
        update.version,
        bytes.len(),
        installed.bundle.display()
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("update-proof-0-13: {e}");
            ExitCode::FAILURE
        }
    }
}

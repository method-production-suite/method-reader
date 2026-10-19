//! The updater signature for the xtask tools: was tools/update-signature.ts.
//!
//! Nothing here is a second implementation. The two minisign containers are
//! read by [`sp_release::boxes`] (re-exported below), the archive is signed
//! by [`sp_release::sign`], the one signer release.yml uses, and a
//! signature is checked by [`method_update::verify`], the check the app itself
//! runs. What stays here is the tag-time gate `cargo xtask
//! update-signature`, over [`sp_update_config::PUBKEY`], the one home of the key
//! the shipped app trusts. (Until the Rust cutover the gate read
//! `plugins.updater.pubkey` out of the Tauri window's tauri.conf.json,
//! `--pubkey-from-config`; that file went with the window.)

use std::path::Path;

pub use sp_release::boxes::{
    PublicKeyBox, SignatureBox, parse_public_key_box, parse_signature_box,
};

/// TAURI_SIGNING_PRIVATE_KEY or `--pubkey` is a path or the key text
/// itself, as tauri-cli reads it.
pub fn private_key_text(value: &str) -> Result<String, String> {
    let path = Path::new(value);
    if !value.is_empty() && path.exists() {
        std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read private key from file {value}: {e}"))
    } else {
        Ok(value.to_string())
    }
}

/// Signs `file` with [`sp_release::sign`] and returns the `.sig` text: the
/// container tauri-cli writes (untrusted comment "signature from tauri
/// secret key"), its trusted comment `timestamp:<unix>\tfile:<name>` plus
/// the `\tversion:<version>` field the Rust app requires.
pub fn sign_file(
    file: &Path,
    private_key: &str,
    password: &str,
    version: &str,
    timestamp: u64,
) -> Result<String, String> {
    let sk = sp_release::sign::load_secret_key(&private_key_text(private_key)?, password)?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let data = std::fs::read(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    sp_release::sign::sign(&sk, &data, &name, version, timestamp)
}

/// Checks `signature` (the `.sig` text) over `file` against `public_key`
/// (base64 of the box) for `version`, as the Rust app will at install time
/// (signed version required; every Tauri window's looser rule then passes).
pub fn verify_file(
    file: &Path,
    signature: &str,
    public_key: &str,
    version: &str,
) -> Result<(), String> {
    let data = std::fs::read(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    method_update::verify::verify_signature(&data, signature, public_key, version, true)
        .map(|_| ())
        .map_err(|e| {
            format!(
                "{} does not verify against the app's public key: {e}",
                file.display()
            )
        })
}

/// The key the gate checks: `--pubkey` (a path, or the key text itself),
/// else [`sp_update_config::PUBKEY`].
pub fn trusted_pubkey(explicit: Option<&str>) -> Result<String, String> {
    match explicit {
        Some(value) => private_key_text(value),
        None => Ok(sp_update_config::PUBKEY.to_string()),
    }
}

/// `cargo xtask update-signature [--pubkey <key or file>]`: release.yml's
/// tag-time gate. 0 and the key id when the app trusts a real key; 1 and a
/// reason otherwise; 2 for a usage error.
pub fn main(args: &[String]) -> u8 {
    use super::util::{FlagSpec, parse_flags};
    let usage = "usage: cargo xtask update-signature [--pubkey <key or file>]\n\n\
                 Checks the updater public key the app trusts: sp_update_config::PUBKEY \
                 (rust/crates/sp-update-config/src/lib.rs), or the one --pubkey names.";
    if super::util::wants_help(args, usage) {
        return 0;
    }
    let flags = match parse_flags(
        args,
        &[FlagSpec {
            name: "pubkey",
            value: true,
        }],
    ) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{e}\n{usage}");
            return 2;
        }
    };
    match trusted_pubkey(flags.get("pubkey").map(String::as_str))
        .and_then(|k| parse_public_key_box(&k))
    {
        Ok(k) => {
            println!("the app trusts updater key {}", k.key_id);
            0
        }
        Err(e) => {
            eprintln!("::error::{e}");
            eprintln!(
                "::error::A tag cannot ship an updater that trusts no key. Generate the pair, add the secrets, and paste the PUBLIC key into PUBKEY in rust/crates/sp-update-config/src/lib.rs: docs/release-secrets.md §4."
            );
            1
        }
    }
}

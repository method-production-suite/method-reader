//! The update signature, made the way tauri-cli 2.11.4 makes it
//! (helpers/updater_signature.rs): the private key is base64 of a minisign
//! secret-key box, password-protected (an empty password included); the
//! `.sig` file is base64 of the signature box, untrusted comment
//! "signature from tauri secret key".
//!
//! One addition: the trusted comment ends `\tversion:<version>`.
//! tauri-plugin-updater 2.12.0 reads that field when it is there and
//! refuses a manifest announcing any other version (updater.rs,
//! `verify_signed_version`), which every 0.11.x window therefore enforces
//! too; the Rust app requires it.
//!
//! The release key never passes through here except in CI, read from
//! TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD, the
//! secrets release.yml already has. `keygen` makes THROWAWAY keys for the
//! handover rehearsal and tests; the release key was made with
//! `cargo tauri signer generate` and is not replaced.

use std::path::Path;

use base64::Engine;

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

fn unb64(s: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| format!("the key is not base64: {e}"))?;
    String::from_utf8(bytes).map_err(|_| "the key is not UTF-8 inside".into())
}

/// A throwaway key pair, as `cargo tauri signer generate` writes it:
/// (private, public), both base64 of their minisign boxes.
pub fn keygen(password: &str) -> Result<(String, String), String> {
    let kp = minisign::KeyPair::generate_encrypted_keypair(Some(password.to_string()))
        .map_err(|e| e.to_string())?;
    let pk = kp.pk.to_box().map_err(|e| e.to_string())?.to_string();
    let sk = kp.sk.to_box(None).map_err(|e| e.to_string())?.to_string();
    Ok((b64(&sk), b64(&pk)))
}

/// The private key from TAURI_SIGNING_PRIVATE_KEY's value: the key text,
/// or a path to a file holding it (tauri-cli accepts both).
pub fn load_secret_key(value: &str, password: &str) -> Result<minisign::SecretKey, String> {
    let text = if Path::new(value).is_file() {
        std::fs::read_to_string(value).map_err(|e| format!("could not read the key file: {e}"))?
    } else {
        value.to_string()
    };
    let boxed = minisign::SecretKeyBox::from_string(&unb64(&text)?)
        .map_err(|e| format!("failed to load updater private key: {e}"))?;
    boxed
        .into_secret_key(Some(password.to_string()))
        .map_err(|_| "incorrect updater private key password".to_string())
}

/// The trusted comment: tauri-cli's two fields, then the version.
pub fn trusted_comment(timestamp: u64, file_name: &str, version: &str) -> String {
    format!("timestamp:{timestamp}\tfile:{file_name}\tversion:{version}")
}

/// The `.sig` file's content for `data`, published as `file_name`.
pub fn sign(
    sk: &minisign::SecretKey,
    data: &[u8],
    file_name: &str,
    version: &str,
    timestamp: u64,
) -> Result<String, String> {
    let sig = minisign::sign(
        None,
        sk,
        std::io::Cursor::new(data),
        Some(&trusted_comment(timestamp, file_name, version)),
        Some("signature from tauri secret key"),
    )
    .map_err(|e| format!("failed to sign file: {e}"))?;
    Ok(b64(&sig.to_string()))
}

/// The untrusted comment of a [`sign_plain`] signature.
pub const PLAIN_COMMENT: &str = "signature from the Method Reader update key";

/// A plain minisign signature (the `.minisig` text, not base64) of `data`,
/// published as `file_name`, with `file:<name>` as its trusted comment:
/// what `minisign -Vm <file> -P <key>` checks. SUITE-STANDARDS.md section 3
/// signs each release's `SHA256SUMS` this way, as Method Storyteller does.
/// Refuses a key whose public half is not `expected_pubkey` (base64 of a
/// minisign public-key box, as [`sp_update_config::PUBKEY`] is), so a
/// release can never carry a signature its own key does not check.
pub fn sign_plain(
    sk: &minisign::SecretKey,
    expected_pubkey: &str,
    data: &[u8],
    file_name: &str,
) -> Result<String, String> {
    let public = minisign::PublicKey::from_secret_key(sk)
        .map_err(|e| format!("the update key's public half: {e}"))?;
    let expected = public_key(expected_pubkey)?;
    if public.to_base64() != expected.to_base64() {
        return Err(
            "the update key is not the one Method Reader checks with (sp_update_config::PUBKEY)"
                .into(),
        );
    }
    let sig = minisign::sign(
        Some(&public),
        sk,
        std::io::Cursor::new(data),
        Some(&format!("file:{file_name}")),
        Some(PLAIN_COMMENT),
    )
    .map_err(|e| format!("signing {file_name}: {e}"))?;
    Ok(sig.to_string())
}

/// Whether `signature` (a `.minisig` text) is `pubkey`'s signature of
/// `data`. Returns the trusted comment.
pub fn verify_plain(pubkey: &str, data: &[u8], signature: &str) -> Result<String, String> {
    let pk = public_key(pubkey)?;
    let boxed = minisign::SignatureBox::from_string(signature)
        .map_err(|e| format!("not a minisign signature: {e}"))?;
    let trusted = boxed
        .trusted_comment()
        .map_err(|e| format!("no trusted comment: {e}"))?;
    minisign::verify(&pk, &boxed, std::io::Cursor::new(data), true, false, false)
        .map_err(|e| format!("the signature does not verify: {e}"))?;
    Ok(trusted)
}

fn public_key(b64_box: &str) -> Result<minisign::PublicKey, String> {
    minisign::PublicKeyBox::from_string(&unb64(b64_box)?)
        .and_then(|b| b.into_public_key())
        .map_err(|e| format!("the public key: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_signature_is_minisign_text_that_checks_against_the_key_and_only_the_key() {
        let (sk_text, pk) = keygen("").unwrap();
        let sk = load_secret_key(&sk_text, "").unwrap();
        let data = b"abc  Method-Reader-macOS.dmg\n";
        let sig = sign_plain(&sk, &pk, data, "SHA256SUMS").unwrap();
        // The .minisig text itself, as minisign writes it: four lines.
        assert!(
            sig.starts_with("untrusted comment: signature from the Method Reader update key\n")
        );
        assert!(sig.contains("\ntrusted comment: file:SHA256SUMS\n"));
        assert_eq!(sig.lines().count(), 4);
        assert_eq!(verify_plain(&pk, data, &sig).unwrap(), "file:SHA256SUMS");
        assert!(verify_plain(&pk, b"changed", &sig).is_err());
        let other = keygen("").unwrap().1;
        assert!(verify_plain(&other, data, &sig).is_err());
        // A key that is not the app's signs nothing.
        let err = sign_plain(&sk, &other, data, "SHA256SUMS").unwrap_err();
        assert!(
            err.contains("not the one Method Reader checks with"),
            "{err}"
        );
        // The shipped key reads as one.
        assert!(public_key(sp_update_config::PUBKEY).is_ok());
    }

    #[test]
    fn a_signature_made_here_passes_the_plugin_s_check_and_the_rust_app_s() {
        let (sk_text, pk) = keygen("").unwrap();
        let sk = load_secret_key(&sk_text, "").unwrap();
        let data = b"an update archive";
        let sig = sign(&sk, data, "Method-Reader-macOS.app.tar.gz", "0.12.0", 1).unwrap();
        // tauri-plugin-updater 2.12.0's rule set, requireSignedVersion off
        // (every Tauri window) and on (the Rust app).
        for required in [false, true] {
            let trusted =
                method_update::verify::verify_signature(data, &sig, &pk, "0.12.0", required)
                    .unwrap();
            assert_eq!(
                trusted,
                "timestamp:1\tfile:Method-Reader-macOS.app.tar.gz\tversion:0.12.0"
            );
        }
        assert!(method_update::verify::verify_signature(data, &sig, &pk, "0.12.1", false).is_err());
        assert!(
            method_update::verify::verify_signature(b"other", &sig, &pk, "0.12.0", false).is_err()
        );
    }

    #[test]
    fn a_password_protected_key_needs_its_password() {
        let (sk_text, _) = keygen("hunter2").unwrap();
        assert!(load_secret_key(&sk_text, "hunter2").is_ok());
        assert_eq!(
            load_secret_key(&sk_text, "").err().as_deref(),
            Some("incorrect updater private key password")
        );
        assert!(load_secret_key("not a key", "").is_err());
    }

    #[test]
    fn a_key_file_path_works_like_the_key_text() {
        let (sk_text, _) = keygen("").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("method-reader.key");
        std::fs::write(&f, &sk_text).unwrap();
        assert!(load_secret_key(f.to_str().unwrap(), "").is_ok());
    }
}

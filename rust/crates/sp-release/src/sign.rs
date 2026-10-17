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

#[cfg(test)]
mod tests {
    use super::*;

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
                sp_update::verify::verify_signature(data, &sig, &pk, "0.12.0", required).unwrap();
            assert_eq!(
                trusted,
                "timestamp:1\tfile:Method-Reader-macOS.app.tar.gz\tversion:0.12.0"
            );
        }
        assert!(sp_update::verify::verify_signature(data, &sig, &pk, "0.12.1", false).is_err());
        assert!(sp_update::verify::verify_signature(b"other", &sig, &pk, "0.12.0", false).is_err());
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

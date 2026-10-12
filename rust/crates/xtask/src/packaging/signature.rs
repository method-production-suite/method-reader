//! The two minisign containers the updater deals in: was
//! tools/update-signature.ts.
//!
//! [`parse_signature_box`] and [`parse_public_key_box`] READ a `.sig` and a
//! public key without any cryptography, exactly as the TypeScript did: the
//! bundle tool, the manifest builder and the tag-time gate only need to know
//! "is this the container it claims to be, and which key does it name?".
//!
//! New with the native app: tauri-cli no longer signs the updater archive,
//! so [`sign_file`] does what tauri-cli 2.11.4's `updater_signature.rs`
//! did, with the same `minisign` crate at the same version, and
//! [`verify_file`] checks a signature against the public key the shipped
//! app trusts, the way tauri-plugin-updater does at install time.
//!
//! Shapes, from minisign's own format:
//!
//! ```text
//! signature box   untrusted comment: <text>
//!                 <base64 of 74 bytes: alg(2) key id(8) signature(64)>
//!                 trusted comment: <text>
//!                 <base64 of 64 bytes: the global signature>
//!
//! public key box  untrusted comment: <text>
//!                 <base64 of 42 bytes: alg(2) key id(8) key(32)>
//! ```
//!
//! The key id is the 8 bytes read little-endian, printed as uppercase hex,
//! taken from the BYTES and never from the untrusted comment.

use std::io::Cursor;
use std::path::Path;

use base64::Engine;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};

const SIGNATURE_BYTES: usize = 74;
const PUBLIC_KEY_BYTES: usize = 42;
const GLOBAL_SIGNATURE_BYTES: usize = 64;

/// Buffer.from(x, 'base64') once the alphabet check has passed: padding
/// optional, trailing bits ignored.
const LENIENT: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::Indifferent)
        .with_decode_allow_trailing_bits(true),
);

/// What a `.sig` says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureBox {
    /// Uppercase hex of the little-endian u64 key id.
    pub key_id: String,
    /// The `file:` field of the trusted comment, when there is one.
    pub file_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKeyBox {
    pub key_id: String,
}

/// Strict base64: characters outside the alphabet are refused rather than
/// skipped, which is what Buffer.from would silently do.
fn decode_base64(what: &str, encoded: &str) -> Result<Vec<u8>, String> {
    let trimmed = encoded.trim();
    if trimmed.is_empty() {
        return Err(format!("{what}: empty; nothing to decode"));
    }
    let body = trimmed.trim_end_matches('=');
    if body.is_empty()
        || !body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
    {
        return Err(format!(
            "{what}: not base64 (it contains characters outside the base64 alphabet)"
        ));
    }
    // Buffer.from drops a dangling sixth bit group; so does trimming to a
    // whole number of 4-character units minus the impossible single one.
    let usable = if body.len() % 4 == 1 {
        &body[..body.len() - 1]
    } else {
        body
    };
    LENIENT
        .decode(usable)
        .map_err(|e| format!("{what}: not base64 ({e})"))
}

fn key_id_of(bytes: &[u8]) -> String {
    let mut id = [0u8; 8];
    id.copy_from_slice(&bytes[2..10]);
    format!("{:X}", u64::from_le_bytes(id))
}

/// `text.trimEnd().split('\n')`.
fn lines_of(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .trim_end()
        .split('\n')
        .map(str::to_string)
        .collect()
}

/// Reads a `.sig` file's contents (the base64 tauri-cli wrote) and says
/// which key it names. Refuses, naming the defect, anything that is not a
/// minisign signature box.
pub fn parse_signature_box(encoded: &str) -> Result<SignatureBox, String> {
    let lines = lines_of(&decode_base64("signature", encoded)?);
    if lines.len() != 4 {
        return Err(format!(
            "signature: expected four lines (untrusted comment, signature, trusted comment, global signature), found {}",
            lines.len()
        ));
    }
    let (untrusted, signature, trusted, global) = (&lines[0], &lines[1], &lines[2], &lines[3]);
    if !untrusted.starts_with("untrusted comment: ") {
        let head: String = untrusted.chars().take(40).collect();
        return Err(format!(
            "signature: does not begin with \"untrusted comment: \" (begins {})",
            serde_json::to_string(&head).unwrap_or_default()
        ));
    }
    let sig = decode_base64("signature line", signature)?;
    if sig.len() != SIGNATURE_BYTES {
        return Err(format!(
            "signature: the signature line decodes to {} bytes, not the {SIGNATURE_BYTES} of a minisign signature",
            sig.len()
        ));
    }
    if !trusted.starts_with("trusted comment: ") {
        return Err("signature: third line is not a \"trusted comment: \" line".into());
    }
    let global = decode_base64("global signature line", global)?;
    if global.len() != GLOBAL_SIGNATURE_BYTES {
        return Err(format!(
            "signature: the global signature decodes to {} bytes, not {GLOBAL_SIGNATURE_BYTES}",
            global.len()
        ));
    }
    let file_name = trusted.split_once("\tfile:").and_then(|(_, rest)| {
        let name = rest.split('\t').next().unwrap_or("");
        (!name.is_empty()).then(|| name.to_string())
    });
    Ok(SignatureBox {
        key_id: key_id_of(&sig),
        file_name,
    })
}

/// Reads a public key as the app's config carries it (base64 of the box)
/// and says which key it is.
pub fn parse_public_key_box(encoded: &str) -> Result<PublicKeyBox, String> {
    if encoded.trim().is_empty() {
        return Err(
            "public key: empty. tauri.conf.json ships with an empty pubkey until the owner generates the key pair and pastes the public half in (docs/release-secrets.md §4)."
                .into(),
        );
    }
    let lines = lines_of(&decode_base64("public key", encoded)?);
    if lines.len() != 2 || !lines[0].starts_with("untrusted comment: ") {
        return Err(format!(
            "public key: expected an \"untrusted comment: \" line and a key line, found {} line(s)",
            lines.len()
        ));
    }
    let key = decode_base64("public key line", &lines[1])?;
    if key.len() != PUBLIC_KEY_BYTES {
        return Err(format!(
            "public key: the key line decodes to {} bytes, not the {PUBLIC_KEY_BYTES} of a minisign public key",
            key.len()
        ));
    }
    Ok(PublicKeyBox {
        key_id: key_id_of(&key),
    })
}

/// The base64 text of the box, decoded to the box itself.
fn box_text(what: &str, encoded: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|e| format!("{what}: failed to decode base64 ({e})"))?;
    String::from_utf8(bytes).map_err(|_| format!("{what}: the decoded box is not UTF-8"))
}

/// TAURI_SIGNING_PRIVATE_KEY is a path or the key text itself, as tauri-cli
/// reads it.
pub fn private_key_text(value: &str) -> Result<String, String> {
    let path = Path::new(value);
    if !value.is_empty() && path.exists() {
        std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read private key from file {value}: {e}"))
    } else {
        Ok(value.to_string())
    }
}

/// Signs `file` the way tauri-cli does and returns the `.sig` text (base64
/// of the signature box): untrusted comment "signature from tauri secret
/// key", trusted comment `timestamp:<unix>\tfile:<name>`.
pub fn sign_file(
    file: &Path,
    private_key: &str,
    password: &str,
    timestamp: u64,
) -> Result<String, String> {
    let sk_box = minisign::SecretKeyBox::from_string(&box_text("private key", private_key)?)
        .map_err(|e| format!("failed to load updater private key: {e}"))?;
    let sk = sk_box
        .into_secret_key(Some(password.to_string()))
        .map_err(|e| format!("incorrect updater private key password: {e}"))?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let data = std::fs::read(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let trusted = format!("timestamp:{timestamp}\tfile:{name}");
    let sig = minisign::sign(
        None,
        &sk,
        Cursor::new(data),
        Some(trusted.as_str()),
        Some("signature from tauri secret key"),
    )
    .map_err(|e| format!("failed to sign {}: {e}", file.display()))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(sig.to_string()))
}

/// Checks `signature` (the `.sig` text) over `file` against `public_key`
/// (the base64 the app's config carries), as the updater will at install
/// time: legacy signatures allowed, as tauri-plugin-updater allows them.
pub fn verify_file(file: &Path, signature: &str, public_key: &str) -> Result<(), String> {
    let pk = minisign::PublicKeyBox::from_string(&box_text("public key", public_key)?)
        .and_then(|b| b.into_public_key())
        .map_err(|e| format!("public key: {e}"))?;
    let sig = minisign::SignatureBox::from_string(&box_text("signature", signature)?)
        .map_err(|e| format!("signature: {e}"))?;
    let data = std::fs::read(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    minisign::verify(&pk, &sig, Cursor::new(data), true, false, true).map_err(|e| {
        format!(
            "{} does not verify against the app's public key: {e}",
            file.display()
        )
    })
}

/// `plugins.updater.pubkey` of a tauri.conf.json.
pub fn pubkey_from_config(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let conf: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    conf.pointer("/plugins/updater/pubkey")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "public key: plugins.updater.pubkey is missing from the config".into())
}

/// `cargo xtask update-signature --pubkey-from-config <tauri.conf.json>`:
/// release.yml's tag-time gate. 0 and the key id when the config trusts a
/// real key; 1 and a reason otherwise; 2 for a usage error.
pub fn main(args: &[String]) -> u8 {
    use super::util::{FlagSpec, parse_flags};
    let usage = "usage: cargo xtask update-signature --pubkey-from-config <tauri.conf.json>";
    if super::util::wants_help(args, usage) {
        return 0;
    }
    let flags = match parse_flags(
        args,
        &[FlagSpec {
            name: "pubkey-from-config",
            value: true,
        }],
    ) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{e}\n{usage}");
            return 2;
        }
    };
    let Some(path) = flags.get("pubkey-from-config") else {
        eprintln!("{usage}");
        return 2;
    };
    match pubkey_from_config(Path::new(path)).and_then(|k| parse_public_key_box(&k)) {
        Ok(k) => {
            println!("the app trusts updater key {}", k.key_id);
            0
        }
        Err(e) => {
            eprintln!("::error::{e}");
            eprintln!(
                "::error::A tag cannot ship an updater that trusts no key. Generate the pair, add the secrets, and paste the PUBLIC key into desktop/src-tauri/tauri.conf.json: docs/release-secrets.md §4."
            );
            1
        }
    }
}

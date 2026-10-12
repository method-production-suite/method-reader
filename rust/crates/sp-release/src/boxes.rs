//! The two minisign containers the updater deals in, READ without any
//! cryptography: was the reading half of tools/update-signature.ts. The
//! bundle tool, the manifest builder, check-latest and the tag-time gate
//! only need to know "is this the container it claims to be, and which key
//! does it name?"; checking a signature is [`sp_update::verify`]'s.
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
//! taken from the BYTES and never from the untrusted comment. Base64 is
//! read as Node's Buffer read it, but strictly enough that text outside the
//! alphabet is refused rather than skipped.

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

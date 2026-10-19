//! Port of tests/update-signature.test.ts (14 checks): reading the two
//! minisign containers without any cryptography. Plus the half the native
//! window adds: signing the archive the way tauri-cli did, and checking it
//! against the key the app trusts.

mod packaging_common;

use std::fs;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use packaging_common::*;
use xtask::packaging::signature::{
    parse_public_key_box, parse_signature_box, sign_file, verify_file,
};

const OTHER_ID: [u8; 8] = [0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18];

// ---- a .sig file ----

#[test]
fn a_well_formed_box_is_read_and_reports_the_key_that_made_it() {
    let parsed =
        parse_signature_box(&fake_signature_box("Screepub.app.tar.gz", FAKE_KEY_ID)).unwrap();
    assert_eq!(parsed.key_id, key_id_hex(FAKE_KEY_ID));
    assert_eq!(parsed.file_name.as_deref(), Some("Screepub.app.tar.gz"));
}

#[test]
fn a_trailing_newline_or_none_makes_no_difference() {
    let b = fake_signature_box("x.tar.gz", FAKE_KEY_ID);
    let trimmed = b64(unb64(&b).trim_end());
    assert_eq!(
        parse_signature_box(&trimmed).unwrap().key_id,
        parse_signature_box(&b).unwrap().key_id
    );
}

#[test]
fn a_404_page_where_a_signature_should_be_is_refused_and_says_so() {
    err_contains(
        parse_signature_box(&b64("<html><body>Not Found</body></html>")),
        "untrusted comment",
    );
}

#[test]
fn something_that_is_not_base64_at_all_is_refused() {
    err_contains(parse_signature_box("this is not base64!!"), "base64");
}

#[test]
fn an_empty_string_is_refused_rather_than_read_as_an_empty_signature() {
    assert!(parse_signature_box("").is_err());
}

fn sig_bytes(sig_len: usize) -> Vec<u8> {
    let mut s = b"ED".to_vec();
    s.extend_from_slice(&FAKE_KEY_ID);
    s.extend(std::iter::repeat_n(7u8, sig_len));
    s
}

#[test]
fn a_signature_line_of_the_wrong_length_is_refused() {
    let text = format!(
        "untrusted comment: signature from tauri secret key\n{}\ntrusted comment: timestamp:1\tfile:x\n{}\n",
        STANDARD.encode(sig_bytes(63)),
        STANDARD.encode([9u8; 64])
    );
    err_contains(parse_signature_box(&b64(&text)), "74");
}

#[test]
fn a_box_missing_its_trusted_comment_and_global_signature_is_refused() {
    let text = format!(
        "untrusted comment: signature from tauri secret key\n{}\n",
        STANDARD.encode(sig_bytes(64))
    );
    err_contains(parse_signature_box(&b64(&text)), "trusted comment");
}

#[test]
fn a_global_signature_of_the_wrong_length_is_refused() {
    let text = format!(
        "untrusted comment: signature from tauri secret key\n{}\ntrusted comment: timestamp:1\tfile:x\n{}\n",
        STANDARD.encode(sig_bytes(64)),
        STANDARD.encode([9u8; 32])
    );
    err_contains(parse_signature_box(&b64(&text)), "64");
}

// ---- a public key ----

#[test]
fn a_well_formed_public_key_is_read_and_reports_its_key_id() {
    assert_eq!(
        parse_public_key_box(&fake_public_key_box(FAKE_KEY_ID))
            .unwrap()
            .key_id,
        key_id_hex(FAKE_KEY_ID)
    );
}

#[test]
fn the_key_id_comes_from_the_bytes_not_from_the_comment_line() {
    let lied =
        unb64(&fake_public_key_box(OTHER_ID)).replace(&key_id_hex(OTHER_ID), "DEADBEEFDEADBEEF");
    assert_eq!(
        parse_public_key_box(&b64(&lied)).unwrap().key_id,
        key_id_hex(OTHER_ID)
    );
}

#[test]
fn an_empty_public_key_is_refused_because_that_is_what_the_config_ships_with() {
    err_contains(parse_public_key_box(""), "empty");
}

#[test]
fn a_key_of_the_wrong_length_is_refused() {
    let mut key = b"Ed".to_vec();
    key.extend_from_slice(&FAKE_KEY_ID);
    key.extend_from_slice(&[3u8; 31]);
    let text = format!(
        "untrusted comment: minisign public key: X\n{}\n",
        STANDARD.encode(&key)
    );
    err_contains(parse_public_key_box(&b64(&text)), "42");
}

#[test]
fn the_tag_time_gate_checks_the_key_the_app_trusts_and_says_yes() {
    // With no flag the gate reads sp_update_config::PUBKEY, the one home of the
    // key the shipped app trusts (was: plugins.updater.pubkey of the Tauri
    // window's tauri.conf.json, read with --pubkey-from-config).
    let (code, out, err) = xtask(&["update-signature"]);
    assert_eq!(code, 0, "{err}");
    let id = parse_public_key_box(sp_update_config::PUBKEY)
        .unwrap()
        .key_id;
    assert_eq!(id, "E4D0668D54F6EB34");
    assert!(out.contains(&id), "{out}");
}

#[test]
fn the_tag_time_gate_checks_an_explicit_key_and_says_yes_or_no() {
    let s = Scratch::new("pubkey-gate");
    let (code, out, _) = xtask(&[
        "update-signature",
        "--pubkey",
        &fake_public_key_box(FAKE_KEY_ID),
    ]);
    assert_eq!(code, 0);
    assert!(out.contains(&key_id_hex(FAKE_KEY_ID)), "{out}");

    // A file holding the key, as the private key may be given.
    let file = s.join("updater.pub");
    fs::write(&file, fake_public_key_box(OTHER_ID)).unwrap();
    let (code, out, _) = xtask(&["update-signature", "--pubkey", file.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains(&key_id_hex(OTHER_ID)), "{out}");

    let (code, _, err) = xtask(&["update-signature", "--pubkey", ""]);
    assert_eq!(code, 1);
    assert!(err.to_lowercase().contains("empty"), "{err}");
    assert!(err.contains("release-secrets"), "{err}");
    assert!(err.contains("sp-update-config"), "{err}");

    let (code, _, _) = xtask(&["update-signature", "--pubkey", "bm90IGEga2V5"]);
    assert_eq!(code, 1);

    // A malformed box: the right shape with a key of the wrong length.
    let mut key = b"Ed".to_vec();
    key.extend_from_slice(&FAKE_KEY_ID);
    key.extend_from_slice(&[3u8; 31]);
    let short = b64(&format!(
        "untrusted comment: minisign public key: X\n{}\n",
        STANDARD.encode(&key)
    ));
    let (code, _, err) = xtask(&["update-signature", "--pubkey", &short]);
    assert_eq!(code, 1);
    assert!(err.contains("42"), "{err}");

    // The retired flag, a missing value or a stray argument is a usage
    // error, not a pass.
    let conf = s.join("tauri.conf.json");
    fs::write(&conf, "{}").unwrap();
    let (code, _, err) = xtask(&[
        "update-signature",
        "--pubkey-from-config",
        conf.to_str().unwrap(),
    ]);
    assert_eq!(code, 2);
    assert!(err.contains("--pubkey-from-config"), "{err}");
    let (code, _, _) = xtask(&["update-signature", "--pubkey"]);
    assert_eq!(code, 2);
    let (code, _, _) = xtask(&["update-signature", "stray"]);
    assert_eq!(code, 2);
}

#[test]
fn a_signature_and_a_public_key_from_the_same_pair_agree_on_the_key_id() {
    assert_eq!(
        parse_signature_box(&fake_signature_box("a", FAKE_KEY_ID))
            .unwrap()
            .key_id,
        parse_public_key_box(&fake_public_key_box(FAKE_KEY_ID))
            .unwrap()
            .key_id
    );
    assert_ne!(
        parse_signature_box(&fake_signature_box("a", OTHER_ID))
            .unwrap()
            .key_id,
        parse_public_key_box(&fake_public_key_box(FAKE_KEY_ID))
            .unwrap()
            .key_id
    );
}

// ---- signing, now that tauri-cli does not: sp-release's signer, with the
// pinned minisign, opening tauri-cli's own published test key ----

#[test]
fn a_signed_archive_is_a_box_the_readers_accept_and_the_app_s_key_verifies() {
    let s = Scratch::new("sign");
    let file = s.join("Screepub.app.tar.gz");
    fs::write(&file, b"gzip bytes, as far as a signature cares").unwrap();
    let sig = sign_file(&file, TAURI_TEST_KEY, "", "0.12.0", 1_758_470_000).unwrap();

    // The container tauri-cli writes: what the bundle tool and the manifest
    // builder read back.
    let parsed = parse_signature_box(&sig).unwrap();
    assert_eq!(parsed.file_name.as_deref(), Some("Screepub.app.tar.gz"));
    let text = unb64(&sig);
    assert!(text.starts_with("untrusted comment: signature from tauri secret key\n"));
    // tauri-cli's two fields, then the version the Rust app requires.
    assert!(text.contains(
        "trusted comment: timestamp:1758470000\tfile:Screepub.app.tar.gz\tversion:0.12.0\n"
    ));

    let pk = tauri_test_public_key();
    assert_eq!(parse_public_key_box(&pk).unwrap().key_id, parsed.key_id);
    verify_file(&file, &sig, &pk, "0.12.0").unwrap();
    // Signed for one version, it does not pass for another.
    assert!(verify_file(&file, &sig, &pk, "0.12.1").is_err());
}

#[test]
fn a_changed_byte_or_another_key_fails_verification() {
    let s = Scratch::new("verify");
    let file = s.join("Screepub.app.tar.gz");
    fs::write(&file, b"the archive").unwrap();
    let sig = sign_file(&file, TAURI_TEST_KEY, "", "0.12.0", 1).unwrap();
    let pk = tauri_test_public_key();

    fs::write(&file, b"the archivE").unwrap();
    assert!(verify_file(&file, &sig, &pk, "0.12.0").is_err());

    fs::write(&file, b"the archive").unwrap();
    let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let other_pk = STANDARD.encode(other.pk.to_box().unwrap().to_string());
    assert!(verify_file(&file, &sig, &other_pk, "0.12.0").is_err());
}

#[test]
fn a_wrong_password_or_a_key_that_is_not_one_is_refused_by_name() {
    let s = Scratch::new("badkey");
    let file = s.join("a");
    fs::write(&file, b"x").unwrap();
    err_contains(
        sign_file(&file, TAURI_TEST_KEY, "not the password", "0.12.0", 1),
        "password",
    );
    err_contains(
        sign_file(&file, &b64("not a key"), "", "0.12.0", 1),
        "private key",
    );
}

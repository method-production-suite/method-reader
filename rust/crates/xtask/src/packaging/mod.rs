//! Building, checking and smoke-testing the app bundles and the updater's
//! files: was tools/build-app-bundle.ts, bundle-archive.ts, smoke-bundle.ts,
//! dmg.ts, update-signature.ts, build-update-manifest.ts and
//! build-update-compare.ts.
//!
//! - [`kinds`] holds the artifact table (installers, the updater archive).
//! - [`bundle`] holds `build-app-bundle`, for the Tauri window and the native one.
//! - [`native`] holds assembling the native window's .app, .deb, .rpm and NSIS
//!   script, and its updater archive.
//! - [`archive`] holds reading (and writing) ar, tar, cpio and rpm containers.
//! - [`smoke`] holds `smoke-bundle`, and mounting a DMG.
//! - [`signature`] and [`manifest`] hold the updater's signature and latest.json.
//! - [`compare`] holds desktop/ui/update-compare.js.

pub mod archive;
pub mod bundle;
pub mod compare;
pub mod kinds;
pub mod manifest;
pub mod native;
pub mod signature;
pub mod util;

/// `cargo xtask build-app-bundle`: its exit code.
pub fn build_app_bundle_main(args: &[String]) -> u8 {
    bundle::main(args)
}

/// `cargo xtask bundle-archive`: its exit code.
pub fn bundle_archive_main(args: &[String]) -> u8 {
    archive::main(args)
}

/// `cargo xtask smoke-bundle`: its exit code.
pub fn smoke_bundle_main(_args: &[String]) -> u8 {
    eprintln!("xtask: smoke-bundle is not ported yet");
    2
}

/// `cargo xtask update-signature`: its exit code.
pub fn update_signature_main(args: &[String]) -> u8 {
    signature::main(args)
}

/// `cargo xtask build-update-manifest`: its exit code.
pub fn build_update_manifest_main(args: &[String]) -> u8 {
    manifest::main(args)
}

/// `cargo xtask build-update-compare`: its exit code.
pub fn build_update_compare_main(args: &[String]) -> u8 {
    compare::main(args)
}

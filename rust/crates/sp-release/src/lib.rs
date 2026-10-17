//! Method Reader's release tools, in Rust: what tools/build-app-bundle.ts,
//! tools/build-cli.ts (its packaging half), tools/build-update-manifest.ts,
//! tools/update-signature.ts and tools/check-latest.ts did for the Tauri
//! window, for the egui app. Each command is a thin caller of a tested
//! function here; release.yml calls the binary, so every release step can
//! also be run by hand on a laptop.
//!
//! It never signs code or notarizes: that is Apple's codesign and
//! notarytool, in release.yml, with the secrets docs/release-secrets.md
//! lists. It signs the update archive with the minisign key only when the
//! key is in its environment. The design is
//! docs/superpowers/specs/2026-10-11-rust-release-design.md.

pub mod archive;
pub mod boxes;
pub mod check_latest;
pub mod linux;
pub mod mac;
pub mod manifest;
pub mod names;
pub mod nsis;
pub mod serve;
pub mod sign;
pub mod version;

//! Building and checking the screepub CLI: was tools/build-cli.ts, smoke-cli.ts, verify-signing.ts, and the retired build-sidecar.ts and sidecar-targets.ts.
//!
//! Not ported yet: placeholders so every command is listed from the start.

/// `cargo xtask build-cli`: its exit code.
pub fn build_cli_main(_args: &[String]) -> u8 {
    eprintln!("xtask: build-cli is not ported yet");
    2
}

/// `cargo xtask smoke-cli`: its exit code.
pub fn smoke_cli_main(_args: &[String]) -> u8 {
    eprintln!("xtask: smoke-cli is not ported yet");
    2
}

/// `cargo xtask verify-signing`: its exit code.
pub fn verify_signing_main(_args: &[String]) -> u8 {
    eprintln!("xtask: verify-signing is not ported yet");
    2
}

/// `cargo xtask build-sidecar`: the Rust app links sp-engine directly, so
/// there is no engine sidecar to compile (rust/PARITY.md, "The engine
/// sidecar"). Exit 2, the code a refused tool answers, with the reason.
pub fn build_sidecar_main(_args: &[String]) -> u8 {
    eprintln!(
        "xtask build-sidecar: retired. The Rust app links the engine directly, so there is no sidecar to build. Build the CLI with `cargo xtask build-cli`."
    );
    2
}

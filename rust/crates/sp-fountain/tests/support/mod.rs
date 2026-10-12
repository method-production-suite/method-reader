//! Running fountain-js itself, for the tests that use it as the oracle.
//!
//! Needs `bun` and the repo's `node_modules` (`bun install`). Tests that
//! call this are `#[ignore]`d so CI's rust job, which has neither, skips
//! them; run them with `cargo test -p sp-fountain -- --ignored`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

const RUNNER: &str = r#"
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const { Fountain } = require(process.argv[2]);
const { Lexer } = require(join(process.argv[2], 'dist/lexer.js'));
const { rules } = require(join(process.argv[2], 'dist/rules.js'));
const inputs = JSON.parse(readFileSync(process.argv[3], 'utf8'));
// fountain-js keeps two module statics across parses; the port starts every
// parse fresh (PARITY.md Decisions), so reset them to a first parse's.
const out = inputs.map((s) => {
  Lexer.lastLineWasDualDialogue = undefined;
  rules.blank_lines.lastIndex = 0;
  try {
    return { tokens: new Fountain().parse(s, true).tokens };
  } catch (e) {
    return { error: e.message };
  }
});
process.stdout.write(JSON.stringify(out));
"#;

/// fountain-js's answer for each input, parsed in order in one process:
/// `{"tokens": [...]}` or `{"error": "..."}`.
pub fn fountain_js(inputs: &[String]) -> Vec<Value> {
    let dir = std::env::temp_dir().join(format!("sp-fountain-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("runner.cjs");
    let data = dir.join("inputs.json");
    std::fs::write(&script, RUNNER).unwrap();
    std::fs::write(&data, serde_json::to_string(inputs).unwrap()).unwrap();
    let module = repo_root().join("node_modules/fountain-js");
    assert!(
        module.exists(),
        "fountain-js is not installed: run `bun install` at the repo root"
    );
    let bun = std::env::var("BUN").unwrap_or_else(|_| "bun".to_owned());
    let out = Command::new(bun)
        .arg(&script)
        .arg(&module)
        .arg(&data)
        .output()
        .expect("bun runs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "bun failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

//! `cargo xtask oracle`: regenerate the oracle dumps. The dump tool is
//! TypeScript because it IS the TypeScript engine, asked to show its work;
//! this only starts it.

use std::process::Command;

use crate::paths;

pub fn run(args: &[String]) -> Result<bool, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("usage: cargo xtask oracle [--committed-only] [--real <dir>]");
        return Ok(true);
    }
    let mut pass: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--committed-only" => pass.push("--committed-only"),
            "--real" => {
                pass.push("--real");
                pass.push(it.next().ok_or("--real needs a folder")?);
            }
            other => return Err(format!("oracle: unknown option {other}")),
        }
    }
    let status = Command::new(paths::bun())
        .arg("tools/oracle/dump-all.ts")
        .args(&pass)
        .current_dir(paths::repo_root())
        .status()
        .map_err(|e| format!("could not start bun ({e}); install it or set $BUN"))?;
    Ok(status.success())
}

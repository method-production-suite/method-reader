//! The window binary with no window, for the bundle smoke test (xtask
//! smoke-bundle, after tools/smoke-bundle.ts): it proves the engine linked
//! into a built app runs.
//!
//! - `--version --json`: `{"ok":true,"version":"…"}`, as `method-reader` prints it.
//! - `--smoke <pdf> [--out <file.epub>]`: convert one PDF with the linked
//!   engine and print its `--json` result. Without `--out` the book goes to
//!   a fresh folder under the system's temp folder, named in the result
//!   (`epubPath`); the caller removes it.
//!
//! Both print exactly what the `method-reader` binary prints for the same
//! command, because they are the same `sp_cli::run`.

use std::path::{Path, PathBuf};

/// What a headless run would do, or None: open the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Headless {
    Version,
    Smoke { pdf: String, out: Option<String> },
    Usage(String),
}

pub fn parse(args: &[String]) -> Option<Headless> {
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("--version") {
        return Some(Headless::Version);
    }
    let at = args.iter().position(|a| a == "--smoke")?;
    let Some(pdf) = args.get(at + 1).filter(|a| !a.starts_with("--")) else {
        return Some(Headless::Usage(
            "--smoke needs a PDF: method-reader-app --smoke <file.pdf> [--out <file.epub>]".into(),
        ));
    };
    let out = match args.iter().position(|a| a == "--out") {
        Some(i) => match args.get(i + 1) {
            Some(o) => Some(o.clone()),
            None => return Some(Headless::Usage("--out needs a file name".into())),
        },
        None => None,
    };
    Some(Headless::Smoke {
        pdf: pdf.clone(),
        out,
    })
}

/// A fresh folder for a smoke book.
fn smoke_folder() -> std::io::Result<PathBuf> {
    let base = std::env::temp_dir();
    for n in 0..1000u32 {
        let dir = base.join(format!("method-reader-smoke-{}-{n}", std::process::id()));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("no free temp folder name"))
}

/// The argv the smoke runs, as tools/smoke-bundle.ts ran the sidecar.
pub fn smoke_argv(pdf: &str, epub: &str) -> Vec<String> {
    [pdf, "-o", epub, "--no-fountain", "--json"]
        .into_iter()
        .map(String::from)
        .collect()
}

/// Run it on this process's streams; the exit code.
pub fn run(what: Headless) -> u8 {
    match what {
        Headless::Version => sp_cli::run(
            &["--version".into(), "--json".into()],
            Box::new(sp_cli::Std),
        ),
        Headless::Usage(message) => {
            let error =
                serde_json::json!({"ok": false, "error": {"code": "usage", "message": message}});
            println!("{error}");
            1
        }
        Headless::Smoke { pdf, out } => {
            let epub = match out {
                Some(o) => o,
                None => match smoke_folder() {
                    Ok(dir) => {
                        let stem = Path::new(&pdf)
                            .file_stem()
                            .map_or("smoke".into(), |s| s.to_string_lossy().into_owned());
                        dir.join(format!("{stem}.epub"))
                            .to_string_lossy()
                            .into_owned()
                    }
                    Err(e) => {
                        let error = serde_json::json!({"ok": false, "error": {"code": "internal", "message": format!("could not make a temp folder: {e}")}});
                        println!("{error}");
                        return 1;
                    }
                },
            };
            sp_cli::run(&smoke_argv(&pdf, &epub), Box::new(sp_cli::Std))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn the_window_opens_unless_asked_otherwise() {
        assert_eq!(parse(&[]), None);
        assert_eq!(parse(&a(&["-psn_0_1234"])), None);
        assert_eq!(parse(&a(&["--version", "--json"])), Some(Headless::Version));
        assert_eq!(
            parse(&a(&["--smoke", "a.pdf", "--out", "/t/a.epub"])),
            Some(Headless::Smoke {
                pdf: "a.pdf".into(),
                out: Some("/t/a.epub".into())
            })
        );
        assert!(matches!(parse(&a(&["--smoke"])), Some(Headless::Usage(_))));
    }

    #[test]
    fn the_smoke_converts_as_the_sidecar_smoke_did() {
        assert_eq!(
            smoke_argv("f.pdf", "/t/f.epub"),
            a(&["f.pdf", "-o", "/t/f.epub", "--no-fountain", "--json"])
        );
    }
}

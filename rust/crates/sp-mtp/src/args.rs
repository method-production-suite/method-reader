//! The command line, parsed into what main.rs runs. Every refusal is a
//! `usage` failure in plain words; a flag this helper does not take is
//! refused rather than ignored, the engine's rule for its own verbs.

use crate::output::{Code, Failure};
use std::path::PathBuf;

pub const USAGE: &str = "method-reader-mtp: list Kindles connected over MTP, and copy a file to one

Usage:
  method-reader-mtp list [--kindles] [--storages]
  method-reader-mtp put <serial> <local-file> <device-path> [--wait-ms <ms>] [--keep-session]
  method-reader-mtp ls <serial> [<device-folder>]
  method-reader-mtp --version

Every run prints one JSON object on stdout.

list            every MTP device, and every Amazon device that is not a drive
  --kindles     Kindles only, from USB descriptors alone: opens nothing
  --storages    also open each device and read its storages (slow; for people)
put             copy <local-file> to <device-path> (e.g. documents/Title.kfx)
                on the Kindle with that USB serial. Refuses any other device
  --wait-ms     keep retrying for this long while another program holds it
  --keep-session  leave the MTP session open afterwards (a hardware-day test)
ls              what is in a folder on the Kindle (default: the storage's top
                level). Reads only; for checking a copy by hand";

/// Upper bound on --wait-ms: a minute is longer than any release takes, and
/// a typo of an extra zero must not hang the engine's send for an hour.
pub const MAX_WAIT_MS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Version,
    Help,
    List {
        kindles_only: bool,
        storages: bool,
    },
    Put {
        serial: String,
        local: PathBuf,
        device_path: String,
        wait_ms: u64,
        keep_session: bool,
    },
    Ls {
        serial: String,
        folder: Option<String>,
    },
}

fn usage(message: impl Into<String>) -> Failure {
    Failure::new(Code::Usage, message)
}

pub fn parse(argv: &[String]) -> Result<Command, Failure> {
    let mut rest = argv.iter();
    let verb = match rest.next() {
        None => return Err(usage("say what to do: list or put (see --help)")),
        Some(v) => v.as_str(),
    };
    match verb {
        "--version" => {
            if let Some(extra) = rest.next() {
                return Err(usage(format!(
                    "--version takes nothing after it (got \"{extra}\")"
                )));
            }
            Ok(Command::Version)
        }
        "-h" | "--help" => Ok(Command::Help),
        "list" => {
            let mut kindles_only = false;
            let mut storages = false;
            for arg in rest {
                match arg.as_str() {
                    "--kindles" => kindles_only = true,
                    "--storages" => storages = true,
                    other => {
                        return Err(usage(format!(
                            "list takes --kindles and --storages, not \"{other}\""
                        )));
                    }
                }
            }
            Ok(Command::List {
                kindles_only,
                storages,
            })
        }
        "put" => {
            let mut positionals: Vec<String> = Vec::new();
            let mut wait_ms = 0u64;
            let mut keep_session = false;
            let mut args = rest.peekable();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--keep-session" => keep_session = true,
                    "--wait-ms" => {
                        let value = args
                            .next()
                            .ok_or_else(|| usage("--wait-ms needs a number of milliseconds"))?;
                        wait_ms = value.parse::<u64>().map_err(|_| {
                            usage(format!(
                                "--wait-ms needs a whole number of milliseconds, not \"{value}\""
                            ))
                        })?;
                        if wait_ms > MAX_WAIT_MS {
                            return Err(usage(format!(
                                "--wait-ms is at most {MAX_WAIT_MS} (a minute)"
                            )));
                        }
                    }
                    flag if flag.starts_with("--") => {
                        return Err(usage(format!(
                            "put takes --wait-ms and --keep-session, not \"{flag}\""
                        )));
                    }
                    _ => positionals.push(arg.clone()),
                }
            }
            if positionals.len() != 3 {
                return Err(usage(format!(
                    "put needs exactly three things: the Kindle's serial, the file, and where it goes (got {})",
                    positionals.len()
                )));
            }
            let device_path = positionals.pop().unwrap();
            let local = PathBuf::from(positionals.pop().unwrap());
            let serial = positionals.pop().unwrap();
            if serial.trim().is_empty() {
                return Err(usage("the Kindle's serial is empty"));
            }
            Ok(Command::Put {
                serial,
                local,
                device_path,
                wait_ms,
                keep_session,
            })
        }
        "ls" => {
            let rest: Vec<String> = rest.cloned().collect();
            if let Some(flag) = rest.iter().find(|a| a.starts_with("--")) {
                return Err(usage(format!("ls takes no flags (got \"{flag}\")")));
            }
            match rest.as_slice() {
                [serial] if !serial.trim().is_empty() => Ok(Command::Ls {
                    serial: serial.clone(),
                    folder: None,
                }),
                [serial, folder] if !serial.trim().is_empty() => Ok(Command::Ls {
                    serial: serial.clone(),
                    folder: Some(folder.clone()),
                }),
                _ => Err(usage(
                    "ls needs the Kindle's serial, and optionally a folder on it, like documents",
                )),
            }
        }
        other => Err(usage(format!(
            "no command is called \"{other}\": use list, put or ls (see --help)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn list_and_its_two_flags() {
        assert_eq!(
            parse(&argv(&["list"])).unwrap(),
            Command::List {
                kindles_only: false,
                storages: false
            }
        );
        assert_eq!(
            parse(&argv(&["list", "--kindles", "--storages"])).unwrap(),
            Command::List {
                kindles_only: true,
                storages: true
            }
        );
    }

    #[test]
    fn put_with_three_positionals_and_its_flags_in_any_order() {
        let got = parse(&argv(&[
            "put",
            "--wait-ms",
            "5000",
            "G0X",
            "/tmp/a.kfx",
            "documents/a.kfx",
            "--keep-session",
        ]))
        .unwrap();
        assert_eq!(
            got,
            Command::Put {
                serial: "G0X".into(),
                local: PathBuf::from("/tmp/a.kfx"),
                device_path: "documents/a.kfx".into(),
                wait_ms: 5000,
                keep_session: true,
            }
        );
    }

    #[test]
    fn refusals_are_usage_failures_in_plain_words() {
        for bad in [
            vec![],
            vec!["copy"],
            vec!["list", "--all"],
            vec!["put", "G0X", "/tmp/a.kfx"],
            vec!["put", "G0X", "/tmp/a.kfx", "documents/a.kfx", "extra"],
            vec!["put", "G0X", "/tmp/a.kfx", "documents/a.kfx", "--wait-ms"],
            vec![
                "put",
                "G0X",
                "/tmp/a.kfx",
                "documents/a.kfx",
                "--wait-ms",
                "soon",
            ],
            vec![
                "put",
                "G0X",
                "/tmp/a.kfx",
                "documents/a.kfx",
                "--wait-ms",
                "600000",
            ],
            vec!["put", "G0X", "/tmp/a.kfx", "documents/a.kfx", "--force"],
            vec!["put", " ", "/tmp/a.kfx", "documents/a.kfx"],
            vec!["--version", "now"],
        ] {
            let err = parse(&argv(&bad)).unwrap_err();
            assert_eq!(err.code, Code::Usage, "{bad:?}");
            assert!(!err.message.is_empty());
        }
    }

    #[test]
    fn ls_with_and_without_a_folder() {
        assert_eq!(
            parse(&argv(&["ls", "G0X"])).unwrap(),
            Command::Ls {
                serial: "G0X".into(),
                folder: None
            }
        );
        assert_eq!(
            parse(&argv(&["ls", "G0X", "documents"])).unwrap(),
            Command::Ls {
                serial: "G0X".into(),
                folder: Some("documents".into())
            }
        );
        for bad in [
            vec!["ls"],
            vec!["ls", "G0X", "a", "b"],
            vec!["ls", "G0X", "--all"],
            vec!["ls", " "],
        ] {
            assert_eq!(parse(&argv(&bad)).unwrap_err().code, Code::Usage, "{bad:?}");
        }
    }

    #[test]
    fn version_and_help() {
        assert_eq!(parse(&argv(&["--version"])).unwrap(), Command::Version);
        assert_eq!(parse(&argv(&["--help"])).unwrap(), Command::Help);
        assert_eq!(parse(&argv(&["-h"])).unwrap(), Command::Help);
    }
}

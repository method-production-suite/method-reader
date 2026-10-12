//! `cargo xtask <command>`: Screepub's own tooling, every developer tool
//! that lived in tools/ as TypeScript, Python or shell.
//!
//! Each command is a module with `pub fn main(args: &[String]) -> u8`, the
//! process exit code, so the tool keeps the flags, output and exit codes
//! its TypeScript had, and the tests in `tests/` can call it directly.
//! rust/PARITY.md's "Tools" table says which tool became which command.

pub mod compare;
#[path = "../../sp-engine/tests/common/machine.rs"]
pub mod machine;
pub mod parity;
pub mod paths;
pub mod table;

pub mod brand;
pub mod bump_tap;
pub mod bump_version;
pub mod calibre_config;
pub mod capture;
pub mod check_latest;
pub mod check_tap;
pub mod cli_build;
pub mod desktop_notes;
pub mod device_checklist;
pub mod dmg;
pub mod fixture;
pub mod mobi_probe;
pub mod packaging;
pub mod rehearse;
pub mod release_hook;
pub mod release_notes;
pub mod scratch;

/// One `cargo xtask` command: its name, a line of help, and its entry.
pub struct Command {
    pub name: &'static str,
    pub help: &'static str,
    pub main: fn(&[String]) -> u8,
}

fn parity_main(args: &[String]) -> u8 {
    exit_of(parity::run(args), "parity")
}

/// The original two commands answer `Result<bool, String>`: true is 0,
/// false 1, and an error 2 with its message.
fn exit_of(result: Result<bool, String>, name: &str) -> u8 {
    match result {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(message) => {
            eprintln!("xtask {name}: {message}");
            2
        }
    }
}

/// Every command, in the order `cargo xtask --help` lists them.
pub const COMMANDS: &[Command] = &[
    Command {
        name: "parity",
        help: "run the Rust CLI over every fixture against the TypeScript's frozen answers (see --help)",
        main: parity_main,
    },
    Command {
        name: "make-fixture",
        help: "write a committed fixture PDF (was tools/make-fixture.py)",
        main: fixture::main,
    },
    Command {
        name: "device-checklist",
        help: "print the device-side half of tools/torture-manifest.json",
        main: device_checklist::main,
    },
    Command {
        name: "mobi-picture-probe",
        help: "write the .mobi picture-size probe books for an older Kindle",
        main: mobi_probe::main,
    },
    Command {
        name: "capture-screens",
        help: "retake the README and site pictures (needs Chrome)",
        main: capture::capture_main,
    },
    Command {
        name: "review-screens",
        help: "which pictures changed, old beside new",
        main: capture::review_main,
    },
    Command {
        name: "make-og",
        help: "render the site's social card (was tools/make-og.sh)",
        main: capture::make_og_main,
    },
    Command {
        name: "bump-version",
        help: "the release bump: every version file and the window's notes",
        main: bump_version::main,
    },
    Command {
        name: "release-notes",
        help: "the facts a release's notes are drafted from",
        main: release_notes::main,
    },
    Command {
        name: "build-desktop-notes",
        help: "regenerate the window's release notes from docs/releases/",
        main: desktop_notes::main,
    },
    Command {
        name: "require-release-notes",
        help: "the Claude Code PreToolUse hook: no release tag without its notes",
        main: release_hook::main,
    },
    Command {
        name: "bump-tap",
        help: "write the Homebrew tap's formula for a release",
        main: bump_tap::main,
    },
    Command {
        name: "check-tap",
        help: "does the Homebrew tap serve the latest release",
        main: check_tap::main,
    },
    Command {
        name: "check-latest",
        help: "does the updater's latest.json serve the latest release",
        main: check_latest::main,
    },
    Command {
        name: "build-cli",
        help: "build, sign and check the screepub CLI for each platform",
        main: cli_build::build_cli_main,
    },
    Command {
        name: "smoke-cli",
        help: "convert the committed fixtures with a built CLI",
        main: cli_build::smoke_cli_main,
    },
    Command {
        name: "verify-signing",
        help: "check a built app or CLI is signed, notarized and stapled",
        main: cli_build::verify_signing_main,
    },
    Command {
        name: "build-app-bundle",
        help: "build the app bundles for this platform (Tauri window, or --app native)",
        main: packaging::build_app_bundle_main,
    },
    Command {
        name: "bundle-archive",
        help: "list the files inside a built .deb or .rpm",
        main: packaging::bundle_archive_main,
    },
    Command {
        name: "smoke-bundle",
        help: "open a built app bundle and check what it carries",
        main: packaging::smoke_bundle_main,
    },
    Command {
        name: "update-signature",
        help: "the tag-time gate: does the app's config trust a real updater key",
        main: packaging::update_signature_main,
    },
    Command {
        name: "build-update-manifest",
        help: "write the updater's latest.json from a release's files",
        main: packaging::build_update_manifest_main,
    },
    Command {
        name: "rehearse-handover",
        help: "a local Tauri 0.11.1 window updating itself into this checkout's app",
        main: rehearse::main,
    },
    Command {
        name: "build-sidecar",
        help: "retired: the Rust app links the engine, there is no sidecar",
        main: cli_build::build_sidecar_main,
    },
];

/// `cargo xtask` itself: the usage text.
pub fn usage() -> String {
    let mut out = String::from("usage: cargo xtask <command> [args]\n\ncommands:\n");
    for c in COMMANDS {
        out.push_str(&format!("  {:<22} {}\n", c.name, c.help));
    }
    out.push_str("\n`cargo xtask <command> --help` prints a command's own flags.\n");
    out
}

/// Runs one command line (without the program name) and answers the exit code.
pub fn run(args: &[String]) -> u8 {
    let Some((name, rest)) = args.split_first() else {
        eprint!("{}", usage());
        return 2;
    };
    if matches!(name.as_str(), "-h" | "--help" | "help") {
        print!("{}", usage());
        return 0;
    }
    match COMMANDS.iter().find(|c| c.name == name) {
        Some(c) => (c.main)(rest),
        None => {
            eprintln!("xtask: unknown command: {name}\n\n{}", usage());
            2
        }
    }
}

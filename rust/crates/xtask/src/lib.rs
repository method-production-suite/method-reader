//! `cargo xtask <command>`: Method Reader's own tooling, every developer tool
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

/// One `cargo xtask` command: its name, a line of help, its entry, and what
/// `--help` prints.
pub struct Command {
    pub name: &'static str,
    pub help: &'static str,
    pub main: fn(&[String]) -> u8,
    pub usage: Usage,
}

/// What `cargo xtask <command> --help` (or `-h`, anywhere in the arguments)
/// prints. Either way it exits 0 and nothing else runs: no command treats
/// `--help` as a version, a path or a step (tests/every_command_help.rs).
pub enum Usage {
    /// `cargo xtask` prints this before the command would run.
    Text(&'static str),
    /// The command answers `--help` itself, with its own text (it is then
    /// called with `--help` alone).
    Own,
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
        usage: Usage::Own,
    },
    Command {
        name: "make-fixture",
        help: "write a committed fixture PDF (was tools/make-fixture.py)",
        main: fixture::main,
        usage: Usage::Own,
    },
    Command {
        name: "device-checklist",
        help: "print the device-side half of tools/torture-manifest.json",
        main: device_checklist::main,
        usage: Usage::Own,
    },
    Command {
        name: "mobi-picture-probe",
        help: "write the .mobi picture-size probe books for an older Kindle",
        main: mobi_probe::main,
        usage: Usage::Text(
            "usage: cargo xtask mobi-picture-probe --out <folder>\n\n\
             Writes the .mobi picture-size probe books for an older Kindle into\n\
             <folder>, and says which book carries which picture size.\n",
        ),
    },
    Command {
        name: "capture-screens",
        help: "retake the README and site pictures (needs Chrome)",
        main: capture::capture_main,
        usage: Usage::Text(
            "usage: cargo xtask capture-screens [--only <shot>]\n\n\
             Retakes every README and site picture (or only <shot>) from the real\n\
             window on tests/fixtures/field-station.pdf, framed by Chrome. Writes\n\
             assets/screens/<shot>-<theme>.png and site/img/ copies, each only when\n\
             its bytes change, and nothing unless every picture was taken.\n\
             Needs Google Chrome (macOS).\n",
        ),
    },
    Command {
        name: "review-screens",
        help: "which pictures changed, old beside new",
        main: capture::review_main,
        usage: Usage::Text(
            "usage: cargo xtask review-screens --out <page.html> [--repo <dir>]\n\n\
             Writes a page showing each picture changed since the last commit, old\n\
             beside new, or says no picture changed and writes nothing.\n",
        ),
    },
    Command {
        name: "make-og",
        help: "render the site's social card (was tools/make-og.sh)",
        main: capture::make_og_main,
        usage: Usage::Text(
            "usage: cargo xtask make-og\n\n\
             Renders site/og-card.html to site/og.png at 1200 by 630 with Chrome.\n\
             Re-run after editing og-card.html. Needs Google Chrome (macOS).\n",
        ),
    },
    Command {
        name: "bump-version",
        help: "the release bump: every version file and the window's notes",
        main: bump_version::main,
        usage: Usage::Text(
            "usage: cargo xtask bump-version <version> [--check] [--repo <dir>]\n\n\
             The release bump: rust/Cargo.toml's version, rust/Cargo.lock, and the\n\
             window's notes from docs/releases/<version>.md. --check only checks\n\
             that the notes are in the shape the window can show.\n",
        ),
    },
    Command {
        name: "release-notes",
        help: "the facts a release's notes are drafted from",
        main: release_notes::main,
        usage: Usage::Text(
            "usage: cargo xtask release-notes <sinceTag>\n\n\
             Prints, as JSON, the facts a release's notes are drafted from: the\n\
             changes, files and registry rows since <sinceTag>.\n",
        ),
    },
    Command {
        name: "build-desktop-notes",
        help: "regenerate the window's release notes from docs/releases/",
        main: desktop_notes::main,
        usage: Usage::Own,
    },
    Command {
        name: "require-release-notes",
        help: "the Claude Code PreToolUse hook: no release tag without its notes",
        main: release_hook::main,
        usage: Usage::Text(
            "usage: cargo xtask require-release-notes < <hook JSON>\n\n\
             The Claude Code PreToolUse hook: reads the tool call on stdin, and\n\
             exits 2 (blocked) when it would create a release tag whose\n\
             docs/releases/<version>.md is not in HEAD. Anything else exits 0.\n",
        ),
    },
    Command {
        name: "bump-tap",
        help: "write the Homebrew tap's formula for a release",
        main: bump_tap::main,
        usage: Usage::Text(
            "usage: cargo xtask bump-tap <version> <tap-checkout>\n\n\
             Writes the Homebrew formula for release <version> into a checkout of\n\
             the tap, from the release's own checksums, and retires the cask.\n\
             METHOD_READER_REPO names the release repository.\n",
        ),
    },
    Command {
        name: "check-tap",
        help: "does the Homebrew tap serve the latest release",
        main: check_tap::main,
        usage: Usage::Own,
    },
    Command {
        name: "check-latest",
        help: "does the updater's latest.json serve the latest release",
        main: check_latest::main,
        usage: Usage::Text(
            "usage: cargo xtask check-latest [--version vX.Y.Z] [--repo owner/name]\n\n\
             Does the PUBLISHED latest.json name the newest release (or\n\
             --version), point every platform at an asset it carries, and carry\n\
             signatures from the key the app trusts (sp_update::PUBKEY)? Exits 0\n\
             when it does, 1 with every problem said when it does not.\n",
        ),
    },
    Command {
        name: "build-cli",
        help: "build, sign and check the method-reader CLI for each platform",
        main: cli_build::build_cli_main,
        usage: Usage::Text(
            "usage: cargo xtask build-cli --version <x.y.z> --out <dir> [--only <target>]...\n\
             \x20      [--mac [--unsigned | --sign <identity> [--notarize]]]\n\n\
             Compiles the method-reader CLI (and its screepub alias) for each release target (or only those\n\
             named), packages and checks each archive, and writes SHA256SUMS.\n\
             --notarize reads AC_API_KEY_PATH, AC_API_KEY_ID and AC_API_ISSUER_ID.\n",
        ),
    },
    Command {
        name: "smoke-cli",
        help: "convert the committed fixtures with a built CLI",
        main: cli_build::smoke_cli_main,
        usage: Usage::Text(
            "usage: cargo xtask smoke-cli --binary <method-reader> [--fixture <pdf>]\n\
             \x20      [--jpx-fixture <pdf>] [--expect-version <x.y.z>]\n\n\
             Runs a built CLI: its version, a conversion, a JPEG 2000 picture, and\n\
             the device list, each checked.\n",
        ),
    },
    Command {
        name: "verify-signing",
        help: "check a built app or CLI is signed, notarized and stapled",
        main: cli_build::verify_signing_main,
        usage: Usage::Text(
            "usage: cargo xtask verify-signing --dmg <downloaded .dmg> --expect coexist|handover\n\n\
             Checks a release's DOWNLOADED .dmg is signed, notarized and stapled,\n\
             and what an installed Swift app does with it. Mac only.\n",
        ),
    },
    Command {
        name: "build-app-bundle",
        help: "build the window's app bundles for this platform",
        main: packaging::build_app_bundle_main,
        usage: Usage::Own,
    },
    Command {
        name: "bundle-archive",
        help: "list the files inside a built .deb or .rpm",
        main: packaging::bundle_archive_main,
        usage: Usage::Own,
    },
    Command {
        name: "smoke-bundle",
        help: "open a built app bundle and check what it carries",
        main: packaging::smoke_bundle_main,
        usage: Usage::Own,
    },
    Command {
        name: "update-signature",
        help: "the tag-time gate: does the app trust a real updater key (sp_update::PUBKEY)",
        main: packaging::update_signature_main,
        usage: Usage::Own,
    },
    Command {
        name: "build-update-manifest",
        help: "write the updater's latest.json from a release's files",
        main: packaging::build_update_manifest_main,
        usage: Usage::Own,
    },
    Command {
        name: "rehearse-handover",
        help: "a local Tauri 0.11.1 window updating itself into this checkout's app",
        main: rehearse::main,
        usage: Usage::Own,
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
        Some(c) if rest.iter().any(|a| a == "-h" || a == "--help") => match c.usage {
            Usage::Text(text) => {
                print!("{text}");
                0
            }
            Usage::Own => (c.main)(&["--help".to_string()]),
        },
        Some(c) => (c.main)(rest),
        None => {
            eprintln!("xtask: unknown command: {name}\n\n{}", usage());
            2
        }
    }
}

//! Ports tests/tool-temp-folders.test.ts: the tools that make a temp folder
//! (smoke-cli, smoke-bundle, verify-signing) remove it again, on the way
//! out of a failure as well as a pass.
//!
//! Each tool runs as a real `xtask` process with its temp folder pointed at
//! a folder of this test's own, so whatever it leaves behind shows up here
//! and never lands in the shared temp folder.

mod cli_build_common;

use std::path::Path;
use std::process::Command;

use cli_build_common::Scratch;

/// Runs `xtask <args>` with a temp folder of its own (TMPDIR, and TMP and
/// TEMP for Windows), and answers the exit code, stderr and what it left.
fn with_own_tmp(
    scratch: &Scratch,
    args: &[&str],
    path_prefix: Option<&Path>,
) -> (i32, String, Vec<String>) {
    let tmp = scratch.join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command
        .args(args)
        .env("TMPDIR", &tmp)
        .env("TMP", &tmp)
        .env("TEMP", &tmp);
    if let Some(prefix) = path_prefix {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let joined = std::env::join_paths(
            std::iter::once(prefix.to_path_buf()).chain(std::env::split_paths(&path)),
        )
        .unwrap();
        command.env("PATH", joined);
    }
    let out = command.output().expect("run xtask");
    let left = std::fs::read_dir(&tmp)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        left,
    )
}

#[test]
fn smoke_cli_removes_its_work_folder_after_a_passing_smoke() {
    if !cfg!(unix) {
        eprintln!("skipping: the stand-in CLI is a shell script, which needs a Unix shell");
        return;
    }
    // A stand-in for a working CLI: it answers every command the smoke asks
    // and writes a zip header wherever -o points. The temp folder is what is
    // under test here, not the engine.
    let scratch = Scratch::new("temp-smoke-pass");
    let binary = scratch.join("screepub");
    #[cfg(unix)]
    cli_build_common::write_script(
        &binary,
        r#"case "$1" in
  --version) echo "method-reader 9.9.9 (Method Reader)"; exit 0;;
  devices) echo '{"ok":true,"devices":[]}'; exit 0;;
esac
input="$1"; out=""
while [ $# -gt 0 ]; do
  if [ "$1" = "-o" ]; then out="$2"; fi
  shift
done
printf 'PK\003\004' > "$out"
case "$input" in *jpx*) extra=',"imageCount":1';; *) extra='';; esac
printf '{"ok":true,"epubPath":"%s","pages":3%s}\n' "$out" "$extra"
"#,
    );
    let binary = binary.display().to_string();
    let (code, stderr, left) = with_own_tmp(
        &scratch,
        &[
            "smoke-cli",
            "--binary",
            &binary,
            "--expect-version",
            "9.9.9",
        ],
        None,
    );
    assert_eq!(stderr, "");
    assert_eq!(code, 0);
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[test]
fn smoke_cli_removes_its_work_folder_after_a_failing_smoke_and_still_fails() {
    let scratch = Scratch::new("temp-smoke-fail");
    let missing = scratch.join("no-such-binary").display().to_string();
    let (code, stderr, left) = with_own_tmp(&scratch, &["smoke-cli", "--binary", &missing], None);
    assert!(stderr.contains("no binary at"), "{stderr}");
    assert_eq!(code, 1);
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[test]
fn verify_signing_removes_its_work_folder_when_the_image_will_not_mount_and_still_fails() {
    let scratch = Scratch::new("temp-verify");
    let dmg = scratch.join("no-such.dmg").display().to_string();
    let args = ["verify-signing", "--dmg", &dmg, "--expect", "coexist"];
    if std::env::consts::OS != "macos" {
        // It refuses to start anywhere else, and makes nothing first.
        let (code, stderr, left) = with_own_tmp(&scratch, &args, None);
        assert_eq!(code, 2);
        assert!(stderr.contains("macOS-only"), "{stderr}");
        assert!(left.is_empty(), "left behind: {left:?}");
        return;
    }
    // A fake hdiutil on PATH that refuses to attach: no real image is ever
    // mounted, and nothing past the mount (codesign, spctl) is reached.
    let bin = scratch.join("fakebin");
    std::fs::create_dir_all(&bin).unwrap();
    #[cfg(unix)]
    cli_build_common::write_script(
        &bin.join("hdiutil"),
        "echo \"hdiutil: attach failed - No such file or directory\" >&2\nexit 1\n",
    );
    let (code, stderr, left) = with_own_tmp(&scratch, &args, Some(&bin));
    assert!(stderr.contains("hdiutil attach failed"), "{stderr}");
    assert_eq!(code, 1);
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[test]
fn smoke_bundle_removes_its_work_folder_after_a_refusal_and_still_fails() {
    let scratch = Scratch::new("temp-smoke-bundle");
    let bundle = scratch.join("not-a-bundle.txt").display().to_string();
    let (code, stderr, left) = with_own_tmp(
        &scratch,
        &[
            "smoke-bundle",
            "--bundle",
            &bundle,
            "--expect-version",
            "0.0.0",
        ],
        None,
    );
    assert!(
        stderr.contains("is not a .deb, .rpm, .dmg or .exe"),
        "{stderr}"
    );
    assert_eq!(code, 1);
    assert!(left.is_empty(), "left behind: {left:?}");
}

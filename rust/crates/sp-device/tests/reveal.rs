//! The handler half of tests/cli-reveal.test.ts (src/reveal.ts and
//! src/cli-reveal.ts), ported. The spawned-CLI half goes with sp-cli.
//! Every test passes a fake opener: nothing here opens a real Finder or
//! Explorer window, and the real-opener tests run `sh`, never an opener.

mod common;
use common::{FakeOpener, s, scratch, strings};
use sp_device::process::{Opener, OpenerOutput, SpawnOpener};
use sp_device::reveal::{RevealResult, reveal_command, reveal_file};
use sp_device::{ErrorCode, Platform};

// The argv per platform.

#[test]
fn darwin_open_r_names_the_file_itself() {
    let o = FakeOpener::ok();
    reveal_file(
        "/Users/ann/Method Reader/My Script/My Script.epub",
        Platform::Darwin,
        &o,
    )
    .unwrap();
    assert_eq!(
        o.calls(),
        [strings(&[
            "open",
            "-R",
            "/Users/ann/Method Reader/My Script/My Script.epub"
        ])]
    );
}

#[test]
fn win32_explorer_opens_the_folder_never_select() {
    let o = FakeOpener::ok();
    reveal_file(
        r"C:\Users\Ann, O'Brien\My Scripts\Field Station.pdf",
        Platform::Win32,
        &o,
    )
    .unwrap();
    assert_eq!(
        o.calls(),
        [strings(&["explorer", r"C:\Users\Ann, O'Brien\My Scripts"])]
    );
}

#[test]
fn linux_and_everything_else_xdg_open_on_the_folder() {
    let o = FakeOpener::ok();
    reveal_file(
        "/home/ann/Method Reader/Script/Script.epub",
        Platform::Linux,
        &o,
    )
    .unwrap();
    assert_eq!(
        o.calls(),
        [strings(&["xdg-open", "/home/ann/Method Reader/Script"])]
    );
}

#[test]
fn win32_a_trailing_dot_component_is_normalised_away_before_dirname() {
    let o = FakeOpener::ok();
    reveal_file("C:\\x\\a.exe\\.", Platform::Win32, &o).unwrap();
    assert_eq!(o.calls(), [strings(&["explorer", "C:\\x"])]);
}

// explorer's exit code is ignored, every other opener's is not.

#[test]
fn win32_explorer_exiting_1_is_still_success() {
    let o = FakeOpener::new(Ok(OpenerOutput {
        code: 1,
        stderr: String::new(),
    }));
    assert!(reveal_file("C:\\a\\b.epub", Platform::Win32, &o).is_ok());
}

#[test]
fn darwin_open_exiting_non_zero_is_reveal_failed_with_the_trimmed_stderr() {
    let o = FakeOpener::new(Ok(OpenerOutput {
        code: 1,
        stderr: "  The file does not exist.\n".into(),
    }));
    let err = reveal_file("/x/y.epub", Platform::Darwin, &o).unwrap_err();
    assert_eq!(err.code, ErrorCode::RevealFailed);
    assert_eq!(
        err.message,
        "could not show /x/y.epub in the file manager: The file does not exist."
    );
}

#[test]
fn linux_xdg_open_exiting_non_zero_with_no_stderr_stays_plain() {
    let o = FakeOpener::new(Ok(OpenerOutput {
        code: 4,
        stderr: "   \n".into(),
    }));
    let err = reveal_file("/x/y.epub", Platform::Linux, &o).unwrap_err();
    assert_eq!(err.code, ErrorCode::RevealFailed);
    assert_eq!(err.message, "could not show /x/y.epub in the file manager");
}

// A spawn that cannot start at all.

#[test]
fn an_opener_that_cannot_start_is_reveal_failed_not_a_crash() {
    for platform in [Platform::Darwin, Platform::Win32, Platform::Linux] {
        let o = FakeOpener::new(Err(
            "ENOENT: no such file or directory, posix_spawn 'open'".into()
        ));
        let err = reveal_file("/x/y.epub", platform, &o).unwrap_err();
        assert_eq!(err.code, ErrorCode::RevealFailed);
        assert_eq!(
            err.message,
            "the file manager could not be opened to show /x/y.epub: ENOENT: no such file or directory, posix_spawn 'open'"
        );
    }
}

// Both checks run before anything opens.

#[test]
fn a_relative_path_is_usage_and_the_opener_sees_nothing() {
    let o = FakeOpener::ok();
    let err = reveal_command("relative/Script.epub", Platform::host(), &o).unwrap_err();
    assert_eq!(err.code, ErrorCode::Usage);
    assert_eq!(
        err.message,
        "reveal needs a full path, not \"relative/Script.epub\""
    );
    assert!(o.calls().is_empty());
}

#[test]
fn a_missing_absolute_path_is_unreadable_and_the_opener_sees_nothing() {
    let sc = scratch("method-reader-cli-reveal-");
    let gone = s(&sc.path().join("gone.epub"));
    let o = FakeOpener::ok();
    let err = reveal_command(&gone, Platform::host(), &o).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unreadable);
    assert_eq!(
        err.message,
        format!("cannot read the file to reveal: {gone}")
    );
    assert!(o.calls().is_empty());
}

#[test]
fn a_directory_given_as_the_file_is_unreadable_too() {
    let sc = scratch("method-reader-cli-reveal-");
    let o = FakeOpener::ok();
    let err = reveal_command(&s(sc.path()), Platform::host(), &o).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unreadable);
    assert!(o.calls().is_empty());
}

// The answer shape.

#[test]
fn an_existing_absolute_path_answers_revealed_and_calls_the_opener_once() {
    let sc = scratch("method-reader-cli-reveal-");
    let file = s(&sc.path().join("Script.epub"));
    std::fs::write(&file, "x").unwrap();
    let o = FakeOpener::ok();
    let result = reveal_command(&file, Platform::Darwin, &o).unwrap();
    assert_eq!(
        result,
        RevealResult {
            revealed: file.clone()
        }
    );
    assert_eq!(o.calls().len(), 1);
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json, serde_json::json!({ "revealed": file }));
}

#[test]
fn a_failing_opener_still_refuses_cleanly_from_the_command_level() {
    let sc = scratch("method-reader-cli-reveal-");
    let file = s(&sc.path().join("Script.epub"));
    std::fs::write(&file, "x").unwrap();
    let o = FakeOpener::new(Ok(OpenerOutput {
        code: 1,
        stderr: "nope".into(),
    }));
    let err = reveal_command(&file, Platform::Darwin, &o).unwrap_err();
    assert_eq!(err.code, ErrorCode::RevealFailed);
    assert_eq!(
        err.message,
        format!("could not show {file} in the file manager: nope")
    );
}

// The real opener, exercised without revealing anything.

#[cfg(unix)]
#[test]
fn spawn_opener_reports_a_real_child_s_exit_code_and_stderr_untrimmed() {
    let out = SpawnOpener
        .open(&strings(&["sh", "-c", "echo ' oops ' >&2; exit 7"]))
        .unwrap();
    assert_eq!(
        out,
        OpenerOutput {
            code: 7,
            stderr: " oops \n".into()
        }
    );
}

#[test]
fn spawn_opener_names_a_program_that_cannot_exist_with_enoent() {
    let err = SpawnOpener
        .open(&strings(&["/no/such/screepub-opener-x"]))
        .unwrap_err();
    assert!(err.starts_with("ENOENT"), "{err}");
}

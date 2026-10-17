//! The handler half of tests/cli-eject.test.ts (src/device/eject.ts and
//! src/cli-eject.ts), ported. The spawned-CLI half goes with sp-cli.
//!
//! NO TEST HERE MAY EJECT A REAL DRIVE. Every handler test passes a
//! recording fake runner and an injected device list, so no diskutil,
//! udisksctl or PowerShell is ever started. The real-runner tests run `sh`,
//! never an eject tool.

mod common;
use common::{Recording, exit, strings};
use sp_device::eject::{EjectDeps, EjectPlan, EjectResult, eject_command, eject_commands};
use sp_device::process::{RunOutput, Runner, SpawnRunner};
use sp_device::{CliError, ConnectedDevice, DeviceKind, ErrorCode, Platform};
use std::sync::Arc;
use std::time::Duration;

fn kindle() -> ConnectedDevice {
    ConnectedDevice::mounted(DeviceKind::Kindle, "Kindle", "/Volumes/Kindle")
}
fn linux_kobo() -> ConnectedDevice {
    ConnectedDevice::mounted(
        DeviceKind::Kobo,
        "KOBOeReader",
        "/run/media/sam/KOBOeReader",
    )
}
fn win_kindle() -> ConnectedDevice {
    ConnectedDevice::mounted(DeviceKind::Kindle, "Kindle", "E:\\")
}

fn eject(
    volume: &str,
    platform: Platform,
    devices: Vec<ConnectedDevice>,
    run: Arc<dyn Runner>,
) -> Result<EjectResult, CliError> {
    let list = move || devices.clone();
    eject_command(
        volume,
        &EjectDeps {
            platform,
            devices: Some(&list),
            run,
            timeout: Duration::from_secs(20),
        },
    )
}

fn ejected(v: &str) -> EjectResult {
    EjectResult { ejected: v.into() }
}

// Refusals come before anything runs.

#[test]
fn a_relative_path_is_usage_and_nothing_runs() {
    let run = Recording::ok();
    let err = eject(
        "Volumes/Kindle",
        Platform::Darwin,
        vec![kindle()],
        run.clone(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Usage);
    assert_eq!(
        err.message,
        "eject needs a full path to a reader's volume, not \"Volumes/Kindle\""
    );
    assert!(run.calls().is_empty());
}

#[test]
fn a_volume_that_is_not_a_listed_reader_is_unknown_device_and_nothing_runs() {
    let run = Recording::ok();
    let err = eject(
        "/Volumes/Backup",
        Platform::Darwin,
        vec![kindle()],
        run.clone(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownDevice);
    assert_eq!(
        err.message,
        "not a reader Method Reader can see: /Volumes/Backup"
    );
    assert!(run.calls().is_empty());
}

#[test]
fn the_comparison_is_on_resolved_paths_a_trailing_slash_still_names_the_reader() {
    let run = Recording::ok();
    let result = eject(
        "/Volumes/Kindle/",
        Platform::Darwin,
        vec![kindle()],
        run.clone(),
    )
    .unwrap();
    assert_eq!(
        run.calls(),
        [strings(&["diskutil", "eject", "/Volumes/Kindle"])]
    );
    // The answer names the volume as the engine listed it, too.
    assert_eq!(result, ejected("/Volumes/Kindle"));
}

#[test]
fn a_remarkable_never_matches_anything() {
    let run = Recording::ok();
    let err = eject(
        "/",
        Platform::Darwin,
        vec![ConnectedDevice::remarkable()],
        run.clone(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownDevice);
    assert!(run.calls().is_empty());
}

// The command per platform.

#[test]
fn darwin_runs_exactly_diskutil_eject_and_answers_ejected() {
    let run = Recording::ok();
    let result = eject(
        "/Volumes/Kindle",
        Platform::Darwin,
        vec![kindle()],
        run.clone(),
    )
    .unwrap();
    assert_eq!(
        run.calls(),
        [strings(&["diskutil", "eject", "/Volumes/Kindle"])]
    );
    assert_eq!(result, ejected("/Volumes/Kindle"));
    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        r#"{"ejected":"/Volumes/Kindle"}"#
    );
}

fn findmnt_then(rest: impl Fn(&[String]) -> RunOutput + Send + Sync + 'static) -> Arc<Recording> {
    Recording::new(move |argv| {
        Ok(if argv[0] == "findmnt" {
            exit(0, "/dev/sdb1\n", "")
        } else {
            rest(argv)
        })
    })
}

#[test]
fn linux_asks_findmnt_then_unmounts_then_powers_off_in_that_order() {
    let run = findmnt_then(|_| exit(0, "", ""));
    let result = eject(
        "/run/media/sam/KOBOeReader",
        Platform::Linux,
        vec![linux_kobo()],
        run.clone(),
    )
    .unwrap();
    assert_eq!(
        run.calls(),
        [
            strings(&[
                "findmnt",
                "-n",
                "-o",
                "SOURCE",
                "--mountpoint",
                "/run/media/sam/KOBOeReader"
            ]),
            strings(&["udisksctl", "unmount", "-b", "/dev/sdb1"]),
            strings(&["udisksctl", "power-off", "-b", "/dev/sdb1"]),
        ]
    );
    assert_eq!(result, ejected("/run/media/sam/KOBOeReader"));
}

#[test]
fn linux_a_failed_unmount_stops_there_and_power_off_never_runs() {
    let run = findmnt_then(|_| exit(1, "", "Error unmounting /dev/sdb1: target is busy\n"));
    let err = eject(
        "/run/media/sam/KOBOeReader",
        Platform::Linux,
        vec![linux_kobo()],
        run.clone(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::EjectFailed);
    assert!(err.message.contains("target is busy"));
    assert_eq!(
        run.calls().iter().map(|a| a[1].clone()).collect::<Vec<_>>(),
        ["-n", "unmount"]
    );
}

#[test]
fn linux_unmounted_but_power_off_refused_is_still_ejected() {
    for refusal in [
        exit(1, "", "Error powering off drive: not authorized\n"),
        RunOutput {
            code: None,
            ..Default::default()
        },
    ] {
        let run = findmnt_then(move |argv| {
            if argv[1] == "power-off" {
                refusal.clone()
            } else {
                exit(0, "", "")
            }
        });
        let result = eject(
            "/run/media/sam/KOBOeReader",
            Platform::Linux,
            vec![linux_kobo()],
            run.clone(),
        )
        .unwrap();
        assert_eq!(result, ejected("/run/media/sam/KOBOeReader"));
        assert_eq!(
            run.calls().iter().map(|a| a[1].clone()).collect::<Vec<_>>(),
            ["-n", "unmount", "power-off"]
        );
    }
}

#[test]
fn linux_findmnt_naming_no_device_is_eject_failed_and_udisksctl_never_runs() {
    let run = Recording::new(|_| Ok(exit(0, "\n", "")));
    let err = eject(
        "/run/media/sam/KOBOeReader",
        Platform::Linux,
        vec![linux_kobo()],
        run.clone(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::EjectFailed);
    assert_eq!(run.calls().len(), 1);
}

#[test]
fn linux_a_source_that_is_not_a_dev_block_device_is_eject_failed() {
    for source in ["/dev/sdb1[/books]", "gvfsd-fuse", "sdb1"] {
        let out = format!("{source}\n");
        let run = Recording::new(move |_| Ok(exit(0, &out, "")));
        let err = eject(
            "/run/media/sam/KOBOeReader",
            Platform::Linux,
            vec![linux_kobo()],
            run.clone(),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::EjectFailed);
        assert!(err.message.contains("could not find the device behind it"));
        assert_eq!(run.calls().len(), 1);
    }
}

#[test]
fn linux_a_reader_that_is_a_plain_folder_never_reaches_the_disk_it_sits_on() {
    let tolino = ConnectedDevice::mounted(DeviceKind::Tolino, "tolino", "/home/sam/tolino");
    let run = Recording::new(|argv| {
        Ok(if argv.iter().any(|a| a == "--target") {
            exit(0, "/dev/sda2\n", "")
        } else {
            exit(1, "", "")
        })
    });
    let err = eject(
        "/home/sam/tolino",
        Platform::Linux,
        vec![tolino],
        run.clone(),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::EjectFailed);
    assert_eq!(
        err.message,
        "could not eject /home/sam/tolino: could not find the device behind it"
    );
    assert_eq!(
        run.calls(),
        [strings(&[
            "findmnt",
            "-n",
            "-o",
            "SOURCE",
            "--mountpoint",
            "/home/sam/tolino"
        ])]
    );
}

#[test]
fn win32_the_powershell_line_names_the_drive_e_for_the_volume_e() {
    let run = Recording::ok();
    let result = eject("E:\\", Platform::Win32, vec![win_kindle()], run.clone()).unwrap();
    let calls = run.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0][0], "powershell");
    let script = calls[0].last().unwrap();
    assert!(script.starts_with(
        "(New-Object -comObject Shell.Application).Namespace(17).ParseName('E:').InvokeVerb('Eject');"
    ));
    assert!(script.contains("Test-Path 'E:\\'"));
    assert_eq!(result, ejected("E:\\"));
}

#[test]
fn win32_the_script_is_the_typescript_one_character_for_character() {
    let EjectPlan::Commands(commands) = eject_commands("E:\\", Platform::Win32) else {
        panic!("a drive root")
    };
    assert_eq!(
        commands,
        [strings(&[
            "powershell",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(New-Object -comObject Shell.Application).Namespace(17).ParseName('E:').InvokeVerb('Eject'); for ($i = 0; $i -lt 20; $i++) { if (-not (Test-Path 'E:\\')) { exit 0 }; Start-Sleep -Milliseconds 250 }; exit 3",
        ])]
    );
}

#[test]
fn win32_the_drive_still_there_after_the_eject_verb_is_eject_failed_in_plain_words() {
    let run = Recording::new(|_| Ok(exit(3, "", "")));
    let err = eject("E:\\", Platform::Win32, vec![win_kindle()], run).unwrap_err();
    assert_eq!(err.code, ErrorCode::EjectFailed);
    assert_eq!(
        err.message,
        "could not eject E:\\: something on this computer is still using the reader. Close it and try again."
    );
}

#[test]
fn win32_a_listed_volume_that_is_not_a_drive_root_is_refused_and_powershell_never_starts() {
    for volume in [
        "\\\\server\\share\\Kindle",
        "E:\\it's\\Kindle",
        "D:\\stuff\\Kindle",
    ] {
        let reader = ConnectedDevice::mounted(DeviceKind::Kindle, "Kindle", volume);
        let run = Recording::ok();
        let err = eject(volume, Platform::Win32, vec![reader], run.clone()).unwrap_err();
        assert_eq!(err.code, ErrorCode::UnknownDevice, "{volume}");
        assert_eq!(
            err.message,
            format!("not a whole drive Method Reader can eject: {volume}")
        );
        assert!(run.calls().is_empty());
    }
}

#[test]
fn eject_commands_on_win32_a_drive_root_with_or_without_its_backslash_and_nothing_else() {
    for root in ["E:\\", "E:", "e:\\"] {
        let EjectPlan::Commands(commands) = eject_commands(root, Platform::Win32) else {
            panic!("{root}")
        };
        let script = commands[0].last().unwrap();
        assert!(script.contains("ParseName('E:').InvokeVerb('Eject')"));
        assert!(script.contains("Test-Path 'E:\\'"));
        // Only that letter: nothing else from the volume reaches the script.
        let rest = script.replace("'E:\\'", "").replace("'E:'", "");
        assert!(
            !rest
                .as_bytes()
                .windows(3)
                .any(|w| w[0] == b'\'' && w[1].is_ascii_alphabetic() && w[2] == b':')
        );
    }
    for other in [
        "D:\\stuff\\Kindle",
        "\\\\server\\share",
        "E:\\'",
        "E:\\\\",
        "Kindle",
    ] {
        assert_eq!(
            eject_commands(other, Platform::Win32),
            EjectPlan::NotADriveRoot,
            "{other}"
        );
    }
}

#[test]
fn eject_commands_is_pure_linux_needs_the_device_first() {
    assert_eq!(
        eject_commands("/Volumes/Kindle", Platform::Darwin),
        EjectPlan::Commands(vec![strings(&["diskutil", "eject", "/Volumes/Kindle"])])
    );
    assert_eq!(
        eject_commands("/run/media/sam/KOBOeReader", Platform::Linux),
        EjectPlan::NeedsDevice
    );
}

// A volume that will not eject says so.

#[test]
fn a_non_zero_exit_is_eject_failed_carrying_the_first_stderr_line() {
    let run = Recording::new(|_| {
        Ok(exit(
            1,
            "",
            "Volume Kindle on disk4 failed to eject\nsecond line\n",
        ))
    });
    let err = eject("/Volumes/Kindle", Platform::Darwin, vec![kindle()], run).unwrap_err();
    assert_eq!(err.code, ErrorCode::EjectFailed);
    assert_eq!(
        err.message,
        "could not eject /Volumes/Kindle: Volume Kindle on disk4 failed to eject"
    );
}

#[test]
fn a_runner_that_never_answers_is_eject_failed_after_the_timeout() {
    let run = Recording::new(|_| {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    });
    let list = || vec![kindle()];
    let err = eject_command(
        "/Volumes/Kindle",
        &EjectDeps {
            platform: Platform::Darwin,
            devices: Some(&list),
            run,
            timeout: Duration::from_millis(20),
        },
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::EjectFailed);
    assert_eq!(
        err.message,
        "could not eject /Volumes/Kindle: it took too long to answer"
    );
}

#[test]
fn a_tool_that_cannot_start_at_all_is_eject_failed_not_a_crash() {
    let run = Recording::new(|_| Err("spawn diskutil ENOENT".into()));
    let err = eject("/Volumes/Kindle", Platform::Darwin, vec![kindle()], run).unwrap_err();
    assert_eq!(err.code, ErrorCode::EjectFailed);
    assert!(err.message.contains("ENOENT"));
}

#[test]
fn the_error_json_is_the_contract_shape() {
    let err = CliError::new(ErrorCode::EjectFailed, "could not eject /Volumes/Kindle");
    assert_eq!(
        serde_json::to_string(&err).unwrap(),
        r#"{"code":"eject-failed","message":"could not eject /Volumes/Kindle"}"#
    );
}

// The real runner, exercised without ejecting anything.

#[cfg(unix)]
#[test]
fn spawn_runner_collects_exit_code_stdout_and_stderr_from_a_throwaway_child() {
    let out = SpawnRunner
        .run(
            &strings(&["sh", "-c", "echo out; echo err >&2; exit 3"]),
            Duration::from_secs(10),
        )
        .unwrap();
    assert_eq!(out, exit(3, "out\n", "err\n"));
}

#[cfg(unix)]
#[test]
fn spawn_runner_kills_a_child_that_outlives_the_timeout_and_answers_code_none() {
    let started = std::time::Instant::now();
    let out = SpawnRunner
        .run(
            &strings(&["sh", "-c", "exec sleep 60"]),
            Duration::from_millis(100),
        )
        .unwrap();
    assert_eq!(out.code, None);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn spawn_runner_answers_128_plus_the_signal_for_a_child_killed_by_one_as_bun_does() {
    let out = SpawnRunner
        .run(
            &strings(&["sh", "-c", "kill -9 $$"]),
            Duration::from_secs(10),
        )
        .unwrap();
    assert_eq!(out.code, Some(137));
}

#[test]
fn spawn_runner_names_a_tool_that_cannot_exist_in_node_s_words() {
    let err = SpawnRunner
        .run(
            &strings(&["/no/such/screepub-tool-x"]),
            Duration::from_secs(10),
        )
        .unwrap_err();
    assert!(
        err.starts_with("ENOENT: no such file or directory, "),
        "{err}"
    );
    assert!(err.ends_with("'/no/such/screepub-tool-x'"), "{err}");
}

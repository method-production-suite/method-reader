//! The `eject` verb: src/device/eject.ts (the per-OS commands) and
//! src/cli-eject.ts (the handler). It ejects ONLY a volume the engine itself
//! lists as a reader right now, so the window can never eject an arbitrary
//! disk, and both refusals run before anything is spawned. Every test hands
//! in a fake [`Runner`], so no test ever ejects a real drive.

use crate::error::{CliError, ErrorCode};
use crate::list::{ListDevicesOptions, list_devices};
use crate::process::{Platform, RunOutput, Runner, SpawnRunner};
use crate::types::ConnectedDevice;
use serde::Serialize;
use sp_fsutil::nodepath::{posix, win32};
use std::sync::{Arc, mpsc};
use std::time::Duration;

/// How long one eject step may take before it counts as failed. A volume
/// still being written to can make diskutil wait; twenty seconds is long
/// enough for that and short enough that the window's button comes back.
pub const EJECT_TIMEOUT: Duration = Duration::from_secs(20);

/// How the Windows script says the drive was still there after its wait.
pub const WINDOWS_STILL_MOUNTED: i32 = 3;
pub const STILL_IN_USE: &str =
    "something on this computer is still using the reader. Close it and try again.";

/// The Windows drive a volume names, like `E:` for `E:\`, but ONLY when the
/// volume is exactly a drive root (`X:\` or `X:`); None for anything else, a
/// subfolder, a UNC share, or a path with a quote in it. So only a letter and
/// a colon ever reach the PowerShell line, and a folder on a drive never
/// ejects the whole drive.
pub fn windows_drive(volume: &str) -> Option<String> {
    let b = volume.as_bytes();
    let shaped = (b.len() == 2 || (b.len() == 3 && b[2] == b'\\'))
        && b[0].is_ascii_alphabetic()
        && b[1] == b':';
    shaped.then(|| format!("{}:", (b[0] as char).to_ascii_uppercase()))
}

/// What one eject takes, per platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EjectPlan {
    /// Run these, in order.
    Commands(Vec<Vec<String>>),
    /// Linux: udisksctl works on the block DEVICE, so findmnt is asked first.
    NeedsDevice,
    /// Windows ejects whole drives only.
    NotADriveRoot,
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// The per-OS argv lists for one eject; pure, so tests read them.
///
/// darwin: `diskutil eject <volume>`. win32: the shell's own Eject verb on
/// the drive, through PowerShell, for a drive root only. Everything else
/// (Linux): findmnt for the device, then [`linux_eject_commands`].
pub fn eject_commands(volume: &str, platform: Platform) -> EjectPlan {
    match platform {
        Platform::Darwin => EjectPlan::Commands(vec![argv(&["diskutil", "eject", volume])]),
        Platform::Win32 => match windows_drive(volume) {
            None => EjectPlan::NotADriveRoot,
            Some(drive) => EjectPlan::Commands(vec![argv(&[
                "powershell",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &windows_eject_script(&drive),
            ])]),
        },
        Platform::Linux => EjectPlan::NeedsDevice,
    }
}

/// The Shell's Eject verb answers at once and exits 0 even when the drive is
/// busy and stays, so the script then looks for the drive for up to five
/// seconds (20 x 250 ms) and exits WINDOWS_STILL_MOUNTED if it is still
/// there. `drive` is windows_drive()'s answer, a letter and a colon.
fn windows_eject_script(drive: &str) -> String {
    format!(
        "(New-Object -comObject Shell.Application).Namespace(17).ParseName('{drive}').InvokeVerb('Eject'); \
         for ($i = 0; $i -lt 20; $i++) {{ if (-not (Test-Path '{drive}\\')) {{ exit 0 }}; Start-Sleep -Milliseconds 250 }}; \
         exit {WINDOWS_STILL_MOUNTED}"
    )
}

/// Linux: the question that finds the block device behind a mount point.
/// `--mountpoint`, never `--target`: a tolino is recognised by its folder
/// name alone, so a listed volume can be a plain folder on an ordinary disk,
/// and `--target` would name the disk holding it.
pub fn find_device_command(volume: &str) -> Vec<String> {
    argv(&["findmnt", "-n", "-o", "SOURCE", "--mountpoint", volume])
}

/// Linux: unmount, then power off, in that order.
pub fn linux_eject_commands(device: &str) -> Vec<Vec<String>> {
    vec![
        argv(&["udisksctl", "unmount", "-b", device]),
        argv(&["udisksctl", "power-off", "-b", device]),
    ]
}

/// `{ "ejected": <volume as the engine listed it> }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EjectResult {
    pub ejected: String,
}

/// The eject handler's seams. `devices` defaults to the mount scan alone (a
/// reMarkable never mounts and an MTP Kindle never mounts, so neither can
/// be ejected).
pub struct EjectDeps<'a> {
    pub platform: Platform,
    pub devices: Option<&'a dyn Fn() -> Vec<ConnectedDevice>>,
    pub run: Arc<dyn Runner>,
    pub timeout: Duration,
}

impl Default for EjectDeps<'_> {
    fn default() -> Self {
        EjectDeps {
            platform: Platform::host(),
            devices: None,
            run: Arc::new(SpawnRunner),
            timeout: EJECT_TIMEOUT,
        }
    }
}

/// The first non-empty line of a tool's complaint, stderr before stdout.
fn first_line(texts: &[&str]) -> String {
    for text in texts {
        if let Some(line) = text.lines().find(|l| !l.trim().is_empty()) {
            return line.trim().to_string();
        }
    }
    String::new()
}

/// A /dev block device and nothing else: a bind mount answers
/// `/dev/sdb1[/books]` and a FUSE mount a name, and either would only reach
/// the person as udisksctl's own error.
fn is_block_device(source: &str) -> bool {
    let Some(rest) = source.strip_prefix("/dev/") else {
        return false;
    };
    !rest.is_empty()
        && !rest
            .chars()
            .any(|c| c.is_whitespace() || c == '[' || c == ']')
}

/// Eject `volume`, answering `{ ejected }` or a contract error.
pub fn eject_command(volume: &str, deps: &EjectDeps) -> Result<EjectResult, CliError> {
    let win = deps.platform == Platform::Win32;
    let is_absolute = if win {
        win32::is_absolute(volume)
    } else {
        posix::is_absolute(volume)
    };
    let resolve = |p: &str| {
        if win {
            win32::resolve(&[p])
        } else {
            posix::resolve(&[p])
        }
    };

    if !is_absolute {
        return Err(CliError::new(
            ErrorCode::Usage,
            format!("eject needs a full path to a reader's volume, not \"{volume}\""),
        ));
    }

    let wanted = resolve(volume);
    let devices = match deps.devices {
        Some(devices) => devices(),
        None => list_devices(&ListDevicesOptions {
            skip_remarkable: true,
            skip_mtp: true,
            ..Default::default()
        }),
    };
    let Some(listed) = devices
        .into_iter()
        .find_map(|d| d.volume.filter(|v| resolve(v) == wanted))
    else {
        return Err(CliError::new(
            ErrorCode::UnknownDevice,
            format!("not a reader Method Reader can see: {volume}"),
        ));
    };

    let failed = |detail: &str| {
        let tail = if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        };
        CliError::new(
            ErrorCode::EjectFailed,
            format!("could not eject {volume}{tail}"),
        )
    };

    // One step, raced against the timeout so even a runner that never
    // answers cannot hold the window's button forever. `any_exit` hands
    // back stdout whatever the exit code, for a question whose silence is
    // itself the answer.
    let step = |argv: Vec<String>, any_exit: bool| -> Result<String, CliError> {
        let (tx, rx) = mpsc::channel::<Result<RunOutput, String>>();
        let run = Arc::clone(&deps.run);
        let timeout = deps.timeout;
        std::thread::spawn(move || {
            let _ = tx.send(run.run(&argv, timeout));
        });
        let answer = match rx.recv_timeout(deps.timeout) {
            Ok(Ok(answer)) => answer,
            Ok(Err(err)) => return Err(failed(&err)),
            Err(_) => return Err(failed("it took too long to answer")),
        };
        let Some(code) = answer.code else {
            return Err(failed("it took too long to answer"));
        };
        // The Windows script's own exit: the Eject verb ran, the drive stayed.
        if win && code == WINDOWS_STILL_MOUNTED {
            return Err(failed(STILL_IN_USE));
        }
        if code != 0 && !any_exit {
            return Err(failed(&first_line(&[&answer.stderr, &answer.stdout])));
        }
        Ok(answer.stdout)
    };

    // The command, and the answer, name the volume exactly as the engine
    // listed it, not as the caller spelled it (a trailing slash, say).
    match eject_commands(&listed, deps.platform) {
        EjectPlan::NotADriveRoot => Err(CliError::new(
            ErrorCode::UnknownDevice,
            format!("not a whole drive Method Reader can eject: {volume}"),
        )),
        EjectPlan::NeedsDevice => {
            // findmnt exits 1 with nothing to say for a folder that is not a
            // mount point; that is the same "no device" as an empty answer.
            let device = first_line(&[&step(find_device_command(&listed), true)?]);
            if !is_block_device(&device) {
                return Err(failed("could not find the device behind it"));
            }
            let mut commands = linux_eject_commands(&device).into_iter();
            let (unmount, power_off) = (
                commands.next().unwrap_or_default(),
                commands.next().unwrap_or_default(),
            );
            step(unmount, false)?;
            // Unmounted is safe to unplug. Power-off is a courtesy (some
            // systems refuse it without a password), so its failure does not
            // make the reader any less ejected.
            let _ = step(power_off, false);
            Ok(EjectResult { ejected: listed })
        }
        EjectPlan::Commands(commands) => {
            for argv in commands {
                step(argv, false)?;
            }
            Ok(EjectResult { ejected: listed })
        }
    }
}

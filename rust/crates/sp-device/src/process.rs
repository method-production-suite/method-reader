//! The child processes the device layer starts, behind traits so every test
//! hands in a fake and no test ever ejects, reveals, or pauses anything
//! real. The real runners are the only code paths a real eject, reveal,
//! launchctl or MTP helper run goes through.

use sp_fsutil::NodeIoError;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// A child started by the window: on Windows, with no console of its own.
/// The window is a GUI-subsystem program, so a console tool it starts
/// (PowerShell for an eject, ebook-convert for minutes) would otherwise open
/// a console window over it. Every Command the engine spawns goes through
/// this. Elsewhere it is the Command as given.
pub fn quiet_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// CREATE_NO_WINDOW, from the Win32 process creation flags.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// The operating system whose rules apply, as Node's `process.platform`
/// splits them: macOS, Windows, and everything else, which the TypeScript
/// treats as Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Darwin,
    Win32,
    /// Linux, and every other platform that is not darwin or win32.
    Linux,
}

impl Platform {
    /// The machine this runs on.
    pub fn host() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::Darwin
        } else if cfg!(windows) {
            Platform::Win32
        } else {
            Platform::Linux
        }
    }
}

/// What one child process answered. `code` is None when it was killed at
/// its timeout. A child that died of a signal answers 128 + the signal, as
/// Bun's `exited` does.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// `EjectRun`: one child process, run to completion or killed at `timeout`.
/// An `Err` is a child that could not be started at all, in the words Node
/// would have used (`ENOENT: no such file or directory, posix_spawn 'x'`).
/// Shared by eject, the MTP helper and the camera-import pause.
pub trait Runner: Send + Sync {
    fn run(&self, argv: &[String], timeout: Duration) -> Result<RunOutput, String>;
}

/// `Opener`: one child process run to completion, with its exit code and
/// stderr. `reveal` and the routes are "open something in the system" verbs
/// and share this. An `Err` is a child that could not be started.
pub trait Opener: Send + Sync {
    fn open(&self, argv: &[String]) -> Result<OpenerOutput, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OpenerOutput {
    pub code: i32,
    pub stderr: String,
}

/// The real [`Runner`] (`spawnEject`). The environment is passed through on
/// purpose (the house rule for every spawn), and stdin is closed: none of
/// these tools reads it. Past `timeout` the child is killed and the answer
/// is `code: None`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpawnRunner;

impl Runner for SpawnRunner {
    fn run(&self, argv: &[String], timeout: Duration) -> Result<RunOutput, String> {
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| "nothing to run".to_string())?;
        let mut child = quiet_command(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| spawn_error(&e, program))?;
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let deadline = Instant::now() + timeout;
        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() >= deadline => {
                    timed_out = true;
                    kill(&mut child);
                    break None;
                }
                Ok(None) => thread::sleep(Duration::from_millis(5)),
                Err(_) => {
                    kill(&mut child);
                    break None;
                }
            }
        };
        let code = if timed_out {
            None
        } else {
            status.map(exit_code)
        };
        Ok(RunOutput {
            code,
            stdout: collect(stdout),
            stderr: collect(stderr),
        })
    }
}

/// The real [`Opener`] (`spawnOpener`): stdout ignored, stderr captured,
/// stdin closed.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpawnOpener;

impl Opener for SpawnOpener {
    fn open(&self, argv: &[String]) -> Result<OpenerOutput, String> {
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| "nothing to run".to_string())?;
        let mut child = quiet_command(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| spawn_error(&e, program))?;
        let stderr = drain(child.stderr.take());
        let status = child.wait().map_err(|e| e.to_string())?;
        Ok(OpenerOutput {
            code: exit_code(status),
            stderr: collect(stderr),
        })
    }
}

/// A spawn that failed, as Node words it.
fn spawn_error(err: &std::io::Error, program: &str) -> String {
    let syscall = if cfg!(windows) {
        "uv_spawn"
    } else {
        "posix_spawn"
    };
    NodeIoError::from_io(err, syscall, Some(Path::new(program)), None).message
}

fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn exit_code(status: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .or_else(|| status.signal().map(|s| 128 + s))
        .unwrap_or(-1)
}

#[cfg(not(unix))]
fn exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(-1)
}

type Drained = mpsc::Receiver<String>;

/// Read a pipe to its end on a thread of its own, so a chatty child never
/// blocks on a full pipe while it is waited for.
fn drain(pipe: Option<impl Read + Send + 'static>) -> Option<Drained> {
    let mut pipe = pipe?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        let _ = tx.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    Some(rx)
}

/// What a drained pipe held. A grandchild that kept the pipe open after its
/// parent was killed must not hold the answer forever, so this waits a
/// short while and then answers with nothing.
fn collect(rx: Option<Drained>) -> String {
    rx.and_then(|rx| rx.recv_timeout(Duration::from_secs(2)).ok())
        .unwrap_or_default()
}

//! The `screepub` binary: the library's [`sp_cli::run`] on the process's
//! own argv and streams.

use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    sp_cli::recover_camera_import();
    ExitCode::from(sp_cli::run(&argv, Box::new(sp_cli::Std)))
}

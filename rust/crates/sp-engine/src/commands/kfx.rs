//! Port of `src/cli-kfx.ts`: the `kfx-status` and `kfx-install` verbs. The
//! checklist is kfx_setup.rs's and the installer kfx.rs's; this only
//! carries their answers out.

use serde::Serialize;

use crate::env::{Env, host_platform};
use crate::errors::{CliError, EngineError, ErrorCode};
use crate::export::calibre::calibre_tool;
use crate::export::kfx::{
    KfxInstallResult, KfxStatus, PLUGIN_INSTALL_TIMEOUT_MS, debug_runner, install_kfx_plugin,
    kfx_status,
};
use crate::export::kfx_setup::{KfxFix, KfxSetup, KfxStep, kfx_possible, kfx_setup, system_name};

/// `KfxCommandDeps`: every seam, each defaulting to the real thing. `env`
/// is what every Calibre spawn gets: a test MUST give a scratch
/// CALIBRE_CONFIG_DIRECTORY there.
#[derive(Default)]
pub struct KfxCommandDeps<'a> {
    pub status: Option<Box<dyn Fn() -> KfxStatus + 'a>>,
    pub install: Option<Box<dyn Fn() -> KfxInstallResult + 'a>>,
    pub platform: Option<String>,
    pub env: Option<Env>,
}

/// `KfxInstallAnswer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KfxInstallAnswer {
    pub version: String,
    /// Conflicting KFX forks cleared to make room.
    pub removed: Vec<String>,
    /// The checklist as Calibre reports it AFTER the install.
    pub setup: KfxSetup,
}

fn env_of(deps: &KfxCommandDeps) -> Env {
    deps.env.clone().unwrap_or_else(Env::process)
}

/// `kfxStatusCommand(deps)`.
pub fn kfx_status_command(deps: &KfxCommandDeps) -> KfxSetup {
    let status = match &deps.status {
        Some(f) => f(),
        None => kfx_status(&env_of(deps)),
    };
    let platform = deps.platform.as_deref().unwrap_or(host_platform());
    kfx_setup(&status, platform)
}

/// `kfxInstallCommand(deps)`: install or update the plugin, then probe
/// again. Refused BEFORE the installer runs where Amazon makes no Kindle
/// Previewer. NEVER call this on the program's own initiative.
pub fn kfx_install_command(deps: &KfxCommandDeps) -> Result<KfxInstallAnswer, EngineError> {
    let platform = deps.platform.as_deref().unwrap_or(host_platform());
    if !kfx_possible(platform) {
        return Err(CliError::new(
            ErrorCode::KfxInstallFailed,
            format!(
                "the KFX plugin is of no use here: it drives Kindle Previewer, which Amazon does not make for {}",
                system_name(platform)
            ),
        )
        .into());
    }
    let result = match &deps.install {
        Some(f) => f(),
        None => {
            let env = env_of(deps);
            let tool = calibre_tool("calibre-debug", &env);
            let runner = debug_runner(env, PLUGIN_INSTALL_TIMEOUT_MS);
            install_kfx_plugin(&runner, tool.as_deref())
        }
    };
    let version = match (&result.version, result.ok) {
        (Some(v), true) if !v.is_empty() => v.clone(),
        _ => {
            let mut message = format!(
                "could not install the KFX plugin: {}",
                result
                    .reason
                    .clone()
                    .unwrap_or_else(|| "Calibre did not say why".into())
            );
            let removed = result.removed.clone().unwrap_or_default();
            if !removed.is_empty() {
                if !message.ends_with(['.', '!', '?']) {
                    message.push('.');
                }
                message.push_str(&format!(
                    " An older copy was removed before the failure: {}. Try again, or reinstall it in Calibre.",
                    removed.join(", ")
                ));
            }
            return Err(CliError::new(ErrorCode::KfxInstallFailed, message).into());
        }
    };
    let setup = kfx_status_command(deps);
    Ok(KfxInstallAnswer {
        version,
        removed: result.removed.unwrap_or_default(),
        setup,
    })
}

fn step_state(step: &KfxStep) -> String {
    match &step.fix {
        None => "installed".into(),
        Some(KfxFix::Link { url, .. }) => format!("not installed: {url}"),
        Some(KfxFix::Install { .. }) => "not installed: run method-reader kfx-install".into(),
        Some(KfxFix::Unavailable { why }) => format!("not available: {why}"),
        Some(KfxFix::After { why }) => format!("not installed: {why}"),
    }
}

/// `setupLines(setup)`: the summary, then one aligned line per step.
pub fn setup_lines(setup: &KfxSetup) -> Vec<String> {
    let width = setup
        .steps
        .iter()
        .map(|s| s.name.encode_utf16().count())
        .max()
        .unwrap_or(0);
    let mut lines = vec![setup.summary.clone()];
    for s in &setup.steps {
        let pad = width.saturating_sub(s.name.encode_utf16().count());
        lines.push(format!(
            "  {}{}  {}",
            s.name,
            " ".repeat(pad),
            step_state(s)
        ));
    }
    lines
}

/// `installLines(answer)`: the version, each removed fork, then the FULL
/// checklist.
pub fn install_lines(answer: &KfxInstallAnswer) -> Vec<String> {
    let mut lines = vec![format!("installed the KFX plugin {}", answer.version)];
    lines.extend(
        answer
            .removed
            .iter()
            .map(|name| format!("removed an older copy: {name}")),
    );
    lines.extend(setup_lines(&answer.setup));
    lines
}

//! Port of `src/export/kfx-setup.ts`: what to tell a person about the KFX
//! rung, and what they can do about it. A three-step checklist with one fix
//! per missing step and one sentence saying what a Kindle gets today. Pure.

use serde::Serialize;

use crate::export::formats::{ExportFormat, ToolchainState, file_extension};
use crate::export::kfx::KfxStatus;

/// `KfxFix`: how to fix one missing step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum KfxFix {
    /// Only the user can install it; this page is where.
    Link { label: String, url: String },
    /// Method Reader can install it: `method-reader kfx-install`.
    Install { label: String },
    /// Needs another step done first.
    After { why: String },
    /// Cannot be had on this platform at all.
    Unavailable { why: String },
}

/// `KfxStep['id']`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KfxStepId {
    Calibre,
    Previewer,
    Plugin,
}

/// `KfxStep`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KfxStep {
    pub id: KfxStepId,
    pub name: String,
    pub installed: bool,
    /// Null exactly when installed.
    pub fix: Option<KfxFix>,
}

/// `KfxSetup`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KfxSetup {
    pub ready: bool,
    /// False where Amazon makes no Kindle Previewer, so KFX cannot happen.
    pub possible: bool,
    /// One sentence: what a Kindle gets today.
    pub summary: String,
    /// Always three, in this order: calibre, previewer, plugin.
    pub steps: Vec<KfxStep>,
}

const CALIBRE_MAC: &str = "https://calibre-ebook.com/download_osx";
const CALIBRE_WINDOWS: &str = "https://calibre-ebook.com/download_windows";
/// Calibre's chooser page, only ever emitted where `possible` is false.
const CALIBRE_ANY: &str = "https://calibre-ebook.com/download";
/// Amazon's Kindle Previewer page (Mac and Windows downloads both).
const PREVIEWER_PAGE: &str = "https://kdp.amazon.com/en_US/help/topic/G202131170";

/// Every URL a link fix can carry where KFX is possible: the window's
/// opener grant allows exactly these.
pub const KFX_LINKS: [&str; 3] = [CALIBRE_MAC, CALIBRE_WINDOWS, PREVIEWER_PAGE];

/// `kfxPossible(platform)`.
pub fn kfx_possible(platform: &str) -> bool {
    platform == "darwin" || platform == "win32"
}

/// `systemName(platform)`.
pub fn system_name(platform: &str) -> &'static str {
    if platform == "linux" {
        "Linux"
    } else {
        "this system"
    }
}

fn calibre_fix(status: &KfxStatus, platform: &str) -> Option<KfxFix> {
    if status.calibre {
        return None;
    }
    let url = match platform {
        "darwin" => CALIBRE_MAC,
        "win32" => CALIBRE_WINDOWS,
        _ => CALIBRE_ANY,
    };
    Some(KfxFix::Link {
        label: "Get Calibre".into(),
        url: url.into(),
    })
}

fn previewer_fix(status: &KfxStatus, platform: &str) -> Option<KfxFix> {
    if status.previewer {
        return None;
    }
    if !kfx_possible(platform) {
        return Some(KfxFix::Unavailable {
            why: format!("Amazon does not make it for {}", system_name(platform)),
        });
    }
    Some(KfxFix::Link {
        label: "Get Kindle Previewer".into(),
        url: PREVIEWER_PAGE.into(),
    })
}

fn plugin_fix(status: &KfxStatus, installed: bool, possible: bool) -> Option<KfxFix> {
    if installed {
        return None;
    }
    if !possible {
        return Some(KfxFix::Unavailable {
            why: "Of no use without Kindle Previewer".into(),
        });
    }
    if !status.calibre {
        return Some(KfxFix::After {
            why: "Install Calibre first".into(),
        });
    }
    Some(KfxFix::Install {
        label: "Install".into(),
    })
}

/// `kfxSetup(status, platform)`.
pub fn kfx_setup(status: &KfxStatus, platform: &str) -> KfxSetup {
    let possible = kfx_possible(platform);
    let plugin_installed = status.calibre && status.plugin_installed;
    let steps = vec![
        KfxStep {
            id: KfxStepId::Calibre,
            name: "Calibre".into(),
            installed: status.calibre,
            fix: calibre_fix(status, platform),
        },
        KfxStep {
            id: KfxStepId::Previewer,
            name: "Kindle Previewer".into(),
            installed: status.previewer,
            fix: previewer_fix(status, platform),
        },
        KfxStep {
            id: KfxStepId::Plugin,
            name: "KFX plugin".into(),
            installed: plugin_installed,
            fix: plugin_fix(status, plugin_installed, possible),
        },
    ];
    let today = format!(
        "{} (beta)",
        file_extension(
            ExportFormat::Kindle,
            ToolchainState {
                calibre_available: status.calibre,
                kfx_ready: false,
            }
        )
        .to_uppercase()
    );
    let summary = if status.ready {
        "Kindles get KFX, the best quality Method Reader can make.".to_owned()
    } else if !possible {
        format!(
            "Amazon does not make Kindle Previewer for {}, so Kindles get {today}, which Method Reader makes itself.",
            system_name(platform)
        )
    } else {
        format!(
            "Kindles get {today} for now, which Method Reader makes itself. KFX looks better, and needs the three free tools below."
        )
    };
    KfxSetup {
        ready: status.ready,
        possible,
        summary,
        steps,
    }
}

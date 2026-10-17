//! Port of `src/export/formats.ts`: what a converted script can be saved
//! as, labeled by PURPOSE (the file you email is not the file you
//! sideload), and the Kindle ladder's rule.

use serde::Serialize;

/// `ExportFormat`. `Kindle` is the ladder; `Kfx`, `Azw3` and `Mobi` are its
/// rungs asked for by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Epub,
    Kindle,
    Kfx,
    Azw3,
    Mobi,
    Fountain,
    Fdx,
}

/// Every value `export --for` takes, in the order its help lists them.
pub const EXPORT_FORMATS: [ExportFormat; 7] = [
    ExportFormat::Epub,
    ExportFormat::Kindle,
    ExportFormat::Kfx,
    ExportFormat::Azw3,
    ExportFormat::Mobi,
    ExportFormat::Fountain,
    ExportFormat::Fdx,
];

impl ExportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            ExportFormat::Epub => "epub",
            ExportFormat::Kindle => "kindle",
            ExportFormat::Kfx => "kfx",
            ExportFormat::Azw3 => "azw3",
            ExportFormat::Mobi => "mobi",
            ExportFormat::Fountain => "fountain",
            ExportFormat::Fdx => "fdx",
        }
    }

    /// The format `--for` names, if it names one.
    pub fn parse(name: &str) -> Option<Self> {
        EXPORT_FORMATS.into_iter().find(|f| f.as_str() == name)
    }
}

/// `KindleRung`: one rung of the Kindle ladder, named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KindleRung {
    Kfx,
    Azw3,
    Mobi,
}

impl KindleRung {
    pub fn as_str(self) -> &'static str {
        match self {
            KindleRung::Kfx => "kfx",
            KindleRung::Azw3 => "azw3",
            KindleRung::Mobi => "mobi",
        }
    }

    pub fn format(self) -> ExportFormat {
        match self {
            KindleRung::Kfx => ExportFormat::Kfx,
            KindleRung::Azw3 => ExportFormat::Azw3,
            KindleRung::Mobi => ExportFormat::Mobi,
        }
    }
}

/// `ToolchainState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToolchainState {
    pub calibre_available: bool,
    pub kfx_ready: bool,
}

/// `ladderRung(state)`: KFX when the full toolchain is present, otherwise
/// the AZW3 Method Reader writes itself. Never the MOBI, which is asked for by
/// name.
pub fn ladder_rung(state: ToolchainState) -> KindleRung {
    if state.kfx_ready {
        KindleRung::Kfx
    } else {
        KindleRung::Azw3
    }
}

/// `fileExtension(format, state)`.
pub fn file_extension(format: ExportFormat, state: ToolchainState) -> &'static str {
    if format == ExportFormat::Kindle {
        return ladder_rung(state).as_str();
    }
    format.as_str()
}

/// `formatLabel(format, state)`.
pub fn format_label(format: ExportFormat, state: ToolchainState) -> String {
    match format {
        ExportFormat::Epub => "EPUB: for emailing to Kindle, and most e-readers".into(),
        ExportFormat::Fountain => {
            "Fountain: the script as plain text, for screenwriting apps".into()
        }
        ExportFormat::Kfx => "KFX: for modern Kindles over USB, the best layout".into(),
        ExportFormat::Azw3 => {
            "AZW3 (beta): for Kindles over USB, made by Method Reader with nothing else installed"
                .into()
        }
        ExportFormat::Mobi => "MOBI: for the oldest Kindles".into(),
        ExportFormat::Fdx => {
            "FDX: the script to keep editing, in Final Draft and most screenwriting apps".into()
        }
        ExportFormat::Kindle => {
            let ext = file_extension(format, state).to_uppercase();
            let hint = if state.kfx_ready {
                " (best quality)"
            } else {
                ""
            };
            let beta = if ext == "AZW3" { " (beta)" } else { "" };
            format!("{ext}{beta}: for USB sideload to Kindle{hint}")
        }
    }
}

/// `isBetaFile(extension)`: the AZW3 Method Reader writes itself, and the FDX.
pub fn is_beta_file(extension: &str) -> bool {
    extension == "azw3" || extension == "fdx"
}

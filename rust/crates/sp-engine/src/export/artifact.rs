//! Port of `src/export/artifact.ts`: the Kindle ladder's work. Each rung's
//! file sits beside the EPUB, is reused while the staleness rule says it is
//! current, and is built again when it is not: KFX and AZW3 from the EPUB
//! as it stands, the MOBI by re-rendering the `.fountain`. No rung changes
//! the EPUB.

use std::collections::BTreeSet;

use serde::Serialize;
use sp_model::FormatOptions;

use crate::errors::EngineError;
use crate::export::calibre::strip_epub;
use crate::export::formats::{ExportFormat, KindleRung, ToolchainState, ladder_rung};
use crate::export::freshness::{modified_date, needs_regeneration};
use crate::export::renderers::Renderers;
use crate::fsx;
use crate::library_images::read_images;

/// `RegenerationFailedError(why)`.
pub fn regeneration_failed(why: &str) -> EngineError {
    EngineError::Named {
        name: "RegenerationFailedError",
        message: format!("Couldn't rebuild the Kindle file: {why}"),
    }
}

/// `CannotRegenerateError()`.
pub fn cannot_regenerate() -> EngineError {
    EngineError::Named {
        name: "CannotRegenerateError",
        message: "Can't rebuild the Kindle file: the script's .fountain is missing.".into(),
    }
}

/// `mobiSibling(epub)`.
pub fn mobi_sibling(epub: &str) -> String {
    format!("{}.mobi", strip_epub(epub))
}

/// `azw3Sibling(epub)`.
pub fn azw3_sibling(epub: &str) -> String {
    format!("{}.azw3", strip_epub(epub))
}

/// `availableFormats()`: the EPUB and the Kindle file, always.
pub fn available_formats() -> Vec<ExportFormat> {
    vec![ExportFormat::Epub, ExportFormat::Kindle]
}

/// `KindleArtifactPlan['builtBy']`. `Screepub` (serialized "screepub")
/// means "this app made it, with nothing else installed". It kept the
/// 0.12 spelling through the rename to Method Reader: it is a token of the
/// `--json` contract the window matches on, not a name a person reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BuiltBy {
    Calibre,
    Screepub,
}

/// `KindleArtifactPlan`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindleArtifactPlan {
    pub path: String,
    /// Present and newer than the EPUB (and, for the AZW3, from this writer
    /// revision or later).
    pub fresh: bool,
    pub built_by: BuiltBy,
    /// When the EPUB was last written (ms), or null.
    pub book_date: Option<f64>,
}

/// `rungPlan(rung, epub)`: one rung's file beside `epub`, without building
/// anything. Every build asks this first, so `export --check` and the build
/// cannot disagree.
pub fn rung_plan(rung: KindleRung, epub: &str, writer_revision: u32) -> KindleArtifactPlan {
    let path = match rung {
        KindleRung::Kfx => crate::export::kfx::kfx_sibling(epub),
        KindleRung::Azw3 => azw3_sibling(epub),
        KindleRung::Mobi => mobi_sibling(epub),
    };
    let built_by = if rung == KindleRung::Kfx {
        BuiltBy::Calibre
    } else {
        BuiltBy::Screepub
    };
    let current = !needs_regeneration(&path, epub)
        && (rung != KindleRung::Azw3
            || sp_azw3::azw3_revision_of_file(std::path::Path::new(&path)).unwrap_or(0)
                >= writer_revision);
    KindleArtifactPlan {
        path,
        fresh: current,
        built_by,
        book_date: modified_date(epub),
    }
}

/// `kindleArtifactPlan(epub, state)`: the ladder's rung on this machine.
pub fn kindle_artifact_plan(
    epub: &str,
    state: ToolchainState,
    writer_revision: u32,
) -> KindleArtifactPlan {
    rung_plan(ladder_rung(state), epub, writer_revision)
}

/// `FreshKindleArtifactOptions`.
#[derive(Debug, Clone)]
pub struct FreshKindleArtifactOptions {
    pub epub: String,
    pub fountain_path: Option<String>,
    /// Must be the script's real settings, not the defaults.
    pub format: FormatOptions,
    pub calibre_available: bool,
    pub kfx_ready: bool,
}

/// `RungBuilders`: what builds the KFX and AZW3 rungs from the EPUB.
pub trait RungBuilders {
    fn kfx(&self, epub: &str, on_stage: &mut dyn FnMut(&str)) -> Result<String, EngineError>;
    fn azw3(&self, epub: &str) -> Result<String, EngineError>;
}

/// `writeAzw3(epub)`: Method Reader's own AZW3 from the EPUB as it stands,
/// written beside it through a temp file and a rename.
pub fn write_azw3(epub: &str, renderers: &dyn Renderers) -> Result<String, EngineError> {
    let bytes = fsx::read(epub)?;
    let azw3 = renderers.build_azw3(&bytes).map_err(|why| {
        regeneration_failed(&format!("the AZW3 could not be made from the book ({why})"))
    })?;
    let out = azw3_sibling(epub);
    fsx::write_atomic(&out, &azw3)?;
    Ok(out)
}

/// `freshKindleArtifact(opts, builders)`: the ladder's rung for this
/// machine, made by the same work as [`fresh_artifact_of`].
pub fn fresh_kindle_artifact(
    opts: &FreshKindleArtifactOptions,
    builders: &dyn RungBuilders,
    renderers: &dyn Renderers,
    on_stage: &mut dyn FnMut(&str),
) -> Result<String, EngineError> {
    let rung = ladder_rung(ToolchainState {
        calibre_available: opts.calibre_available,
        kfx_ready: opts.kfx_ready,
    });
    build_rung(rung, opts, builders, renderers, on_stage)
}

/// `freshArtifactOf(rung, opts, builders)`: one named rung, never the
/// ladder.
pub fn fresh_artifact_of(
    rung: KindleRung,
    opts: &FreshKindleArtifactOptions,
    builders: &dyn RungBuilders,
    renderers: &dyn Renderers,
    on_stage: &mut dyn FnMut(&str),
) -> Result<String, EngineError> {
    build_rung(rung, opts, builders, renderers, on_stage)
}

/// The one copy of each rung's work.
fn build_rung(
    rung: KindleRung,
    opts: &FreshKindleArtifactOptions,
    builders: &dyn RungBuilders,
    renderers: &dyn Renderers,
    on_stage: &mut dyn FnMut(&str),
) -> Result<String, EngineError> {
    let plan = rung_plan(rung, &opts.epub, renderers.azw3_writer_revision());
    if plan.fresh {
        return Ok(plan.path);
    }
    match rung {
        KindleRung::Kfx => builders.kfx(&opts.epub, on_stage),
        KindleRung::Azw3 => {
            on_stage("building the AZW3 for Kindle…");
            builders.azw3(&opts.epub)
        }
        KindleRung::Mobi => {
            let mobi = plan.path;
            let Some(fountain_path) = opts.fountain_path.as_deref().filter(|p| !p.is_empty())
            else {
                return Err(cannot_regenerate());
            };
            on_stage("rebuilding the Kindle file…");
            let fountain_text = fsx::read_text(fountain_path)?;
            // The script's pictures, read under the same rules as every
            // other render from the text; not read with includeImages off.
            let images = opts
                .format
                .include_images
                .then(|| read_images(fountain_path, &BTreeSet::new()).images);
            let rendered = renderers
                .render_mobi(&fountain_text, &opts.format, images.as_ref())
                .map_err(EngineError::plain)?;
            let Some(bytes) = rendered else {
                return Err(regeneration_failed(
                    "the engine reported success but produced no .mobi",
                ));
            };
            fsx::write_atomic(&mobi, &bytes)?;
            Ok(mobi)
        }
    }
}

/// The real builders: `toKfx` with this run's environment, and the AZW3
/// writer behind `renderers`.
pub struct RealBuilders<'a> {
    pub env: &'a crate::env::Env,
    pub renderers: &'a dyn Renderers,
}

impl RungBuilders for RealBuilders<'_> {
    fn kfx(&self, epub: &str, on_stage: &mut dyn FnMut(&str)) -> Result<String, EngineError> {
        crate::export::kfx::to_kfx(
            epub,
            on_stage,
            &crate::export::kfx::KfxDeps::default(),
            self.env,
        )
    }

    fn azw3(&self, epub: &str) -> Result<String, EngineError> {
        write_azw3(epub, self.renderers)
    }
}

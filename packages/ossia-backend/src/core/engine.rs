use super::omr;
use crate::{invalid, Result};
use ossia_engine::{CompileOptions, ConvertOptions, Engine, RunContext};
use ossia_library::EngineBackend;
use ossia_types::*;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

/// Routes score conversion between the local engine and optional remote OMR.
///
/// Rendering, rasterization and audio generation always delegate to the local engine.
pub(crate) struct RuntimeEngine {
    engine: Engine,
    home: PathBuf,
    build_id: String,
    audio_root: Option<PathBuf>,
}

impl RuntimeEngine {
    pub fn new(engine: Engine, home: PathBuf, audio_root: Option<PathBuf>) -> Self {
        let build_id = format!("{}+omr.1", engine.build_id());
        Self {
            engine,
            home,
            build_id,
            audio_root,
        }
    }

    fn report(
        &self,
        mut report: ImportReport,
        input: &[u8],
        confirmation: Option<&ContentHash>,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<ImportReport> {
        report.input_hash = ContentHash::of(input);
        report.engine_build_id = self.build_id.clone();
        if report.status == ImportStatus::Refused && confirmation.is_some() {
            return Err(AppError::new(
                ErrorCode::ImportRefused,
                "blocking conversion errors cannot be confirmed",
            ));
        }
        let token = if report.status == ImportStatus::NeedsConfirmation {
            Some(ContentHash::of(
                &serde_json::to_vec(&(
                    &report.input_hash,
                    &self.build_id,
                    &report.source_hash,
                    &report.losses,
                ))
                .map_err(|error| invalid(format!("cannot encode import confirmation: {error}")))?,
            ))
        } else {
            None
        };
        if let Some(supplied) = confirmation {
            if token.as_ref() != Some(supplied) {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "import confirmation does not match this input and loss report",
                ));
            }
        }
        report.confirmed = token.is_some() && confirmation == token.as_ref();
        report.confirmation = token;
        let mut seen = report
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    format!("{:?}", diagnostic.severity),
                    diagnostic.message.clone(),
                )
            })
            .collect::<HashSet<_>>();
        for diagnostic in diagnostics {
            if seen.insert((
                format!("{:?}", diagnostic.severity),
                diagnostic.message.clone(),
            )) {
                report.diagnostics.push(diagnostic);
            }
        }
        if report.diagnostics.len() > 128 {
            report.diagnostics.truncate(128);
        }
        Ok(report)
    }
}

impl EngineBackend for RuntimeEngine {
    fn render_musicxml(&self, snapshot: &SourceSnapshot, context: &RunContext) -> Result<Vec<u8>> {
        self.engine.export_musicxml(snapshot, context)
    }

    fn migrate_lilypond(&self, input: &[u8], context: &RunContext) -> Result<ImportReport> {
        let report = self.engine.migrate_lilypond(input, context)?;
        self.report(report, input, None, Vec::new())
    }
    fn render_wav(&self, playback: &ScorePlayback, context: &RunContext) -> Result<Vec<u8>> {
        let root = self.audio_root.as_deref().ok_or_else(|| {
            invalid("WAV export requires a static UI directory containing audio samples")
        })?;
        ossia_engine::render_wav(playback, root, context)
    }

    fn rasterize_svg(
        &self,
        svg: &[u8],
        width: u32,
        root: &Path,
        context: &RunContext,
    ) -> Result<ArtifactRef> {
        self.engine.rasterize_svg(svg, width, root, context)
    }

    fn rasterize_svg_with_options(
        &self,
        svg: &[u8],
        width: u32,
        quality: Option<u8>,
        transparent: bool,
        root: &Path,
        context: &RunContext,
    ) -> Result<ArtifactRef> {
        self.engine
            .rasterize_svg_with_options(svg, width, quality, transparent, root, context)
    }

    fn build_id(&self) -> &str {
        &self.build_id
    }

    fn convert(
        &self,
        input: &[u8],
        format: InputFormat,
        confirmation: Option<&ContentHash>,
        context: &RunContext,
    ) -> Result<ImportReport> {
        if format.is_omr() {
            let (musicxml, mut diagnostics) =
                omr::transcribe(&self.home, format.try_into()?, input, context)?;
            let mut report = self.engine.convert_with_options(
                &musicxml,
                InputFormat::MusicXml,
                None,
                context,
                ConvertOptions {
                    ignore_page_layout: true,
                },
            )?;
            match omr::retain_musicxml(&self.home, input, &musicxml) {
                Ok(path) => report.omr_source = Some(path_text(&path)),
                Err(error) => diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    message: format!("could not retain OMR MusicXML: {error}"),
                    raw_position: None,
                    position: None,
                }),
            }
            self.report(report, input, confirmation, diagnostics)
        } else {
            let report = self.engine.convert(input, format, None, context)?;
            self.report(report, input, confirmation, Vec::new())
        }
    }

    fn compile(
        &self,
        snapshot: &SourceSnapshot,
        options: &CompileOptions,
        artifact_root: &Path,
        context: &RunContext,
    ) -> Result<RenderManifest> {
        let mut render = self
            .engine
            .compile(snapshot, options, artifact_root, context)?;
        render.engine_build_id = self.build_id.clone();
        Ok(render)
    }
}

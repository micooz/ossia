use crate::Result;
use ossia_engine::{CompileOptions, Engine, RunContext};
use ossia_types::*;
use std::path::Path;

/// Native Engine is the production adapter; deterministic adapters exercise failure/race paths.
pub trait EngineBackend: Send + Sync {
    /// Export one complete layout score with its editable notation.
    fn render_musicxml(
        &self,
        _snapshot: &SourceSnapshot,
        _context: &RunContext,
    ) -> Result<Vec<u8>> {
        Err(AppError::unsupported(
            "MusicXML unavailable in this engine backend",
        ))
    }

    /// Render the score playback timeline as a WAV file when supported.
    fn render_wav(&self, _playback: &ScorePlayback, _context: &RunContext) -> Result<Vec<u8>> {
        Err(AppError::unsupported(
            "WAV unavailable in this engine backend",
        ))
    }

    /// Produce a content-addressed PNG when supported by this engine backend.
    fn rasterize_svg(
        &self,
        _svg: &[u8],
        _width: u32,
        _root: &Path,
        _context: &RunContext,
    ) -> Result<ArtifactRef> {
        Err(AppError::unsupported(
            "PNG unavailable in this engine backend",
        ))
    }

    /// Produce a PNG with optional lossy quantization and transparency.
    fn rasterize_svg_with_options(
        &self,
        svg: &[u8],
        width: u32,
        _quality: Option<u8>,
        _transparent: bool,
        root: &Path,
        context: &RunContext,
    ) -> Result<ArtifactRef> {
        self.rasterize_svg(svg, width, root, context)
    }

    fn build_id(&self) -> &str;
    /// Migrate LilyPond source into an uncommitted candidate.
    fn migrate_lilypond(&self, _input: &[u8], _context: &RunContext) -> Result<ImportReport> {
        Err(AppError::unsupported(
            "LilyPond migration is unavailable in this engine backend",
        ))
    }
    fn convert(
        &self,
        input: &[u8],
        format: InputFormat,
        confirmation: Option<&ContentHash>,
        context: &RunContext,
    ) -> Result<ImportReport>;
    fn compile(
        &self,
        snapshot: &SourceSnapshot,
        options: &CompileOptions,
        artifact_root: &Path,
        context: &RunContext,
    ) -> Result<RenderManifest>;
}

impl EngineBackend for Engine {
    fn render_musicxml(&self, snapshot: &SourceSnapshot, context: &RunContext) -> Result<Vec<u8>> {
        self.export_musicxml(snapshot, context)
    }

    fn migrate_lilypond(&self, input: &[u8], context: &RunContext) -> Result<ImportReport> {
        self.migrate_lilypond(input, context)
    }
    fn render_wav(&self, playback: &ScorePlayback, context: &RunContext) -> Result<Vec<u8>> {
        self.render_wav(playback, context)
    }

    fn rasterize_svg(
        &self,
        svg: &[u8],
        width: u32,
        root: &Path,
        context: &RunContext,
    ) -> Result<ArtifactRef> {
        self.rasterize_svg(svg, width, root, context)
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
        self.rasterize_svg_with_options(svg, width, quality, transparent, root, context)
    }

    fn build_id(&self) -> &str {
        self.build_id()
    }

    fn convert(
        &self,
        input: &[u8],
        format: InputFormat,
        confirmation: Option<&ContentHash>,
        context: &RunContext,
    ) -> Result<ImportReport> {
        self.convert(input, format, confirmation, context)
    }

    fn compile(
        &self,
        snapshot: &SourceSnapshot,
        options: &CompileOptions,
        artifact_root: &Path,
        context: &RunContext,
    ) -> Result<RenderManifest> {
        self.compile(snapshot, options, artifact_root, context)
    }
}

pub struct ImportRequest<'a> {
    pub title: &'a str,
    pub input: &'a [u8],
    pub format: InputFormat,
    pub confirmation: Option<&'a ContentHash>,
    pub author: &'a str,
}

pub struct BeginEditRequest<'a> {
    pub document_id: DocumentId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: &'a ContentHash,
    pub author: &'a str,
    /// Explicitly attribute a previously unknown workspace to this new edit.
    pub adopt_workspace: bool,
}

pub struct CheckRequest<'a> {
    pub expect: Option<&'a MusicExpectation>,
    pub reference: Option<&'a ReferenceMusic>,
    pub document_id: DocumentId,
    pub edit_id: Option<EditId>,
    pub expected_source_hash: &'a ContentHash,
    pub options: &'a CompileOptions,
}

pub struct CommitRequest<'a> {
    pub edit_id: EditId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: &'a ContentHash,
    pub summary: &'a str,
}

pub struct RestoreRequest<'a> {
    pub target_version_id: VersionId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: &'a ContentHash,
    pub discard_current_edit: bool,
    pub author: &'a str,
    pub summary: &'a str,
}

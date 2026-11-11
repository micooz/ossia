use crate::Result;
use ossia_engine::{CompileOptions, Engine, RunContext};
use ossia_types::*;
use std::path::Path;

/// Native Engine is the production adapter; deterministic adapters exercise failure/race paths.
pub trait EngineBackend: Send + Sync {
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

    fn build_id(&self) -> &str;
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
    fn rasterize_svg(
        &self,
        svg: &[u8],
        width: u32,
        root: &Path,
        context: &RunContext,
    ) -> Result<ArtifactRef> {
        self.rasterize_svg(svg, width, root, context)
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
    pub author: &'a str,
    pub summary: &'a str,
}

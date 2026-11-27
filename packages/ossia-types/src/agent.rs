//! Agent operations on owned source files and immutable draft snapshots.
use crate::*;

/// Read or export a score's freehand drawing.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadDrawingArgs {
    pub document_id: DocumentId,
    pub page: Option<u32>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    pub expected_revision: Option<u64>,
    pub output_path: Option<String>,
}

/// A stroke's index is valid only at the returned drawing revision.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct IndexedDrawingStroke {
    pub index: usize,
    pub stroke: DrawingStroke,
}

/// A bounded drawing page, or the path of an exported drawing.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ReadDrawingOutcome {
    pub revision: u64,
    pub stroke_count: usize,
    pub strokes: Vec<IndexedDrawingStroke>,
    pub next_offset: Option<usize>,
    pub output_path: Option<String>,
}

/// A change referring to an index in the original drawing.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum DrawingEdit {
    Add { stroke: DrawingStroke },
    Delete { index: usize },
    Replace { index: usize, stroke: DrawingStroke },
}

/// Replace from a file or an explicit array, or apply an atomic edit batch.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DrawingChange {
    Import { input_path: String },
    Replace { strokes: Vec<DrawingStroke> },
    Edit { edits: Vec<DrawingEdit> },
}

/// Apply a drawing change only at the expected destination revision.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteDrawingArgs {
    pub document_id: DocumentId,
    pub expected_revision: u64,
    pub change: DrawingChange,
}

/// The saved drawing's revision and total number of strokes.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct WriteDrawingOutcome {
    pub revision: u64,
    pub stroke_count: usize,
}

/// Start of a bounded source read. Line numbers are one-based.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SourceReadStart {
    Start,
    Line { line: u32 },
    Span { span: SourceSpan },
}

/// Read a bounded portion of the public source at one exact hash.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadSourceArgs {
    pub document_id: DocumentId,
    pub expected_source_hash: ContentHash,
    pub start: Option<SourceReadStart>,
    pub max_lines: Option<usize>,
    pub max_bytes: Option<usize>,
    #[serde(default)]
    pub include_line_spans: bool,
}

/// A UTF-8-safe source page and the exact remainder for the next call.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ReadSourceOutcome {
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub content: String,
    pub span: SourceSpan,
    pub next: Option<SourceSpan>,
    pub complete: bool,
    pub returned_bytes: usize,
    pub total_bytes: usize,
    pub total_lines: usize,
    pub line_spans: Vec<SourceSpan>,
}

/// One exact original-text replacement. Empty text inserts or deletes.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceEdit {
    pub span: SourceSpan,
    pub expected_text: String,
    pub replacement: String,
}

/// Whole-source replacement or one atomic batch of non-overlapping edits.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SourceWriteChange {
    Replace { text: String },
    Edit { edits: Vec<SourceEdit> },
}

/// Apply one atomic source change to the active owned edit.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteSourceArgs {
    pub edit_id: EditId,
    pub expected_source_hash: ContentHash,
    pub change: SourceWriteChange,
    #[serde(default = "preview_enabled")]
    pub preview: bool,
}

const fn preview_enabled() -> bool {
    true
}

/// Source state after a guarded atomic write.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct WriteSourceOutcome {
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub dirty: bool,
    pub source_bytes: usize,
}

/// Agent responses are compact unless the caller explicitly requests full evidence.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum DetailLevel {
    #[default]
    Summary,
    Full,
}

/// Bounded document listing for Agent transports.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListArgs {
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: usize,
}

/// Replace one active score's library title.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateMetadataArgs {
    pub document_id: DocumentId,
    pub title: String,
}

/// Compact document metadata without render manifests or check evidence.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub document_id: DocumentId,
    /// Stable `scores?documentId=…` link supplied by runtime Agent responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    pub title: String,
    pub updated_at: String,
    pub head_version_id: Option<VersionId>,
    pub source_hash: ContentHash,
    pub dirty: bool,
    pub edit_state: DocumentEditState,
}

/// One bounded page of compact document metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPage {
    pub items: Vec<DocumentSummary>,
    pub total: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

/// One bounded page of version nodes.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct VersionPage {
    pub items: Vec<Version>,
    pub total: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

/// Compact runtime state, excluding terminal task history.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub instance_id: String,
    pub runtime: RuntimeInfo,
    pub engine_build_id: String,
    pub document_count: usize,
    pub seq: u64,
    pub jobs: Vec<Job>,
    pub documentation: DocumentationStatus,
    pub language: Option<UiLanguage>,
    pub update: Option<AgentUpdate>,
}

/// The desktop updater's most recent check, with age evaluated at read time.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdate {
    pub state: UpdateState,
    pub current: String,
    pub latest: Option<String>,
    pub source: UpdateSource,
    pub channel: UpdateChannel,
    /// Unix timestamp in seconds.
    pub checked_at: u64,
    pub stale: bool,
}

/// Current document metadata without duplicated render manifests.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentDocument {
    pub document_id: DocumentId,
    /// Stable `scores?documentId=…` link for the browser score page.
    pub url: String,
    pub title: String,
    pub head_version_id: Option<VersionId>,
    pub source_hash: ContentHash,
    pub source_bytes: usize,
    pub dirty: bool,
    pub edit_state: DocumentEditState,
    pub last_check: Option<AgentCheckSummary>,
}

/// Compact musical coverage without embedded source or performance event arrays.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentMusicSummary {
    pub source_status: AnalysisStatus,
    pub source_reasons: Vec<String>,
    pub measures: Option<u32>,
    pub voices: Option<usize>,
    pub note_tokens: Option<usize>,
    pub source_events: usize,
    pub performance_status: AnalysisStatus,
    pub performance_reasons: Vec<String>,
    pub performed_notes: usize,
    pub performance_measures: Option<u32>,
    pub measure_reason: Option<String>,
}

/// One completed or failed check without duplicated evidence arrays.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentCheckSummary {
    pub job_id: JobId,
    pub kind: String,
    pub state: JobState,
    pub phase: String,
    pub source_hash: Option<ContentHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<AppError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub verification_status: Option<VerificationStatus>,
    pub difference_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reference: Option<ReferenceVerification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub music: Option<AgentMusicSummary>,
    pub render_pages: usize,
}

/// Stable preview identity with a page count; use job evidence or output to read a page.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentRender {
    pub render_key: ContentHash,
    pub source_hash: ContentHash,
    pub pages: usize,
}

/// Compact document context for an Agent task.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentDocumentContext {
    pub instance_id: String,
    pub seq: u64,
    pub document: AgentDocument,
    pub selection: Option<Selection>,
    pub regions: Vec<SelectionRegion>,
    pub annotation_regions: Vec<SelectionRegion>,
    pub render: Option<AgentRender>,
    pub edit_activity: Option<EditActivity>,
}

/// Which immutable evidence to page through.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum AnalysisSection {
    #[default]
    Performance,
    Source,
    Differences,
}

/// A bounded page of source events, performed notes or verification differences.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisPage {
    pub source_job_id: JobId,
    pub source_hash: ContentHash,
    pub engine_build_id: String,
    pub section: AnalysisSection,
    pub total: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub items: Vec<serde_json::Value>,
}

/// Live progress within one export stage; counts do not describe the entire job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub phase: String,
    pub completed: Option<u32>,
    pub total: Option<u32>,
}

/// A job and, when requested, one bounded evidence page from that job.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct JobOutcome {
    pub job: Job,
    #[serde(default)]
    pub progress: Option<JobProgress>,
    pub analysis: Option<AnalysisPage>,
}

/// Move one active score into the recoverable trash.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteArgs {
    pub document_id: DocumentId,
    pub expected_source_hash: ContentHash,
    #[serde(default)]
    pub force: bool,
}

/// Select one trashed or active document.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentIdArgs {
    pub document_id: DocumentId,
}

/// Bounded pagination over the recoverable trash.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrashArgs {
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: usize,
}

/// Compact metadata for one trashed score.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct TrashedDocumentSummary {
    pub document_id: DocumentId,
    pub title: String,
    pub deleted_at: String,
    pub source_hash: ContentHash,
    pub head_version_id: Option<VersionId>,
}

/// One bounded page of trashed scores.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct TrashPage {
    pub items: Vec<TrashedDocumentSummary>,
    pub total: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

/// Result of permanently removing one trashed score.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PurgedDocument {
    pub document_id: DocumentId,
}

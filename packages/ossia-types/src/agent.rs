//! Agent operations on owned source files and immutable draft snapshots.
use crate::*;

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
    pub view_id: ViewId,
    pub expected_source_hash: ContentHash,
    pub start: Option<SourceReadStart>,
    pub max_lines: Option<usize>,
    pub max_bytes: Option<usize>,
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
    pub view_id: ViewId,
    pub edit_id: EditId,
    pub expected_source_hash: ContentHash,
    pub change: SourceWriteChange,
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

/// Compact document metadata without render manifests or check evidence.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub document_id: DocumentId,
    pub title: String,
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
}

/// Current document metadata without duplicated render manifests.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentDocument {
    pub document_id: DocumentId,
    pub title: String,
    pub head_version_id: Option<VersionId>,
    pub source_hash: ContentHash,
    pub source_bytes: usize,
    pub dirty: bool,
    pub edit_state: DocumentEditState,
    pub last_check: Option<Job>,
}

/// Stable preview identity with a page count; use job evidence or output to read a page.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentRender {
    pub render_key: ContentHash,
    pub source_hash: ContentHash,
    pub pages: usize,
}

/// Isolated view metadata for an Agent task.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentView {
    pub instance_id: String,
    pub seq: u64,
    pub view_id: ViewId,
    pub document: Option<AgentDocument>,
    pub selection: Option<Selection>,
    pub regions: Vec<LocatedRegion>,
    pub render: Option<AgentRender>,
    pub edit_activity: Option<EditActivity>,
}

/// A newly isolated Agent view and its initial compact context.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentOpenOutcome {
    pub view_id: ViewId,
    pub url: String,
    pub context: AgentView,
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

/// A job and, when requested, one bounded evidence page from that job.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct JobOutcome {
    pub job: Job,
    pub analysis: Option<AnalysisPage>,
}

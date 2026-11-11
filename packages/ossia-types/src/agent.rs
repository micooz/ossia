//! Agent operations on owned source files and immutable draft snapshots.
use crate::*;

/// Replace only the active edit's source; the runtime computes the new hash.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteSourceArgs {
    pub document_id: DocumentId,
    pub view_id: ViewId,
    pub edit_id: EditId,
    pub expected_source_hash: ContentHash,
    pub text: String,
}
/// Source state after a guarded atomic write.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct WriteSourceOutcome {
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub source_path: String,
    pub dirty: bool,
}

/// A draft artifact format, independent of versioned exports.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum DraftFormat {
    Svg,
    Midi,
    Pdf,
    Png,
}
/// Prepare an artifact from a finished import/check, never from the latest mutable file.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactArgs {
    pub job_id: JobId,
    pub format: DraftFormat,
    pub page: Option<u32>,
    pub width: Option<u32>,
}
/// Verified local artifact location and immutable provenance.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactOutcome {
    pub source_job_id: JobId,
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub engine_build_id: String,
    pub format: DraftFormat,
    pub page: Option<u32>,
    pub path: String,
    pub sha256: ContentHash,
    pub bytes: u64,
    pub media_type: String,
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
/// Runtime status projection for Agent transports.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusArgs {
    #[serde(default)]
    pub detail: DetailLevel,
}
/// Compact runtime state, excluding terminal task history.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub instance_id: String,
    pub runtime: RuntimeInfo,
    pub engine_build_id: String,
    pub document_count: usize,
    pub jobs: Vec<Job>,
}
/// Current document metadata without duplicated render manifests.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentDocument {
    pub document_id: DocumentId,
    pub title: String,
    pub source_path: String,
    pub head_version_id: Option<VersionId>,
    pub source_hash: ContentHash,
    pub dirty: bool,
    pub edit_state: DocumentEditState,
    pub last_check: Option<Job>,
}
/// Stable preview identity with a page count; use artifact to read a page.
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
/// Read bounded evidence from a completed import/check job.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisArgs {
    pub job_id: JobId,
    #[serde(default)]
    pub section: AnalysisSection,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
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

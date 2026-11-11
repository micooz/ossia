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

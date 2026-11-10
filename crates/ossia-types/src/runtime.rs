use crate::*;
use schemars::JsonSchema;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub runtime_version: String,
    pub protocol_version: u32,
    pub schema_version: u32,
    pub instance_id: String,
    pub home: String,
    pub web_base_url: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub view_id: ViewId,
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub locators: Vec<Locator>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RenderReference {
    pub render_key: ContentHash,
    pub manifest: RenderManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ViewSnapshot {
    pub instance_id: String,
    pub seq: u64,
    pub view_id: ViewId,
    pub document: Option<DocumentState>,
    pub selection: Option<Selection>,
    pub regions: Vec<LocatedRegion>,
    pub render: Option<RenderReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpenView {
    pub view_id: ViewId,
    pub url: String,
    pub ticket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeRequest {
    pub ticket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeResponse {
    pub token: String,
    pub view: ViewSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEvent {
    pub instance_id: String,
    pub seq: u64,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub view_id: Option<ViewId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub document_id: Option<DocumentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_hash: Option<ContentHash>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportArgs {
    pub title: String,
    pub format: InputFormat,
    pub path: String,
    pub confirmation: Option<ContentHash>,
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenArgs {
    pub document_id: Option<DocumentId>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextArgs {
    pub view_id: ViewId,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum EditAction {
    Begin,
    Resume,
    End,
    Discard,
    ResolveRecoveryConflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditArgs {
    pub action: EditAction,
    pub document_id: DocumentId,
    pub view_id: ViewId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: ContentHash,
    pub edit_id: Option<EditId>,
    #[serde(default)]
    pub adopt_workspace: bool,
    #[serde(default = "default_author")]
    pub author: String,
}
fn default_author() -> String {
    "agent".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckArgs {
    pub document_id: DocumentId,
    pub view_id: ViewId,
    pub edit_id: Option<EditId>,
    pub expected_source_hash: ContentHash,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitArgs {
    pub document_id: DocumentId,
    pub view_id: ViewId,
    pub edit_id: EditId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: ContentHash,
    pub summary: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryArgs {
    pub document_id: DocumentId,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreArgs {
    pub document_id: DocumentId,
    pub version_id: VersionId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: ContentHash,
    pub author: String,
    pub summary: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ExportFormat {
    Ly,
    Pdf,
    Midi,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportArgs {
    pub document_id: DocumentId,
    pub version_id: VersionId,
    pub format: ExportFormat,
    pub output_path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportOutcome {
    pub document_id: DocumentId,
    pub version_id: VersionId,
    pub source_hash: ContentHash,
    pub format: ExportFormat,
    pub path: String,
    pub bytes: u64,
    pub sha256: ContentHash,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum JobAction {
    Get,
    Cancel,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobArgs {
    pub job_id: JobId,
    pub action: JobAction,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocateArgs {
    pub source_hash: ContentHash,
    pub page: u32,
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocateOutcome {
    pub capability: LocationCapability,
    pub reason: Option<String>,
    pub region: Option<LocatedRegion>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct StatusSnapshot {
    pub runtime: RuntimeInfo,
    pub engine_build_id: String,
    pub jobs: Vec<Job>,
    pub instance_id: String,
    pub seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmptyArgs {}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RpcRequest {
    pub method: String,
    pub params: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RpcResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<AppError>,
}

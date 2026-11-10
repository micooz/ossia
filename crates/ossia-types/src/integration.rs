use crate::*;
use schemars::JsonSchema;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum HostKind {
    Codex,
    Dsh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum IntegrationMode {
    SkillMcp,
    Plugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum IntegrationAction {
    Detect,
    Plan,
    Apply,
    Status,
    Repair,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum IntegrationState {
    Unavailable,
    Available,
    Planned,
    PendingHost,
    Installed,
    Conflict,
    Broken,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum IntegrationOperation {
    Create,
    Replace,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntegrationRequest {
    pub action: IntegrationAction,
    pub host: Option<HostKind>,
    pub profile: Option<String>,
    pub mode: Option<IntegrationMode>,
    pub plan_hash: Option<ContentHash>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct HostDetection {
    pub host: HostKind,
    pub installed: bool,
    pub version: Option<String>,
    pub executable: Option<String>,
    pub config_root: String,
    pub skill_root: Option<String>,
    pub profiles: Vec<String>,
    pub capabilities: Vec<String>,
    pub problem: Option<AppError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationDetection {
    pub hosts: Vec<HostDetection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationEntry {
    pub operation: IntegrationOperation,
    pub ownership_id: String,
    pub path: String,
    pub kind: String,
    pub sha256: Option<ContentHash>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationConflict {
    pub target: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationPlan {
    pub plan_hash: ContentHash,
    pub management_id: String,
    pub host: HostKind,
    pub profile: String,
    pub mode: IntegrationMode,
    pub home: String,
    pub executable: String,
    pub host_version: Option<String>,
    pub entries: Vec<IntegrationEntry>,
    pub conflicts: Vec<IntegrationConflict>,
    pub warnings: Vec<String>,
    pub requires_host_completion: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationRecordView {
    pub management_id: String,
    pub mode: IntegrationMode,
    pub executable: String,
    pub home: String,
    pub template_version: u32,
    pub updated_at: String,
    pub entries: Vec<IntegrationEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationObservation {
    pub target: String,
    pub kind: String,
    pub state: IntegrationState,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub detection: HostDetection,
    pub state: IntegrationState,
    pub mode: Option<IntegrationMode>,
    pub management: Option<IntegrationRecordView>,
    pub observations: Vec<IntegrationObservation>,
    pub conflicts: Vec<IntegrationConflict>,
    pub message: Option<String>,
}

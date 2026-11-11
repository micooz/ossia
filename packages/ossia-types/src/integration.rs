use crate::AppError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Stable Agent identifier. Adapters own the meaning; the UI never branches on it.
pub type AgentId = String;

/// How Ossia exposes itself to one Agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum InstallMethod {
    Plugin,
    Skill,
    Mcp,
}

/// One Agent's current integration state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum AgentState {
    Available,
    Installed,
    Disabled,
    Broken,
    Conflict,
    Unavailable,
    UpdateRequired,
}

/// A host-side problem that prevents Ossia from taking ownership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentConflict {
    pub target: String,
    pub kind: String,
    pub message: String,
}

/// One supported Agent and the resources Ossia can install for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentIntegration {
    pub id: AgentId,
    pub name: String,
    pub icon: String,
    pub method: InstallMethod,
    pub state: AgentState,
    pub installed: bool,
    pub enabled: bool,
    pub managed: bool,
    pub version: Option<String>,
    pub executable: Option<String>,
    pub config_root: String,
    /// Exact host configuration file changed by the integration.
    pub config_path: String,
    pub profile: Option<String>,
    pub path: Option<String>,
    pub reload_hint: Option<String>,
    pub message: Option<String>,
    pub conflicts: Vec<AgentConflict>,
}

/// The complete Agent list shown by the management window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentsSnapshot {
    pub agents: Vec<AgentIntegration>,
    pub syncing: bool,
    pub error: Option<AppError>,
}

/// One batch install or remove request from the management window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentsChange {
    pub agent_ids: Vec<AgentId>,
    pub enabled: bool,
}

/// One Agent whose write failed while the rest of the batch continued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentProblem {
    pub agent_id: AgentId,
    pub message: String,
}

/// Result of one batch change, with state read back from disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentsMutation {
    pub snapshot: AgentsSnapshot,
    pub problems: Vec<AgentProblem>,
}

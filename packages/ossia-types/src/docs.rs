//! Queries and health for the bundled version-matched LilyPond reference.
use crate::*;
/// Offline reference operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum DocsAction {
    Search,
    Read,
}
/// Search keywords or read one stable chapter ID with character-based pagination.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocsArgs {
    pub action: DocsAction,
    pub query: Option<String>,
    pub chapter_id: Option<String>,
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: usize,
}
/// Search result carrying authoritative source and local readable text paths.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct DocsHit {
    pub chapter_id: String,
    pub manual: String,
    pub title: String,
    pub excerpt: String,
    pub url: String,
    pub path: String,
}
/// A bounded local documentation response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct DocsOutcome {
    pub version: String,
    pub root: String,
    pub hits: Vec<DocsHit>,
    pub text: Option<String>,
    pub offset: usize,
    pub next_offset: Option<usize>,
}
/// Documentation errors do not disable score operations.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentationStatus {
    pub available: bool,
    pub version: String,
    pub root: Option<String>,
    pub error: Option<AppError>,
}

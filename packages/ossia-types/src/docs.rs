//! Queries and health for the bundled version-matched LilyPond reference.
use crate::*;

/// Search keywords or read one stable chapter ID with character-based pagination.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(
    tag = "action",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum DocsRequest {
    Search {
        query: String,
        limit: Option<usize>,
    },
    Read {
        chapter_id: String,
        #[serde(default)]
        offset: usize,
        limit: Option<usize>,
    },
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

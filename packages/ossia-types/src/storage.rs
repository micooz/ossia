use crate::*;
use schemars::JsonSchema;
use ts_rs::TS;

/// Regenerable artifact cache budgets exposed by the management window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum StorageCacheLimit {
    Mib512,
    #[default]
    Gib1,
    #[serde(alias = "gib2")]
    Gib3,
    Gib10,
}

impl StorageCacheLimit {
    pub fn bytes(self) -> u64 {
        match self {
            Self::Mib512 => 512 * 1024 * 1024,
            Self::Gib1 => 1024 * 1024 * 1024,
            Self::Gib3 => 3 * 1024 * 1024 * 1024,
            Self::Gib10 => 10 * 1024 * 1024 * 1024,
        }
    }
}

/// Current data-directory usage grouped by cleanup policy.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct StorageStatus {
    pub home: String,
    pub total_bytes: u64,
    pub core_bytes: u64,
    pub cache_bytes: u64,
    pub cache_limit_bytes: u64,
    pub database_bytes: u64,
    pub documents_bytes: u64,
    pub contents_bytes: u64,
    pub artifacts_bytes: u64,
    pub transient_bytes: u64,
    pub integrations_bytes: u64,
    pub documentation_bytes: u64,
    pub job_count: u64,
    pub render_count: u64,
    pub core_warning: bool,
    pub idle: bool,
}

/// Result of one policy-driven storage cleanup.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct StorageCleanupReport {
    pub freed_bytes: u64,
    pub removed_jobs: u64,
    pub removed_renders: u64,
    pub removed_content_objects: u64,
    pub removed_cache_files: u64,
    pub removed_staging_dirs: u64,
    pub over_limit: bool,
    pub warnings: Vec<String>,
}

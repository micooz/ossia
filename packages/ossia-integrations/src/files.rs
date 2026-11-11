use crate::*;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(crate) struct FilePlan {
    pub(crate) path: PathBuf,
    pub(crate) bytes: Vec<u8>,
    pub(crate) kind: &'static str,
}

pub(crate) fn json_file(path: PathBuf, value: serde_json::Value, kind: &'static str) -> FilePlan {
    FilePlan {
        path,
        bytes: serde_json::to_vec_pretty(&value).expect("JSON serialization"),
        kind,
    }
}

pub(crate) fn records_from_plan(plan: &IntegrationPlan, files: &[FilePlan]) -> Vec<ManagedEntry> {
    plan.entries
        .iter()
        .map(|entry| ManagedEntry {
            ownership_id: entry.ownership_id.clone(),
            path: entry.path.clone(),
            kind: entry.kind.clone(),
            sha256: entry.sha256.clone().or_else(|| {
                files
                    .iter()
                    .find(|file| file.path.to_string_lossy() == entry.path)
                    .map(|file| ContentHash::of(&file.bytes))
            }),
        })
        .collect()
}

pub(crate) fn zero_hash() -> ContentHash {
    serde_json::from_value(serde_json::Value::String("0".repeat(64))).expect("valid zero hash")
}

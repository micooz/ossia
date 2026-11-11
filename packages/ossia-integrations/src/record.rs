use ossia_types::{AppError, ContentHash, ErrorCode, InstallMethod};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const TEMPLATE_VERSION: u32 = 2;
const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedEntry {
    pub ownership_id: String,
    pub path: String,
    pub kind: String,
    pub sha256: Option<ContentHash>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRecord {
    pub management_id: String,
    pub agent_id: String,
    pub method: InstallMethod,
    pub profile: String,
    pub template_version: u32,
    pub home: String,
    pub executable: String,
    pub shared_root: String,
    pub updated_at: String,
    pub entries: Vec<ManagedEntry>,
}

#[derive(Debug, Clone)]
pub struct LegacyRecord {
    pub agent_id: String,
    pub profile: String,
    pub mode: String,
    pub entries: Vec<ManagedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedStore {
    pub schema_version: u32,
    #[serde(default)]
    pub connection_order: Vec<String>,
    #[serde(default)]
    pub records: BTreeMap<String, ManagedRecord>,
}

impl Default for ManagedStore {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            connection_order: Vec::new(),
            records: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedStore {
    pub store: ManagedStore,
    pub legacy: Vec<LegacyRecord>,
}

pub fn integration_root(home: &Path) -> PathBuf {
    home.join("integrations")
}

pub fn store_path(home: &Path) -> PathBuf {
    integration_root(home).join("managed.json")
}

pub fn management_id(agent_id: &str, profile: &str) -> String {
    format!("ossia:{agent_id}:{profile}")
}

pub fn load(home: &Path) -> Result<LoadedStore, AppError> {
    let path = store_path(home);
    if !path.exists() {
        return Ok(LoadedStore {
            store: ManagedStore::default(),
            legacy: Vec::new(),
        });
    }
    let bytes = fs::read(&path).map_err(storage)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::new(
            ErrorCode::StorageError,
            format!("invalid managed integration record: {error}"),
        )
    })?;
    let schema = value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    match schema {
        2 => {
            let mut store: ManagedStore = serde_json::from_value(value).map_err(|error| {
                AppError::new(
                    ErrorCode::StorageError,
                    format!("invalid managed integration record: {error}"),
                )
            })?;
            // Older stores only retain the last write time, not the original connection order.
            let mut existing: Vec<_> = store.records.values().collect();
            existing.sort_by_key(|record| (record.updated_at.len(), &record.updated_at));
            for record in existing {
                if !store.connection_order.contains(&record.agent_id) {
                    store.connection_order.push(record.agent_id.clone());
                }
            }
            Ok(LoadedStore {
                store,
                legacy: Vec::new(),
            })
        }
        1 => Ok(LoadedStore {
            store: ManagedStore::default(),
            legacy: legacy_records(&value)?,
        }),
        version => Err(AppError::new(
            ErrorCode::RuntimeIncompatible,
            format!("integration record schema {version} is not supported"),
        )),
    }
}

fn legacy_records(value: &Value) -> Result<Vec<LegacyRecord>, AppError> {
    let mut records = Vec::new();
    let Some(items) = value.get("records").and_then(Value::as_object) else {
        return Ok(records);
    };
    for record in items.values() {
        let Some(object) = record.as_object() else {
            continue;
        };
        let agent_id = match object.get("host").and_then(Value::as_str) {
            Some("codex") => "codex",
            Some("dsh") => "dsh",
            _ => continue,
        };
        let profile = object
            .get("profile")
            .and_then(Value::as_str)
            .unwrap_or("default");
        let mode = object
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("plugin");
        let entries = object
            .get("entries")
            .cloned()
            .map(serde_json::from_value::<Vec<ManagedEntry>>)
            .transpose()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::StorageError,
                    format!("invalid legacy integration entries: {error}"),
                )
            })?
            .unwrap_or_default();
        records.push(LegacyRecord {
            agent_id: agent_id.into(),
            profile: profile.into(),
            mode: mode.into(),
            entries,
        });
    }
    Ok(records)
}

pub fn write(home: &Path, store: &ManagedStore) -> Result<(), AppError> {
    let root = integration_root(home);
    create_private_dir(&root)?;
    let bytes = serde_json::to_vec_pretty(store).map_err(storage)?;
    atomic_write(&store_path(home), &bytes)
}

pub fn lock(home: &Path) -> Result<File, AppError> {
    lock_file(&integration_root(home).join(".lock"))
}

pub fn shared_lock(shared_root: &Path) -> Result<File, AppError> {
    lock_file(&shared_root.join(".lock"))
}

fn lock_file(path: &Path) -> Result<File, AppError> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(storage)?;
    fs2::FileExt::lock_exclusive(&file).map_err(storage)?;
    Ok(file)
}

pub fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{seconds}")
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new(ErrorCode::StorageError, "managed file has no parent"))?;
    fs::create_dir_all(parent).map_err(storage)?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(storage)?;
    file.write_all(bytes).map_err(storage)?;
    file.as_file().sync_all().map_err(storage)?;
    file.persist(path)
        .map(|_| ())
        .map_err(|error| AppError::new(ErrorCode::StorageError, error.error.to_string()))
}

pub fn create_private_dir(path: &Path) -> Result<(), AppError> {
    fs::create_dir_all(path).map_err(storage)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(storage)?;
    }
    Ok(())
}

pub fn storage(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::StorageError, error.to_string())
}

#[cfg(test)]
mod tests;

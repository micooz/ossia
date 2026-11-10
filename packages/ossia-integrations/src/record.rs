use ossia_types::{AppError, ContentHash, ErrorCode, HostKind, IntegrationMode};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const TEMPLATE_VERSION: u32 = 1;

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
    pub host: HostKind,
    pub profile: String,
    pub mode: IntegrationMode,
    pub template_version: u32,
    pub home: String,
    pub executable: String,
    pub updated_at: String,
    pub entries: Vec<ManagedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedStore {
    pub schema_version: u32,
    #[serde(default)]
    pub records: BTreeMap<String, ManagedRecord>,
}

impl Default for ManagedStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            records: BTreeMap::new(),
        }
    }
}

pub fn integration_root(home: &Path) -> PathBuf {
    home.join("integrations")
}

pub fn store_path(home: &Path) -> PathBuf {
    integration_root(home).join("managed.json")
}

pub fn management_id(host: HostKind, profile: &str) -> String {
    format!(
        "ossia:{}:{profile}",
        match host {
            HostKind::Codex => "codex",
            HostKind::Dsh => "dsh",
        }
    )
}

pub fn load(home: &Path) -> Result<ManagedStore, AppError> {
    let path = store_path(home);
    if !path.exists() {
        return Ok(ManagedStore::default());
    }
    let bytes = fs::read(&path).map_err(storage)?;
    let store: ManagedStore = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::new(
            ErrorCode::StorageError,
            format!("invalid managed integration record: {error}"),
        )
    })?;
    if store.schema_version != 1 {
        return Err(AppError::new(
            ErrorCode::RuntimeIncompatible,
            format!(
                "integration record schema {} is not supported",
                store.schema_version
            ),
        ));
    }
    Ok(store)
}

pub fn write(home: &Path, store: &ManagedStore) -> Result<(), AppError> {
    let root = integration_root(home);
    create_private_dir(&root)?;
    let path = store_path(home);
    let bytes = serde_json::to_vec_pretty(store).map_err(storage)?;
    atomic_write(&path, &bytes)
}

pub fn lock(home: &Path) -> Result<File, AppError> {
    let root = integration_root(home);
    create_private_dir(&root)?;
    let path = root.join(".lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
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

use crate::*;
use ossia_types::*;
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn managed_from_plan(plan: &IntegrationPlan) -> Vec<ManagedEntry> {
    plan.entries
        .iter()
        .map(|entry| ManagedEntry {
            ownership_id: entry.ownership_id.clone(),
            path: entry.path.clone(),
            kind: entry.kind.clone(),
            sha256: entry.sha256.clone(),
        })
        .collect()
}

pub(crate) fn file_entry(
    path: &Path,
    bytes: &[u8],
    kind: &str,
    ownership: &str,
) -> IntegrationEntry {
    let exists = path.exists();
    IntegrationEntry {
        operation: if exists {
            IntegrationOperation::Replace
        } else {
            IntegrationOperation::Create
        },
        ownership_id: ownership.into(),
        path: path.to_string_lossy().into_owned(),
        kind: kind.into(),
        sha256: Some(ContentHash::of(bytes)),
    }
}

pub(crate) fn on_disk(path: &Path) -> Option<ContentHash> {
    fs::read(path).ok().map(|bytes| ContentHash::of(&bytes))
}

pub(crate) fn delete_owned(path: &Path, expected: &ContentHash) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }
    let current = on_disk(path)
        .ok_or_else(|| AppError::new(ErrorCode::StorageError, "managed file is unreadable"))?;
    if &current != expected {
        return Err(AppError::new(
            ErrorCode::Conflict,
            format!("managed file was modified: {}", path.display()),
        ));
    }
    fs::remove_file(path).map_err(storage)
}

pub(crate) fn verify_owned_files(entries: &[ManagedEntry]) -> Result<(), AppError> {
    for entry in entries.iter().filter(|entry| entry.sha256.is_some()) {
        let path = Path::new(&entry.path);
        if !path.exists() {
            continue;
        }
        let current = on_disk(path)
            .ok_or_else(|| AppError::new(ErrorCode::StorageError, "managed file is unreadable"))?;
        if entry.sha256.as_ref() != Some(&current) {
            return Err(AppError::new(
                ErrorCode::Conflict,
                format!("managed file was modified: {}", path.display()),
            ));
        }
    }
    Ok(())
}

pub(crate) fn record_view(record: &ManagedRecord) -> IntegrationRecordView {
    IntegrationRecordView {
        management_id: record.management_id.clone(),
        mode: record.mode,
        executable: record.executable.clone(),
        home: record.home.clone(),
        template_version: record.template_version,
        updated_at: record.updated_at.clone(),
        entries: record
            .entries
            .iter()
            .map(|entry| IntegrationEntry {
                operation: IntegrationOperation::Create,
                ownership_id: entry.ownership_id.clone(),
                path: entry.path.clone(),
                kind: entry.kind.clone(),
                sha256: entry.sha256.clone(),
            })
            .collect(),
    }
}

pub(crate) fn conflict(
    target: impl Into<String>,
    kind: &str,
    message: impl Into<String>,
) -> IntegrationConflict {
    IntegrationConflict {
        target: target.into(),
        kind: kind.into(),
        message: message.into(),
    }
}

pub(crate) fn observation(
    target: impl Into<String>,
    kind: &str,
    state: IntegrationState,
    message: Option<String>,
) -> IntegrationObservation {
    IntegrationObservation {
        target: target.into(),
        kind: kind.into(),
        state,
        message,
    }
}

pub(crate) fn storage(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::StorageError, error.to_string())
}

pub(crate) fn absolute_path(path: &Path, label: &str) -> Result<PathBuf, AppError> {
    if !path.is_absolute() {
        return Err(AppError::new(
            ErrorCode::InvalidRequest,
            format!("{label} must be absolute"),
        ));
    }
    if path.exists() {
        path.canonicalize().map_err(storage)
    } else {
        Ok(path.to_path_buf())
    }
}

pub(crate) fn absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

pub(crate) fn profile_for(host: HostKind, profile: Option<&str>) -> String {
    profile
        .unwrap_or(match host {
            HostKind::Codex => "default",
            HostKind::Dsh => "desktop",
        })
        .to_string()
}

pub(crate) fn mode_name(mode: IntegrationMode) -> &'static str {
    match mode {
        IntegrationMode::SkillMcp => "skill-mcp",
        IntegrationMode::Plugin => "plugin",
    }
}

pub(crate) fn missing(field: &str) -> AppError {
    AppError::new(ErrorCode::InvalidRequest, format!("{field} is required"))
}

pub(crate) fn serialize(value: impl Serialize) -> Result<Value, AppError> {
    serde_json::to_value(value).map_err(storage)
}

pub(crate) fn read_resource(root: &Path, name: &str) -> Result<String, AppError> {
    read_resource_bytes(root, name).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) fn read_resource_bytes(root: &Path, name: &str) -> Result<Vec<u8>, AppError> {
    fs::read(root.join(name)).map_err(|error| {
        AppError::new(
            ErrorCode::UnsupportedSource,
            format!("integration resource {name} is unavailable: {error}"),
        )
    })
}

pub(crate) fn zero_plan_hash() -> ContentHash {
    serde_json::from_value(Value::String("0".repeat(64))).expect("valid zero hash")
}

pub(crate) fn plan_hash(plan: &IntegrationPlan) -> Result<ContentHash, AppError> {
    let mut normalized = plan.clone();
    normalized.plan_hash = zero_plan_hash();
    let bytes = serde_json::to_vec(&normalized).map_err(storage)?;
    Ok(ContentHash::of(&bytes))
}

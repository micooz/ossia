use crate::*;
use ossia_types::{AgentConflict, AppError, ContentHash, ErrorCode};
use std::{
    fs,
    path::{Path, PathBuf},
};

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

pub(crate) fn file_entry(path: &Path, bytes: &[u8], kind: &str) -> ManagedEntry {
    ManagedEntry {
        ownership_id: path.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        kind: kind.into(),
        sha256: Some(ContentHash::of(bytes)),
    }
}

pub(crate) fn records_from_files(files: &[FilePlan]) -> Vec<ManagedEntry> {
    files
        .iter()
        .map(|file| file_entry(&file.path, &file.bytes, file.kind))
        .collect()
}

pub(crate) fn write_files(files: &[FilePlan]) -> Result<(), AppError> {
    for file in files {
        record::atomic_write(&file.path, &file.bytes)?;
    }
    Ok(())
}

pub(crate) fn snapshot_files(files: &[FilePlan]) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    files
        .iter()
        .map(|file| (file.path.clone(), fs::read(&file.path).ok()))
        .collect()
}

pub(crate) fn restore_files(snapshot: &[(PathBuf, Option<Vec<u8>>)]) -> Result<(), AppError> {
    for (path, bytes) in snapshot {
        match bytes {
            Some(bytes) => record::atomic_write(path, bytes)?,
            None if path.exists() => fs::remove_file(path).map_err(record::storage)?,
            None => {}
        }
    }
    Ok(())
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
    fs::remove_file(path).map_err(record::storage)
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

pub(crate) fn file_conflicts(
    files: &[FilePlan],
    previous: Option<&ManagedRecord>,
) -> Vec<AgentConflict> {
    let mut conflicts = Vec::new();
    for file in files {
        if !file.path.exists() {
            continue;
        }
        let Some(previous) = previous else {
            conflicts.push(conflict(
                file.path.to_string_lossy(),
                "file",
                "an unowned file already exists at the managed target",
            ));
            continue;
        };
        let owned = previous
            .entries
            .iter()
            .find(|entry| entry.path == file.path.to_string_lossy());
        let Some(owned) = owned else {
            conflicts.push(conflict(
                file.path.to_string_lossy(),
                "file",
                "an unowned file already exists at the managed target",
            ));
            continue;
        };
        if owned.sha256.as_ref() != on_disk(&file.path).as_ref() {
            conflicts.push(conflict(
                file.path.to_string_lossy(),
                "file",
                "the managed file was modified after installation",
            ));
        }
    }
    conflicts
}

pub(crate) fn record_file_conflicts(entries: &[ManagedEntry]) -> Vec<AgentConflict> {
    let mut conflicts = Vec::new();
    for entry in entries.iter().filter(|entry| entry.sha256.is_some()) {
        let path = Path::new(&entry.path);
        match on_disk(path) {
            None => {}
            Some(actual) if entry.sha256.as_ref() != Some(&actual) => conflicts.push(conflict(
                &entry.path,
                &entry.kind,
                "managed file was modified",
            )),
            _ => {}
        }
    }
    conflicts
}

pub(crate) fn record_files_missing(entries: &[ManagedEntry]) -> bool {
    entries
        .iter()
        .filter(|entry| entry.sha256.is_some())
        .any(|entry| !Path::new(&entry.path).exists())
}

pub(crate) fn conflict(
    target: impl Into<String>,
    kind: &str,
    message: impl Into<String>,
) -> AgentConflict {
    AgentConflict {
        target: target.into(),
        kind: kind.into(),
        message: message.into(),
    }
}

pub(crate) fn conflict_error(conflicts: &[AgentConflict]) -> AppError {
    AppError::new(
        ErrorCode::Conflict,
        conflicts
            .iter()
            .map(|item| item.message.as_str())
            .collect::<Vec<_>>()
            .join("; "),
    )
}

pub(crate) fn absolute_path(path: &Path, label: &str) -> Result<PathBuf, AppError> {
    if !path.is_absolute() {
        return Err(AppError::new(
            ErrorCode::InvalidRequest,
            format!("{label} must be absolute"),
        ));
    }
    if path.exists() {
        path.canonicalize().map_err(record::storage)
    } else {
        Ok(path.to_path_buf())
    }
}

pub(crate) fn absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

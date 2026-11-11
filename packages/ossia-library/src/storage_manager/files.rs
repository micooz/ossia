use super::*;
use fs2::FileExt;
use rusqlite::Connection;
use std::{
    fs::{self, OpenOptions},
    path::Path,
    time::{Duration, SystemTime},
};

impl Library {
    pub(super) fn cleanup_staging(
        &self,
        policy: StoragePolicy,
        report: &mut StorageCleanupReport,
    ) -> Result<()> {
        let imports = self.store.path("imports")?;
        remove_matching_dirs(&imports, |name| name.starts_with("import-"), report)?;
        let scratch = self.store.path("run/scratch")?;
        remove_old_entries(&scratch, policy.scratch_retention, "ossia-engine-", report)?;
        Ok(())
    }

    pub(super) fn cleanup_docs(&self, report: &mut StorageCleanupReport) -> Result<()> {
        let root = self.store.path("docs/lilypond")?;
        let current: Option<String> = fs::read(root.join("current.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value["contentHash"].as_str().map(str::to_owned));
        let Some(current) = current else {
            return Ok(());
        };
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(storage_error(error)),
        };
        for entry in entries {
            let entry = entry.map_err(storage_error)?;
            let kind = entry.file_type().map_err(storage_error)?;
            if kind.is_dir() && !kind.is_symlink() {
                remove_old_docs(&entry.path(), &current, report)?;
            }
        }
        Ok(())
    }

    pub(super) fn cleanup_stale_locks(
        &self,
        age: Duration,
        report: &mut StorageCleanupReport,
    ) -> Result<()> {
        let root = self.store.path("artifacts/locks")?;
        for entry in read_dir_optional(&root)? {
            let path = entry.path();
            let kind = entry.file_type().map_err(storage_error)?;
            if !kind.is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("lock")
                || !older_than(&entry.metadata().map_err(storage_error)?, age)
            {
                continue;
            }
            let file = match OpenOptions::new().read(true).write(true).open(&path) {
                Ok(file) => file,
                Err(error) => {
                    report
                        .warnings
                        .push(format!("cannot inspect {}: {error}", path.display()));
                    continue;
                }
            };
            if file.try_lock_exclusive().is_err() {
                continue;
            }
            let unlocked = file.unlock();
            drop(file);
            if unlocked.is_ok() {
                delete_file(&path, report)?;
            }
        }
        Ok(())
    }
}

fn remove_old_docs(
    version_root: &Path,
    current: &str,
    report: &mut StorageCleanupReport,
) -> Result<()> {
    for entry in read_dir_optional(version_root)? {
        let path = entry.path();
        let kind = entry.file_type().map_err(storage_error)?;
        if kind.is_dir()
            && entry.file_name().to_string_lossy() != current
            && remove_dir(&path, report)?
        {
            report.removed_staging_dirs += 1;
        }
    }
    Ok(())
}

fn remove_matching_dirs(
    root: &Path,
    matches: impl Fn(&str) -> bool,
    report: &mut StorageCleanupReport,
) -> Result<()> {
    for entry in read_dir_optional(root)? {
        let kind = entry.file_type().map_err(storage_error)?;
        if kind.is_dir()
            && matches(&entry.file_name().to_string_lossy())
            && remove_dir(&entry.path(), report)?
        {
            report.removed_staging_dirs += 1;
        }
    }
    Ok(())
}

fn remove_old_entries(
    root: &Path,
    age: Duration,
    prefix: &str,
    report: &mut StorageCleanupReport,
) -> Result<()> {
    for entry in read_dir_optional(root)? {
        if entry.file_name().to_string_lossy().starts_with(prefix)
            && entry.file_type().map_err(storage_error)?.is_dir()
            && older_than(&entry.metadata().map_err(storage_error)?, age)
            && remove_dir(&entry.path(), report)?
        {
            report.removed_staging_dirs += 1;
        }
    }
    Ok(())
}

pub(super) fn remove_old_files(
    root: &Path,
    cutoff_ms: i64,
    report: &mut StorageCleanupReport,
    managed: impl Fn(&str) -> bool,
) -> Result<()> {
    for entry in read_dir_optional(root)? {
        if !entry.file_type().map_err(storage_error)?.is_file()
            || !managed(&entry.file_name().to_string_lossy())
        {
            continue;
        }
        let modified = entry
            .metadata()
            .map_err(storage_error)?
            .modified()
            .map_err(storage_error)?
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        if modified < cutoff_ms {
            delete_file(&entry.path(), report)?;
        }
    }
    Ok(())
}

pub(super) fn valid_cache_file(name: &str, extension: &str) -> bool {
    let Some(stem) = name.strip_suffix(&format!(".{extension}")) else {
        return false;
    };
    stem.len() == 64 && stem.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn remove_dir(path: &Path, report: &mut StorageCleanupReport) -> Result<bool> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(true),
        Err(error) => {
            report
                .warnings
                .push(format!("cannot remove {}: {error}", path.display()));
            Ok(false)
        }
    }
}

pub(super) fn delete_file(path: &Path, report: &mut StorageCleanupReport) -> Result<bool> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Ok(false);
    }
    match fs::remove_file(path) {
        Ok(()) => {
            report.removed_cache_files += 1;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            report
                .warnings
                .push(format!("cannot remove {}: {error}", path.display()));
            Ok(false)
        }
    }
}

pub(super) fn read_dir_optional(path: &Path) -> Result<Vec<fs::DirEntry>> {
    match fs::read_dir(path) {
        Ok(entries) => entries.map(|entry| entry.map_err(storage_error)).collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(storage_error(error)),
    }
}

pub(super) fn directory_size(path: &Path) -> Result<u64> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(storage_error(error)),
    };
    if metadata.file_type().is_symlink() {
        return Ok(0);
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total: u64 = 0;
    for entry in read_dir_optional(path)? {
        total = total.saturating_add(directory_size(&entry.path())?);
    }
    Ok(total)
}

pub(super) fn database_size(home: &Path) -> Result<u64> {
    let mut total: u64 = 0;
    for suffix in ["", "-wal", "-shm"] {
        total = total.saturating_add(file_size(
            &home.join(format!("{}{suffix}", storage::DATABASE)),
        )?);
    }
    Ok(total)
}

pub(super) fn file_size(path: &Path) -> Result<u64> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            Ok(metadata.len())
        }
        Ok(_) => Ok(0),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(storage_error(error)),
    }
}

pub(super) fn count(conn: &Connection, table: &str) -> Result<u64> {
    let value: i64 = conn
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(storage_error)?;
    u64::try_from(value).map_err(storage_error)
}

fn older_than(metadata: &fs::Metadata, age: Duration) -> bool {
    metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|elapsed| elapsed > age)
}

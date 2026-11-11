//! Stable CLI entry published in the selected data directory.
use crate::record::storage;
use ossia_types::{AppError, ContentHash, ErrorCode};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Path agents can use without depending on PATH or the application location.
pub fn cli_path(home: &Path) -> PathBuf {
    home.join(if cfg!(windows) { "ossia.exe" } else { "ossia" })
}

/// Publish or refresh the CLI entry without replacing a foreign regular file.
pub fn publish(home: &Path, executable: &Path) -> Result<(), AppError> {
    let executable = executable.canonicalize().map_err(storage)?;
    fs::create_dir_all(home).map_err(storage)?;
    #[cfg(unix)]
    {
        let entry = cli_path(home);
        if fs::read_link(&entry).ok().as_ref() == Some(&executable) {
            return Ok(());
        }
        match fs::symlink_metadata(&entry) {
            Ok(meta) if !meta.file_type().is_symlink() => return Err(foreign(&entry)),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(storage(error)),
        }
        let staged = tempfile::tempdir_in(home).map_err(storage)?;
        let link = staged.path().join("ossia");
        std::os::unix::fs::symlink(executable, &link).map_err(storage)?;
        fs::rename(link, entry).map_err(storage)?;
    }
    #[cfg(windows)]
    publish_copy(home, &executable)?;

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Origin {
    source: PathBuf,
    sha256: ContentHash,
}

/// Resolve bundled resources relative to the original app when running a CLI copy.
pub fn resource_executable(executable: &Path) -> Result<PathBuf, AppError> {
    let parent = executable
        .parent()
        .ok_or_else(|| storage("executable has no parent"))?;
    if executable.file_name() != cli_path(parent).file_name() {
        return Ok(executable.into());
    }
    let origin = match fs::read(parent.join("cli-origin.json")) {
        Ok(bytes) => serde_json::from_slice::<Origin>(&bytes).map_err(storage)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(executable.into()),
        Err(error) => return Err(storage(error)),
    };
    if ContentHash::of(&fs::read(executable).map_err(storage)?) != origin.sha256 {
        return Err(foreign(executable));
    }
    if !origin.source.is_absolute() {
        return Err(storage("CLI origin must be an absolute executable path"));
    }
    Ok(origin.source)
}

#[cfg(any(windows, test))]
fn publish_copy(home: &Path, executable: &Path) -> Result<(), AppError> {
    let entry = cli_path(home);
    if entry == executable {
        return Ok(());
    }
    let source_hash = ContentHash::of(&fs::read(executable).map_err(storage)?);
    match fs::symlink_metadata(&entry) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
            let existing = ContentHash::of(&fs::read(&entry).map_err(storage)?);
            // An identical copy may be running on Windows; avoid replacing it.
            if existing == source_hash {
                return write_origin(home, executable, source_hash);
            }
            let origin: Origin = serde_json::from_slice(
                &fs::read(home.join("cli-origin.json")).map_err(|_| foreign(&entry))?,
            )
            .map_err(storage)?;
            if existing != origin.sha256 {
                return Err(foreign(&entry));
            }
        }
        Ok(_) => return Err(foreign(&entry)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(storage(error)),
    }
    let staged = tempfile::NamedTempFile::new_in(home).map_err(storage)?;
    fs::copy(executable, staged.path()).map_err(storage)?;
    if ContentHash::of(&fs::read(staged.path()).map_err(storage)?) != source_hash {
        return Err(storage("Ossia executable changed while publishing its CLI"));
    }
    staged
        .persist(&entry)
        .map_err(|error| storage(error.error))?;
    write_origin(home, executable, source_hash)
}

#[cfg(any(windows, test))]
fn write_origin(home: &Path, executable: &Path, sha256: ContentHash) -> Result<(), AppError> {
    crate::record::atomic_write(
        &home.join("cli-origin.json"),
        &serde_json::to_vec(&Origin {
            source: executable.into(),
            sha256,
        })
        .map_err(storage)?,
    )
}

fn foreign(entry: &Path) -> AppError {
    AppError::new(
        ErrorCode::Conflict,
        format!("CLI entry is not managed by Ossia: {}", entry.display()),
    )
}

pub(crate) fn guide_command(home: &Path, env: &BTreeMap<String, String>) -> String {
    let mut args = vec![
        cli_path(home).to_string_lossy().into_owned(),
        "--home".into(),
        home.to_string_lossy().into_owned(),
    ];
    for (key, flag) in [
        ("OSSIA_ENGINE_ROOT", "--engine-root"),
        ("OSSIA_STATIC_DIR", "--static-dir"),
        ("OSSIA_DOCS_ROOT", "--docs-root"),
    ] {
        if let Some(value) = env.get(key) {
            args.extend([flag.into(), value.clone()]);
        }
    }
    args.push("guide".into());
    let command = args
        .iter()
        .map(|arg| {
            if cfg!(windows) {
                format!("'{}'", arg.replace('\'', "''"))
            } else {
                format!("'{}'", arg.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if cfg!(windows) {
        format!("& {command}")
    } else {
        command
    }
}

#[cfg(test)]
#[path = "../tests/unit/launcher.rs"]
mod tests;

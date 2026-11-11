use crate::{conflict, storage_error, Result};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use ossia_types::{AppError, ContentHash, ErrorCode, SCHEMA_VERSION};
use rusqlite::Connection;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub(crate) const APPLICATION_ID: u32 = 0x4f535349;
pub(crate) const DATABASE: &str = "library.sqlite";

pub(crate) struct Store {
    pub home: PathBuf,
}

impl Store {
    pub fn path(&self, relative: &str) -> Result<PathBuf> {
        if relative.is_empty()
            || relative.contains(['\\', ':'])
            || Path::new(relative)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(storage_error("invalid library-relative path"));
        }
        let mut path = self.home.clone();
        for part in Path::new(relative).components() {
            path.push(part);
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(storage_error("library entries must not be symbolic links"));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(storage_error(e)),
            }
        }
        Ok(path)
    }

    pub fn put(&self, bytes: &[u8]) -> Result<ContentHash> {
        let hash = ContentHash::of(bytes);
        let path = self.path(&format!("contents/{hash}.gz"))?;
        if path.exists() {
            self.get(&hash)?;
            return Ok(hash);
        }
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(bytes).map_err(storage_error)?;
        atomic_write(&path, &gzip.finish().map_err(storage_error)?)?;
        Ok(hash)
    }

    pub fn get(&self, hash: &ContentHash) -> Result<Vec<u8>> {
        let file = File::open(self.path(&format!("contents/{hash}.gz"))?).map_err(storage_error)?;
        let mut bytes = Vec::new();
        GzDecoder::new(file)
            .read_to_end(&mut bytes)
            .map_err(storage_error)?;
        if ContentHash::of(&bytes) != *hash {
            return Err(storage_error("content hash mismatch"));
        }
        Ok(bytes)
    }

    pub fn read_source(&self, relative: &str) -> Result<Vec<u8>> {
        let path = self.path(relative)?;
        for _ in 0..3 {
            let first = read_regular(&path)?;
            let second = read_regular(&path)?;
            if first == second {
                return Ok(second);
            }
        }
        Err(conflict("source is changing; retry with a stable file"))
    }
}

fn read_regular(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::new(ErrorCode::NotFound, "public source is temporarily missing")
        } else {
            storage_error(e)
        }
    })?;
    if !meta.file_type().is_file() {
        return Err(storage_error("public source must be a regular file"));
    }
    fs::read(path).map_err(storage_error)
}

pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(storage_error)?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| storage_error("missing parent"))?;
    ensure_directory(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(storage_error)?;
    temp.write_all(bytes).map_err(storage_error)?;
    temp.as_file().sync_all().map_err(storage_error)?;
    temp.persist(path).map_err(|e| storage_error(e.error))?;
    sync_directory(parent)
}

pub(crate) fn ensure_directory(path: &Path) -> Result<()> {
    let absolute = std::path::absolute(path).map_err(storage_error)?;
    let path = absolute.as_path();
    let mut missing = Vec::new();
    let mut ancestor = path;
    while !ancestor.try_exists().map_err(storage_error)? {
        missing.push(ancestor);
        ancestor = ancestor
            .parent()
            .ok_or_else(|| storage_error("missing directory ancestor"))?;
    }
    fs::create_dir_all(path).map_err(storage_error)?;
    for directory in missing.into_iter().rev() {
        if let Some(parent) = directory.parent() {
            sync_directory(parent)?;
        }
    }
    Ok(())
}

/// Inspect the header without opening SQLite, which could create WAL sidecars.
pub(crate) fn preflight(home: &Path) -> Result<Option<u32>> {
    let database = home.join(DATABASE);
    let mut header = [0; 100];
    let exists = match fs::symlink_metadata(&database) {
        Ok(meta) if meta.file_type().is_file() => true,
        Ok(_) => return Err(storage_error("library database must be a regular file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(storage_error(e)),
    };
    let recognized = exists
        && File::open(&database)
            .map_err(storage_error)?
            .read_exact(&mut header)
            .is_ok()
        && &header[..16] == b"SQLite format 3\0"
        && u32::from_be_bytes(header[68..72].try_into().unwrap()) == APPLICATION_ID;
    if !recognized {
        return Err(AppError::new(
            ErrorCode::RuntimeIncompatible,
            "unrecognized library database or schema",
        ));
    }
    let version = u32::from_be_bytes(header[60..64].try_into().unwrap());
    if version == 0 || version > SCHEMA_VERSION {
        return Err(AppError::new(
            ErrorCode::RuntimeIncompatible,
            "unsupported library schema",
        ));
    }
    Ok(Some(version))
}

pub(crate) fn initialize(store: &Store) -> Result<()> {
    let temp = tempfile::NamedTempFile::new_in(&store.home).map_err(storage_error)?;
    let conn = Connection::open(temp.path()).map_err(storage_error)?;
    conn.execute_batch(include_str!("schema.sql"))
        .map_err(storage_error)?;
    conn.pragma_update(None, "application_id", APPLICATION_ID)
        .map_err(storage_error)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(storage_error)?;
    conn.execute(
        "INSERT INTO library_meta VALUES (?1)",
        [ossia_types::DocumentId::new().to_string()],
    )
    .map_err(storage_error)?;
    conn.close().map_err(|(_, e)| storage_error(e))?;
    temp.as_file().sync_all().map_err(storage_error)?;
    temp.persist_noclobber(store.path(DATABASE)?)
        .map_err(|e| storage_error(e.error))?;
    sync_directory(&store.home)
}

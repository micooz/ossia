use crate::process::Tool;
use crate::{failure, io_error};
use ossia_types::{AppError, ContentHash};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Lock {
    pub(crate) schema_version: u32,
    pub(crate) version: String,
    pub(crate) targets: BTreeMap<String, Target>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Target {
    pub(crate) engine_build_id: String,
    pub(crate) archive: Option<Archive>,
    pub(crate) tools: BTreeMap<String, Tool>,
    pub(crate) required_directories: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Archive {
    pub(crate) sha256: ContentHash,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Inventory {
    pub(crate) schema_version: u32,
    pub(crate) target: String,
    pub(crate) engine_build_id: String,
    pub(crate) archive_sha256: ContentHash,
    pub(crate) files: Vec<ResourceFile>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResourceFile {
    pub(crate) path: String,
    pub(crate) sha256: ContentHash,
    pub(crate) bytes: u64,
    pub(crate) executable: bool,
}

pub(crate) fn load_lock() -> Result<Lock, AppError> {
    serde_json::from_str(include_str!("../../../vendor/ENGINE.lock"))
        .map_err(|_| failure("engine-lock", "invalid embedded engine lock"))
}

pub(crate) fn resource_files(
    root: &Path,
    directory: &Path,
    out: &mut BTreeSet<String>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "resources.json"
            || relative.split('/').any(|p| p == "__pycache__")
            || relative.ends_with(".pyc")
        {
            continue;
        }
        let ty = entry.file_type().map_err(io_error)?;
        if ty.is_dir() {
            resource_files(root, &path, out)?;
        } else if ty.is_file() {
            out.insert(relative);
        } else {
            return Err(failure(
                "integrity",
                "unexpected engine resource symlink or special file",
            ));
        }
    }
    Ok(())
}

use crate::{failure, io_error, RunContext};
use fs2::FileExt;
use ossia_types::{AppError, ArtifactRef, ContentHash, RenderFormat, RenderManifest};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub(crate) fn confined(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    if relative.is_empty()
        || relative.contains(['\\', ':'])
        || Path::new(relative)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(failure(
            "path",
            "artifact path must be a confined relative path",
        ));
    }
    let base = root.canonicalize().map_err(io_error)?;
    let path = base.join(relative);
    let mut ancestor = path.as_path();
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() && !ancestor.exists() {
                    return Err(failure("path", "dangling resource symlink"));
                }
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| failure("path", "invalid resource path"))?;
            }
            Err(error) => return Err(io_error(error)),
        }
    }
    if !ancestor
        .canonicalize()
        .map_err(io_error)?
        .starts_with(&base)
    {
        return Err(failure("path", "resource path escapes root"));
    }
    Ok(path)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| failure("path", "missing artifact parent"))?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
    temp.write_all(bytes).map_err(io_error)?;
    temp.as_file().sync_all().map_err(io_error)?;
    temp.persist(path).map_err(|e| io_error(e.error))?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(io_error)?;
    Ok(())
}

pub(crate) struct Cache {
    pub root: PathBuf,
}
impl Cache {
    pub fn new(root: &Path) -> Result<Self, AppError> {
        fs::create_dir_all(root).map_err(io_error)?;
        Ok(Self {
            root: root.canonicalize().map_err(io_error)?,
        })
    }
    pub fn key(
        source: &ContentHash,
        build: &str,
        options: &ContentHash,
        format: RenderFormat,
    ) -> String {
        ContentHash::of(
            &serde_json::to_vec(&(source, build, options, format)).expect("cache key serializes"),
        )
        .to_string()
    }
    pub fn lock(&self, key: &str, context: &RunContext) -> Result<File, AppError> {
        let path = confined(&self.root, &format!("locks/{key}.lock"))?;
        fs::create_dir_all(path.parent().unwrap()).map_err(io_error)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(io_error)?;
        loop {
            context.check()?;
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(file),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(e) => return Err(io_error(e)),
            }
        }
    }
    pub fn read(
        &self,
        key: &str,
        expected: &RenderManifest,
        format: RenderFormat,
    ) -> Result<Option<RenderManifest>, AppError> {
        let path = confined(&self.root, &format!("renders/{key}.json"))?;
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_error(e)),
        };
        let manifest: RenderManifest = match serde_json::from_slice(&bytes) {
            Ok(m) => m,
            Err(_) => return Ok(None),
        };
        if manifest.source_hash != expected.source_hash
            || manifest.engine_build_id != expected.engine_build_id
            || manifest.options_hash != expected.options_hash
            || !manifest.format_errors.is_empty()
        {
            return Ok(None);
        }
        let refs: Vec<_> = match format {
            RenderFormat::Svg
                if !manifest.pages.is_empty()
                    && manifest.pdf.is_none()
                    && manifest.midi.is_none()
                    && manifest
                        .pages
                        .iter()
                        .enumerate()
                        .all(|(i, p)| p.page as usize == i + 1) =>
            {
                manifest.pages.iter().map(|p| &p.artifact).collect()
            }
            RenderFormat::Pdf if manifest.pages.is_empty() && manifest.midi.is_none() => {
                manifest.pdf.iter().collect()
            }
            RenderFormat::Midi if manifest.pages.is_empty() && manifest.pdf.is_none() => {
                manifest.midi.iter().collect()
            }
            _ => return Ok(None),
        };
        if refs.is_empty() {
            return Ok(None);
        }
        for asset in refs {
            // Invalid paths are errors, never trusted even when their referenced files are missing.
            let path = confined(&self.root, &asset.path)?;
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(_) => return Ok(None),
            };
            if bytes.len() as u64 != asset.bytes || ContentHash::of(&bytes) != asset.sha256 {
                return Ok(None);
            }
        }
        Ok(Some(manifest))
    }
    pub fn asset(&self, bytes: &[u8], extension: &str) -> Result<ArtifactRef, AppError> {
        let hash = ContentHash::of(bytes);
        let relative = format!("assets/{hash}.{extension}");
        let path = confined(&self.root, &relative)?;
        if fs::read(&path).ok().as_deref() != Some(bytes) {
            atomic_write(&path, bytes)?;
        }
        Ok(ArtifactRef {
            path: relative,
            sha256: hash,
            bytes: bytes.len() as u64,
        })
    }
    pub fn publish(&self, key: &str, manifest: &RenderManifest) -> Result<(), AppError> {
        let path = confined(&self.root, &format!("renders/{key}.json"))?;
        atomic_write(
            &path,
            &serde_json::to_vec(manifest)
                .map_err(|_| failure("manifest", "cannot serialize render manifest"))?,
        )
    }
}

use crate::{
    storage::{self, Store},
    *,
};
use rusqlite::OpenFlags;
use serde::Deserialize;
use std::{collections::BTreeSet, path::PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocsLock {
    version: String,
    format_version: u32,
    sha256: ContentHash,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pack {
    version: String,
    format_version: u32,
    archive_sha256: ContentHash,
    content_hash: ContentHash,
    files: Vec<PackFile>,
}
#[derive(Deserialize)]
struct PackFile {
    path: String,
    sha256: ContentHash,
    bytes: u64,
}

fn lock() -> Result<DocsLock> {
    serde_json::from_str(include_str!("../../../vendor/DOCS.lock")).map_err(storage_error)
}
fn path(root: &Path, relative: &str) -> Result<PathBuf> {
    Store {
        home: root.to_owned(),
    }
    .path(relative)
}

fn install_file(source: &Path, destination: &Path) -> Result<()> {
    match fs::hard_link(source, destination) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::CrossesDevices
                    | std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::Unsupported
            ) =>
        {
            fs::copy(source, destination)
                .map(|_| ())
                .map_err(storage_error)
        }
        Err(error) => Err(storage_error(error)),
    }
}

fn verify(root: &Path) -> Result<Pack> {
    if fs::symlink_metadata(root)
        .map_err(storage_error)?
        .file_type()
        .is_symlink()
    {
        return Err(storage_error(
            "documentation root cannot be a symbolic link",
        ));
    }
    let pack: Pack =
        serde_json::from_slice(&fs::read(path(root, "manifest.json")?).map_err(storage_error)?)
            .map_err(storage_error)?;
    let expected = lock()?;
    if pack.version != expected.version
        || pack.format_version != expected.format_version
        || pack.archive_sha256 != expected.sha256
    {
        return Err(AppError::new(
            ErrorCode::RuntimeIncompatible,
            "documentation does not match the bundled LilyPond version",
        ));
    }
    let mut identity = String::new();
    let mut names = BTreeSet::new();
    for file in &pack.files {
        if !names.insert(&file.path) {
            return Err(storage_error("duplicate documentation resource"));
        }
        let bytes = fs::read(path(root, &file.path)?).map_err(storage_error)?;
        if bytes.len() as u64 != file.bytes || ContentHash::of(&bytes) != file.sha256 {
            return Err(storage_error(format!(
                "damaged documentation resource: {}",
                file.path
            )));
        }
        identity.push_str(&format!("{}\0{}\0{}\n", file.path, file.sha256, file.bytes));
    }
    if !names.contains(&"index.sqlite".to_string())
        || ContentHash::of(identity.as_bytes()) != pack.content_hash
    {
        return Err(storage_error("documentation manifest identity mismatch"));
    }
    Ok(pack)
}

/// A verified, immutable offline reference pack independent of the score database.
pub struct Documentation {
    root: PathBuf,
    version: String,
    index: PackFile,
}
impl Documentation {
    /// Install by content identity, publishing only complete verified directories.
    pub fn install(home: &Path, resources: &Path) -> Result<Self> {
        let source_manifest: Pack = serde_json::from_slice(
            &fs::read(path(resources, "manifest.json")?).map_err(storage_error)?,
        )
        .map_err(storage_error)?;
        let expected = lock()?;
        if source_manifest.version != expected.version
            || source_manifest.format_version != expected.format_version
            || source_manifest.archive_sha256 != expected.sha256
        {
            return Err(AppError::new(
                ErrorCode::RuntimeIncompatible,
                "documentation does not match the bundled LilyPond version",
            ));
        }
        let relative = format!(
            "docs/lilypond/{}/{}",
            source_manifest.version, source_manifest.content_hash
        );
        let destination = path(home, &relative)?;
        let parent = destination.parent().unwrap();
        storage::ensure_directory(parent)?;
        if !destination.exists() {
            let pack = verify(resources)?;
            let temp = tempfile::tempdir_in(parent).map_err(storage_error)?;
            for file in &pack.files {
                let dest = path(temp.path(), &file.path)?;
                storage::ensure_directory(dest.parent().unwrap())?;
                install_file(&path(resources, &file.path)?, &dest)?;
            }
            fs::copy(
                path(resources, "manifest.json")?,
                temp.path().join("manifest.json"),
            )
            .map_err(storage_error)?;
            verify(temp.path())?;
            fs::rename(temp.path(), &destination).map_err(storage_error)?;
            storage::sync_directory(parent)?;
        }
        let installed = verify(&destination)?;
        storage::atomic_write(
            &parent.join("current.json"),
            &serde_json::to_vec(&serde_json::json!({"contentHash":installed.content_hash}))
                .map_err(storage_error)?,
        )?;
        let index = installed
            .files
            .into_iter()
            .find(|f| f.path == "index.sqlite")
            .ok_or_else(|| storage_error("documentation index missing"))?;
        Ok(Self {
            root: destination,
            version: installed.version,
            index,
        })
    }
    /// Validate package resources without installing or downloading them.
    pub fn inspect(resources: &Path) -> DocumentationStatus {
        match verify(resources) {
            Ok(pack) => DocumentationStatus {
                available: true,
                version: pack.version,
                root: Some(resources.to_string_lossy().into_owned()),
                error: None,
            },
            Err(error) => Self::unavailable(error),
        }
    }
    pub fn unavailable(error: AppError) -> DocumentationStatus {
        DocumentationStatus {
            available: false,
            version: lock().map(|l| l.version).unwrap_or_default(),
            root: None,
            error: Some(error),
        }
    }
    pub fn status(&self) -> DocumentationStatus {
        DocumentationStatus {
            available: true,
            version: self.version.clone(),
            root: Some(self.root.to_string_lossy().into_owned()),
            error: None,
        }
    }
    fn database(&self) -> Result<Connection> {
        let index = path(&self.root, &self.index.path)?;
        let bytes = fs::read(&index).map_err(storage_error)?;
        if ContentHash::of(&bytes) != self.index.sha256 {
            return Err(storage_error(
                "documentation index changed since installation",
            ));
        }
        Connection::open_with_flags(
            index,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(storage_error)
    }
    /// Search official chapters or read one chapter; offsets count Unicode scalars.
    pub fn query(&self, request: DocsRequest) -> Result<DocsOutcome> {
        let conn = self.database()?;
        let offset = match &request {
            DocsRequest::Search { .. } => 0,
            DocsRequest::Read { offset, .. } => *offset,
        };
        let mut response = DocsOutcome {
            version: self.version.clone(),
            root: self.root.to_string_lossy().into_owned(),
            hits: vec![],
            text: None,
            offset,
            next_offset: None,
            keywords: Vec::new(),
            match_mode: DocsMatchMode::All,
            query_suggestions: Vec::new(),
        };
        match request {
            DocsRequest::Search {
                query,
                limit,
                match_mode,
            } => {
                let limit = limit.unwrap_or(5);
                if limit == 0 || limit > 20 {
                    return Err(AppError::invalid("docs search limit must be 1..20"));
                }
                if query.len() > 512 {
                    return Err(AppError::invalid("docs query exceeds 512 bytes"));
                }
                let mut query = query;
                for (from, to) in [
                    ("相对音高", "relative"),
                    ("绝对音高", "absolute"),
                    ("八度", "octave"),
                    ("时值", "duration"),
                    ("音阶", "scale"),
                    ("谱号", "clef"),
                    ("连音", "tie"),
                    ("排版", "layout"),
                ] {
                    query = query.replace(from, to);
                }
                let words: Vec<_> = query
                    .split(|c: char| !c.is_alphanumeric() && c != '-')
                    .filter(|s| !s.is_empty())
                    .collect();
                if words.is_empty() {
                    return Err(AppError::invalid("docs query needs a keyword or command"));
                }
                let mut unique = words.clone();
                unique.sort();
                unique.dedup();
                response.keywords = words.iter().map(|word| (*word).to_owned()).collect();
                response.match_mode = match_mode;
                let operator = if match_mode == DocsMatchMode::Any {
                    " OR "
                } else {
                    " AND "
                };
                let search = words
                    .iter()
                    .map(|w| format!("\"{w}\""))
                    .collect::<Vec<_>>()
                    .join(operator);
                let mut statement=conn.prepare("SELECT c.id,c.manual,c.title,snippet(search,3,'','',' … ',28),c.url,c.path FROM search JOIN chapters c ON c.id=search.id WHERE search MATCH ?1 ORDER BY bm25(search,0,10,20,1),c.id LIMIT ?2").map_err(storage_error)?;
                let rows = statement
                    .query_map(params![search, limit as i64], |r| {
                        Ok(DocsHit {
                            chapter_id: r.get(0)?,
                            manual: r.get(1)?,
                            title: r.get(2)?,
                            excerpt: r.get(3)?,
                            url: r.get(4)?,
                            path: r.get(5)?,
                        })
                    })
                    .map_err(storage_error)?;
                for row in rows {
                    let mut hit = row.map_err(storage_error)?;
                    hit.path = path(&self.root, &hit.path)?.to_string_lossy().into_owned();
                    response.hits.push(hit);
                }
                if response.hits.is_empty() {
                    response.query_suggestions =
                        unique.into_iter().take(3).map(str::to_owned).collect();
                }
            }
            DocsRequest::Read {
                chapter_id,
                offset,
                limit,
            } => {
                let id = chapter_id;
                let limit = limit.unwrap_or(6000);
                if limit == 0 || limit > 16000 {
                    return Err(AppError::invalid(
                        "docs read limit must be 1..16000 characters",
                    ));
                }
                let (title, body, url, local): (String, String, String, String) = conn
                    .query_row(
                        "SELECT title,body,url,path FROM chapters WHERE id=?1",
                        [&id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                    )
                    .optional()
                    .map_err(storage_error)?
                    .ok_or_else(not_found)?;
                let total = body.chars().count();
                if offset > total {
                    return Err(AppError::invalid("docs offset exceeds chapter length"));
                }
                let end = offset.saturating_add(limit).min(total);
                response.text = Some(body.chars().skip(offset).take(limit).collect());
                response.next_offset = (end < total).then_some(end);
                response.hits.push(DocsHit {
                    chapter_id: id,
                    manual: String::new(),
                    title,
                    excerpt: String::new(),
                    url,
                    path: path(&self.root, &local)?.to_string_lossy().into_owned(),
                });
            }
        }
        Ok(response)
    }
}

#[cfg(test)]
#[path = "../tests/unit/documentation.rs"]
mod tests;

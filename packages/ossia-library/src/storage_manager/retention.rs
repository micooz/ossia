use super::files::{
    delete_file, directory_size, file_size, read_dir_optional, remove_old_files, valid_cache_file,
};
use super::*;
use crate::db;
use rusqlite::Connection;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

struct JobRow {
    job_id: String,
    document_id: Option<String>,
    source_hash: Option<String>,
    kind: String,
    state: String,
    created_at: i64,
}

struct RenderRow {
    rowid: i64,
    document_id: String,
    body: RenderManifest,
    created_at: i64,
    last_used_at: i64,
}

impl Library {
    pub(super) fn prune_jobs(
        &self,
        policy: StoragePolicy,
        now: i64,
        report: &mut StorageCleanupReport,
    ) -> Result<()> {
        let current = self.current_sources()?;
        let conn = self.db()?;
        let mut statement = conn
            .prepare("SELECT job_id,document_id,source_hash,kind,state,created_at FROM jobs ORDER BY created_at DESC,rowid DESC")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(JobRow {
                    job_id: row.get(0)?,
                    document_id: row.get(1)?,
                    source_hash: row.get(2)?,
                    kind: row.get(3)?,
                    state: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(storage_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        drop(statement);

        let mut protected = receipt_and_active_request_jobs(&conn)?;
        let mut seen: HashMap<Option<String>, usize> = HashMap::new();
        let mut terminal_seen: HashMap<Option<String>, usize> = HashMap::new();
        let mut current_seen: HashSet<String> = HashSet::new();
        let mut over_limit = HashSet::new();
        for row in &rows {
            let index = seen.entry(row.document_id.clone()).or_default();
            if *index < policy.latest_per_document {
                protected.insert(row.job_id.clone());
            }
            *index += 1;
            if matches!(
                row.state.as_str(),
                "done" | "error" | "cancelled" | "interrupted"
            ) {
                let count = terminal_seen.entry(row.document_id.clone()).or_default();
                if *count < policy.jobs_per_document {
                    protected.insert(row.job_id.clone());
                } else {
                    over_limit.insert(row.job_id.clone());
                }
                *count += 1;
            }
            if matches!(row.kind.as_str(), "check" | "import")
                && row.document_id.as_ref().is_some_and(|document| {
                    row.source_hash.as_ref().is_some_and(|hash| {
                        current
                            .get(document)
                            .is_some_and(|current| current.as_str() == hash)
                    })
                })
                && row
                    .document_id
                    .as_ref()
                    .is_some_and(|document| current_seen.insert(document.clone()))
            {
                protected.insert(row.job_id.clone());
            }
        }
        let cutoff = now.saturating_sub(policy.job_retention_ms);
        let candidates: Vec<String> = rows
            .iter()
            .filter(|row| {
                matches!(
                    row.state.as_str(),
                    "done" | "error" | "cancelled" | "interrupted"
                ) && !protected.contains(&row.job_id)
                    && (row.created_at < cutoff || over_limit.contains(&row.job_id))
            })
            .map(|row| row.job_id.clone())
            .collect();
        drop(conn);
        if candidates.is_empty() {
            return Ok(());
        }
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        for job in &candidates {
            tx.execute("DELETE FROM check_receipts WHERE job_id=?1", [job])
                .map_err(storage_error)?;
            tx.execute("DELETE FROM check_requests WHERE job_id=?1", [job])
                .map_err(storage_error)?;
            tx.execute("DELETE FROM jobs WHERE job_id=?1", [job])
                .map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)?;
        report.removed_jobs = candidates.len() as u64;
        Ok(())
    }

    pub(super) fn prune_renders(
        &self,
        policy: StoragePolicy,
        now: i64,
        _view_manifests: &[RenderManifest],
        report: &mut StorageCleanupReport,
    ) -> Result<()> {
        let current = self.current_sources()?;
        let conn = self.db()?;
        let rows = render_rows(&conn)?;
        let protected_rows = protected_render_rows(&conn, &rows, &current, policy)?;
        let cutoff = now.saturating_sub(policy.render_retention_ms);
        let mut per_doc: HashMap<String, usize> = HashMap::new();
        let mut candidates = Vec::new();
        for row in &rows {
            let count = per_doc.entry(row.document_id.clone()).or_default();
            let over_count = *count >= policy.renders_per_document;
            *count += 1;
            if !protected_rows.contains(&row.rowid) && (row.last_used_at < cutoff || over_count) {
                candidates.push(row.rowid);
            }
        }
        drop(conn);
        if !candidates.is_empty() {
            let mut conn = self.db()?;
            let tx = conn.transaction().map_err(storage_error)?;
            for rowid in &candidates {
                tx.execute("DELETE FROM renders WHERE rowid=?1", [rowid])
                    .map_err(storage_error)?;
            }
            tx.commit().map_err(storage_error)?;
            report.removed_renders = candidates.len() as u64;
        }
        Ok(())
    }

    pub(super) fn enforce_cache_limit(
        &self,
        policy: StoragePolicy,
        view_manifests: &[RenderManifest],
        report: &mut StorageCleanupReport,
    ) -> Result<()> {
        let artifacts = self.store.path("artifacts")?;
        let protected = self.protected_assets(view_manifests)?;
        let sizes = asset_sizes(&artifacts)?;
        self.cleanup_orphan_cache(view_manifests, report)?;
        let mut previous = u64::MAX;
        loop {
            let current = directory_size(&artifacts)?;
            if current <= policy.cache_target() || current >= previous {
                break;
            }
            previous = current;
            let current_sources = self.current_sources()?;
            let conn = self.db()?;
            let rows = render_rows(&conn)?;
            let protected_rows = protected_render_rows(&conn, &rows, &current_sources, policy)?;
            drop(conn);
            let mut refs: HashMap<String, usize> = HashMap::new();
            for row in &rows {
                for asset in manifest_assets(&row.body) {
                    *refs.entry(asset).or_default() += 1;
                }
            }
            let mut candidates: Vec<&RenderRow> = rows
                .iter()
                .filter(|row| !protected_rows.contains(&row.rowid))
                .collect();
            candidates.sort_by_key(|row| (row.last_used_at, row.created_at, row.rowid));
            let mut size = current;
            let mut selected = Vec::new();
            for row in candidates {
                if size <= policy.cache_target() {
                    break;
                }
                let mut freed = 0;
                for asset in manifest_assets(&row.body) {
                    if protected.contains(&asset) {
                        continue;
                    }
                    if let Some(count) = refs.get_mut(&asset) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            freed += sizes.get(&asset).copied().unwrap_or(0);
                        }
                    }
                }
                size = size.saturating_sub(freed);
                selected.push(row.rowid);
            }
            if selected.is_empty() {
                break;
            }
            let mut conn = self.db()?;
            let tx = conn.transaction().map_err(storage_error)?;
            for rowid in &selected {
                tx.execute("DELETE FROM renders WHERE rowid=?1", [rowid])
                    .map_err(storage_error)?;
            }
            tx.commit().map_err(storage_error)?;
            drop(conn);
            report.removed_renders += selected.len() as u64;
            self.cleanup_orphan_cache(view_manifests, report)?;
        }
        let current = directory_size(&artifacts)?;
        if current > policy.cache_target() {
            self.evict_png_cache(policy, view_manifests, report)?;
        }
        self.cleanup_orphan_cache(view_manifests, report)?;
        let retain_after = db::now_millis().saturating_sub(policy.render_retention_ms);
        remove_old_files(&artifacts.join("renders"), retain_after, report, |name| {
            valid_cache_file(name, "json")
        })?;
        Ok(())
    }

    fn protected_assets(&self, views: &[RenderManifest]) -> Result<HashSet<String>> {
        let conn = self.db()?;
        let receipt_rows = receipt_renders(&conn)?;
        let mut protected = HashSet::new();
        let mut statement = conn
            .prepare("SELECT last_render FROM documents WHERE last_render IS NOT NULL")
            .map_err(storage_error)?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        for value in values {
            let manifest: RenderManifest = db::decode(value.map_err(storage_error)?)?;
            protected.extend(manifest_assets(&manifest));
        }
        drop(statement);
        for row in render_rows(&conn)? {
            if receipt_rows.contains(&row.rowid) {
                protected.extend(manifest_assets(&row.body));
            }
        }
        for manifest in views {
            protected.extend(manifest_assets(manifest));
        }
        Ok(protected)
    }

    pub(super) fn cleanup_orphan_cache(
        &self,
        views: &[RenderManifest],
        report: &mut StorageCleanupReport,
    ) -> Result<()> {
        let artifacts = self.store.path("artifacts")?;
        let mut referenced = self.protected_assets(views)?;
        {
            let conn = self.db()?;
            for row in render_rows(&conn)? {
                referenced.extend(manifest_assets(&row.body));
            }
        }
        for entry in read_dir_optional(&artifacts.join("png"))? {
            if entry.file_type().map_err(storage_error)?.is_file() {
                if let Ok(bytes) = fs::read(entry.path()) {
                    if let Ok(asset) = serde_json::from_slice::<ArtifactRef>(&bytes) {
                        referenced.insert(asset.path);
                    }
                }
            }
        }
        for entry in read_dir_optional(&artifacts.join("assets"))? {
            let path = entry.path();
            if !entry.file_type().map_err(storage_error)?.is_file() {
                continue;
            }
            let relative = format!("assets/{}", entry.file_name().to_string_lossy());
            let name = entry.file_name().to_string_lossy().into_owned();
            let valid = ["svg", "pdf", "midi", "mid", "png"]
                .iter()
                .any(|extension| valid_cache_file(&name, extension));
            if valid && !referenced.contains(&relative) {
                delete_file(&path, report)?;
            }
        }
        Ok(())
    }

    fn evict_png_cache(
        &self,
        policy: StoragePolicy,
        views: &[RenderManifest],
        report: &mut StorageCleanupReport,
    ) -> Result<()> {
        let root = self.store.path("artifacts/png")?;
        let mut files: Vec<(PathBuf, SystemTime)> = read_dir_optional(&root)?
            .into_iter()
            .filter_map(|entry| {
                if !entry.file_type().ok()?.is_file() {
                    return None;
                }
                if !valid_cache_file(&entry.file_name().to_string_lossy(), "json") {
                    return None;
                }
                Some((entry.path(), entry.metadata().ok()?.modified().ok()?))
            })
            .collect();
        files.sort_by_key(|(_, modified)| *modified);
        let artifacts = self.store.path("artifacts")?;
        for (path, _) in files {
            if directory_size(&artifacts)? <= policy.cache_target() {
                break;
            }
            delete_file(&path, report)?;
            self.cleanup_orphan_cache(views, report)?;
        }
        Ok(())
    }

    pub(super) fn cleanup_contents(&self, report: &mut StorageCleanupReport) -> Result<()> {
        let conn = self.db()?;
        let intents: i64 = conn
            .query_row("SELECT count(*) FROM worktree_intents", [], |row| {
                row.get(0)
            })
            .map_err(storage_error)?;
        if intents > 0 {
            report
                .warnings
                .push("content cleanup skipped while a worktree intent is unresolved".into());
            return Ok(());
        }
        let mut referenced = HashSet::new();
        for query in [
            "SELECT initial_hash FROM documents",
            "SELECT source_hash FROM versions",
            "SELECT source_hash FROM jobs WHERE source_hash IS NOT NULL",
            "SELECT source_hash FROM renders",
            "SELECT source_hash FROM check_receipts",
            "SELECT source_hash FROM evidence_runs",
        ] {
            let mut statement = conn.prepare(query).map_err(storage_error)?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(storage_error)?;
            for value in rows {
                referenced.insert(value.map_err(storage_error)?);
            }
        }
        let available: Vec<String> = conn
            .prepare("SELECT hash FROM contents")
            .map_err(storage_error)?
            .query_map([], |row| row.get(0))
            .map_err(storage_error)?
            .collect::<std::result::Result<_, _>>()
            .map_err(storage_error)?;
        drop(conn);
        let orphaned: Vec<String> = available
            .into_iter()
            .filter(|hash| !referenced.contains(hash))
            .collect();
        if !orphaned.is_empty() {
            let mut conn = self.db()?;
            let tx = conn.transaction().map_err(storage_error)?;
            for hash in &orphaned {
                tx.execute("DELETE FROM contents WHERE hash=?1", [hash])
                    .map_err(storage_error)?;
            }
            tx.commit().map_err(storage_error)?;
        }
        let root = self.store.path("contents")?;
        for entry in read_dir_optional(&root)? {
            let path = entry.path();
            let kind = entry.file_type().map_err(storage_error)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let hash = name.strip_suffix(".gz").unwrap_or("");
            if kind.is_file()
                && hash.len() == 64
                && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                && !referenced.contains(hash)
                && delete_file(&path, report)?
            {
                report.removed_content_objects += 1;
            }
        }
        Ok(())
    }

    pub(super) fn vacuum_database(&self) -> Result<()> {
        let conn = self.db()?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(storage_error)?;
        let free: i64 = conn
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .map_err(storage_error)?;
        let page: i64 = conn
            .pragma_query_value(None, "page_size", |row| row.get(0))
            .map_err(storage_error)?;
        let bytes = file_size(&self.store.path(storage::DATABASE)?)?;
        if bytes >= 128 * 1024 * 1024 && (free * page) as u64 >= 64 * 1024 * 1024 {
            conn.execute_batch("VACUUM;").map_err(storage_error)?;
        }
        Ok(())
    }

    fn current_sources(&self) -> Result<HashMap<String, String>> {
        let ids = {
            let conn = self.db()?;
            let mut statement = conn
                .prepare("SELECT document_id FROM documents")
                .map_err(storage_error)?;
            let values = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(storage_error)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(storage_error)?;
            values
        };
        let mut result = HashMap::new();
        for id in ids {
            let document = db::parse::<DocumentId>(id).map_err(storage_error)?;
            let state = self.get_document(document)?;
            result.insert(document.to_string(), state.source_hash.to_string());
        }
        Ok(result)
    }
}

fn render_rows(conn: &Connection) -> Result<Vec<RenderRow>> {
    let mut statement = conn
        .prepare("SELECT rowid,document_id,body,created_at,last_used_at FROM renders ORDER BY last_used_at DESC,rowid DESC")
        .map_err(storage_error)?;
    let values: Vec<(i64, String, String, i64, i64)> = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(storage_error)?
        .collect::<std::result::Result<_, _>>()
        .map_err(storage_error)?;
    drop(statement);
    values
        .into_iter()
        .map(|(rowid, document_id, body, created_at, last_used_at)| {
            Ok(RenderRow {
                rowid,
                document_id,
                body: db::decode(body)?,
                created_at,
                last_used_at,
            })
        })
        .collect()
}

fn protected_render_rows(
    conn: &Connection,
    rows: &[RenderRow],
    current: &HashMap<String, String>,
    policy: StoragePolicy,
) -> Result<BTreeSet<i64>> {
    let mut protected = receipt_renders(conn)?;
    let mut last_renders = HashSet::new();
    let mut statement = conn
        .prepare("SELECT document_id,last_render FROM documents WHERE last_render IS NOT NULL")
        .map_err(storage_error)?;
    let documents = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(storage_error)?;
    for document in documents {
        let (document, body) = document.map_err(storage_error)?;
        let manifest: RenderManifest = db::decode(body)?;
        last_renders.insert((
            document,
            manifest.source_hash.to_string(),
            manifest.engine_build_id,
            manifest.options_hash.to_string(),
        ));
    }
    drop(statement);
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut current_seen: HashSet<&str> = HashSet::new();
    for row in rows {
        if last_renders.contains(&(
            row.document_id.clone(),
            row.body.source_hash.to_string(),
            row.body.engine_build_id.clone(),
            row.body.options_hash.to_string(),
        )) {
            protected.insert(row.rowid);
        }
        let rank = seen.entry(&row.document_id).or_default();
        if *rank < policy.latest_per_document {
            protected.insert(row.rowid);
        }
        *rank += 1;
        if current
            .get(&row.document_id)
            .is_some_and(|hash| hash == row.body.source_hash.as_str())
            && current_seen.insert(&row.document_id)
        {
            protected.insert(row.rowid);
        }
    }
    Ok(protected)
}

fn receipt_renders(conn: &Connection) -> Result<BTreeSet<i64>> {
    let mut statement = conn
        .prepare(
            "SELECT r.rowid FROM check_receipts c
             JOIN edits e ON e.edit_id=c.edit_id
             JOIN renders r ON r.document_id=e.document_id
               AND r.source_hash=c.source_hash
               AND r.engine_build_id=c.engine_build_id
               AND r.options_hash=c.options_hash",
        )
        .map_err(storage_error)?;
    let values = statement
        .query_map([], |row| row.get(0))
        .map_err(storage_error)?
        .collect::<std::result::Result<_, _>>()
        .map_err(storage_error)?;
    drop(statement);
    Ok(values)
}

fn receipt_and_active_request_jobs(conn: &Connection) -> Result<HashSet<String>> {
    let mut result = HashSet::new();
    for query in [
        "SELECT job_id FROM check_receipts",
        "SELECT r.job_id FROM check_requests r
         JOIN edits e ON e.edit_id=r.edit_id
         WHERE e.status IN ('active','interrupted')",
    ] {
        let mut statement = conn.prepare(query).map_err(storage_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        for row in rows {
            result.insert(row.map_err(storage_error)?);
        }
    }
    Ok(result)
}

fn manifest_assets(manifest: &RenderManifest) -> Vec<String> {
    manifest
        .pages
        .iter()
        .map(|page| page.artifact.path.clone())
        .chain(manifest.pdf.iter().map(|asset| asset.path.clone()))
        .chain(manifest.midi.iter().map(|asset| asset.path.clone()))
        .collect()
}

fn asset_sizes(root: &Path) -> Result<HashMap<String, u64>> {
    let mut result = HashMap::new();
    for entry in read_dir_optional(&root.join("assets"))? {
        if entry.file_type().map_err(storage_error)?.is_file() {
            result.insert(
                format!("assets/{}", entry.file_name().to_string_lossy()),
                entry.metadata().map_err(storage_error)?.len(),
            );
        }
    }
    Ok(result)
}

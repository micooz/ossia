use crate::{db, *};
use std::time::Duration;

mod files;
mod retention;

use files::{count, database_size, directory_size, file_size};

#[cfg(test)]
#[path = "../tests/unit/storage_manager.rs"]
mod tests;

const CORE_WARNING_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const JOB_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;
const RENDER_RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;
const JOBS_PER_DOCUMENT: usize = 200;
const RENDERS_PER_DOCUMENT: usize = 500;
const LATEST_PER_DOCUMENT: usize = 20;
const LOCK_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const SCRATCH_RETENTION: Duration = Duration::from_secs(60 * 60);
const CACHE_TARGET_PERCENT: u64 = 75;

#[derive(Clone, Copy)]
pub(crate) struct StoragePolicy {
    pub(crate) cache_limit_bytes: u64,
    pub(crate) job_retention_ms: i64,
    pub(crate) render_retention_ms: i64,
    pub(crate) jobs_per_document: usize,
    pub(crate) renders_per_document: usize,
    pub(crate) latest_per_document: usize,
    pub(crate) lock_retention: Duration,
    pub(crate) scratch_retention: Duration,
}

impl StoragePolicy {
    fn from_limit(limit: StorageCacheLimit) -> Self {
        Self {
            cache_limit_bytes: limit.bytes(),
            job_retention_ms: JOB_RETENTION_MS,
            render_retention_ms: RENDER_RETENTION_MS,
            jobs_per_document: JOBS_PER_DOCUMENT,
            renders_per_document: RENDERS_PER_DOCUMENT,
            latest_per_document: LATEST_PER_DOCUMENT,
            lock_retention: LOCK_RETENTION,
            scratch_retention: SCRATCH_RETENTION,
        }
    }

    fn cache_target(&self) -> u64 {
        self.cache_limit_bytes * CACHE_TARGET_PERCENT / 100
    }
}

impl Library {
    /// Measure the selected data directory without mutating it.
    pub fn storage_status(&self, limit: StorageCacheLimit) -> Result<StorageStatus> {
        let database = database_size(self.home())?;
        let documents = directory_size(&self.store.path("documents")?)?;
        let contents = directory_size(&self.store.path("contents")?)?;
        let artifacts = directory_size(&self.store.path("artifacts")?)?;
        let imports = directory_size(&self.store.path("imports")?)?;
        let run = directory_size(&self.store.path("run")?)?;
        let integrations = directory_size(&self.store.path("integrations")?)?;
        let documentation = directory_size(&self.store.path("docs")?)?;
        let preferences = file_size(&self.store.path("preferences.json")?)?;
        let lock = file_size(&self.store.path("library.lock")?)?;
        let core = database
            .saturating_add(documents)
            .saturating_add(contents)
            .saturating_add(preferences)
            .saturating_add(lock);
        let transient = imports.saturating_add(run);
        let conn = self.db()?;
        let jobs = count(&conn, "jobs")?;
        let renders = count(&conn, "renders")?;
        Ok(StorageStatus {
            home: self.home().to_string_lossy().into_owned(),
            total_bytes: core
                .saturating_add(artifacts)
                .saturating_add(transient)
                .saturating_add(integrations)
                .saturating_add(documentation),
            core_bytes: core,
            cache_bytes: artifacts,
            cache_limit_bytes: limit.bytes(),
            database_bytes: database,
            documents_bytes: documents,
            contents_bytes: contents,
            artifacts_bytes: artifacts,
            transient_bytes: transient,
            integrations_bytes: integrations,
            documentation_bytes: documentation,
            job_count: jobs,
            render_count: renders,
            core_warning: core > CORE_WARNING_BYTES,
            idle: true,
        })
    }

    /// Apply retention and cache limits while preserving all user-visible sources.
    pub fn storage_cleanup(
        &self,
        limit: StorageCacheLimit,
        view_manifests: &[RenderManifest],
    ) -> Result<StorageCleanupReport> {
        self.storage_cleanup_with_policy(StoragePolicy::from_limit(limit), view_manifests)
    }

    pub(crate) fn storage_cleanup_with_policy(
        &self,
        policy: StoragePolicy,
        view_manifests: &[RenderManifest],
    ) -> Result<StorageCleanupReport> {
        let before = directory_size(self.home())?;
        let now = db::now_millis();
        let mut report = StorageCleanupReport::default();
        self.cleanup_staging(policy, &mut report)?;
        self.cleanup_docs(&mut report)?;
        self.cleanup_stale_locks(policy.lock_retention, &mut report)?;
        self.prune_jobs(policy, now, &mut report)?;
        self.prune_renders(policy, now, view_manifests, &mut report)?;
        self.cleanup_orphan_cache(view_manifests, &mut report)?;
        self.enforce_cache_limit(policy, view_manifests, &mut report)?;
        self.cleanup_contents(&mut report)?;
        self.vacuum_database()?;
        let after = directory_size(self.home())?;
        report.freed_bytes = before.saturating_sub(after);
        report.over_limit =
            directory_size(&self.store.path("artifacts")?)? > policy.cache_limit_bytes;
        Ok(report)
    }
}

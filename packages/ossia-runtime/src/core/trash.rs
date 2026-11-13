use super::*;
use serde_json::json;
use std::time::{Duration, Instant};

impl Runtime {
    /// Move one score into the recoverable trash after checking its source and active work.
    pub fn delete_document(&self, args: DeleteArgs) -> Result<TrashedDocumentSummary> {
        self.available()?;
        let result = self.with_document_lifecycle(args.document_id, || {
            let state = self.library.get_document(args.document_id)?;
            let jobs = self
                .library
                .list_jobs()?
                .into_iter()
                .filter(|job| {
                    job.document_id == Some(args.document_id)
                        && matches!(job.state, JobState::Queued | JobState::Running)
                })
                .collect::<Vec<_>>();
            let blocked =
                state.edit_state != DocumentEditState::Idle || state.dirty || !jobs.is_empty();
            if blocked && !args.force {
                let mut blockers = Vec::new();
                if state.edit_state != DocumentEditState::Idle {
                    blockers.push("active-edit");
                }
                if state.dirty {
                    blockers.push("dirty-source");
                }
                if !jobs.is_empty() {
                    blockers.push("active-jobs");
                }
                let mut error = AppError::new(
                    ErrorCode::Conflict,
                    "document has unsaved work or active tasks; retry with force",
                );
                error.detail = Some(json!({
                    "blockers": blockers,
                    "editState": state.edit_state,
                    "dirty": state.dirty,
                    "jobIds": jobs.iter().map(|job| job.job_id).collect::<Vec<_>>(),
                }));
                return Err(error);
            }
            if args.force {
                for job in &jobs {
                    self.cancel(job.job_id)?;
                }
                self.wait_for_document_jobs(args.document_id, &jobs, Duration::from_secs(10))?;
            }
            self.library
                .trash(args.document_id, &args.expected_source_hash)
        })?;
        {
            let mut state = self.state();
            state.selections.remove(&args.document_id);
            state.observed.remove(&args.document_id);
            state.suppressed_previews.remove(&args.document_id);
            self.emit_locked(
                &mut state,
                "document-deleted",
                Some(args.document_id),
                Some(result.source_hash.clone()),
                json!({}),
            );
        }
        Ok(result)
    }

    /// Page through recoverable scores.
    pub fn trash(&self, args: TrashArgs) -> Result<TrashPage> {
        self.available()?;
        let limit = args.limit.unwrap_or(50);
        if !(1..=200).contains(&limit) {
            return Err(invalid("trash limit must be 1..200"));
        }
        self.library.trash_page(limit, args.offset)
    }

    /// Restore one score with its original identity, source and history.
    pub fn untrash(&self, args: DocumentIdArgs) -> Result<DocumentSummary> {
        self.available()?;
        self.with_document_lifecycle(args.document_id, || {
            let summary = self.library.untrash(args.document_id)?;
            self.emit(
                "document-restored",
                Some(args.document_id),
                Some(summary.source_hash.clone()),
                json!({}),
            );
            Ok(summary)
        })
    }

    /// Permanently remove one trashed score after all runtime work has stopped.
    pub fn purge(&self, args: DocumentIdArgs) -> Result<PurgedDocument> {
        self.available()?;
        let active = {
            let state = self.state();
            state
                .pending
                .iter()
                .any(|task| task.job.document_id == Some(args.document_id))
                || state
                    .running
                    .values()
                    .any(|(document, _)| *document == Some(args.document_id))
        };
        if active {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "cancel active score tasks before purging",
            ));
        }
        let result = self
            .with_document_lifecycle(args.document_id, || self.library.purge(args.document_id))?;
        {
            let mut state = self.state();
            state.selections.remove(&args.document_id);
            state.observed.remove(&args.document_id);
            state.suppressed_previews.remove(&args.document_id);
            self.emit_locked(
                &mut state,
                "document-purged",
                Some(args.document_id),
                None,
                json!({}),
            );
        }
        Ok(result)
    }

    fn with_document_lifecycle<T>(
        &self,
        document: DocumentId,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let _lifecycle = self.begin_document_lifecycle(document)?;
        operation()
    }

    fn begin_document_lifecycle(&self, document: DocumentId) -> Result<DocumentLifecycle<'_>> {
        {
            let mut state = self.state();
            if !state.lifecycle_documents.insert(document) {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "document lifecycle operation is already running",
                ));
            }
        }
        if let Err(error) = self.wait_for_document_mutations(document, Duration::from_secs(10)) {
            self.state().lifecycle_documents.remove(&document);
            return Err(error);
        }
        Ok(DocumentLifecycle {
            runtime: self,
            document,
        })
    }

    fn wait_for_document_mutations(&self, document: DocumentId, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if !self.state().mutating_documents.contains_key(&document) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "document change did not finish within 10 seconds",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn wait_for_document_jobs(
        &self,
        document: DocumentId,
        jobs: &[Job],
        timeout: Duration,
    ) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let durable_busy = jobs.iter().any(|job| {
                self.library
                    .get_job(job.job_id)
                    .is_ok_and(|job| matches!(job.state, JobState::Queued | JobState::Running))
            });
            let runtime_busy = {
                let state = self.state();
                state
                    .pending
                    .iter()
                    .any(|task| task.job.document_id == Some(document))
                    || state
                        .running
                        .values()
                        .any(|(active, _)| *active == Some(document))
            };
            if !durable_busy && !runtime_busy {
                return Ok(());
            }
            if Instant::now() >= deadline {
                let mut error = AppError::new(
                    ErrorCode::Conflict,
                    "score tasks did not stop within 10 seconds",
                );
                error.detail = Some(json!({
                    "jobIds": jobs.iter().map(|job| job.job_id).collect::<Vec<_>>(),
                }));
                return Err(error);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

struct DocumentLifecycle<'a> {
    runtime: &'a Runtime,
    document: DocumentId,
}

impl Drop for DocumentLifecycle<'_> {
    fn drop(&mut self) {
        self.runtime
            .state()
            .lifecycle_documents
            .remove(&self.document);
    }
}

use super::*;
use ossia_engine::CompileOptions;
use ossia_library::CheckRequest;
use serde_json::json;
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

mod work;
mod worker;

use work::{output_destination, ImportWork, OutputDestination};
pub(super) use work::{Pending, Work};

impl Runtime {
    pub(super) fn enqueue(&self, job: Job, work: Work, preview: bool) -> Result<Job> {
        let mut state = self.state();
        if let Err(mut error) = self.available() {
            let _ = self.library.transition_job(
                job.job_id,
                JobState::Queued,
                JobState::Cancelled,
                "runtime-stopping",
                None,
                None,
            );
            error.detail = Some(json!({"jobId": job.job_id}));
            return Err(error);
        }
        if let Some(document) = job.document_id {
            if state.lifecycle_documents.contains(&document) {
                self.library.transition_job(
                    job.job_id,
                    JobState::Queued,
                    JobState::Cancelled,
                    "document-lifecycle",
                    None,
                    None,
                )?;
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "document is being removed or restored",
                ));
            }
        }
        if preview {
            let mut retained = VecDeque::new();
            while let Some(old) = state.pending.pop_front() {
                if old.preview && old.job.document_id == job.document_id {
                    self.library.transition_job(
                        old.job.job_id,
                        JobState::Queued,
                        JobState::Cancelled,
                        "superseded",
                        None,
                        None,
                    )?;
                    self.emit_locked(
                        &mut state,
                        "job-changed",
                        old.job.document_id,
                        old.job.source_hash,
                        json!({"jobId":old.job.job_id,"state":"cancelled"}),
                    );
                } else {
                    retained.push_back(old);
                }
            }
            state.pending = retained;
        }
        let timeout = match &work {
            Work::Import(ImportWork { format, .. }) if format.is_omr() => {
                Duration::from_secs(30 * 60)
            }
            Work::Output(
                OutputArgs {
                    format: OutputFormat::Wav,
                    ..
                },
                _,
                _,
                _,
            ) => Duration::from_secs(10 * 60),
            _ => Duration::from_secs(120),
        };
        state.pending.push_back(Pending {
            job: job.clone(),
            work,
            preview,
            context: RunContext::new(timeout),
        });
        self.emit_locked(
            &mut state,
            "job-changed",
            job.document_id,
            job.source_hash.clone(),
            json!({"jobId":job.job_id,"state":"queued"}),
        );
        self.changed.notify_one();
        Ok(job)
    }

    pub fn check(&self, args: CheckArgs, preview: bool) -> Result<Job> {
        self.available()?;
        if args.expect.is_some() && args.reference.is_some() {
            return Err(invalid("expect and reference cannot be used together"));
        }
        let wait_ms = validate_wait_ms(args.wait_ms)?;
        let reference = args
            .reference
            .as_ref()
            .map(|reference| self.resolve_reference(reference))
            .transpose()?;
        let document_id = if let Some(edit_id) = args.edit_id {
            let edit = self.edit_capability(edit_id)?;
            if args
                .document_id
                .is_some_and(|document| document != edit.document_id)
            {
                return Err(forbidden());
            }
            edit.document_id
        } else {
            args.document_id
                .ok_or_else(|| invalid("check requires documentId or editId"))?
        };
        let _mutation = self.begin_document_mutation(document_id)?;
        self.observe(document_id)?;
        let job = self.library.queue_check(CheckRequest {
            expect: args.expect.as_ref(),
            reference: reference.as_ref(),
            document_id,
            edit_id: args.edit_id,
            expected_source_hash: &args.expected_source_hash,
            options: &CompileOptions {
                formats: if preview {
                    vec![RenderFormat::Svg]
                } else {
                    vec![RenderFormat::Svg, RenderFormat::Midi]
                },
                point_and_click: true,
            },
        })?;
        let job = self.enqueue(job, Work::Check, preview)?;
        self.wait_for_job(job.job_id, Some(wait_ms))
    }

    pub fn import(&self, args: ImportArgs, bytes: Vec<u8>) -> Result<Job> {
        self.available()?;
        if args.expect.is_some() && args.reference.is_some() {
            return Err(invalid("expect and reference cannot be used together"));
        }
        let wait_ms = validate_wait_ms(args.wait_ms)?;
        let reference = args
            .reference
            .as_ref()
            .map(|reference| self.resolve_reference(reference))
            .transpose()?;
        let format = args
            .format
            .or_else(|| input_format_from_path(&args.path))
            .ok_or_else(|| {
                invalid("import format is required when it cannot be inferred from the path")
            })?;
        let title = args
            .title
            .filter(|title| !title.is_empty())
            .or_else(|| {
                PathBuf::from(&args.path)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "Untitled".into());
        let author = args
            .author
            .filter(|author| !author.is_empty())
            .unwrap_or_else(|| DEFAULT_AUTHOR.into());
        let job = self.library.create_job("import-request", None, None)?;
        let job = self.enqueue(
            job,
            Work::Import(ImportWork {
                title,
                format,
                author,
                confirmation: args.confirmation,
                expect: args.expect,
                reference,
                bytes,
            }),
            false,
        )?;
        self.wait_for_job(job.job_id, Some(wait_ms))
    }

    pub fn restore(&self, args: RestoreArgs) -> Result<Job> {
        self.available()?;
        let wait_ms = validate_wait_ms(args.wait_ms)?;
        let snapshot = self.library.version_snapshot(args.version_id)?;
        let document_id = snapshot.document_id();
        let _mutation = self.begin_document_mutation(document_id)?;
        let job = self
            .library
            .create_job("restore", Some(document_id), None)?;
        let job = self.enqueue(job, Work::Restore(args), false)?;
        self.wait_for_job(job.job_id, Some(wait_ms))
    }

    pub fn render_version(&self, document: DocumentId, version: VersionId) -> Result<Job> {
        self.available()?;
        let _mutation = self.begin_document_mutation(document)?;
        self.library.get_version(document, version)?;
        let snapshot = self.library.version_snapshot(version)?;
        let job = self
            .library
            .create_job("version-render", Some(document), Some(&snapshot))?;
        self.enqueue(job, Work::VersionRender(document, version), false)
    }

    pub fn output(&self, args: OutputArgs) -> Result<Job> {
        self.available()?;
        let wait_ms = validate_wait_ms(args.wait_ms)?;
        let snapshot = match &args.source {
            OutputSource::Job { job_id } => {
                let (job, _) = self.library.checked_job(*job_id)?;
                self.library
                    .job_snapshot(job.job_id)?
                    .ok_or_else(|| invalid("output source job has no snapshot"))?
            }
            OutputSource::Version { version_id } => self.library.version_snapshot(*version_id)?,
        };
        let document_id = snapshot.document_id();
        let _mutation = self.begin_document_mutation(document_id)?;
        if args.include_drawing == Some(true)
            && !matches!(
                args.format,
                OutputFormat::Pdf | OutputFormat::Png | OutputFormat::Svg
            )
        {
            return Err(invalid("includeDrawing supports only PDF, PNG and SVG"));
        }
        let drawing = if args.include_drawing == Some(true) {
            Some(self.library.drawing(document_id)?)
        } else {
            None
        };
        let destination = output_destination(&args, document_id)?;
        let settings = if args.format == OutputFormat::Wav {
            Some(self.library.score_settings(document_id)?)
        } else {
            None
        };
        let job = self
            .library
            .create_job("output", Some(document_id), Some(&snapshot))?;
        let job = self.enqueue(
            job,
            Work::Output(args, destination, drawing, settings),
            false,
        )?;
        self.wait_for_job(job.job_id, Some(wait_ms))
    }

    pub(crate) fn job_progress(&self, job: &Job) -> Option<JobProgress> {
        if job.kind != "output" || job.state != JobState::Running || job.phase == "cancelling" {
            return None;
        }
        self.state()
            .running
            .get(&job.job_id)
            .and_then(|running| running.context.progress())
    }

    /// Wait for a job to leave queued/running, bounded to the caller's timeout.
    pub fn wait_for_job(&self, id: JobId, wait_ms: Option<u64>) -> Result<Job> {
        let wait_ms = validate_wait_ms(wait_ms)?;
        if wait_ms == 0 {
            return self.library.get_job(id);
        }
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        loop {
            let job = self.library.get_job(id)?;
            if !matches!(job.state, JobState::Queued | JobState::Running)
                || Instant::now() >= deadline
            {
                return Ok(job);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn cancel(&self, id: JobId) -> Result<Job> {
        let mut state = self.state();
        let mut job = self.library.get_job(id)?;
        if !matches!(job.state, JobState::Queued | JobState::Running) {
            return Ok(job);
        }
        if let Some(running) = state.running.get(&id) {
            running.context.cancellation.cancel();
        }
        state.pending.retain(|p| p.job.job_id != id);
        for _ in 0..2 {
            if !matches!(job.state, JobState::Queued | JobState::Running)
                || job.phase == "cancelling"
            {
                break;
            }
            // Running operations acknowledge cancellation after cleanup. A completed durable write wins.
            let next = if job.state == JobState::Queued {
                JobState::Cancelled
            } else {
                JobState::Running
            };
            let phase = if next == JobState::Cancelled {
                "cancelled"
            } else {
                "cancelling"
            };
            match self
                .library
                .transition_job(id, job.state, next, phase, None, None)
            {
                Ok(updated) => {
                    job = updated;
                    break;
                }
                Err(error) if error.code == ErrorCode::Conflict => {
                    job = self.library.get_job(id)?;
                }
                Err(error) => return Err(error),
            }
        }
        self.emit_locked(
            &mut state,
            "job-changed",
            job.document_id,
            job.source_hash.clone(),
            json!({"jobId":id,"state":job.state,"phase":job.phase}),
        );
        Ok(job)
    }

    pub fn stop(&self) -> Result<()> {
        self.stopping.store(true, Ordering::Release);
        let jobs = self.library.list_jobs()?;
        for job in jobs {
            if matches!(job.state, JobState::Queued | JobState::Running) {
                self.cancel(job.job_id)?;
            }
        }
        self.changed.notify_waiters();
        Ok(())
    }

    pub async fn schedule(self: Arc<Self>) {
        loop {
            let notified = self.changed.notified();
            let tasks = {
                let mut state = self.state();
                if self.maintenance.load(Ordering::Acquire) {
                    None
                } else {
                    let mut occupied: HashSet<_> = state
                        .running
                        .values()
                        .filter_map(|running| running.document)
                        .collect();
                    let mut tasks = Vec::new();
                    let mut remaining = VecDeque::new();
                    while let Some(task) = state.pending.pop_front() {
                        if state.running.len() < 2
                            && task
                                .job
                                .document_id
                                .is_none_or(|id| !occupied.contains(&id))
                        {
                            if let Some(id) = task.job.document_id {
                                occupied.insert(id);
                            }
                            state.running.insert(
                                task.job.job_id,
                                RunningJob {
                                    document: task.job.document_id,
                                    context: task.context.clone(),
                                    preview: task.preview,
                                },
                            );
                            tasks.push(task);
                        } else {
                            remaining.push_back(task);
                        }
                    }
                    state.pending = remaining;
                    Some(tasks)
                }
            };
            let Some(tasks) = tasks else {
                tokio::select! {_=notified=>{},_=tokio::time::sleep(Duration::from_millis(50))=>{}}
                continue;
            };
            for task in tasks {
                let runtime = self.clone();
                tokio::spawn(async move {
                    let id = task.job.job_id;
                    let document = task.job.document_id;
                    let worker = runtime.clone();
                    let result = tokio::task::spawn_blocking(move || worker.run(task)).await;
                    if let Err(error) = result {
                        let _ = runtime.library.transition_job(
                            id,
                            JobState::Running,
                            JobState::Error,
                            "panic",
                            None,
                            Some(storage(error)),
                        );
                    }
                    runtime.state().running.remove(&id);
                    runtime.emit("job-changed", document, None, json!({"jobId":id}));
                    if let Some(document) = document {
                        let _ = runtime.observe(document);
                    }
                    runtime.maybe_cleanup_storage();
                    runtime.changed.notify_one();
                });
            }
            if self.stopping.load(Ordering::Acquire) && self.state().running.is_empty() {
                break;
            }
            tokio::select! {_=notified=>{},_=tokio::time::sleep(Duration::from_millis(50))=>{}}
        }
    }
}

use super::*;
use ossia_engine::CompileOptions;
use ossia_library::{CheckRequest, ImportRequest, RestoreRequest};
use serde_json::json;
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    time::Duration,
};

pub(super) enum Work {
    Check,
    Artifact(ArtifactArgs),
    VersionRender(DocumentId, VersionId),
    Import { args: ImportArgs, bytes: Vec<u8> },
    Restore(RestoreArgs),
    Export(ExportArgs, PathBuf),
}

pub(super) struct Pending {
    pub(super) job: Job,
    pub(super) work: Work,
    pub(super) preview: bool,
    pub(super) context: RunContext,
}

impl Runtime {
    fn enqueue(&self, job: Job, work: Work, preview: bool) -> Result<Job> {
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
                        None,
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
        state.pending.push_back(Pending {
            job: job.clone(),
            work,
            preview,
            context: RunContext::new(Duration::from_secs(120)),
        });
        self.emit_locked(
            &mut state,
            "job-changed",
            None,
            job.document_id,
            job.source_hash.clone(),
            json!({"jobId":job.job_id,"state":"queued"}),
        );
        self.changed.notify_one();
        Ok(job)
    }

    pub fn check(&self, args: CheckArgs, preview: bool) -> Result<Job> {
        self.available()?;
        if !preview {
            self.document_in_view(args.view_id, args.document_id)?;
        }
        if let Some(edit) = args.edit_id {
            self.owned_edit(args.document_id, args.view_id, edit, false)?;
        }
        self.observe(args.document_id)?;
        let job = self.library.queue_check(CheckRequest {
            expect: args.expect.as_ref(),
            document_id: args.document_id,
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
        self.enqueue(job, Work::Check, preview)
    }

    pub fn artifact(&self, args: ArtifactArgs) -> Result<Job> {
        self.available()?;
        let (source, _) = self.library.checked_job(args.job_id)?;
        let snapshot = self
            .library
            .job_snapshot(source.job_id)?
            .ok_or_else(|| invalid("source job has no snapshot"))?;
        let job = self
            .library
            .create_job("artifact", source.document_id, Some(&snapshot))?;
        self.enqueue(job, Work::Artifact(args), false)
    }

    pub fn import(&self, args: ImportArgs, bytes: Vec<u8>) -> Result<Job> {
        self.available()?;
        let job = self.library.create_job("import-request", None, None)?;
        self.enqueue(job, Work::Import { args, bytes }, false)
    }

    pub fn restore(&self, args: RestoreArgs) -> Result<Job> {
        self.available()?;
        self.library
            .get_version(args.document_id, args.version_id)?;
        let job = self
            .library
            .create_job("restore", Some(args.document_id), None)?;
        self.enqueue(job, Work::Restore(args), false)
    }

    pub fn render_version(&self, document: DocumentId, version: VersionId) -> Result<Job> {
        self.available()?;
        self.library.get_version(document, version)?;
        let snapshot = self.library.version_snapshot(version)?;
        let job = self
            .library
            .create_job("version-render", Some(document), Some(&snapshot))?;
        self.enqueue(job, Work::VersionRender(document, version), false)
    }

    pub fn export(&self, args: ExportArgs) -> Result<Job> {
        self.available()?;
        self.library
            .get_version(args.document_id, args.version_id)?;
        let destination = if let Some(path) = &args.output_path {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(invalid("outputPath must be absolute"));
            }
            path
        } else {
            let folder = dirs::download_dir()
                .ok_or_else(|| storage("system download directory unavailable"))?
                .join("Ossia");
            std::fs::create_dir_all(&folder).map_err(storage)?;
            let ext = match args.format {
                ExportFormat::Ly => "ly",
                ExportFormat::Pdf => "pdf",
                ExportFormat::Midi => "mid",
            };
            folder.join(format!(
                "{}-{}.{}",
                args.document_id,
                uuid::Uuid::new_v4(),
                ext
            ))
        };
        let job = self
            .library
            .create_job("export", Some(args.document_id), None)?;
        self.enqueue(job, Work::Export(args, destination), false)
    }

    pub fn cancel(&self, id: JobId) -> Result<Job> {
        let mut state = self.state();
        let mut job = self.library.get_job(id)?;
        if !matches!(job.state, JobState::Queued | JobState::Running) {
            return Ok(job);
        }
        if let Some((_, context)) = state.running.get(&id) {
            context.cancellation.cancel();
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
            None,
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
                let mut occupied: HashSet<_> =
                    state.running.values().filter_map(|(doc, _)| *doc).collect();
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
                            (task.job.document_id, task.context.clone()),
                        );
                        tasks.push(task);
                    } else {
                        remaining.push_back(task);
                    }
                }
                state.pending = remaining;
                tasks
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
                    runtime.changed.notify_one();
                });
            }
            if self.stopping.load(Ordering::Acquire) && self.state().running.is_empty() {
                break;
            }
            tokio::select! {_=notified=>{},_=tokio::time::sleep(Duration::from_millis(50))=>{}}
        }
    }

    fn run(&self, task: Pending) -> Result<()> {
        if self.library.get_job(task.job.job_id)?.state != JobState::Queued {
            return Ok(());
        }
        if matches!(task.work, Work::Check) {
            let _ = self
                .library
                .run_check(self.engine.as_ref(), task.job.job_id, &task.context);
            return Ok(());
        }
        self.library.transition_job(
            task.job.job_id,
            JobState::Queued,
            JobState::Running,
            "execute",
            None,
            None,
        )?;
        let result: Result<Value> = (|| {
            task.context.check()?;
            match task.work {
                Work::Check => unreachable!(),
                Work::Artifact(args) => value(self.library.draft_artifact(
                    self.engine.as_ref(),
                    &args,
                    &task.context,
                )?),
                Work::VersionRender(document, version) => value(self.library.render_version(
                    self.engine.as_ref(),
                    document,
                    version,
                    &task.context,
                )?),
                Work::Import { args, bytes } => {
                    let outcome = self.library.import(
                        self.engine.as_ref(),
                        ImportRequest {
                            title: &args.title,
                            input: &bytes,
                            format: args.format,
                            confirmation: args.confirmation.as_ref(),
                            author: &args.author,
                        },
                        &task.context,
                    )?;
                    if let Some(document) = &outcome.document {
                        self.emit(
                            "document-created",
                            Some(document.document_id),
                            Some(document.source_hash.clone()),
                            json!({}),
                        );
                    }
                    value(outcome)
                }
                Work::Restore(args) => value(self.library.restore(
                    self.engine.as_ref(),
                    RestoreRequest {
                        target_version_id: args.version_id,
                        base_version_id: args.base_version_id,
                        expected_source_hash: &args.expected_source_hash,
                        author: &args.author,
                        summary: &args.summary,
                    },
                    &task.context,
                )?),
                Work::Export(args, destination) => value(self.library.export_version(
                    self.engine.as_ref(),
                    &args,
                    &destination,
                    &task.context,
                )?),
            }
        })();
        let current = self.library.get_job(task.job.job_id)?;
        if current.state != JobState::Running {
            return Ok(());
        }
        match result {
            Ok(result) => {
                self.library.transition_job(
                    task.job.job_id,
                    JobState::Running,
                    JobState::Done,
                    "finished",
                    Some(result),
                    None,
                )?;
            }
            Err(error) => {
                let state = if error.code == ErrorCode::Cancelled {
                    JobState::Cancelled
                } else {
                    JobState::Error
                };
                self.library.transition_job(
                    task.job.job_id,
                    JobState::Running,
                    state,
                    "finished",
                    None,
                    Some(error),
                )?;
            }
        }
        Ok(())
    }
}

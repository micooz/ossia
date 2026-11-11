use super::*;
use ossia_engine::CompileOptions;
use ossia_library::{CheckRequest, ImportRequest, RestoreRequest};
use serde_json::json;
use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(super) enum Work {
    Check,
    Evidence,
    VersionRender(DocumentId, VersionId),
    Import(ImportWork),
    Restore(RestoreArgs),
    Output(OutputArgs, PathBuf),
}

pub(super) struct ImportWork {
    title: String,
    format: InputFormat,
    author: String,
    confirmation: Option<ContentHash>,
    expect: Option<MusicExpectation>,
    reference: Option<ReferenceMusic>,
    bytes: Vec<u8>,
}

fn input_format_from_path(path: &str) -> Option<InputFormat> {
    let extension = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "ly" => Some(InputFormat::Ly),
        "musicxml" | "xml" => Some(InputFormat::MusicXml),
        "mid" | "midi" => Some(InputFormat::Midi),
        _ => None,
    }
}

fn output_destination(args: &OutputArgs, document: DocumentId) -> Result<PathBuf> {
    if let Some(path) = &args.output_path {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(invalid("outputPath must be absolute"));
        }
        return Ok(path);
    }
    let folder = dirs::download_dir()
        .ok_or_else(|| storage("system download directory unavailable"))?
        .join("Ossia");
    std::fs::create_dir_all(&folder).map_err(storage)?;
    let extension = match args.format {
        OutputFormat::Ly => "ly",
        OutputFormat::Svg => "svg",
        OutputFormat::Png => "png",
        OutputFormat::Pdf => "pdf",
        OutputFormat::Midi => "mid",
    };
    Ok(folder.join(format!(
        "{}-{}.{}",
        document,
        uuid::Uuid::new_v4(),
        extension
    )))
}

pub(super) struct Pending {
    pub(super) job: Job,
    pub(super) work: Work,
    pub(super) preview: bool,
    pub(super) context: RunContext,
}

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
            let edit = self.edit_capability(args.view_id, edit_id, false)?;
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
        if !preview {
            self.document_in_view(args.view_id, document_id)?;
        }
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
        let job = self
            .library
            .create_job("restore", Some(document_id), None)?;
        let job = self.enqueue(job, Work::Restore(args), false)?;
        self.wait_for_job(job.job_id, Some(wait_ms))
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
        let destination = output_destination(&args, document_id)?;
        let job = self
            .library
            .create_job("output", Some(document_id), Some(&snapshot))?;
        let job = self.enqueue(job, Work::Output(args, destination), false)?;
        self.wait_for_job(job.job_id, Some(wait_ms))
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
                if self.maintenance.load(Ordering::Acquire) {
                    None
                } else {
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
                Work::Evidence => {
                    let snapshot = self
                        .library
                        .job_snapshot(task.job.job_id)?
                        .ok_or_else(|| storage("evidence job has no snapshot"))?;
                    let (report, cacheable) = self.library.derive_evidence(
                        self.engine.as_ref(),
                        &snapshot,
                        &task.context,
                    )?;
                    if cacheable {
                        self.library.save_evidence(&report)?;
                    }
                    value(report)
                }
                Work::VersionRender(document, version) => value(self.library.render_version(
                    self.engine.as_ref(),
                    document,
                    version,
                    &task.context,
                )?),
                Work::Import(args) => {
                    let request = ImportRequest {
                        title: &args.title,
                        input: &args.bytes,
                        format: args.format,
                        confirmation: args.confirmation.as_ref(),
                        author: &args.author,
                    };
                    let outcome = if let Some(reference) = args.reference.as_ref() {
                        self.library.import_with_reference(
                            self.engine.as_ref(),
                            request,
                            Some(reference),
                            &task.context,
                        )?
                    } else {
                        self.library.import_with_expect(
                            self.engine.as_ref(),
                            request,
                            args.expect.as_ref(),
                            &task.context,
                        )?
                    };
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
                        author: args.author.as_deref().unwrap_or(DEFAULT_AUTHOR),
                        summary: &args.summary,
                    },
                    &task.context,
                )?),
                Work::Output(args, destination) => value(self.library.output(
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

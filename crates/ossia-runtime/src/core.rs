use crate::*;
use notify::{RecursiveMode, Watcher};
use ossia_engine::{CompileOptions, RunContext};
use ossia_library::{
    BeginEditRequest, CheckRequest, CommitRequest, EngineBackend, ImportRequest, Library,
    RestoreRequest,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::Notify;

const EVENT_CAPACITY: usize = 512;
#[derive(Clone)]
struct View {
    document: Option<DocumentId>,
    selection: Option<Selection>,
}
struct Ticket {
    view: ViewId,
    expires: Instant,
}
struct Token {
    view: ViewId,
    expires: Instant,
}
struct State {
    views: HashMap<ViewId, View>,
    tickets: HashMap<String, Ticket>,
    tokens: HashMap<String, Token>,
    seq: u64,
    events: VecDeque<RuntimeEvent>,
    pending: VecDeque<Pending>,
    running: HashMap<JobId, (Option<DocumentId>, RunContext)>,
    observed: HashMap<DocumentId, ContentHash>,
}
#[derive(Clone)]
enum Work {
    Check,
    VersionRender(DocumentId, VersionId),
    Import { args: ImportArgs, bytes: Vec<u8> },
    Restore(RestoreArgs),
    Export(ExportArgs, PathBuf),
}
struct Pending {
    job: Job,
    work: Work,
    preview: bool,
    context: RunContext,
}

pub struct Runtime {
    pub library: Arc<Library>,
    pub engine: Arc<dyn EngineBackend>,
    pub info: RuntimeInfo,
    state: Mutex<State>,
    pub changed: Notify,
    pub stopping: AtomicBool,
}

fn secret() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}
fn forbidden() -> AppError {
    AppError::new(
        ErrorCode::Forbidden,
        "view or edit belongs to a different session",
    )
}
fn stale() -> AppError {
    AppError::new(
        ErrorCode::StaleSelection,
        "source changed; read context and select again",
    )
}

impl Runtime {
    pub fn new(
        library: Library,
        engine: Arc<dyn EngineBackend>,
        web_base_url: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            info: RuntimeInfo {
                runtime_version: env!("CARGO_PKG_VERSION").into(),
                protocol_version: PROTOCOL_VERSION,
                schema_version: SCHEMA_VERSION,
                instance_id: uuid::Uuid::new_v4().to_string(),
                home: library.home().to_string_lossy().into_owned(),
                web_base_url,
                capabilities: vec![
                    "documents".into(),
                    "views".into(),
                    "jobs".into(),
                    "events".into(),
                    "cli".into(),
                    "mcp".into(),
                    "render-geometry".into(),
                    "version-render".into(),
                    "export-reveal".into(),
                ],
            },
            library: Arc::new(library),
            engine,
            state: Mutex::new(State {
                views: HashMap::new(),
                tickets: HashMap::new(),
                tokens: HashMap::new(),
                seq: 0,
                events: VecDeque::new(),
                pending: VecDeque::new(),
                running: HashMap::new(),
                observed: HashMap::new(),
            }),
            changed: Notify::new(),
            stopping: AtomicBool::new(false),
        })
    }
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("runtime state poisoned")
    }
    fn available(&self) -> Result<()> {
        if self.stopping.load(Ordering::Acquire) {
            Err(AppError::new(
                ErrorCode::RuntimeUnavailable,
                "runtime is stopping",
            ))
        } else {
            Ok(())
        }
    }
    fn emit_locked(
        &self,
        state: &mut State,
        kind: &str,
        view: Option<ViewId>,
        document: Option<DocumentId>,
        hash: Option<ContentHash>,
        payload: Value,
    ) {
        state.seq += 1;
        let seq = state.seq;
        state.events.push_back(RuntimeEvent {
            instance_id: self.info.instance_id.clone(),
            seq,
            r#type: kind.into(),
            view_id: view,
            document_id: document,
            source_hash: hash,
            payload,
        });
        if state.events.len() > EVENT_CAPACITY {
            state.events.pop_front();
        }
        self.changed.notify_waiters();
    }
    pub fn emit(
        &self,
        kind: &str,
        document: Option<DocumentId>,
        hash: Option<ContentHash>,
        payload: Value,
    ) {
        self.emit_locked(&mut self.state(), kind, None, document, hash, payload);
    }
    pub fn status(&self) -> Result<StatusSnapshot> {
        let state = self.state();
        let jobs = self.library.list_jobs()?;
        Ok(StatusSnapshot {
            runtime: self.info.clone(),
            engine_build_id: self.engine.build_id().into(),
            jobs,
            instance_id: self.info.instance_id.clone(),
            seq: state.seq,
        })
    }
    pub fn open_view(&self, document: Option<DocumentId>) -> Result<OpenView> {
        self.available()?;
        if let Some(id) = document {
            self.library.get_document(id)?;
        }
        let view = ViewId::new();
        let ticket = secret();
        let mut state = self.state();
        state.views.insert(
            view,
            View {
                document,
                selection: None,
            },
        );
        state
            .tickets
            .retain(|_, ticket| ticket.expires > Instant::now());
        state.tickets.insert(
            ticket.clone(),
            Ticket {
                view,
                expires: Instant::now() + Duration::from_secs(300),
            },
        );
        Ok(OpenView {
            view_id: view,
            url: format!("{}/#ticket={ticket}", self.info.web_base_url),
            ticket,
        })
    }
    pub fn exchange(&self, ticket: &str) -> Result<ExchangeResponse> {
        let view = {
            let mut state = self.state();
            let ticket = state.tickets.remove(ticket).ok_or_else(|| {
                AppError::new(ErrorCode::Unauthorized, "invalid or consumed ticket")
            })?;
            if ticket.expires <= Instant::now() {
                return Err(AppError::new(ErrorCode::Unauthorized, "ticket expired"));
            }
            ticket.view
        };
        let token = secret();
        self.state().tokens.insert(
            token.clone(),
            Token {
                view,
                expires: Instant::now() + Duration::from_secs(86400),
            },
        );
        Ok(ExchangeResponse {
            token,
            view: self.context(view)?,
        })
    }
    pub fn authenticate(&self, token: &str) -> Result<ViewId> {
        let state = self.state();
        let token = state
            .tokens
            .get(token)
            .filter(|t| t.expires > Instant::now())
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::Unauthorized,
                    "missing, expired or invalid bearer token",
                )
            })?;
        Ok(token.view)
    }
    fn view(&self, id: ViewId) -> Result<View> {
        self.state()
            .views
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, "view not found; open a new view"))
    }
    fn document_in_view(&self, view: ViewId, document: DocumentId) -> Result<()> {
        if self.view(view)?.document != Some(document) {
            Err(forbidden())
        } else {
            Ok(())
        }
    }
    pub fn observe(&self, document: DocumentId) -> Result<DocumentState> {
        let doc = self.library.observe_source(document)?;
        let mut state = self.state();
        if state
            .observed
            .insert(document, doc.source_hash.clone())
            .as_ref()
            != Some(&doc.source_hash)
        {
            for view in state
                .views
                .values_mut()
                .filter(|v| v.document == Some(document))
            {
                view.selection = None;
            }
            self.emit_locked(
                &mut state,
                "source-changed",
                None,
                Some(document),
                Some(doc.source_hash.clone()),
                json!({}),
            );
        }
        Ok(doc)
    }
    pub fn context(&self, id: ViewId) -> Result<ViewSnapshot> {
        let view = self.view(id)?;
        if let Some(document) = view.document {
            self.observe(document)?;
        }
        // The cursor and view state share the event lock, preventing a snapshot/subscription gap.
        let state = self.state();
        let view = state
            .views
            .get(&id)
            .ok_or_else(|| invalid("view disappeared"))?;
        let document = view
            .document
            .map(|d| self.library.observe_source(d))
            .transpose()?;
        let selection = view.selection.clone();
        let mut regions = Vec::new();
        if let (Some(doc), Some(selection)) = (&document, &selection) {
            if selection.source_hash != doc.source_hash {
                return Err(stale());
            }
            let source = self.library.source_snapshot(doc.document_id)?;
            if source.source_hash() != &selection.source_hash {
                return Err(stale());
            }
            let index = ossia_score::build_index(source.source());
            for locator in &selection.locators {
                regions.push(ossia_score::resolve_locator(&index, locator)?);
            }
        }
        let render = document.as_ref().and_then(|d| {
            d.last_successful_render.as_ref().map(|m| RenderReference {
                render_key: ossia_library::render_key(d.document_id, m),
                manifest: m.clone(),
            })
        });
        let edit_activity = document
            .as_ref()
            .map(|d| self.library.edit_activity(d.document_id, id))
            .transpose()?
            .flatten();
        Ok(ViewSnapshot {
            instance_id: self.info.instance_id.clone(),
            seq: state.seq,
            view_id: id,
            document,
            selection,
            regions,
            render,
            edit_activity,
        })
    }
    pub fn select_document(
        &self,
        view: ViewId,
        document: Option<DocumentId>,
    ) -> Result<ViewSnapshot> {
        if let Some(id) = document {
            self.library.get_document(id)?;
        }
        {
            let mut state = self.state();
            let v = state
                .views
                .get_mut(&view)
                .ok_or_else(|| invalid("unknown view"))?;
            v.document = document;
            v.selection = None;
            self.emit_locked(
                &mut state,
                "view-changed",
                Some(view),
                document,
                None,
                json!({}),
            );
        }
        self.context(view)
    }
    pub fn selection(&self, selection: Selection) -> Result<ViewSnapshot> {
        self.document_in_view(selection.view_id, selection.document_id)?;
        let snapshot = self.library.source_snapshot(selection.document_id)?;
        if snapshot.source_hash() != &selection.source_hash {
            return Err(stale());
        }
        let index = ossia_score::build_index(snapshot.source());
        for locator in &selection.locators {
            let region = ossia_score::resolve_locator(&index, locator)?;
            if region.capability != LocationCapability::Measure {
                return Err(AppError::unsupported(
                    "selection requires reliable measure locations",
                ));
            }
        }
        {
            let mut state = self.state();
            let view = state
                .views
                .get_mut(&selection.view_id)
                .ok_or_else(|| invalid("unknown view"))?;
            view.selection = if selection.locators.is_empty() {
                None
            } else {
                Some(selection.clone())
            };
            state
                .observed
                .insert(selection.document_id, selection.source_hash.clone());
            self.emit_locked(
                &mut state,
                "selection-changed",
                Some(selection.view_id),
                Some(selection.document_id),
                Some(selection.source_hash.clone()),
                json!({}),
            );
        }
        let context = self.context(selection.view_id)?;
        if context
            .document
            .as_ref()
            .is_none_or(|doc| doc.source_hash != selection.source_hash)
        {
            return Err(stale());
        }
        Ok(context)
    }
    pub fn locate(&self, view: ViewId, args: LocateArgs) -> Result<LocateOutcome> {
        let document = self
            .view(view)?
            .document
            .ok_or_else(|| invalid("view has no document"))?;
        let doc = self.observe(document)?;
        let snapshot = self.library.source_snapshot(document)?;
        if args.source_hash != doc.source_hash || snapshot.source_hash() != &args.source_hash {
            return Err(stale());
        }
        let manifest = doc
            .last_successful_render
            .filter(|r| r.source_hash == args.source_hash)
            .ok_or_else(|| AppError::unsupported("no current successful render"))?;
        let mut pages = Vec::new();
        for page in &manifest.pages {
            let bytes = self.library.read_artifact(&page.artifact)?;
            pages.push((page.page, String::from_utf8(bytes).map_err(storage)?));
        }
        let pages: Vec<_> = pages
            .iter()
            .map(|(page, svg)| ossia_score::SvgPage { page: *page, svg })
            .collect();
        let index = ossia_score::build_index(snapshot.source());
        let layout = ossia_score::build_layout(&index, &pages)?;
        let result = ossia_score::locate(
            &index,
            &layout,
            &ossia_score::LocateQuery::Point {
                page: args.page,
                x: args.x,
                y: args.y,
            },
        )?;
        Ok(LocateOutcome {
            capability: result.capability,
            reason: result.reason,
            region: result.region,
        })
    }
    fn owned_edit(
        &self,
        document: DocumentId,
        view: ViewId,
        edit: EditId,
        transfer: bool,
    ) -> Result<EditSession> {
        let edit = self.library.get_edit(edit)?;
        if edit.document_id != document || (!transfer && edit.owner_view_id != view) {
            return Err(forbidden());
        }
        Ok(edit)
    }
    pub fn edit(&self, args: EditArgs, trusted: bool) -> Result<Value> {
        self.available()?;
        self.document_in_view(args.view_id, args.document_id)?;
        if let Some(id) = args.edit_id {
            self.owned_edit(
                args.document_id,
                args.view_id,
                id,
                trusted && matches!(args.action, EditAction::Resume),
            )?;
        }
        let doc = self.observe(args.document_id)?;
        if doc.source_hash != args.expected_source_hash {
            return Err(AppError::new(ErrorCode::Conflict, "public source changed"));
        }
        let edit = match args.action {
            EditAction::Begin => Some(self.library.begin_edit(BeginEditRequest {
                document_id: args.document_id,
                view_id: args.view_id,
                base_version_id: args.base_version_id,
                expected_source_hash: &args.expected_source_hash,
                author: &args.author,
                adopt_workspace: args.adopt_workspace,
            })?),
            EditAction::Resume => {
                let id = args
                    .edit_id
                    .ok_or_else(|| invalid("resume requires editId"))?;
                self.owned_edit(args.document_id, args.view_id, id, trusted)?;
                Some(self.library.resume_edit(
                    id,
                    args.view_id,
                    args.base_version_id,
                    &args.expected_source_hash,
                )?)
            }
            EditAction::End => {
                let id = args.edit_id.ok_or_else(|| invalid("end requires editId"))?;
                self.owned_edit(args.document_id, args.view_id, id, false)?;
                self.library.end_edit(id)?;
                None
            }
            EditAction::Discard => {
                if let Some(id) = args.edit_id {
                    self.owned_edit(args.document_id, args.view_id, id, false)?;
                }
                self.library.discard(
                    args.document_id,
                    args.edit_id,
                    args.base_version_id,
                    &args.expected_source_hash,
                )?;
                None
            }
            EditAction::ResolveRecoveryConflict => {
                self.library
                    .resolve_recovery_conflict(args.document_id, &args.expected_source_hash)?;
                None
            }
        };
        self.emit("document-changed", Some(args.document_id), None, json!({}));
        value(edit)
    }
    pub fn commit(&self, args: CommitArgs) -> Result<Version> {
        self.available()?;
        self.document_in_view(args.view_id, args.document_id)?;
        self.owned_edit(args.document_id, args.view_id, args.edit_id, false)?;
        self.observe(args.document_id)?;
        let version = self.library.commit(CommitRequest {
            edit_id: args.edit_id,
            base_version_id: args.base_version_id,
            expected_source_hash: &args.expected_source_hash,
            summary: &args.summary,
        })?;
        self.emit(
            "version-created",
            Some(args.document_id),
            Some(version.source_hash.clone()),
            value(&version)?,
        );
        Ok(version)
    }
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
            error.detail = Some(json!({"jobId":job.job_id}));
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
            document_id: args.document_id,
            edit_id: args.edit_id,
            expected_source_hash: &args.expected_source_hash,
            options: &CompileOptions::default(),
        })?;
        self.enqueue(job, Work::Check, preview)
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
    pub fn events(&self, view: ViewId, instance: &str, after: u64) -> Result<Vec<RuntimeEvent>> {
        self.view(view)?;
        let state = self.state();
        if instance != self.info.instance_id
            || after > state.seq
            || state
                .events
                .front()
                .is_some_and(|e| after < e.seq.saturating_sub(1))
        {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "event cursor expired; fetch a new snapshot",
            ));
        }
        Ok(state
            .events
            .iter()
            .filter(|e| e.seq > after && e.view_id.is_none_or(|v| v == view))
            .cloned()
            .collect())
    }
    pub async fn schedule(self: Arc<Self>) {
        loop {
            let notified = self.changed.notified();
            let tasks = {
                let mut state = self.state();
                let occupied: HashSet<_> =
                    state.running.values().filter_map(|(doc, _)| *doc).collect();
                let mut occupied = occupied;
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
    pub async fn watch(self: Arc<Self>) -> Result<()> {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let root = self.library.home().join("documents");
        std::fs::create_dir_all(&root).map_err(storage)?;
        let native_sender = sender.clone();
        let native = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let _ = native_sender.send(event);
        });
        let _native = match native {
            Ok(mut watcher) => match watcher.watch(&root, RecursiveMode::Recursive) {
                Ok(()) => Some(watcher),
                Err(error) => {
                    self.emit(
                        "watch-fallback",
                        None,
                        None,
                        json!({"message":error.to_string(),"mode":"content-poll"}),
                    );
                    None
                }
            },
            Err(error) => {
                self.emit(
                    "watch-fallback",
                    None,
                    None,
                    json!({"message":error.to_string(),"mode":"content-poll"}),
                );
                None
            }
        };
        // Native event streams may lose changes; a content poll also detects same-timestamp saves and rename/recreation.
        let mut polling = notify::PollWatcher::new(
            move |event| {
                let _ = sender.send(event);
            },
            notify::Config::default()
                .with_poll_interval(Duration::from_secs(1))
                .with_compare_contents(true),
        )
        .map_err(storage)?;
        polling
            .watch(&root, RecursiveMode::Recursive)
            .map_err(storage)?;
        let mut saves: HashMap<DocumentId, Instant> = HashMap::new();
        loop {
            tokio::select! {
                event=receiver.recv()=>{if let Some(event)=event {match event {Ok(event) if !matches!(event.kind,notify::EventKind::Access(_))=>{for path in event.paths {if let Ok(path)=path.strip_prefix(&root){if let Some(component)=path.components().next(){if let Some(id)=component.as_os_str().to_str().and_then(|s|s.parse::<DocumentId>().ok()){saves.insert(id,Instant::now());}}}}},Ok(_)=>{},Err(error)=>self.emit("watch-error",None,None,json!({"message":error.to_string()}))}}},
                _=tokio::time::sleep(Duration::from_millis(50))=>{}
            }
            if self.stopping.load(Ordering::Acquire) {
                break;
            }
            let ready: Vec<_> = saves
                .iter()
                .filter(|(_, time)| time.elapsed() >= Duration::from_millis(300))
                .map(|(id, _)| *id)
                .collect();
            for id in ready {
                saves.remove(&id);
                match self.observe(id) {
                    Ok(doc) => {
                        let already = {
                            let state = self.state();
                            state.pending.iter().any(|p| {
                                p.job.document_id == Some(id)
                                    && p.job.source_hash.as_ref() == Some(&doc.source_hash)
                            }) || state.running.keys().any(|job| {
                                self.library.get_job(*job).ok().is_some_and(|j| {
                                    j.document_id == Some(id)
                                        && j.source_hash.as_ref() == Some(&doc.source_hash)
                                })
                            })
                        };
                        if !already
                            && doc
                                .last_successful_render
                                .as_ref()
                                .is_none_or(|r| r.source_hash != doc.source_hash)
                        {
                            let _ = self.check(
                                CheckArgs {
                                    document_id: id,
                                    view_id: ViewId::new(),
                                    edit_id: None,
                                    expected_source_hash: doc.source_hash,
                                },
                                true,
                            );
                        }
                    }
                    Err(error) => self.emit("source-error", Some(id), None, json!({"error":error})),
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod expiry_tests {
    use super::*;
    #[test]
    fn tickets_and_tokens_expire_without_recovery_or_reuse() {
        struct UnusedEngine;
        impl EngineBackend for UnusedEngine {
            fn build_id(&self) -> &str {
                "unused"
            }
            fn convert(
                &self,
                _: &[u8],
                _: InputFormat,
                _: Option<&ContentHash>,
                _: &RunContext,
            ) -> Result<ImportReport> {
                unreachable!()
            }
            fn compile(
                &self,
                _: &SourceSnapshot,
                _: &CompileOptions,
                _: &std::path::Path,
                _: &RunContext,
            ) -> Result<RenderManifest> {
                unreachable!()
            }
        }
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(
            Library::open(root.path()).unwrap(),
            Arc::new(UnusedEngine),
            "http://127.0.0.1:3211".into(),
        );
        let open = runtime.open_view(None).unwrap();
        runtime
            .state()
            .tickets
            .get_mut(&open.ticket)
            .unwrap()
            .expires = Instant::now();
        assert_eq!(
            runtime.exchange(&open.ticket).unwrap_err().code,
            ErrorCode::Unauthorized
        );
        let open = runtime.open_view(None).unwrap();
        let session = runtime.exchange(&open.ticket).unwrap();
        assert!(runtime.authenticate(&session.token).is_ok());
        runtime
            .state()
            .tokens
            .get_mut(&session.token)
            .unwrap()
            .expires = Instant::now();
        assert_eq!(
            runtime.authenticate(&session.token).unwrap_err().code,
            ErrorCode::Unauthorized
        );
    }
}

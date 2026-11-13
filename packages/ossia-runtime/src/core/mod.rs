use crate::*;
use ossia_engine::RunContext;
use ossia_library::{BeginEditRequest, CommitRequest, EngineBackend, Library};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use tokio::sync::Notify;

mod analysis;
mod context;
mod evidence;
mod jobs;
mod storage;
mod watch;
use jobs::Pending;

const EVENT_CAPACITY: usize = 512;

struct State {
    selections: HashMap<DocumentId, Selection>,
    seq: u64,
    events: VecDeque<RuntimeEvent>,
    pending: VecDeque<Pending>,
    running: HashMap<JobId, (Option<DocumentId>, RunContext)>,
    observed: HashMap<DocumentId, ContentHash>,
    suppressed_previews: HashMap<DocumentId, ContentHash>,
}

pub struct Runtime {
    pub(crate) documentation: std::sync::RwLock<Result<ossia_library::Documentation>>,
    pub(crate) documentation_resources: std::sync::RwLock<Option<std::path::PathBuf>>,
    pub(crate) documentation_cache: std::sync::RwLock<Option<DocumentationStatus>>,
    pub library: Arc<Library>,
    pub engine: Arc<dyn EngineBackend>,
    pub info: RuntimeInfo,
    state: Mutex<State>,
    analysis: Mutex<analysis::AnalysisCache>,
    ui_language: std::sync::RwLock<Option<UiLanguage>>,
    parent_pid: Mutex<Option<u32>>,
    pub changed: Notify,
    pub stopping: AtomicBool,
    maintenance: AtomicBool,
    last_maintenance: Mutex<Instant>,
}

fn forbidden() -> AppError {
    AppError::new(ErrorCode::Forbidden, "edit belongs to a different document")
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
            documentation: std::sync::RwLock::new(Err(invalid(
                "offline documentation is not configured",
            ))),
            documentation_resources: std::sync::RwLock::new(None),
            documentation_cache: std::sync::RwLock::new(None),
            info: RuntimeInfo {
                runtime_version: env!("CARGO_PKG_VERSION").into(),
                protocol_version: PROTOCOL_VERSION,
                schema_version: SCHEMA_VERSION,
                instance_id: uuid::Uuid::new_v4().to_string(),
                home: library.home().to_string_lossy().into_owned(),
                web_base_url,
                capabilities: vec![
                    "documents".into(),
                    "write-source".into(),
                    "source-read".into(),
                    "source-paged".into(),
                    "source-edits".into(),
                    "outputs".into(),
                    "job-evidence".into(),
                    "version-evidence".into(),
                    "music-expectations".into(),
                    "reference-expectations".into(),
                    "offline-docs".into(),
                    "document-context".into(),
                    "jobs".into(),
                    "events".into(),
                    "cli".into(),
                    "mcp".into(),
                    "render-geometry".into(),
                    "version-render".into(),
                    "output-reveal".into(),
                    "ui-language".into(),
                    "storage".into(),
                ],
            },
            library: Arc::new(library),
            engine,
            analysis: Mutex::new(analysis::AnalysisCache::default()),
            state: Mutex::new(State {
                selections: HashMap::new(),
                seq: 0,
                events: VecDeque::new(),
                pending: VecDeque::new(),
                running: HashMap::new(),
                observed: HashMap::new(),
                suppressed_previews: HashMap::new(),
            }),
            ui_language: std::sync::RwLock::new(None),
            parent_pid: Mutex::new(None),
            changed: Notify::new(),
            stopping: AtomicBool::new(false),
            maintenance: AtomicBool::new(false),
            last_maintenance: Mutex::new(Instant::now()),
        })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("runtime state poisoned")
    }

    pub(crate) fn event_seq(&self) -> u64 {
        self.state().seq
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

    pub fn ui_language(&self) -> Option<UiLanguage> {
        *self.ui_language.read().expect("runtime language poisoned")
    }

    pub fn initialize_ui_language(&self, language: Option<UiLanguage>) {
        *self.ui_language.write().expect("runtime language poisoned") = language;
    }

    pub fn set_ui_language(&self, language: Option<UiLanguage>) {
        let mut current = self.ui_language.write().expect("runtime language poisoned");
        if *current == language {
            return;
        }
        *current = language;
        drop(current);
        self.emit(
            "ui-language-changed",
            None,
            None,
            json!({"language": language}),
        );
    }

    pub fn set_parent_pid(&self, pid: u32) {
        *self.parent_pid.lock().expect("runtime parent poisoned") = Some(pid);
    }

    pub fn parent_pid(&self) -> Option<u32> {
        *self.parent_pid.lock().expect("runtime parent poisoned")
    }

    fn emit_locked(
        &self,
        state: &mut State,
        kind: &str,
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
        self.emit_locked(&mut self.state(), kind, document, hash, payload);
    }

    pub fn write_source(&self, args: WriteSourceArgs) -> Result<WriteSourceOutcome> {
        self.available()?;
        let edit = self.edit_capability(args.edit_id)?;
        let result = self.library.write_source(&args)?;
        self.observe(edit.document_id)?;
        if !args.preview {
            self.state()
                .suppressed_previews
                .insert(edit.document_id, result.source_hash.clone());
        }
        Ok(result)
    }

    pub fn read_source(&self, args: ReadSourceArgs) -> Result<ReadSourceOutcome> {
        self.available()?;
        self.library.get_document(args.document_id)?;
        self.library.read_source(&args)
    }

    fn owned_edit(&self, document: DocumentId, edit: EditId) -> Result<EditSession> {
        let edit = self.library.get_edit(edit)?;
        if edit.document_id != document {
            return Err(forbidden());
        }
        Ok(edit)
    }

    fn edit_capability(&self, edit: EditId) -> Result<EditSession> {
        self.library.get_edit(edit)
    }

    pub fn edit(&self, args: EditArgs) -> Result<Value> {
        self.available()?;
        if let Some(id) = args.edit_id {
            self.owned_edit(args.document_id, id)?;
        }
        let doc = self.observe(args.document_id)?;
        if doc.source_hash != args.expected_source_hash {
            let mut error = AppError::new(ErrorCode::Conflict, "public source changed");
            error.detail = Some(json!({
                "currentSourceHash": doc.source_hash,
                "headVersionId": doc.head_version_id,
                "editState": doc.edit_state,
            }));
            return Err(error);
        }
        let edit = match args.action {
            EditAction::Begin => Some(self.library.begin_edit(BeginEditRequest {
                document_id: args.document_id,
                base_version_id: args.base_version_id,
                expected_source_hash: &args.expected_source_hash,
                author: &args.author,
                adopt_workspace: args.adopt_workspace,
            })?),
            EditAction::Resume => {
                let id = args
                    .edit_id
                    .ok_or_else(|| invalid("resume requires editId"))?;
                self.owned_edit(args.document_id, id)?;
                Some(self.library.resume_edit(
                    id,
                    args.base_version_id,
                    &args.expected_source_hash,
                )?)
            }
            EditAction::End => {
                let id = args.edit_id.ok_or_else(|| invalid("end requires editId"))?;
                self.owned_edit(args.document_id, id)?;
                self.library.end_edit(id)?;
                None
            }
            EditAction::Discard => {
                if let Some(id) = args.edit_id {
                    self.owned_edit(args.document_id, id)?;
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
        let edit = self.edit_capability(args.edit_id)?;
        self.observe(edit.document_id)?;
        let version = self.library.commit(CommitRequest {
            edit_id: args.edit_id,
            base_version_id: edit.base_version_id,
            expected_source_hash: &args.expected_source_hash,
            summary: &args.summary,
        })?;
        self.emit(
            "version-created",
            Some(edit.document_id),
            Some(version.source_hash.clone()),
            value(&version)?,
        );
        Ok(version)
    }
}

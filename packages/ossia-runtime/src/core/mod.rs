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

mod jobs;
mod views;
mod watch;
use jobs::Pending;

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

pub struct Runtime {
    pub(crate) documentation: std::sync::RwLock<Result<ossia_library::Documentation>>,
    pub(crate) documentation_resources: std::sync::RwLock<Option<std::path::PathBuf>>,
    pub library: Arc<Library>,
    pub engine: Arc<dyn EngineBackend>,
    pub info: RuntimeInfo,
    state: Mutex<State>,
    ui_language: std::sync::RwLock<Option<UiLanguage>>,
    parent_pid: Mutex<Option<u32>>,
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
            documentation: std::sync::RwLock::new(Err(invalid(
                "offline documentation is not configured",
            ))),
            documentation_resources: std::sync::RwLock::new(None),
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
                    "music-expectations".into(),
                    "offline-docs".into(),
                    "views".into(),
                    "jobs".into(),
                    "events".into(),
                    "cli".into(),
                    "mcp".into(),
                    "render-geometry".into(),
                    "version-render".into(),
                    "output-reveal".into(),
                    "ui-language".into(),
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
            ui_language: std::sync::RwLock::new(None),
            parent_pid: Mutex::new(None),
            changed: Notify::new(),
            stopping: AtomicBool::new(false),
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

    pub fn write_source(&self, args: WriteSourceArgs) -> Result<WriteSourceOutcome> {
        self.available()?;
        let edit = self.edit_capability(args.view_id, args.edit_id, false)?;
        self.document_in_view(args.view_id, edit.document_id)?;
        let result = self.library.write_source(&args)?;
        self.observe(edit.document_id)?;
        Ok(result)
    }

    pub fn read_source(&self, args: ReadSourceArgs) -> Result<ReadSourceOutcome> {
        self.available()?;
        self.document_in_view(args.view_id, args.document_id)?;
        self.library.read_source(&args)
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

    fn edit_capability(&self, view: ViewId, edit: EditId, transfer: bool) -> Result<EditSession> {
        let edit = self.library.get_edit(edit)?;
        if !transfer && edit.owner_view_id != view {
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
        let edit = self.edit_capability(args.view_id, args.edit_id, false)?;
        self.document_in_view(args.view_id, edit.document_id)?;
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

#[cfg(test)]
mod expiry_tests {
    use super::*;
    use ossia_engine::CompileOptions;

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

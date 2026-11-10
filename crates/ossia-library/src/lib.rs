#![doc = include_str!("../README.md")]
mod artifacts;
pub use artifacts::render_key;
mod db;
mod recovery;
mod storage;
mod workflow;

use fs2::FileExt;
pub use ossia_engine::{CompileOptions, Engine, RunContext};
pub use ossia_types::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};
use storage::{atomic_write, Store};

pub type Result<T> = std::result::Result<T, AppError>;

pub(crate) fn storage_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::StorageError, error.to_string())
}
pub(crate) fn conflict(message: &str) -> AppError {
    AppError::new(ErrorCode::Conflict, message)
}
pub(crate) fn not_found() -> AppError {
    AppError::new(ErrorCode::NotFound, "library entry not found")
}

/// Native Engine is the production adapter; deterministic adapters exercise failure/race paths.
pub trait EngineBackend: Send + Sync {
    fn build_id(&self) -> &str;
    fn convert(
        &self,
        input: &[u8],
        format: InputFormat,
        confirmation: Option<&ContentHash>,
        context: &RunContext,
    ) -> Result<ImportReport>;
    fn compile(
        &self,
        snapshot: &SourceSnapshot,
        options: &CompileOptions,
        artifact_root: &Path,
        context: &RunContext,
    ) -> Result<RenderManifest>;
}
impl EngineBackend for Engine {
    fn build_id(&self) -> &str {
        self.build_id()
    }
    fn convert(
        &self,
        input: &[u8],
        format: InputFormat,
        confirmation: Option<&ContentHash>,
        context: &RunContext,
    ) -> Result<ImportReport> {
        self.convert(input, format, confirmation, context)
    }
    fn compile(
        &self,
        snapshot: &SourceSnapshot,
        options: &CompileOptions,
        artifact_root: &Path,
        context: &RunContext,
    ) -> Result<RenderManifest> {
        self.compile(snapshot, options, artifact_root, context)
    }
}

pub struct ImportRequest<'a> {
    pub title: &'a str,
    pub input: &'a [u8],
    pub format: InputFormat,
    pub confirmation: Option<&'a ContentHash>,
    pub author: &'a str,
}

pub struct BeginEditRequest<'a> {
    pub document_id: DocumentId,
    pub view_id: ViewId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: &'a ContentHash,
    pub author: &'a str,
    /// Explicitly attribute a previously unknown workspace to this new edit.
    pub adopt_workspace: bool,
}

pub struct CheckRequest<'a> {
    pub document_id: DocumentId,
    pub edit_id: Option<EditId>,
    pub expected_source_hash: &'a ContentHash,
    pub options: &'a CompileOptions,
}

pub struct CommitRequest<'a> {
    pub edit_id: EditId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: &'a ContentHash,
    pub summary: &'a str,
}

pub struct RestoreRequest<'a> {
    pub target_version_id: VersionId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: &'a ContentHash,
    pub author: &'a str,
    pub summary: &'a str,
}

pub struct Library {
    store: Store,
    conn: Mutex<Connection>,
    _lock: File,
    #[cfg(test)]
    hooks: Mutex<std::collections::HashMap<&'static str, TestHook>>,
}

#[cfg(test)]
type TestHook = Box<dyn FnOnce(&Library) -> Result<()> + Send>;

impl Library {
    pub fn open(home: &Path) -> Result<Self> {
        storage::preflight(home)?;
        storage::ensure_directory(home)?;
        let store = Store {
            home: home.canonicalize().map_err(storage_error)?,
        };
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(store.path("library.lock")?)
            .map_err(storage_error)?;
        lock.try_lock_exclusive().map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                AppError::new(ErrorCode::LibraryBusy, "another process owns this library")
            } else {
                storage_error(e)
            }
        })?;
        if !storage::preflight(&store.home)? {
            storage::initialize(&store)?;
        }
        let conn = Connection::open(store.path(storage::DATABASE)?).map_err(storage_error)?;
        let schema: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(storage_error)?;
        if schema != SCHEMA_VERSION {
            return Err(AppError::new(
                ErrorCode::RuntimeIncompatible,
                "unsupported library schema",
            ));
        }
        let _: String = conn
            .query_row("SELECT library_id FROM library_meta", [], |r| r.get(0))
            .map_err(storage_error)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(storage_error)?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )
        .map_err(storage_error)?;
        let library = Self {
            store,
            conn: Mutex::new(conn),
            _lock: lock,
            #[cfg(test)]
            hooks: Mutex::default(),
        };
        library.recover()?;
        Ok(library)
    }

    pub fn home(&self) -> &Path {
        &self.store.home
    }
    pub fn artifact_root(&self) -> Result<PathBuf> {
        self.store.path("artifacts")
    }
    fn db(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| storage_error("library database mutex poisoned"))
    }
    fn hit(&self, point: &'static str) -> Result<()> {
        #[cfg(test)]
        {
            let hook = self.hooks.lock().unwrap().remove(point);
            if let Some(hook) = hook {
                hook(self)?;
            }
        }
        let _ = point;
        Ok(())
    }
    fn no_intent(&self, conn: &Connection, document: DocumentId) -> Result<()> {
        if conn
            .query_row(
                "SELECT 1 FROM worktree_intents WHERE document_id=?1",
                [document.to_string()],
                |r| r.get::<_, i32>(0),
            )
            .optional()
            .map_err(storage_error)?
            .is_some()
        {
            return Err(conflict("document has an unresolved worktree operation"));
        }
        Ok(())
    }
    fn snapshot(&self, doc: &db::Document) -> Result<SourceSnapshot> {
        SourceSnapshot::new(doc.id, self.store.read_source(&doc.source_path)?)
    }
    fn expect_snapshot(
        &self,
        doc: &db::Document,
        expected: &ContentHash,
    ) -> Result<SourceSnapshot> {
        let snapshot = self.snapshot(doc)?;
        if snapshot.source_hash() != expected {
            return Err(conflict("public source changed"));
        }
        Ok(snapshot)
    }
    fn expect_head(&self, doc: &db::Document, base: Option<VersionId>) -> Result<()> {
        if doc.head != base {
            Err(conflict("base version is stale"))
        } else {
            Ok(())
        }
    }

    pub fn get_document(&self, id: DocumentId) -> Result<DocumentState> {
        self.observe_source(id)
    }
    pub fn observe_source(&self, id: DocumentId) -> Result<DocumentState> {
        let conn = self.db()?;
        let doc = db::document(&conn, id)?;
        let snapshot = self.snapshot(&doc)?;
        let dirty = match doc.head {
            Some(head) => db::version(&conn, head)?.source_hash != *snapshot.source_hash(),
            None => true,
        };
        let pending = conn
            .query_row(
                "SELECT 1 FROM worktree_intents WHERE document_id=?1",
                [id.to_string()],
                |r| r.get::<_, i32>(0),
            )
            .optional()
            .map_err(storage_error)?
            .is_some();
        let edit_state = if pending {
            DocumentEditState::RecoveryConflict
        } else {
            match db::active_edit(&conn, id)?.map(|e| e.status) {
                Some(EditStatus::Active) => DocumentEditState::Active,
                Some(EditStatus::Interrupted) => DocumentEditState::Interrupted,
                _ => DocumentEditState::Idle,
            }
        };
        let current = conn.query_row("SELECT body FROM renders WHERE document_id=?1 AND source_hash=?2 ORDER BY rowid DESC LIMIT 1",
            params![id.to_string(),snapshot.source_hash().to_string()],|r|r.get::<_,String>(0)).optional().map_err(storage_error)?;
        let fallback = if current.is_none() {
            conn.query_row(
                "SELECT last_render FROM documents WHERE document_id=?1",
                [id.to_string()],
                |r| r.get::<_, Option<String>>(0),
            )
            .map_err(storage_error)?
        } else {
            None
        };
        Ok(DocumentState { document_id:id,title:doc.title,source_path:self.store.path(&doc.source_path)?.to_string_lossy().into_owned(),
            head_version_id:doc.head,source_hash:snapshot.source_hash().clone(),dirty,edit_state,
            last_successful_render:current.or(fallback).map(db::decode).transpose()?,
            last_check:conn.query_row("SELECT body FROM jobs WHERE document_id=?1 AND source_hash=?2 AND kind IN ('check','import') ORDER BY rowid DESC LIMIT 1", params![id.to_string(),snapshot.source_hash().to_string()],|r|r.get::<_,String>(0)).optional().map_err(storage_error)?.map(db::decode).transpose()? })
    }
    pub fn list_documents(&self) -> Result<Vec<DocumentState>> {
        let ids: Vec<String> = {
            let conn = self.db()?;
            let mut stmt = conn
                .prepare("SELECT document_id FROM documents ORDER BY created_at,document_id")
                .map_err(storage_error)?;
            let rows = stmt.query_map([], |r| r.get(0)).map_err(storage_error)?;
            rows.collect::<std::result::Result<_, _>>()
                .map_err(storage_error)?
        };
        ids.into_iter()
            .map(|id| self.get_document(db::parse(id)?))
            .collect()
    }

    pub fn import(
        &self,
        engine: &dyn EngineBackend,
        request: ImportRequest<'_>,
        context: &RunContext,
    ) -> Result<ImportOutcome> {
        context.check()?;
        let report =
            engine.convert(request.input, request.format, request.confirmation, context)?;
        if report.status == ImportStatus::Refused
            || (report.status == ImportStatus::NeedsConfirmation && !report.confirmed)
        {
            return Ok(ImportOutcome {
                report,
                document: None,
                render: None,
                job: None,
            });
        }
        let source = report
            .source
            .as_ref()
            .ok_or_else(|| storage_error("accepted import has no source"))?;
        let id = DocumentId::new();
        let snapshot = SourceSnapshot::new(id, source.as_bytes().to_vec())?;
        if report.source_hash.as_ref() != Some(snapshot.source_hash())
            || report.input_hash != ContentHash::of(request.input)
            || report.engine_build_id != engine.build_id()
        {
            return Err(storage_error(
                "import report does not match immutable input",
            ));
        }
        let options = CompileOptions::default();
        let render = engine.compile(&snapshot, &options, &self.artifact_root()?, context)?;
        let svg = self.validate_render(engine, &snapshot, &render)?;
        if !svg && request.format != InputFormat::Ly {
            return Err(compile_error(&render));
        }
        let hash = self.store.put(snapshot.bytes())?;
        self.hit("after-cas")?;
        context.check()?;
        let relative = format!("documents/{id}/score.ly");
        let imports = self.store.path("imports")?;
        storage::ensure_directory(&imports)?;
        let staged = tempfile::Builder::new()
            .prefix("import-")
            .tempdir_in(imports)
            .map_err(storage_error)?;
        atomic_write(&staged.path().join("score.ly"), snapshot.bytes())?;
        let original = match request.format {
            InputFormat::Ly => "input.ly",
            InputFormat::MusicXml => "input.musicxml",
            InputFormat::Midi => "input.mid",
        };
        atomic_write(
            &staged.path().join("original").join(original),
            request.input,
        )?;
        let destination = self.store.path(&format!("documents/{id}"))?;
        storage::ensure_directory(destination.parent().unwrap())?;
        storage::sync_directory(staged.path())?;
        fs::rename(staged.path(), &destination).map_err(storage_error)?;
        storage::sync_directory(destination.parent().unwrap())?;
        self.hit("after-import-files")?;
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        db::record_content(&tx, &hash, snapshot.bytes().len())?;
        let now = db::now();
        tx.execute("INSERT INTO documents(document_id,title,input_format,source_path,initial_hash,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?6)",
            params![id.to_string(),request.title,db::encode(&request.format)?,relative,hash.to_string(),now]).map_err(storage_error)?;
        if svg {
            db::record_render(&tx, id, &render)?;
            db::set_current_render(&tx, id, &render)?;
            let version = db::next_version(
                &tx,
                &db::document(&tx, id)?,
                hash,
                VersionOrigin::Import,
                request.author,
                "Import",
            )?;
            db::publish_version(&tx, &version)?;
        }
        let job = Job {
            job_id: JobId::new(),
            document_id: Some(id),
            kind: "import".into(),
            state: if svg { JobState::Done } else { JobState::Error },
            source_hash: Some(snapshot.source_hash().clone()),
            phase: "finished".into(),
            result: Some(serde_json::to_value(&render).map_err(storage_error)?),
            error: if svg {
                None
            } else {
                Some(compile_error(&render))
            },
        };
        db::save_job(&tx, &job)?;
        tx.commit().map_err(storage_error)?;
        drop(conn);
        Ok(ImportOutcome {
            report,
            document: Some(self.get_document(id)?),
            render: Some(render),
            job: Some(job),
        })
    }

    pub fn begin_edit(&self, request: BeginEditRequest<'_>) -> Result<EditSession> {
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let doc = db::document(&tx, request.document_id)?;
        self.no_intent(&tx, doc.id)?;
        if db::active_edit(&tx, doc.id)?.is_some() {
            return Err(AppError::new(
                ErrorCode::EditBusy,
                "document already has an editing task",
            ));
        }
        self.expect_head(&doc, request.base_version_id)?;
        let snapshot = self.expect_snapshot(&doc, request.expected_source_hash)?;
        let baseline = match doc.head {
            Some(id) => db::version(&tx, id)?.source_hash,
            None => doc.initial_hash.clone(),
        };
        if snapshot.source_hash() != &baseline && !request.adopt_workspace {
            return Err(AppError::new(
                ErrorCode::DirtyWorkspace,
                "explicitly adopt or discard the unknown workspace",
            ));
        }
        let edit = EditSession {
            edit_id: EditId::new(),
            document_id: doc.id,
            owner_view_id: request.view_id,
            base_version_id: doc.head,
            status: EditStatus::Active,
            started_at: db::now(),
            source_path: self
                .store
                .path(&doc.source_path)?
                .to_string_lossy()
                .into_owned(),
            author: request.author.to_owned(),
        };
        db::save_edit(&tx, &edit)?;
        tx.commit().map_err(storage_error)?;
        Ok(edit)
    }

    /// Look up ownership only when the caller already holds the edit capability.
    pub fn get_edit(&self, id: EditId) -> Result<EditSession> {
        db::edit(&*self.db()?, id)
    }

    pub fn resume_edit(
        &self,
        id: EditId,
        view: ViewId,
        base: Option<VersionId>,
        expected: &ContentHash,
    ) -> Result<EditSession> {
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let mut edit = db::edit(&tx, id)?;
        let doc = db::document(&tx, edit.document_id)?;
        self.no_intent(&tx, doc.id)?;
        if edit.status != EditStatus::Interrupted {
            return Err(conflict("only an interrupted edit can be resumed"));
        }
        self.expect_head(&doc, base)?;
        if edit.base_version_id != base {
            return Err(conflict("edit base changed"));
        }
        self.expect_snapshot(&doc, expected)?;
        edit.owner_view_id = view;
        edit.status = EditStatus::Active;
        db::save_edit(&tx, &edit)?;
        tx.execute(
            "DELETE FROM check_receipts WHERE edit_id=?1",
            [id.to_string()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(edit)
    }

    pub fn end_edit(&self, id: EditId) -> Result<()> {
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let mut edit = db::edit(&tx, id)?;
        self.no_intent(&tx, edit.document_id)?;
        match edit.status {
            EditStatus::Active | EditStatus::Interrupted => {
                edit.status = EditStatus::Ended;
                db::save_edit(&tx, &edit)?;
            }
            EditStatus::Ended => {}
            EditStatus::Committed => return Err(conflict("edit has already committed")),
        }
        tx.commit().map_err(storage_error)
    }

    pub fn history(&self, document: DocumentId) -> Result<Vec<Version>> {
        let conn = self.db()?;
        db::document(&conn, document)?;
        let mut stmt = conn
            .prepare("SELECT body FROM versions WHERE document_id=?1 ORDER BY seq")
            .map_err(storage_error)?;
        let rows = stmt
            .query_map([document.to_string()], |r| r.get::<_, String>(0))
            .map_err(storage_error)?;
        rows.map(|row| db::decode(row.map_err(storage_error)?))
            .collect()
    }
    pub fn source_snapshot(&self, document: DocumentId) -> Result<SourceSnapshot> {
        let conn = self.db()?;
        self.snapshot(&db::document(&conn, document)?)
    }

    pub fn version_snapshot(&self, id: VersionId) -> Result<SourceSnapshot> {
        let version = db::version(&*self.db()?, id)?;
        SourceSnapshot::new(version.document_id, self.store.get(&version.source_hash)?)
    }

    fn validate_render(
        &self,
        engine: &dyn EngineBackend,
        snapshot: &SourceSnapshot,
        render: &RenderManifest,
    ) -> Result<bool> {
        if render.source_hash != *snapshot.source_hash()
            || render.engine_build_id != engine.build_id()
        {
            return Err(storage_error(
                "render manifest does not match the checked snapshot/engine",
            ));
        }
        if render.pages.is_empty()
            || render
                .format_errors
                .iter()
                .any(|e| e.format == RenderFormat::Svg)
        {
            return Ok(false);
        }
        for (i, page) in render.pages.iter().enumerate() {
            if page.page as usize != i + 1 || page.artifact.bytes == 0 {
                return Err(storage_error("invalid SVG page manifest"));
            }
            self.validate_asset(&page.artifact)?;
        }
        Ok(true)
    }
    fn validate_asset(&self, asset: &ArtifactRef) -> Result<()> {
        if asset.path.is_empty()
            || asset.path.contains(['\\', ':'])
            || Path::new(&asset.path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(storage_error(
                "artifact reference must be a confined relative path",
            ));
        }
        let path = self.store.path(&format!("artifacts/{}", asset.path))?;
        let bytes = fs::read(path).map_err(storage_error)?;
        if bytes.len() as u64 != asset.bytes || ContentHash::of(&bytes) != asset.sha256 {
            return Err(storage_error("render artifact is missing or damaged"));
        }
        Ok(())
    }
}

fn compile_error(render: &RenderManifest) -> AppError {
    render
        .format_errors
        .iter()
        .find(|e| e.format == RenderFormat::Svg)
        .map(|e| e.error.clone())
        .unwrap_or_else(|| AppError::new(ErrorCode::CompileFailed, "successful SVG required"))
}

#[cfg(test)]
mod tests;

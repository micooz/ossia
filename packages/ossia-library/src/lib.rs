#![doc = include_str!("../README.md")]
mod api;
mod artifacts;
pub use artifacts::render_key;
mod db;
mod documentation;
mod drafts;
pub use documentation::Documentation;
mod lifecycle;
mod music;
mod recovery;
mod source;
mod storage;
mod storage_manager;
#[cfg(test)]
#[path = "../tests/unit/support.rs"]
mod test_support;
mod workflow;

pub use ossia_engine::{CompileOptions, Engine, RunContext};
pub use ossia_types::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    fs::{self, File},
    path::Path,
    sync::Mutex,
};
use storage::{atomic_write, Store};

pub use api::{
    BeginEditRequest, CheckRequest, CommitRequest, EngineBackend, ImportRequest, RestoreRequest,
};

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

fn import_expectation_error(verification: &Verification, music: &MusicReport) -> AppError {
    let notes = &music.performance.notes;
    let pitches = notes
        .iter()
        .take(64)
        .map(|note| note.pitch)
        .collect::<Vec<_>>();
    let mut error = AppError::invalid("import does not match the declared music expectation");
    error.detail = Some(serde_json::json!({
        "verification": verification,
        "performance": {
            "status": music.performance.status,
            "notes": notes.len(),
            "pitches": pitches,
            "truncated": notes.len() > 64,
        },
        "source": {
            "status": music.source.status,
            "noteTokens": music.source.note_tokens,
            "measures": music.source.measures,
        },
    }));
    error
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
        let current_row = conn.query_row("SELECT rowid,body FROM renders WHERE document_id=?1 AND source_hash=?2 ORDER BY rowid DESC LIMIT 1",
            params![id.to_string(),snapshot.source_hash().to_string()],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).optional().map_err(storage_error)?;
        if let Some((rowid, _)) = &current_row {
            conn.execute(
                "UPDATE renders SET last_used_at=?1 WHERE rowid=?2",
                params![db::now_millis(), rowid],
            )
            .map_err(storage_error)?;
        }
        let current = current_row.map(|(_, body)| body);
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

    /// List compact document metadata without loading render or check history.
    pub fn document_page(&self, limit: usize, offset: usize) -> Result<DocumentPage> {
        let conn = self.db()?;
        let total: i64 = conn
            .query_row("SELECT count(*) FROM documents", [], |r| r.get(0))
            .map_err(storage_error)?;
        let total = usize::try_from(total).map_err(storage_error)?;
        let mut statement = conn
            .prepare(
                "SELECT document_id FROM documents ORDER BY created_at,document_id LIMIT ?1 OFFSET ?2",
            )
            .map_err(storage_error)?;
        let ids = statement
            .query_map(
                params![
                    i64::try_from(limit).map_err(storage_error)?,
                    i64::try_from(offset).map_err(storage_error)?
                ],
                |r| r.get::<_, String>(0),
            )
            .map_err(storage_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        let items = ids
            .into_iter()
            .map(|id| {
                let doc = db::document(&conn, db::parse(id)?)?;
                self.document_summary(&conn, &doc)
            })
            .collect::<Result<Vec<_>>>()?;
        let end = offset.saturating_add(items.len());
        Ok(DocumentPage {
            items,
            total,
            offset,
            next_offset: (end < total).then_some(end),
        })
    }

    fn document_summary(&self, conn: &Connection, doc: &db::Document) -> Result<DocumentSummary> {
        let snapshot = self.snapshot(doc)?;
        let dirty = match doc.head {
            Some(head) => db::version(conn, head)?.source_hash != *snapshot.source_hash(),
            None => true,
        };
        let pending = conn
            .query_row(
                "SELECT 1 FROM worktree_intents WHERE document_id=?1",
                [doc.id.to_string()],
                |r| r.get::<_, i32>(0),
            )
            .optional()
            .map_err(storage_error)?
            .is_some();
        let edit_state = if pending {
            DocumentEditState::RecoveryConflict
        } else {
            match db::active_edit(conn, doc.id)?.map(|e| e.status) {
                Some(EditStatus::Active) => DocumentEditState::Active,
                Some(EditStatus::Interrupted) => DocumentEditState::Interrupted,
                _ => DocumentEditState::Idle,
            }
        };
        Ok(DocumentSummary {
            document_id: doc.id,
            title: doc.title.clone(),
            head_version_id: doc.head,
            source_hash: snapshot.source_hash().clone(),
            dirty,
            edit_state,
        })
    }

    pub fn import(
        &self,
        engine: &dyn EngineBackend,
        request: ImportRequest<'_>,
        context: &RunContext,
    ) -> Result<ImportOutcome> {
        self.import_with_expect(engine, request, None, context)
    }

    /// Import and optionally reject the result before publishing a version.
    pub fn import_with_expect(
        &self,
        engine: &dyn EngineBackend,
        request: ImportRequest<'_>,
        expect: Option<&MusicExpectation>,
        context: &RunContext,
    ) -> Result<ImportOutcome> {
        context.check()?;
        if let Some(expect) = expect {
            ossia_score::validate_expectation(expect)?;
        }
        let report =
            engine.convert(request.input, request.format, request.confirmation, context)?;
        if report.status == ImportStatus::Refused
            || (report.status == ImportStatus::NeedsConfirmation && !report.confirmed)
        {
            return Ok(ImportOutcome {
                music: None,
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
        let options = CompileOptions {
            formats: vec![RenderFormat::Svg, RenderFormat::Midi],
            point_and_click: true,
        };
        let render = engine.compile(&snapshot, &options, &self.artifact_root()?, context)?;
        let svg = self.validate_render(engine, &snapshot, &render)?;
        if !svg && request.format != InputFormat::Ly {
            return Err(compile_error(&render));
        }
        let music = self.music_report(&snapshot, &render)?;
        let verification = ossia_score::verify_music(&music, expect);
        if expect.is_some() && verification.status != VerificationStatus::Passed {
            return Err(import_expectation_error(&verification, &music));
        }
        let check_result = CheckResult {
            render: render.clone(),
            music: music.clone(),
            verification,
        };
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
            result: Some(serde_json::to_value(&check_result).map_err(storage_error)?),
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
            music: Some(music),
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

    /// Page through version nodes without loading the whole history.
    pub fn history_page(
        &self,
        document: DocumentId,
        limit: usize,
        offset: usize,
    ) -> Result<VersionPage> {
        let conn = self.db()?;
        db::document(&conn, document)?;
        let total: i64 = conn
            .query_row(
                "SELECT count(*) FROM versions WHERE document_id=?1",
                [document.to_string()],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        let total = usize::try_from(total).map_err(storage_error)?;
        let mut statement = conn
            .prepare(
                "SELECT body FROM versions WHERE document_id=?1 ORDER BY seq LIMIT ?2 OFFSET ?3",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![
                    document.to_string(),
                    i64::try_from(limit).map_err(storage_error)?,
                    i64::try_from(offset).map_err(storage_error)?
                ],
                |r| r.get::<_, String>(0),
            )
            .map_err(storage_error)?;
        let items = rows
            .map(|row| db::decode(row.map_err(storage_error)?))
            .collect::<Result<Vec<Version>>>()?;
        let end = offset.saturating_add(items.len());
        Ok(VersionPage {
            items,
            total,
            offset,
            next_offset: (end < total).then_some(end),
        })
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

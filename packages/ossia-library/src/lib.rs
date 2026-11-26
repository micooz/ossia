#![doc = include_str!("../README.md")]
mod api;
mod artifacts;
mod audio_output;
pub use artifacts::render_key;
mod crop_output;
mod db;
mod document;
mod documentation;
mod drafts;
mod drawing;
mod drawing_output;
mod evidence;
mod initial;
pub use documentation::Documentation;
mod lifecycle;
mod management;
mod music;
mod page_output;
mod recovery;
mod score_settings;
mod source;
mod storage;
mod storage_manager;
#[cfg(test)]
#[path = "../tests/unit/support.rs"]
mod test_support;
mod trash;
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
        self.import_checked(engine, request, expect, None, context)
    }

    /// Import and optionally verify against an immutable reference performance.
    pub fn import_with_reference(
        &self,
        engine: &dyn EngineBackend,
        request: ImportRequest<'_>,
        reference: Option<&ReferenceMusic>,
        context: &RunContext,
    ) -> Result<ImportOutcome> {
        self.import_checked(engine, request, None, reference, context)
    }

    fn import_checked(
        &self,
        engine: &dyn EngineBackend,
        request: ImportRequest<'_>,
        expect: Option<&MusicExpectation>,
        reference: Option<&ReferenceMusic>,
        context: &RunContext,
    ) -> Result<ImportOutcome> {
        context.check()?;
        if expect.is_some() && reference.is_some() {
            return Err(AppError::invalid(
                "expect and reference cannot be used together",
            ));
        }
        if let Some(expect) = expect {
            ossia_score::validate_expectation(expect)?;
        }
        let mut report = engine.convert(request.input, request.format, None, context)?;
        if report.status == ImportStatus::Refused {
            return Ok(ImportOutcome {
                music: None,
                verification: None,
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
        let render = match engine.compile(&snapshot, &options, &self.artifact_root()?, context) {
            Ok(render) => render,
            Err(error) => return Err(import_error(error, &report, None)),
        };
        let svg = match self.validate_render(engine, &snapshot, &render) {
            Ok(svg) => svg,
            Err(error) => return Err(import_error(error, &report, Some(&render))),
        };
        if !svg {
            return Err(import_error(compile_error(&render), &report, Some(&render)));
        }
        report.playback_error = render
            .format_errors
            .iter()
            .find(|error| error.format == RenderFormat::Midi)
            .map(|error| error.error.clone())
            .or_else(|| {
                render
                    .midi
                    .is_none()
                    .then(|| AppError::unsupported("MIDI output is unavailable"))
            });
        if let Some(error) = &report.playback_error {
            if ossia_score::is_jianpu_source(snapshot.source()) {
                return Err(import_error(error.clone(), &report, Some(&render)));
            }
            report.status = ImportStatus::NeedsConfirmation;
            report.confirmation = Some(ContentHash::of(
                &serde_json::to_vec(&(
                    &report.input_hash,
                    &report.engine_build_id,
                    &report.source_hash,
                    &report.losses,
                    error,
                ))
                .map_err(storage_error)?,
            ));
        }
        if let Some(supplied) = request.confirmation {
            if report.confirmation.as_ref() != Some(supplied) {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "import confirmation does not match this input and report",
                ));
            }
            report.confirmed = true;
        }
        if report.status == ImportStatus::NeedsConfirmation && !report.confirmed {
            return Ok(ImportOutcome {
                music: None,
                verification: None,
                report,
                document: None,
                render: Some(render),
                job: None,
            });
        }
        let music = self.music_report(&snapshot, &render)?;
        let verification = match (expect, reference) {
            (Some(expect), None) => ossia_score::verify_music(&music, Some(expect)),
            (None, Some(reference)) => ossia_score::verify_reference(&music, reference),
            (None, None) => ossia_score::verify_music(&music, None),
            (Some(_), Some(_)) => unreachable!(),
        };
        if (expect.is_some() || reference.is_some())
            && verification.status != VerificationStatus::Passed
        {
            return Err(import_expectation_error(&verification, &music));
        }
        let check_result = CheckResult {
            analysis_version: EVIDENCE_ANALYSIS_VERSION,
            render: render.clone(),
            music: music.clone(),
            verification: verification.clone(),
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
        let original = request.format.original_name();
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
        self.cache_check_evidence(id, &check_result)?;
        Ok(ImportOutcome {
            music: Some(music),
            verification: Some(verification),
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
            base_version_id: doc.head,
            status: EditStatus::Active,
            started_at: db::now(),
            author: request.author.to_owned(),
        };
        db::save_edit(&tx, &edit)?;
        tx.commit().map_err(storage_error)?;
        Ok(edit)
    }

    pub fn get_edit(&self, id: EditId) -> Result<EditSession> {
        db::edit(&*self.db()?, id)
    }

    pub fn resume_edit(
        &self,
        id: EditId,
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

fn import_error(
    mut error: AppError,
    report: &ImportReport,
    render: Option<&RenderManifest>,
) -> AppError {
    let mut detail = error.detail.take().unwrap_or_else(|| serde_json::json!({}));
    if !detail.is_object() {
        detail = serde_json::json!({ "cause": detail });
    }
    detail["report"] = serde_json::json!(report);
    if let Some(render) = render {
        detail["formatErrors"] = serde_json::json!(render.format_errors);
    }
    error.detail = Some(detail);
    error
}

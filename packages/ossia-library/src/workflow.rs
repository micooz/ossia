use crate::{db, *};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckParameters {
    edit_id: Option<EditId>,
    #[serde(default)]
    expect: Option<MusicExpectation>,
    #[serde(default)]
    reference: Option<ReferenceMusic>,
    formats: Vec<RenderFormat>,
    point_and_click: bool,
}

impl Library {
    pub fn create_job(
        &self,
        kind: &str,
        document: Option<DocumentId>,
        snapshot: Option<&SourceSnapshot>,
    ) -> Result<Job> {
        if let Some(snapshot) = snapshot {
            if Some(snapshot.document_id()) != document {
                return Err(AppError::new(
                    ErrorCode::InvalidRequest,
                    "job snapshot belongs to another document",
                ));
            }
            self.store.put(snapshot.bytes())?;
        }
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        if let Some(document) = document {
            db::document(&tx, document)?;
        }
        if let Some(snapshot) = snapshot {
            db::record_content(&tx, snapshot.source_hash(), snapshot.bytes().len())?;
        }
        let job = Job {
            job_id: JobId::new(),
            document_id: document,
            kind: kind.to_owned(),
            state: JobState::Queued,
            source_hash: snapshot.map(|s| s.source_hash().clone()),
            phase: "queued".into(),
            result: None,
            error: None,
        };
        db::save_job(&tx, &job)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }
    pub fn get_job(&self, id: JobId) -> Result<Job> {
        db::job(&*self.db()?, id)
    }
    /// Compare-and-set job state; terminal states cannot transition again.
    pub fn transition_job(
        &self,
        id: JobId,
        expected: JobState,
        next: JobState,
        phase: &str,
        result: Option<serde_json::Value>,
        error: Option<AppError>,
    ) -> Result<Job> {
        let valid = matches!(
            (expected, next),
            (
                JobState::Queued,
                JobState::Queued
                    | JobState::Running
                    | JobState::Cancelled
                    | JobState::Error
                    | JobState::Interrupted
            ) | (
                JobState::Running,
                JobState::Running
                    | JobState::Done
                    | JobState::Error
                    | JobState::Cancelled
                    | JobState::Interrupted
            )
        );
        if !valid {
            return Err(conflict("invalid job state transition"));
        }
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let mut job = db::job(&tx, id)?;
        if job.state != expected {
            return Err(conflict("job state changed"));
        }
        job.state = next;
        job.phase = phase.to_owned();
        job.result = result;
        job.error = error;
        db::save_job(&tx, &job)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }
    pub fn job_snapshot(&self, id: JobId) -> Result<Option<SourceSnapshot>> {
        let job = self.get_job(id)?;
        match (job.document_id, job.source_hash) {
            (Some(document), Some(hash)) => {
                self.get_document(document)?;
                Ok(Some(SourceSnapshot::new(document, self.store.get(&hash)?)?))
            }
            _ => Ok(None),
        }
    }

    /// Persist the exact input before P04 schedules execution; credentials stay out of Job.
    pub fn queue_check(&self, request: CheckRequest<'_>) -> Result<Job> {
        if request.expect.is_some() && request.reference.is_some() {
            return Err(AppError::invalid(
                "expect and reference cannot be used together",
            ));
        }
        if let Some(expect) = request.expect {
            ossia_score::validate_expectation(expect)?;
        }
        let snapshot = {
            let conn = self.db()?;
            let doc = db::document(&conn, request.document_id)?;
            self.no_intent(&conn, doc.id)?;
            if let Some(id) = request.edit_id {
                let edit = db::edit(&conn, id)?;
                if edit.document_id != doc.id || edit.status != EditStatus::Active {
                    return Err(conflict("check requires the active document edit"));
                }
                self.expect_head(&doc, edit.base_version_id)?;
            }
            self.expect_snapshot(&doc, request.expected_source_hash)?
        };
        self.store.put(snapshot.bytes())?;
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        db::record_content(&tx, snapshot.source_hash(), snapshot.bytes().len())?;
        let job = Job {
            job_id: JobId::new(),
            document_id: Some(snapshot.document_id()),
            kind: "check".into(),
            state: JobState::Queued,
            source_hash: Some(snapshot.source_hash().clone()),
            phase: "queued".into(),
            result: None,
            error: None,
        };
        db::save_job(&tx, &job)?;
        if let Some(edit) = request.edit_id {
            tx.execute(
                "DELETE FROM check_receipts WHERE edit_id=?1",
                [edit.to_string()],
            )
            .map_err(storage_error)?;
        }
        let parameters = CheckParameters {
            expect: request.expect.cloned(),
            reference: request.reference.cloned(),
            edit_id: request.edit_id,
            formats: request.options.formats.clone(),
            point_and_click: request.options.point_and_click,
        };
        tx.execute(
            "INSERT INTO check_requests(job_id,parameters,edit_id) VALUES (?1,?2,?3)",
            params![
                job.job_id.to_string(),
                db::encode(&parameters)?,
                request.edit_id.map(|e| e.to_string())
            ],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(job)
    }

    pub fn check(
        &self,
        engine: &dyn EngineBackend,
        request: CheckRequest<'_>,
        context: &RunContext,
    ) -> Result<CheckOutcome> {
        context.check()?;
        let job = self.queue_check(request)?;
        self.run_check(engine, job.job_id, context)
    }

    /// Execute a queued snapshot even if the public file has since changed.
    pub fn run_check(
        &self,
        engine: &dyn EngineBackend,
        id: JobId,
        context: &RunContext,
    ) -> Result<CheckOutcome> {
        let parameters: CheckParameters = {
            let conn = self.db()?;
            let body = conn
                .query_row(
                    "SELECT parameters FROM check_requests WHERE job_id=?1",
                    [id.to_string()],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .map_err(storage_error)?
                .ok_or_else(not_found)?;
            db::decode(body)?
        };
        let snapshot = self
            .job_snapshot(id)?
            .ok_or_else(|| storage_error("queued check has no snapshot"))?;
        let options = CompileOptions {
            formats: parameters.formats,
            point_and_click: parameters.point_and_click,
        };
        let job = self.transition_job(
            id,
            JobState::Queued,
            JobState::Running,
            "compile",
            None,
            None,
        )?;
        let compiled: Result<(RenderManifest, bool, MusicReport)> = (|| {
            let render = engine.compile(&snapshot, &options, &self.artifact_root()?, context)?;
            let svg = self.validate_render(engine, &snapshot, &render)?;
            let music = self.music_report(&snapshot, &render)?;
            Ok((render, svg, music))
        })();
        let (render, svg, music) = match compiled {
            Ok(result) => result,
            Err(mut error) => {
                let state = if error.code == ErrorCode::Cancelled {
                    JobState::Cancelled
                } else {
                    JobState::Error
                };
                self.transition_job(
                    job.job_id,
                    JobState::Running,
                    state,
                    "finished",
                    None,
                    Some(error.clone()),
                )?;
                error.detail = Some(serde_json::json!({"jobId":job.job_id,"cause":error.detail}));
                return Err(error);
            }
        };
        let verification = match (&parameters.expect, &parameters.reference) {
            (Some(expect), None) => ossia_score::verify_music(&music, Some(expect)),
            (None, Some(reference)) => ossia_score::verify_reference(&music, reference),
            (None, None) => ossia_score::verify_music(&music, None),
            (Some(_), Some(_)) => unreachable!(),
        };
        let eligible = matches!(
            verification.status,
            VerificationStatus::NotRequested | VerificationStatus::Passed
        );
        let result = CheckResult {
            analysis_version: EVIDENCE_ANALYSIS_VERSION,
            render: render.clone(),
            music,
            verification,
        };
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let doc = db::document(&tx, snapshot.document_id())?;
        let current = self
            .snapshot(&doc)
            .map(|s| s.source_hash() == snapshot.source_hash())
            .unwrap_or(false);
        if svg {
            db::record_render(&tx, doc.id, &render)?;
            if current {
                db::set_current_render(&tx, doc.id, &render)?;
                if let Some(id) = parameters.edit_id {
                    let edit = db::edit(&tx, id)?;
                    let latest: Option<String> = tx.query_row("SELECT job_id FROM check_requests WHERE edit_id=?1 ORDER BY rowid DESC LIMIT 1", [id.to_string()], |r|r.get(0)).optional().map_err(storage_error)?;
                    if eligible
                        && latest.as_deref() == Some(&job.job_id.to_string())
                        && edit.status == EditStatus::Active
                        && edit.base_version_id == doc.head
                    {
                        tx.execute("INSERT INTO check_receipts(edit_id,source_hash,engine_build_id,options_hash,job_id) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(edit_id) DO UPDATE SET source_hash=excluded.source_hash,engine_build_id=excluded.engine_build_id,options_hash=excluded.options_hash,job_id=excluded.job_id",
                            params![id.to_string(),snapshot.source_hash().to_string(),render.engine_build_id,render.options_hash.to_string(),job.job_id.to_string()]).map_err(storage_error)?;
                    }
                }
            }
        }
        let mut job = db::job(&tx, job.job_id)?;
        if job.state != JobState::Running {
            return Err(conflict("check job no longer running"));
        }
        job.state = if svg { JobState::Done } else { JobState::Error };
        job.phase = "finished".into();
        job.result = Some(serde_json::to_value(&result).map_err(storage_error)?);
        job.error = if svg {
            None
        } else {
            Some(compile_error(&render))
        };
        db::save_job(&tx, &job)?;
        tx.commit().map_err(storage_error)?;
        drop(conn);
        self.cache_check_evidence(snapshot.document_id(), &result)?;
        Ok(CheckOutcome {
            job,
            render,
            current,
        })
    }

    pub fn commit(&self, request: CommitRequest<'_>) -> Result<Version> {
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let mut edit = db::edit(&tx, request.edit_id)?;
        if edit.status != EditStatus::Active {
            return Err(conflict("commit requires an active edit"));
        }
        let doc = db::document(&tx, edit.document_id)?;
        self.no_intent(&tx, doc.id)?;
        self.expect_head(&doc, request.base_version_id)?;
        if edit.base_version_id != request.base_version_id {
            return Err(conflict("edit base does not match commit"));
        }
        let snapshot = self.expect_snapshot(&doc, request.expected_source_hash)?;
        let latest: Option<String> = tx
            .query_row(
                "SELECT job_id FROM check_requests WHERE edit_id=?1 ORDER BY rowid DESC LIMIT 1",
                [edit.edit_id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if let Some(latest) = latest {
            let receipt: Option<String> = tx
                .query_row(
                    "SELECT job_id FROM check_receipts WHERE edit_id=?1",
                    [edit.edit_id.to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(storage_error)?;
            if receipt.as_ref() != Some(&latest) {
                return Err(AppError::new(
                    ErrorCode::CompileFailed,
                    "latest explicit check has not passed",
                ));
            }
        }
        if let Some(head) = doc.head {
            let version = db::version(&tx, head)?;
            if version.source_hash == *snapshot.source_hash() {
                edit.status = EditStatus::Committed;
                db::save_edit(&tx, &edit)?;
                tx.commit().map_err(storage_error)?;
                return Ok(version);
            }
        }
        let render: RenderManifest = tx.query_row(
            "SELECT r.body FROM check_receipts c JOIN renders r ON r.document_id=?1 AND r.source_hash=c.source_hash AND r.engine_build_id=c.engine_build_id AND r.options_hash=c.options_hash WHERE c.edit_id=?2 AND c.source_hash=?3",
            params![doc.id.to_string(),edit.edit_id.to_string(),snapshot.source_hash().to_string()],|r|r.get::<_,String>(0))
            .optional().map_err(storage_error)?.map(db::decode).transpose()?
            .ok_or_else(||AppError::new(ErrorCode::CompileFailed,"check this edit's exact source before committing"))?;
        if render.pages.is_empty()
            || render
                .format_errors
                .iter()
                .any(|e| e.format == RenderFormat::Svg)
        {
            return Err(compile_error(&render));
        }
        for page in &render.pages {
            self.validate_asset(&page.artifact)?;
        }
        if self.store.get(snapshot.source_hash())? != snapshot.bytes() {
            return Err(storage_error(
                "checked content differs from source snapshot",
            ));
        }
        self.expect_snapshot(&doc, request.expected_source_hash)?;
        self.hit("after-commit-check")?;
        let origin = if doc.head.is_none() {
            VersionOrigin::Import
        } else {
            VersionOrigin::Edit
        };
        let version = db::next_version(
            &tx,
            &doc,
            snapshot.source_hash().clone(),
            origin,
            &edit.author,
            request.summary,
        )?;
        db::publish_version(&tx, &version)?;
        edit.status = EditStatus::Committed;
        db::save_edit(&tx, &edit)?;
        self.hit("before-version-commit")?;
        tx.commit().map_err(storage_error)?;
        drop(conn);
        self.hit("after-version-commit")?;
        Ok(version)
    }
}

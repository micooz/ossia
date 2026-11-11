use crate::{db, *};
use ossia_score::SourceMap;
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "../tests/unit/recovery.rs"]
mod tests;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Intent {
    document_id: DocumentId,
    old_hash: ContentHash,
    target_hash: ContentHash,
    base_version_id: Option<VersionId>,
    version: Option<Version>,
    end_edit: Option<EditId>,
}

impl Library {
    const MAX_EDITS: usize = 64;
    const MAX_SOURCE_BYTES: usize = 1024 * 1024;

    /// Write an owned edit through the same durable replacement journal as restore.
    pub fn write_source(&self, args: &WriteSourceArgs) -> Result<WriteSourceOutcome> {
        let document_id = {
            let conn = self.db()?;
            db::edit(&conn, args.edit_id)?.document_id
        };
        let snapshot = match &args.change {
            SourceWriteChange::Replace { text } => {
                if text.len() > Self::MAX_SOURCE_BYTES {
                    return Err(AppError::invalid("source exceeds 1 MiB"));
                }
                SourceSnapshot::new(document_id, text.as_bytes().to_vec())?
            }
            SourceWriteChange::Edit { edits } => {
                if edits.is_empty() || edits.len() > Self::MAX_EDITS {
                    return Err(AppError::invalid("source edit count must be 1..64"));
                }
                let current = {
                    let conn = self.db()?;
                    let doc = db::document(&conn, document_id)?;
                    self.expect_snapshot(&doc, &args.expected_source_hash)?
                };
                let mut edited = current.source().to_owned();
                let map = SourceMap::new(current.source());
                let mut ranges = edits
                    .iter()
                    .map(|edit| {
                        let original = map.slice(edit.span)?;
                        if original != edit.expected_text {
                            return Err(AppError::new(
                                ErrorCode::Conflict,
                                "source edit does not match the expected original text",
                            ));
                        }
                        Ok((edit.span.byte_start, edit.span.byte_end, edit))
                    })
                    .collect::<Result<Vec<_>>>()?;
                ranges.sort_by_key(|(start, end, _)| (*start, *end));
                for pair in ranges.windows(2) {
                    let (previous_start, previous_end, _) = pair[0];
                    let (start, _, _) = pair[1];
                    if start < previous_end || start == previous_start {
                        return Err(AppError::invalid(
                            "source edits must not overlap or share a start offset",
                        ));
                    }
                }
                for (start, end, edit) in ranges.into_iter().rev() {
                    edited.replace_range(start..end, &edit.replacement);
                }
                if edited.len() > Self::MAX_SOURCE_BYTES {
                    return Err(AppError::invalid("source exceeds 1 MiB"));
                }
                SourceSnapshot::new(document_id, edited.into_bytes())?
            }
        };
        let intent = {
            let mut conn = self.db()?;
            let tx = conn.transaction().map_err(storage_error)?;
            let doc = db::document(&tx, document_id)?;
            self.no_intent(&tx, doc.id)?;
            let edit = db::edit(&tx, args.edit_id)?;
            if edit.document_id != doc.id || edit.status != EditStatus::Active {
                return Err(AppError::new(
                    ErrorCode::Forbidden,
                    "source write requires the active edit",
                ));
            }
            self.expect_head(&doc, edit.base_version_id)?;
            self.expect_snapshot(&doc, &args.expected_source_hash)?;
            self.store.put(snapshot.bytes())?;
            db::record_content(&tx, snapshot.source_hash(), snapshot.bytes().len())?;
            let intent = Intent {
                document_id: doc.id,
                old_hash: args.expected_source_hash.clone(),
                target_hash: snapshot.source_hash().clone(),
                base_version_id: doc.head,
                version: None,
                end_edit: None,
            };
            save_intent(&tx, &intent)?;
            tx.execute(
                "DELETE FROM check_receipts WHERE edit_id=?1",
                [edit.edit_id.to_string()],
            )
            .map_err(storage_error)?;
            tx.commit().map_err(storage_error)?;
            intent
        };
        self.execute_intent(&intent)?;
        let doc = self.get_document(document_id)?;
        Ok(WriteSourceOutcome {
            document_id: doc.document_id,
            source_hash: doc.source_hash,
            dirty: doc.dirty,
            source_bytes: snapshot.bytes().len(),
        })
    }

    pub fn discard(
        &self,
        document: DocumentId,
        edit_id: Option<EditId>,
        base: Option<VersionId>,
        expected: &ContentHash,
    ) -> Result<()> {
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let doc = db::document(&tx, document)?;
        self.no_intent(&tx, document)?;
        self.expect_head(&doc, base)?;
        self.expect_snapshot(&doc, expected)?;
        match (db::active_edit(&tx, document)?, edit_id) {
            (Some(edit), Some(id)) if edit.edit_id == id => {}
            (None, None) => {}
            _ => {
                return Err(AppError::new(
                    ErrorCode::EditBusy,
                    "discard requires the document's edit capability",
                ))
            }
        }
        let head = doc.head.ok_or_else(|| {
            AppError::new(
                ErrorCode::DirtyWorkspace,
                "no committed snapshot to discard to",
            )
        })?;
        let hash = db::version(&tx, head)?.source_hash;
        self.store.get(&hash)?;
        if hash == *expected {
            if let Some(id) = edit_id {
                let mut edit = db::edit(&tx, id)?;
                edit.status = EditStatus::Ended;
                db::save_edit(&tx, &edit)?;
            }
            tx.commit().map_err(storage_error)?;
            return Ok(());
        }
        let intent = Intent {
            document_id: document,
            old_hash: expected.clone(),
            target_hash: hash,
            base_version_id: base,
            version: None,
            end_edit: edit_id,
        };
        save_intent(&tx, &intent)?;
        tx.commit().map_err(storage_error)?;
        drop(conn);
        self.execute_intent(&intent)
    }

    pub fn restore(
        &self,
        engine: &dyn EngineBackend,
        request: RestoreRequest<'_>,
        context: &RunContext,
    ) -> Result<Version> {
        context.check()?;
        let snapshot = {
            let conn = self.db()?;
            let target = db::version(&conn, request.target_version_id)?;
            let doc = db::document(&conn, target.document_id)?;
            self.restore_guard(
                &conn,
                &doc,
                request.base_version_id,
                request.expected_source_hash,
            )?;
            SourceSnapshot::new(doc.id, self.store.get(&target.source_hash)?)?
        };
        let render = engine.compile(
            &snapshot,
            &CompileOptions::default(),
            &self.artifact_root()?,
            context,
        )?;
        if !self.validate_render(engine, &snapshot, &render)? {
            return Err(compile_error(&render));
        }
        context.check()?;
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let doc = db::document(&tx, snapshot.document_id())?;
        self.restore_guard(
            &tx,
            &doc,
            request.base_version_id,
            request.expected_source_hash,
        )?;
        db::record_render(&tx, doc.id, &render)?;
        let version = db::next_version(
            &tx,
            &doc,
            snapshot.source_hash().clone(),
            VersionOrigin::Restore,
            request.author,
            request.summary,
        )?;
        if snapshot.source_hash() == request.expected_source_hash {
            db::set_current_render(&tx, doc.id, &render)?;
            db::publish_version(&tx, &version)?;
            tx.commit().map_err(storage_error)?;
            return Ok(version);
        }
        let intent = Intent {
            document_id: doc.id,
            old_hash: request.expected_source_hash.clone(),
            target_hash: snapshot.source_hash().clone(),
            base_version_id: doc.head,
            version: Some(version.clone()),
            end_edit: None,
        };
        save_intent(&tx, &intent)?;
        tx.commit().map_err(storage_error)?;
        drop(conn);
        self.execute_intent(&intent)?;
        Ok(version)
    }

    fn restore_guard(
        &self,
        conn: &Connection,
        doc: &db::Document,
        base: Option<VersionId>,
        expected: &ContentHash,
    ) -> Result<()> {
        self.no_intent(conn, doc.id)?;
        if db::active_edit(conn, doc.id)?.is_some() {
            return Err(AppError::new(
                ErrorCode::EditBusy,
                "end or recover the current edit before restoring",
            ));
        }
        self.expect_head(doc, base)?;
        let snapshot = self.expect_snapshot(doc, expected)?;
        if doc
            .head
            .map(|id| db::version(conn, id))
            .transpose()?
            .map(|v| v.source_hash)
            .as_ref()
            != Some(snapshot.source_hash())
        {
            return Err(AppError::new(
                ErrorCode::DirtyWorkspace,
                "restore requires a clean workspace",
            ));
        }
        Ok(())
    }

    fn execute_intent(&self, intent: &Intent) -> Result<()> {
        self.hit("after-intent")?;
        let bytes = self.store.get(&intent.target_hash)?;
        let path = {
            let conn = self.db()?;
            let doc = db::document(&conn, intent.document_id)?;
            if self.expect_head(&doc, intent.base_version_id).is_err()
                || self.expect_snapshot(&doc, &intent.old_hash).is_err()
            {
                mark_conflict(&conn, intent.document_id)?;
                return Err(conflict(
                    "workspace changed before replacement; file preserved",
                ));
            }
            self.store.path(&doc.source_path)?
        };
        atomic_write(&path, &bytes)?;
        self.hit("after-replace")?;
        self.finalize_intent(intent)
    }

    fn finalize_intent(&self, intent: &Intent) -> Result<()> {
        // Verify durable content before allowing recovery to advance the head.
        self.store.get(&intent.target_hash)?;
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let doc = db::document(&tx, intent.document_id)?;
        if doc.head != intent.base_version_id
            || self.expect_snapshot(&doc, &intent.target_hash).is_err()
        {
            mark_conflict(&tx, doc.id)?;
            tx.commit().map_err(storage_error)?;
            return Err(conflict("workspace operation conflicted; file preserved"));
        }
        if let Some(version) = &intent.version {
            db::publish_version(&tx, version)?;
        }
        tx.execute("UPDATE documents SET last_render=(SELECT body FROM renders WHERE document_id=?1 AND source_hash=?2 ORDER BY rowid DESC LIMIT 1) WHERE document_id=?1",
            params![doc.id.to_string(),intent.target_hash.to_string()]).map_err(storage_error)?;
        if let Some(id) = intent.end_edit {
            let mut edit = db::edit(&tx, id)?;
            edit.status = EditStatus::Ended;
            db::save_edit(&tx, &edit)?;
        }
        tx.execute(
            "DELETE FROM worktree_intents WHERE document_id=?1",
            [doc.id.to_string()],
        )
        .map_err(storage_error)?;
        self.hit("before-intent-finalize")?;
        tx.commit().map_err(storage_error)?;
        drop(conn);
        self.hit("after-intent-finalize")
    }

    pub(crate) fn recover(&self) -> Result<()> {
        {
            let mut conn = self.db()?;
            let tx = conn.transaction().map_err(storage_error)?;
            let edits: Vec<String> = {
                let mut stmt = tx
                    .prepare("SELECT body FROM edits WHERE status='active'")
                    .map_err(storage_error)?;
                let rows = stmt.query_map([], |r| r.get(0)).map_err(storage_error)?;
                rows.collect::<std::result::Result<_, _>>()
                    .map_err(storage_error)?
            };
            for body in edits {
                let mut edit: EditSession = db::decode(body)?;
                edit.status = EditStatus::Interrupted;
                db::save_edit(&tx, &edit)?;
            }
            let jobs: Vec<String> = {
                let mut stmt = tx
                    .prepare("SELECT body FROM jobs WHERE state IN ('queued','running')")
                    .map_err(storage_error)?;
                let rows = stmt.query_map([], |r| r.get(0)).map_err(storage_error)?;
                rows.collect::<std::result::Result<_, _>>()
                    .map_err(storage_error)?
            };
            for body in jobs {
                let mut job: Job = db::decode(body)?;
                job.state = JobState::Interrupted;
                job.phase = "interrupted".into();
                job.result = None;
                job.error = None;
                db::save_job(&tx, &job)?;
            }
            tx.execute("DELETE FROM check_receipts", [])
                .map_err(storage_error)?;
            tx.commit().map_err(storage_error)?;
        }
        let intents: Vec<Intent> = {
            let conn = self.db()?;
            let mut stmt = conn
                .prepare("SELECT body FROM worktree_intents WHERE status='pending'")
                .map_err(storage_error)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(storage_error)?;
            rows.map(|row| db::decode(row.map_err(storage_error)?))
                .collect::<Result<_>>()?
        };
        for intent in intents {
            let conn = self.db()?;
            let doc = db::document(&conn, intent.document_id)?;
            let hash = self
                .store
                .read_source(&doc.source_path)
                .map(|bytes| ContentHash::of(&bytes));
            if doc.head != intent.base_version_id {
                mark_conflict(&conn, doc.id)?;
            } else if hash.as_ref().ok() == Some(&intent.old_hash) {
                conn.execute(
                    "DELETE FROM worktree_intents WHERE document_id=?1",
                    [doc.id.to_string()],
                )
                .map_err(storage_error)?;
            } else if hash.as_ref().ok() == Some(&intent.target_hash) {
                drop(conn);
                self.finalize_intent(&intent)?;
            } else {
                mark_conflict(&conn, doc.id)?;
            }
        }
        Ok(())
    }

    /// Acknowledge a conflict without changing the public file or advancing history.
    pub fn resolve_recovery_conflict(
        &self,
        document: DocumentId,
        expected: &ContentHash,
    ) -> Result<()> {
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let doc = db::document(&tx, document)?;
        self.expect_snapshot(&doc, expected)?;
        let count = tx
            .execute(
                "DELETE FROM worktree_intents WHERE document_id=?1 AND status='conflict'",
                [document.to_string()],
            )
            .map_err(storage_error)?;
        if count != 1 {
            return Err(conflict("no acknowledged recovery conflict"));
        }
        tx.commit().map_err(storage_error)
    }
}

fn save_intent(conn: &Connection, intent: &Intent) -> Result<()> {
    conn.execute(
        "INSERT INTO worktree_intents(document_id,status,body) VALUES (?1,'pending',?2)",
        params![intent.document_id.to_string(), db::encode(intent)?],
    )
    .map_err(storage_error)?;
    Ok(())
}
fn mark_conflict(conn: &Connection, document: DocumentId) -> Result<()> {
    conn.execute(
        "UPDATE worktree_intents SET status='conflict' WHERE document_id=?1",
        [document.to_string()],
    )
    .map_err(storage_error)?;
    Ok(())
}

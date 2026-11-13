use crate::{db, storage, *};
use serde_json::Value;

impl Library {
    /// Return whether either an active or trashed document owns this ID.
    pub fn document_exists(&self, document: DocumentId) -> Result<bool> {
        let exists: Option<i32> = self
            .db()?
            .query_row(
                "SELECT 1 FROM documents WHERE document_id=?1",
                [document.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        Ok(exists.is_some())
    }

    /// Return whether a document currently resides in the recoverable trash.
    pub fn is_trashed(&self, document: DocumentId) -> Result<bool> {
        let conn = self.db()?;
        let active: Option<i32> = conn
            .query_row(
                "SELECT 1 FROM documents WHERE document_id=?1 AND deleted_at IS NULL",
                [document.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if active.is_some() {
            return Ok(false);
        }
        let trashed: Option<i32> = conn
            .query_row(
                "SELECT 1 FROM documents WHERE document_id=?1 AND deleted_at IS NOT NULL",
                [document.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        Ok(trashed.is_some())
    }

    /// Move an active score and its current source into the recoverable trash.
    pub fn trash(
        &self,
        document: DocumentId,
        expected: &ContentHash,
    ) -> Result<TrashedDocumentSummary> {
        let snapshot = {
            let conn = self.db()?;
            self.snapshot(&db::document(&conn, document)?)?
        };
        if snapshot.source_hash() != expected {
            return Err(conflict("public source changed"));
        }
        let hash = self.store.put(snapshot.bytes())?;
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let doc = db::document(&tx, document)?;
        if self.snapshot(&doc)?.source_hash() != expected {
            return Err(conflict("public source changed"));
        }
        self.no_intent(&tx, document)?;
        db::record_content(&tx, &hash, snapshot.bytes().len())?;
        let deleted_at = db::now();
        tx.execute(
            "UPDATE documents SET deleted_at=?1,trash_source_hash=?2,updated_at=?1 WHERE document_id=?3",
            params![deleted_at, hash.to_string(), document.to_string()],
        )
        .map_err(storage_error)?;
        let edits: Vec<String> = {
            let mut statement = tx
                .prepare("SELECT body FROM edits WHERE document_id=?1 AND status IN ('active','interrupted')")
                .map_err(storage_error)?;
            let values = statement
                .query_map([document.to_string()], |row| row.get(0))
                .map_err(storage_error)?
                .collect::<std::result::Result<_, _>>()
                .map_err(storage_error)?;
            values
        };
        for body in edits {
            let mut edit: EditSession = db::decode(body)?;
            edit.status = EditStatus::Ended;
            db::save_edit(&tx, &edit)?;
        }
        tx.commit().map_err(storage_error)?;
        Ok(TrashedDocumentSummary {
            document_id: document,
            title: doc.title,
            deleted_at,
            source_hash: hash,
            head_version_id: doc.head,
        })
    }

    /// Page through trashed scores without loading mutable source files or render history.
    pub fn trash_page(&self, limit: usize, offset: usize) -> Result<TrashPage> {
        let conn = self.db()?;
        let total: i64 = conn
            .query_row(
                "SELECT count(*) FROM documents WHERE deleted_at IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        let total = usize::try_from(total).map_err(storage_error)?;
        let mut statement = conn
            .prepare(
                "SELECT document_id,title,deleted_at,trash_source_hash,head_version_id
                 FROM documents WHERE deleted_at IS NOT NULL
                 ORDER BY deleted_at,document_id LIMIT ?1 OFFSET ?2",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![
                    i64::try_from(limit).map_err(storage_error)?,
                    i64::try_from(offset).map_err(storage_error)?
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .map_err(storage_error)?;
        let items = rows
            .map(|row| {
                let (document, title, deleted_at, source_hash, head) =
                    row.map_err(storage_error)?;
                Ok(TrashedDocumentSummary {
                    document_id: db::parse(document)?,
                    title,
                    deleted_at,
                    source_hash: source_hash.try_into().map_err(storage_error)?,
                    head_version_id: head.map(db::parse).transpose()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let end = offset.saturating_add(items.len());
        Ok(TrashPage {
            items,
            total,
            offset,
            next_offset: (end < total).then_some(end),
        })
    }

    /// Restore the original document identity, source and complete version history.
    pub fn untrash(&self, document: DocumentId) -> Result<DocumentSummary> {
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        db::trashed_document(&tx, document)?;
        tx.execute(
            "UPDATE documents SET deleted_at=NULL,trash_source_hash=NULL,updated_at=?1 WHERE document_id=?2",
            params![db::now(), document.to_string()],
        )
        .map_err(storage_error)?;
        let doc = db::document(&tx, document)?;
        let summary = self.document_summary(&tx, &doc)?;
        tx.commit().map_err(storage_error)?;
        Ok(summary)
    }

    /// Permanently remove a trashed score and all records owned by it.
    pub fn purge(&self, document: DocumentId) -> Result<PurgedDocument> {
        let mut conn = self.db()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let doc = db::trashed_document(&tx, document)?;
        let busy_edit: Option<String> = tx
            .query_row(
                "SELECT edit_id FROM edits WHERE document_id=?1 AND status IN ('active','interrupted') LIMIT 1",
                [document.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if busy_edit.is_some() {
            return Err(conflict("trashed document has an active edit"));
        }
        let busy_job: Option<String> = tx
            .query_row(
                "SELECT job_id FROM jobs WHERE document_id=?1 AND state IN ('queued','running') LIMIT 1",
                [document.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if busy_job.is_some() {
            return Err(conflict("trashed document has an active job"));
        }
        tx.execute(
            "INSERT INTO purge_intents(document_id,source_path,created_at) VALUES (?1,?2,?3)",
            params![document.to_string(), doc.source_path.clone(), db::now()],
        )
        .map_err(storage_error)?;
        redact_import_requests(&tx, document)?;
        tx.execute(
            "DELETE FROM check_receipts WHERE edit_id IN (SELECT edit_id FROM edits WHERE document_id=?1) OR job_id IN (SELECT job_id FROM jobs WHERE document_id=?1)",
            [document.to_string()],
        )
        .map_err(storage_error)?;
        tx.execute(
            "DELETE FROM check_requests WHERE edit_id IN (SELECT edit_id FROM edits WHERE document_id=?1) OR job_id IN (SELECT job_id FROM jobs WHERE document_id=?1)",
            [document.to_string()],
        )
        .map_err(storage_error)?;
        tx.execute(
            "DELETE FROM jobs WHERE document_id=?1",
            [document.to_string()],
        )
        .map_err(storage_error)?;
        tx.execute(
            "DELETE FROM worktree_intents WHERE document_id=?1",
            [document.to_string()],
        )
        .map_err(storage_error)?;
        tx.execute(
            "DELETE FROM renders WHERE document_id=?1",
            [document.to_string()],
        )
        .map_err(storage_error)?;
        tx.execute(
            "DELETE FROM evidence_runs WHERE document_id=?1",
            [document.to_string()],
        )
        .map_err(storage_error)?;
        tx.execute(
            "DELETE FROM edits WHERE document_id=?1",
            [document.to_string()],
        )
        .map_err(storage_error)?;
        tx.execute(
            "UPDATE documents SET head_version_id=NULL WHERE document_id=?1",
            [document.to_string()],
        )
        .map_err(storage_error)?;
        let versions: Vec<String> = {
            let mut statement = tx
                .prepare("SELECT version_id FROM versions WHERE document_id=?1 ORDER BY seq DESC")
                .map_err(storage_error)?;
            let values = statement
                .query_map([document.to_string()], |row| row.get(0))
                .map_err(storage_error)?
                .collect::<std::result::Result<_, _>>()
                .map_err(storage_error)?;
            values
        };
        for version in versions {
            tx.execute("DELETE FROM versions WHERE version_id=?1", [version])
                .map_err(storage_error)?;
        }
        tx.execute(
            "DELETE FROM documents WHERE document_id=?1",
            [document.to_string()],
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        drop(conn);
        self.finish_purge(document, &doc.source_path)?;
        Ok(PurgedDocument {
            document_id: document,
        })
    }

    pub(crate) fn recover_purges(&self) -> Result<()> {
        let intents: Vec<(String, String)> = {
            let conn = self.db()?;
            let mut statement = conn
                .prepare("SELECT document_id,source_path FROM purge_intents ORDER BY created_at")
                .map_err(storage_error)?;
            let values = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(storage_error)?
                .collect::<std::result::Result<_, _>>()
                .map_err(storage_error)?;
            values
        };
        for (document, source_path) in intents {
            self.finish_purge(db::parse(document)?, &source_path)?;
        }
        Ok(())
    }

    fn finish_purge(&self, document: DocumentId, source_path: &str) -> Result<()> {
        let document_root = self.store.path(&format!("documents/{document}"))?;
        if document_root.exists() {
            fs::remove_dir_all(&document_root).map_err(storage_error)?;
            storage::sync_directory(document_root.parent().expect("document parent"))?;
        }
        self.db()?
            .execute(
                "DELETE FROM purge_intents WHERE document_id=?1 AND source_path=?2",
                params![document.to_string(), source_path],
            )
            .map_err(storage_error)?;
        Ok(())
    }
}

fn redact_import_requests(conn: &Connection, document: DocumentId) -> Result<()> {
    let rows: Vec<(String, String)> = {
        let mut statement = conn
            .prepare(
                "SELECT job_id,body FROM jobs WHERE kind='import-request' AND instr(body,?1)>0",
            )
            .map_err(storage_error)?;
        let values = statement
            .query_map([document.to_string()], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(storage_error)?
            .collect::<std::result::Result<_, _>>()
            .map_err(storage_error)?;
        values
    };
    for (_job_id, body) in rows {
        let mut job: Job = db::decode(body)?;
        if let Some(result) = job.result.as_mut().and_then(Value::as_object_mut) {
            result.insert("document".into(), Value::Null);
            result.insert("render".into(), Value::Null);
            if let Some(child) = result.get_mut("job").and_then(Value::as_object_mut) {
                child.insert("result".into(), Value::Null);
            }
        }
        db::save_job(conn, &job)?;
    }
    Ok(())
}

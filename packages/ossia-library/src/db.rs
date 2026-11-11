use crate::{conflict, not_found, storage_error, Result};
use ossia_types::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::str::FromStr;

#[cfg(test)]
#[path = "../tests/unit/db.rs"]
mod tests;

pub(crate) fn encode(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(storage_error)
}
pub(crate) fn decode<T: DeserializeOwned>(value: String) -> Result<T> {
    serde_json::from_str(&value).map_err(storage_error)
}
pub(crate) fn parse<T: FromStr>(value: String) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    value.parse().map_err(storage_error)
}
pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub(crate) fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) struct Document {
    pub id: DocumentId,
    pub title: String,
    pub source_path: String,
    pub initial_hash: ContentHash,
    pub head: Option<VersionId>,
}

pub(crate) fn document(conn: &Connection, id: DocumentId) -> Result<Document> {
    let row = conn.query_row(
        "SELECT title,source_path,initial_hash,head_version_id FROM documents WHERE document_id=?1",
        [id.to_string()],
        |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,Option<String>>(3)?)),
    ).optional().map_err(storage_error)?.ok_or_else(not_found)?;
    Ok(Document {
        id,
        title: row.0,
        source_path: row.1,
        initial_hash: row.2.try_into().map_err(storage_error)?,
        head: row.3.map(parse).transpose()?,
    })
}

pub(crate) fn version(conn: &Connection, id: VersionId) -> Result<Version> {
    let body = conn
        .query_row(
            "SELECT body FROM versions WHERE version_id=?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or_else(not_found)?;
    decode(body)
}

pub(crate) fn edit(conn: &Connection, id: EditId) -> Result<EditSession> {
    let body = conn
        .query_row(
            "SELECT body FROM edits WHERE edit_id=?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or_else(not_found)?;
    decode(body)
}

pub(crate) fn active_edit(conn: &Connection, id: DocumentId) -> Result<Option<EditSession>> {
    conn.query_row(
        "SELECT body FROM edits WHERE document_id=?1 AND status IN ('active','interrupted')",
        [id.to_string()],
        |r| r.get(0),
    )
    .optional()
    .map_err(storage_error)?
    .map(decode)
    .transpose()
}

pub(crate) fn save_edit(conn: &Connection, edit: &EditSession) -> Result<()> {
    let status = serde_json::to_value(edit.status).map_err(storage_error)?;
    conn.execute("INSERT INTO edits(edit_id,document_id,status,body) VALUES (?1,?2,?3,?4) ON CONFLICT(edit_id) DO UPDATE SET status=excluded.status,body=excluded.body",
        params![edit.edit_id.to_string(),edit.document_id.to_string(),status.as_str(),encode(edit)?])
        .map_err(storage_error)?;
    Ok(())
}

pub(crate) fn next_version(
    conn: &Connection,
    doc: &Document,
    hash: ContentHash,
    origin: VersionOrigin,
    author: &str,
    summary: &str,
) -> Result<Version> {
    Ok(Version {
        version_id: VersionId::new(),
        document_id: doc.id,
        seq: match doc.head {
            Some(id) => version(conn, id)?.seq + 1,
            None => 1,
        },
        parent_version_id: doc.head,
        source_hash: hash,
        origin,
        author: author.to_owned(),
        summary: summary.to_owned(),
        created_at: now(),
    })
}

pub(crate) fn publish_version(conn: &Connection, value: &Version) -> Result<()> {
    let doc = document(conn, value.document_id)?;
    if doc.head != value.parent_version_id {
        return Err(conflict("head changed"));
    }
    conn.execute("INSERT INTO versions(version_id,document_id,seq,parent_version_id,source_hash,body) VALUES (?1,?2,?3,?4,?5,?6)",
        params![value.version_id.to_string(),value.document_id.to_string(),i64::try_from(value.seq).map_err(storage_error)?,value.parent_version_id.map(|v|v.to_string()),value.source_hash.to_string(),encode(value)?])
        .map_err(storage_error)?;
    conn.execute(
        "UPDATE documents SET head_version_id=?1,updated_at=?2 WHERE document_id=?3",
        params![
            value.version_id.to_string(),
            now(),
            value.document_id.to_string()
        ],
    )
    .map_err(storage_error)?;
    Ok(())
}

pub(crate) fn record_content(conn: &Connection, hash: &ContentHash, bytes: usize) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO contents(hash,bytes) VALUES (?1,?2)",
        params![
            hash.to_string(),
            i64::try_from(bytes).map_err(storage_error)?
        ],
    )
    .map_err(storage_error)?;
    Ok(())
}

pub(crate) fn record_render(
    conn: &Connection,
    doc: DocumentId,
    render: &RenderManifest,
) -> Result<()> {
    conn.execute("INSERT INTO renders(document_id,source_hash,engine_build_id,options_hash,body,created_at,last_used_at) VALUES (?1,?2,?3,?4,?5,?6,?6) ON CONFLICT(document_id,source_hash,engine_build_id,options_hash) DO UPDATE SET body=excluded.body,last_used_at=excluded.last_used_at",
        params![doc.to_string(),render.source_hash.to_string(),render.engine_build_id,render.options_hash.to_string(),encode(render)?,now_millis()]).map_err(storage_error)?;
    Ok(())
}

pub(crate) fn job(conn: &Connection, id: JobId) -> Result<Job> {
    let body = conn
        .query_row(
            "SELECT body FROM jobs WHERE job_id=?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or_else(not_found)?;
    decode(body)
}

pub(crate) fn save_job(conn: &Connection, value: &Job) -> Result<()> {
    let state = serde_json::to_value(value.state).map_err(storage_error)?;
    conn.execute("INSERT INTO jobs(job_id,document_id,source_hash,state,body,kind,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?7) ON CONFLICT(job_id) DO UPDATE SET state=excluded.state,body=excluded.body,updated_at=excluded.updated_at",
        params![value.job_id.to_string(),value.document_id.map(|v|v.to_string()),value.source_hash.as_ref().map(|v|v.to_string()),state.as_str(),encode(value)?,value.kind,now_millis()]).map_err(storage_error)?;
    Ok(())
}

pub(crate) fn set_current_render(
    conn: &Connection,
    doc: DocumentId,
    render: &RenderManifest,
) -> Result<()> {
    conn.execute(
        "UPDATE documents SET last_render=?1 WHERE document_id=?2",
        params![encode(render)?, doc.to_string()],
    )
    .map_err(storage_error)?;
    Ok(())
}

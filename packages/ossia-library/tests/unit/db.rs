use crate::storage;
use crate::*;
use rusqlite::Connection;
use std::{fs, process::Command};

#[test]
fn initialization_schema_and_writer_lock_are_isolated() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("db.sqlite"), b"unrelated bytes").unwrap();
    let library = Library::open(root.path()).unwrap();
    assert!(root.path().join(storage::DATABASE).is_file());
    assert_eq!(
        fs::read(root.path().join("db.sqlite")).unwrap(),
        b"unrelated bytes"
    );
    drop(library);
    let other = tempfile::tempdir().unwrap();
    let library = Library::open(other.path()).unwrap();
    assert_eq!(
        Library::open(other.path()).err().unwrap().code,
        ErrorCode::LibraryBusy
    );
    let conn = library.db().unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert_eq!(
        conn.pragma_query_value(None, "foreign_keys", |r| r.get::<_, i32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.pragma_query_value(None, "synchronous", |r| r.get::<_, i32>(0))
            .unwrap(),
        2
    );
    assert!(rusqlite::version_number() >= 3_051_003);
    drop(conn);
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "test_support::process_fixture", "--ignored"])
        .env("OSSIA_LIBRARY_TEST_MODE", "lock")
        .env("OSSIA_LIBRARY_TEST_HOME", other.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    drop(library);
    let conn = Connection::open(other.path().join(storage::DATABASE)).unwrap();
    conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    drop(conn);
    assert_eq!(
        Library::open(other.path()).err().unwrap().code,
        ErrorCode::RuntimeIncompatible
    );
}

#[test]
fn schema_one_migration_preserves_documents_and_adds_storage_timestamps() {
    let root = tempfile::tempdir().unwrap();
    let document = {
        let library = Library::open(root.path()).unwrap();
        let engine = crate::test_support::TestEngine::default();
        library
            .import(
                &engine,
                ImportRequest {
                    title: "migration",
                    input: b"A c'1",
                    format: InputFormat::Ly,
                    confirmation: None,
                    author: "test",
                },
                &RunContext::default(),
            )
            .unwrap()
            .document
            .unwrap()
    };
    {
        let conn = Connection::open(root.path().join(storage::DATABASE)).unwrap();
        conn.execute_batch(
            "ALTER TABLE jobs DROP COLUMN updated_at;
             ALTER TABLE jobs DROP COLUMN created_at;
             ALTER TABLE renders DROP COLUMN last_used_at;
             ALTER TABLE renders DROP COLUMN created_at;
             PRAGMA user_version=1;",
        )
        .unwrap();
    }
    let library = Library::open(root.path()).unwrap();
    let migrated = library.get_document(document.document_id).unwrap();
    assert_eq!(migrated.title, "migration");
    assert!(migrated.last_successful_render.is_some());
    let conn = library.db().unwrap();
    let evidence_table: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='evidence_runs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(evidence_table, 1);
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap(),
        SCHEMA_VERSION
    );
    for (table, column) in [
        ("jobs", "created_at"),
        ("jobs", "updated_at"),
        ("renders", "created_at"),
        ("renders", "last_used_at"),
    ] {
        let timestamp: i64 = conn
            .query_row(
                &format!("SELECT {column} FROM {table} LIMIT 1"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(timestamp > 0, "{table}.{column}");
    }
}

#[test]
fn schema_two_migration_adds_the_evidence_cache() {
    let root = tempfile::tempdir().unwrap();
    drop(Library::open(root.path()).unwrap());
    {
        let conn = Connection::open(root.path().join(storage::DATABASE)).unwrap();
        conn.execute_batch(
            "DROP TABLE evidence_runs;
             PRAGMA user_version=2;",
        )
        .unwrap();
    }
    let library = Library::open(root.path()).unwrap();
    let table: i32 = library
        .db()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='evidence_runs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table, 1);
    let version: u32 = library
        .db()
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}

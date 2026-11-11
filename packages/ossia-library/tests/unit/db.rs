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

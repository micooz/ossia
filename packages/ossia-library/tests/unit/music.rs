use crate::{test_support::Fixture, *};

#[test]
fn latest_explicit_check_wins_and_preview_cannot_authorize_commit() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash = f.write("changed c'1");
    f.check(&edit, &hash);
    let expectation = MusicExpectation {
        note_count: Some(8),
        tracks: vec![],
    };
    let old = f
        .library
        .queue_check(CheckRequest {
            expect: None,
            document_id: f.doc.document_id,
            edit_id: Some(edit.edit_id),
            expected_source_hash: &hash,
            options: &CompileOptions::default(),
        })
        .unwrap();
    let latest = f
        .library
        .queue_check(CheckRequest {
            expect: Some(&expectation),
            document_id: f.doc.document_id,
            edit_id: Some(edit.edit_id),
            expected_source_hash: &hash,
            options: &CompileOptions::default(),
        })
        .unwrap();
    assert!(f.commit(&edit, &hash).is_err());
    f.library
        .run_check(&f.engine, latest.job_id, &RunContext::default())
        .unwrap();
    f.library
        .run_check(&f.engine, old.job_id, &RunContext::default())
        .unwrap();
    let result: CheckResult =
        serde_json::from_value(f.library.get_job(latest.job_id).unwrap().result.unwrap()).unwrap();
    assert_eq!(result.verification.status, VerificationStatus::Unavailable);
    f.library
        .check(
            &f.engine,
            CheckRequest {
                expect: None,
                document_id: f.doc.document_id,
                edit_id: None,
                expected_source_hash: &hash,
                options: &CompileOptions::default(),
            },
            &RunContext::default(),
        )
        .unwrap();
    assert!(f.commit(&edit, &hash).is_err());
    f.check(&edit, &hash);
    f.commit(&edit, &hash).unwrap();
}

#[test]
fn existing_schema_adds_check_binding_without_trusting_old_receipts() {
    let root = tempfile::tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();
    drop(library);
    let conn = Connection::open(root.path().join("library.sqlite")).unwrap();
    conn.execute_batch("ALTER TABLE check_receipts DROP COLUMN job_id; ALTER TABLE check_requests DROP COLUMN edit_id; PRAGMA user_version=1;").unwrap();
    drop(conn);
    let library = Library::open(root.path()).unwrap();
    assert_eq!(
        library
            .db()
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert!(library.list_documents().unwrap().is_empty());
}

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

#[test]
fn source_writes_require_owned_edit_hash_and_recover_after_replacement() {
    let f = Fixture::new();
    let edit = f.begin();
    let mut args = WriteSourceArgs {
        document_id: f.doc.document_id,
        view_id: edit.owner_view_id,
        edit_id: edit.edit_id,
        expected_source_hash: f.doc.source_hash.clone(),
        text: "new c'1".into(),
    };
    args.view_id = ViewId::new();
    assert_eq!(
        f.library.write_source(&args).unwrap_err().code,
        ErrorCode::Forbidden
    );
    args.view_id = edit.owner_view_id;
    args.expected_source_hash = ContentHash::of(b"stale");
    assert_eq!(
        f.library.write_source(&args).unwrap_err().code,
        ErrorCode::Conflict
    );
    args.expected_source_hash = f.doc.source_hash.clone();
    let state = f.library.write_source(&args).unwrap();
    assert_eq!(state.source_hash, ContentHash::of(args.text.as_bytes()));
    assert!(state.dirty);
    args.expected_source_hash = state.source_hash;
    args.text = "after crash c'1".into();
    f.hook("after-replace", |_| Err(storage_error("interrupted write")));
    assert!(f.library.write_source(&args).is_err());
    let home = f.library.home().to_owned();
    drop(f.library);
    let library = Library::open(&home).unwrap();
    assert_eq!(
        library.source_snapshot(args.document_id).unwrap().source(),
        args.text
    );
    assert_eq!(library.history(args.document_id).unwrap().len(), 1);
}

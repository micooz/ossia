use crate::test_support::Fixture;
use crate::*;
use std::{fs, process::Command};

#[test]
fn external_save_after_final_commit_check_remains_a_dirty_workspace() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash = f.write("B");
    f.check(&edit, &hash);
    let path = f.doc.source_path.clone();
    f.hook("after-commit-check", move |_| {
        fs::write(path, b"C").unwrap();
        Ok(())
    });
    let version = f.commit(&edit, &hash).unwrap();
    assert_eq!(
        f.library
            .version_snapshot(version.version_id)
            .unwrap()
            .bytes(),
        b"B"
    );
    let doc = f.library.get_document(f.doc.document_id).unwrap();
    assert!(doc.dirty);
    assert_eq!(doc.source_hash, ContentHash::of(b"C"));
    assert_eq!(doc.edit_state, DocumentEditState::Idle);
}

#[test]
fn cas_and_database_failures_never_publish_unverified_versions() {
    let f = Fixture::new();
    f.hook("after-cas", |_| Err(storage_error("injected")));
    assert!(f
        .library
        .import(
            &f.engine,
            ImportRequest {
                title: "interrupted",
                input: b"unpublished",
                format: InputFormat::Ly,
                confirmation: None,
                author: "test"
            },
            &RunContext::default()
        )
        .is_err());
    assert_eq!(f.library.list_documents().unwrap().len(), 1);
    let edit = f.begin();
    let hash = f.write("B");
    f.check(&edit, &hash);
    f.hook("before-version-commit", |_| Err(storage_error("injected")));
    assert!(f.commit(&edit, &hash).is_err());
    assert_eq!(f.library.history(f.doc.document_id).unwrap().len(), 1);
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"B");
    assert_eq!(
        f.library
            .get_document(f.doc.document_id)
            .unwrap()
            .edit_state,
        DocumentEditState::Active
    );
    let version = f.commit(&edit, &hash).unwrap();
    fs::write(
        f.library
            .store
            .path(&format!("contents/{}.gz", version.source_hash))
            .unwrap(),
        b"corrupt",
    )
    .unwrap();
    assert_eq!(
        f.library
            .version_snapshot(version.version_id)
            .unwrap_err()
            .code,
        ErrorCode::StorageError
    );
}

#[test]
fn restore_faults_recover_by_actual_file_hash_without_duplicate_nodes() {
    for (point, expected, count) in [
        ("after-intent", b"B".as_slice(), 2),
        ("after-replace", b"A c'1".as_slice(), 3),
        ("before-intent-finalize", b"A c'1".as_slice(), 3),
        ("after-intent-finalize", b"A c'1".as_slice(), 3),
    ] {
        let f = Fixture::new();
        f.change("B");
        f.hook(point, |_| Err(storage_error("crash")));
        assert!(f.restore(f.doc.head_version_id.unwrap()).is_err());
        let home = f.library.home().to_owned();
        drop(f.library);
        for _ in 0..2 {
            let library = Library::open(&home).unwrap();
            assert_eq!(fs::read(&f.doc.source_path).unwrap(), expected);
            assert_eq!(library.history(f.doc.document_id).unwrap().len(), count);
            assert!(!library.get_document(f.doc.document_id).unwrap().dirty);
        }
    }
}

#[test]
fn recovery_conflict_preserves_external_file_until_explicit_acknowledgement() {
    let f = Fixture::new();
    f.change("B");
    f.hook("after-replace", |_| Err(storage_error("crash")));
    assert!(f.restore(f.doc.head_version_id.unwrap()).is_err());
    let hash = f.write("external C");
    let home = f.library.home().to_owned();
    drop(f.library);
    let library = Library::open(&home).unwrap();
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"external C");
    assert_eq!(library.history(f.doc.document_id).unwrap().len(), 2);
    assert_eq!(
        library.get_document(f.doc.document_id).unwrap().edit_state,
        DocumentEditState::RecoveryConflict
    );
    assert_eq!(
        library
            .resolve_recovery_conflict(f.doc.document_id, &f.doc.source_hash)
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    library
        .resolve_recovery_conflict(f.doc.document_id, &hash)
        .unwrap();
    assert!(library.get_document(f.doc.document_id).unwrap().dirty);
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"external C");
}

#[test]
fn discard_recovery_ends_edit_only_when_replacement_occurred() {
    for (point, expected, status) in [
        (
            "after-intent",
            b"B".as_slice(),
            DocumentEditState::Interrupted,
        ),
        (
            "after-replace",
            b"A c'1".as_slice(),
            DocumentEditState::Idle,
        ),
    ] {
        let f = Fixture::new();
        let edit = f.begin();
        let hash = f.write("B");
        f.hook(point, |_| Err(storage_error("crash")));
        assert!(f
            .library
            .discard(
                f.doc.document_id,
                Some(edit.edit_id),
                f.doc.head_version_id,
                &hash
            )
            .is_err());
        let home = f.library.home().to_owned();
        drop(f.library);
        let library = Library::open(&home).unwrap();
        assert_eq!(fs::read(&f.doc.source_path).unwrap(), expected);
        assert_eq!(
            library.get_document(f.doc.document_id).unwrap().edit_state,
            status
        );
        assert_eq!(library.history(f.doc.document_id).unwrap().len(), 1);
    }
}

#[test]
fn process_death_releases_lock_and_recovers_durable_restore_intent() {
    for (point, expected, count) in [
        ("after-intent", b"B".as_slice(), 2),
        ("after-replace", b"A c'1".as_slice(), 3),
        ("before-intent-finalize", b"A c'1".as_slice(), 3),
    ] {
        let f = Fixture::new();
        f.change("B");
        let home = f.library.home().to_owned();
        drop(f.library);
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "test_support::process_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("OSSIA_LIBRARY_TEST_MODE", point)
            .env("OSSIA_LIBRARY_TEST_HOME", &home)
            .env(
                "OSSIA_LIBRARY_TEST_TARGET",
                f.doc.head_version_id.unwrap().to_string(),
            )
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(73),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let library = Library::open(&home).unwrap();
        assert_eq!(fs::read(&f.doc.source_path).unwrap(), expected);
        assert_eq!(library.history(f.doc.document_id).unwrap().len(), count);
        assert!(!library.get_document(f.doc.document_id).unwrap().dirty);
    }
}

#[test]
fn converted_compile_failure_and_replacement_conflict_preserve_existing_documents() {
    let f = Fixture::new();
    *f.engine.error.lock().unwrap() = Some(AppError::new(
        ErrorCode::CompileFailed,
        "conversion compiled unsuccessfully",
    ));
    assert!(f
        .library
        .import(
            &f.engine,
            ImportRequest {
                title: "converted failure",
                input: b"xml",
                format: InputFormat::MusicXml,
                confirmation: None,
                author: "test"
            },
            &RunContext::default()
        )
        .is_err());
    assert_eq!(f.library.list_documents().unwrap().len(), 1);
    f.change("B");
    let path = f.doc.source_path.clone();
    f.hook("after-intent", move |_| {
        fs::write(path, b"external").unwrap();
        Ok(())
    });
    assert_eq!(
        f.restore(f.doc.head_version_id.unwrap()).unwrap_err().code,
        ErrorCode::Conflict
    );
    assert_eq!(fs::read(&f.doc.source_path).unwrap(), b"external");
    assert_eq!(f.library.history(f.doc.document_id).unwrap().len(), 2);
    assert_eq!(
        f.library
            .get_document(f.doc.document_id)
            .unwrap()
            .edit_state,
        DocumentEditState::RecoveryConflict
    );
}

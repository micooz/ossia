mod common;

use common::{until, Fixture};
use ossia_runtime::*;
use serde_json::json;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn delete_trash_untrash_and_purge_preserve_dirty_source_until_explicit_purge() {
    let f = Fixture::new(false).await;
    let cursor = f.runtime.agent_status().unwrap().seq;
    let changed = f.save(b"\\score { { e'1 } }");
    let error = f
        .runtime
        .delete_document(DeleteArgs {
            document_id: f.doc.document_id,
            expected_source_hash: changed.clone(),
            force: false,
        })
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Conflict);
    assert_eq!(error.detail.unwrap()["dirty"], json!(true));

    let deleted = f
        .runtime
        .delete_document(DeleteArgs {
            document_id: f.doc.document_id,
            expected_source_hash: changed.clone(),
            force: true,
        })
        .unwrap();
    assert_eq!(deleted.source_hash, changed);
    assert_eq!(
        f.runtime.context(f.doc.document_id).unwrap_err().code,
        ErrorCode::NotFound
    );
    let page = f
        .runtime
        .trash(TrashArgs {
            limit: Some(10),
            offset: 0,
        })
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].document_id, f.doc.document_id);

    let restored = f
        .runtime
        .untrash(DocumentIdArgs {
            document_id: f.doc.document_id,
        })
        .unwrap();
    assert!(restored.dirty);
    assert_eq!(
        std::fs::read(&f.doc.source_path).unwrap(),
        b"\\score { { e'1 } }"
    );
    assert_eq!(
        f.runtime.library.history(f.doc.document_id).unwrap().len(),
        1
    );

    f.runtime
        .delete_document(DeleteArgs {
            document_id: f.doc.document_id,
            expected_source_hash: restored.source_hash,
            force: true,
        })
        .unwrap();
    let source_root = std::path::Path::new(&f.doc.source_path)
        .parent()
        .unwrap()
        .to_path_buf();
    f.runtime
        .purge(DocumentIdArgs {
            document_id: f.doc.document_id,
        })
        .unwrap();
    assert!(!source_root.exists());
    assert!(f
        .runtime
        .trash(TrashArgs {
            limit: None,
            offset: 0,
        })
        .unwrap()
        .items
        .is_empty());
    let events = f
        .runtime
        .events(Some(f.doc.document_id), &f.runtime.info.instance_id, cursor)
        .unwrap();
    assert_eq!(
        events
            .iter()
            .map(|event| event.r#type.as_str())
            .collect::<Vec<_>>(),
        vec![
            "document-deleted",
            "document-restored",
            "document-deleted",
            "document-purged"
        ]
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forced_delete_cancels_a_running_task_before_trashing() {
    let f = Fixture::new(true).await;
    let changed = f.save(b"\\score { { g'1 } }");
    f.engine.hold.store(true, Ordering::Release);
    let job = f.check(changed.clone());
    until(|| f.engine.active.load(Ordering::SeqCst) == 1).await;

    let deleted = f
        .runtime
        .delete_document(DeleteArgs {
            document_id: f.doc.document_id,
            expected_source_hash: changed,
            force: true,
        })
        .unwrap();
    assert_eq!(deleted.document_id, f.doc.document_id);
    assert_eq!(
        f.runtime.library.get_job(job.job_id).unwrap().state,
        JobState::Cancelled
    );
    assert_eq!(f.engine.active.load(Ordering::SeqCst), 0);
    f.finish().await;
}

#[tokio::test]
async fn trash_operations_are_available_over_ipc() {
    let f = Fixture::new(false).await;
    let changed = f.save(b"\\score { { a'1 } }");
    ipc::dispatch(
        &f.runtime,
        "delete",
        json!({
            "documentId": f.doc.document_id,
            "expectedSourceHash": changed,
            "force": true,
        }),
    )
    .unwrap();
    assert_eq!(
        ipc::dispatch(&f.runtime, "trash", json!({"limit": 1})).unwrap()["total"],
        json!(1)
    );
    ipc::dispatch(
        &f.runtime,
        "untrash",
        json!({"documentId": f.doc.document_id}),
    )
    .unwrap();
    ipc::dispatch(
        &f.runtime,
        "delete",
        json!({
            "documentId": f.doc.document_id,
            "expectedSourceHash": ContentHash::of(b"\\score { { a'1 } }"),
            "force": true,
        }),
    )
    .unwrap();
    ipc::dispatch(
        &f.runtime,
        "purge",
        json!({"documentId": f.doc.document_id}),
    )
    .unwrap();
    f.finish().await;
}

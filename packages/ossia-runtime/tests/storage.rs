mod common;

use common::Fixture;
use ossia_runtime::*;
use serde_json::json;

#[tokio::test]
async fn storage_status_and_cleanup_are_exposed_over_ipc() {
    let f = Fixture::new(false).await;
    let status = ipc::dispatch(&f.runtime, "storage-status", json!({})).unwrap();
    assert_eq!(
        status["cacheLimitBytes"],
        json!(StorageCacheLimit::Gib1.bytes())
    );
    assert_eq!(status["jobCount"], json!(1));
    assert_eq!(status["renderCount"], json!(1));
    assert_eq!(status["idle"], json!(true));
    let report = ipc::dispatch(&f.runtime, "storage-cleanup", json!({})).unwrap();
    assert!(report["freedBytes"].is_number());
    assert_eq!(report["overLimit"], json!(false));
    f.finish().await;
}

#[tokio::test]
async fn cleanup_refuses_while_queued_work_exists() {
    let f = Fixture::new(false).await;
    f.runtime
        .check(
            CheckArgs {
                expect: None,
                document_id: Some(f.doc.document_id),
                view_id: f.view,
                edit_id: None,
                expected_source_hash: f.doc.source_hash.clone(),
                wait_ms: Some(0),
            },
            false,
        )
        .unwrap();
    assert_eq!(
        f.runtime.storage_cleanup().unwrap_err().code,
        ErrorCode::Conflict
    );
    assert!(!f.runtime.storage_status().unwrap().idle);
    f.finish().await;
}

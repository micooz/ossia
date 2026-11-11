mod common;
use common::Fixture;
use ossia_runtime::*;
use serde_json::json;

#[tokio::test]
async fn compact_status_and_context_exclude_history_and_render_duplication() {
    let f = Fixture::new(false).await;
    let before = ipc::dispatch(&f.runtime, "status", json!({})).unwrap();
    for _ in 0..50 {
        let job = f
            .runtime
            .library
            .create_job("synthetic", None, None)
            .unwrap();
        f.runtime
            .library
            .transition_job(
                job.job_id,
                JobState::Queued,
                JobState::Error,
                "finished",
                None,
                Some(AppError::invalid("old failure")),
            )
            .unwrap();
    }
    let after = ipc::dispatch(&f.runtime, "status", json!({})).unwrap();
    assert_eq!(before, after);
    assert_eq!(after["documentCount"], 1);
    let compact = ipc::dispatch(&f.runtime, "context", json!({"viewId":f.view})).unwrap();
    assert!(compact["document"].get("lastSuccessfulRender").is_none());
    assert!(compact["render"].get("manifest").is_none());
    let full = ipc::dispatch(
        &f.runtime,
        "context",
        json!({"viewId":f.view,"detail":"full"}),
    )
    .unwrap();
    assert!(full["render"]["manifest"].is_object());
    assert_eq!(
        ipc::dispatch(&f.runtime, "status", json!({"detail":"full"})).unwrap()["jobs"]
            .as_array()
            .unwrap()
            .len(),
        51
    );
    f.finish().await;
}
#[tokio::test]
async fn analysis_pages_are_bounded_and_legacy_evidence_is_explicit() {
    let f = Fixture::new(false).await;
    let source = f.doc.last_check.as_ref().unwrap().job_id;
    let page = f
        .runtime
        .analysis(AnalysisArgs {
            job_id: source,
            section: AnalysisSection::Performance,
            offset: 0,
            limit: Some(1),
        })
        .unwrap();
    assert!(page.items.is_empty());
    assert!(f
        .runtime
        .analysis(AnalysisArgs {
            job_id: source,
            section: AnalysisSection::Performance,
            offset: 0,
            limit: Some(513)
        })
        .is_err());
    let legacy = f
        .runtime
        .library
        .create_job(
            "check",
            Some(f.doc.document_id),
            Some(
                &f.runtime
                    .library
                    .source_snapshot(f.doc.document_id)
                    .unwrap(),
            ),
        )
        .unwrap();
    f.runtime
        .library
        .transition_job(
            legacy.job_id,
            JobState::Queued,
            JobState::Running,
            "execute",
            None,
            None,
        )
        .unwrap();
    f.runtime
        .library
        .transition_job(
            legacy.job_id,
            JobState::Running,
            JobState::Done,
            "finished",
            Some(json!({"pages":[]})),
            None,
        )
        .unwrap();
    assert_eq!(
        f.runtime
            .analysis(AnalysisArgs {
                job_id: legacy.job_id,
                section: AnalysisSection::Source,
                offset: 0,
                limit: None
            })
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedSource
    );
    f.finish().await;
}

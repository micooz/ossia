mod common;
use common::{Fixture, SOURCE};
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
    assert_eq!(after["jobs"].as_array().unwrap().len(), 0);
    assert!(after["documentation"]["available"].is_boolean());
    let compact = ipc::dispatch(&f.runtime, "context", json!({"viewId":f.view})).unwrap();
    assert!(compact["document"].get("lastSuccessfulRender").is_none());
    assert!(compact["document"].get("sourcePath").is_none());
    assert_eq!(compact["document"]["sourceBytes"], json!(SOURCE.len()));
    assert!(compact["render"].get("manifest").is_none());
    ipc::dispatch(&f.runtime, "status", json!({"detail":"full"})).unwrap_err();
    f.finish().await;
}
#[tokio::test]
async fn analysis_pages_are_bounded_and_legacy_evidence_is_explicit() {
    let f = Fixture::new(false).await;
    let source = f.doc.last_check.as_ref().unwrap().job_id;
    let page = f
        .runtime
        .analysis(source, AnalysisSection::Performance, 0, Some(1))
        .unwrap();
    assert!(page.items.is_empty());
    assert!(f
        .runtime
        .analysis(source, AnalysisSection::Performance, 0, Some(513))
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
            .analysis(legacy.job_id, AnalysisSection::Source, 0, None)
            .unwrap_err()
            .code,
        ErrorCode::UnsupportedSource
    );
    f.finish().await;
}

#[tokio::test]
async fn list_history_and_job_sections_are_compact_and_paged() {
    let f = Fixture::new(false).await;
    let page = f
        .runtime
        .agent_list(ListArgs {
            limit: Some(1),
            offset: 0,
        })
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.total, 1);
    assert_eq!(page.next_offset, None);
    let item = serde_json::to_value(&page.items[0]).unwrap();
    assert!(item.get("lastCheck").is_none());
    assert!(item.get("lastSuccessfulRender").is_none());
    assert_eq!(
        f.runtime
            .agent_list(ListArgs {
                limit: Some(0),
                offset: 0,
            })
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
    let history = f
        .runtime
        .agent_history(HistoryArgs {
            document_id: f.doc.document_id,
            limit: Some(1),
            offset: 0,
        })
        .unwrap();
    assert_eq!(history.items.len(), 1);
    assert_eq!(history.total, 1);
    assert_eq!(history.next_offset, None);

    let source_job = f.doc.last_check.as_ref().unwrap().job_id;
    let outcome = f
        .runtime
        .job_outcome(JobArgs {
            detail: DetailLevel::Summary,
            job_id: source_job,
            action: JobAction::Get,
            wait_ms: None,
            section: Some(AnalysisSection::Performance),
            offset: 0,
            limit: Some(1),
        })
        .unwrap();
    assert!(outcome.job.result.is_none());
    assert!(outcome.analysis.is_some());
    assert_eq!(
        f.runtime
            .job_outcome(JobArgs {
                detail: DetailLevel::Full,
                job_id: source_job,
                action: JobAction::Get,
                wait_ms: None,
                section: Some(AnalysisSection::Performance),
                offset: 0,
                limit: None,
            })
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn import_infers_path_metadata_and_can_wait_for_completion() {
    let f = Fixture::new(true).await;
    let work = tempfile::tempdir().unwrap();
    let input = work.path().join("Inferred scale.ly");
    std::fs::write(&input, SOURCE).unwrap();
    let job = f
        .runtime
        .import(
            ImportArgs {
                title: None,
                format: None,
                path: input.to_string_lossy().into_owned(),
                confirmation: None,
                author: None,
                wait_ms: Some(5_000),
            },
            SOURCE.to_vec(),
        )
        .unwrap();
    assert_eq!(job.state, JobState::Done);
    let outcome: ImportOutcome = serde_json::from_value(job.result.unwrap()).unwrap();
    assert_eq!(outcome.document.unwrap().title, "Inferred scale");
    f.finish().await;
}

#[tokio::test]
async fn stale_source_errors_carry_recovery_context() {
    let f = Fixture::new(false).await;
    let error = f
        .runtime
        .edit(
            EditArgs {
                action: EditAction::Begin,
                document_id: f.doc.document_id,
                view_id: f.view,
                base_version_id: f.doc.head_version_id,
                expected_source_hash: ContentHash::of(b"stale"),
                edit_id: None,
                adopt_workspace: false,
                author: "test".into(),
            },
            true,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Conflict);
    let detail = error.detail.unwrap();
    assert_eq!(detail["currentSourceHash"], json!(f.doc.source_hash));
    assert_eq!(detail["headVersionId"], json!(f.doc.head_version_id));
    f.finish().await;
}

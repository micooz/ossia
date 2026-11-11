mod common;

use common::{until, wait_job, Fixture, SOURCE};
use ossia_library::ImportRequest;
use ossia_runtime::*;
use std::sync::atomic::Ordering;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queued_snapshots_previews_and_explicit_checks_are_distinct() {
    let mut f = Fixture::new(false).await;
    let old = f.save(b"\\score { { e'1 } }");
    let explicit = f.check(old.clone());
    let preview = f
        .runtime
        .check(
            CheckArgs {
                expect: None,
                reference: None,
                document_id: Some(f.doc.document_id),
                edit_id: None,
                expected_source_hash: old.clone(),
                wait_ms: None,
            },
            true,
        )
        .unwrap();
    let latest = f.save(b"\\score { { f'1 } }");
    let next = f
        .runtime
        .check(
            CheckArgs {
                expect: None,
                reference: None,
                document_id: Some(f.doc.document_id),
                edit_id: None,
                expected_source_hash: latest.clone(),
                wait_ms: None,
            },
            true,
        )
        .unwrap();
    assert_eq!(
        f.runtime.library.get_job(preview.job_id).unwrap().state,
        JobState::Cancelled
    );
    assert_eq!(
        f.runtime.library.get_job(explicit.job_id).unwrap().state,
        JobState::Queued
    );
    f.scheduler = Some(tokio::spawn(f.runtime.clone().schedule()));
    assert_eq!(
        wait_job(&f.runtime, explicit.job_id).await.state,
        JobState::Done
    );
    assert_eq!(
        wait_job(&f.runtime, next.job_id).await.state,
        JobState::Done
    );
    assert!(f.engine.seen.lock().unwrap().contains(&old));
    assert_eq!(
        f.runtime
            .library
            .get_document(f.doc.document_id)
            .unwrap()
            .last_successful_render
            .unwrap()
            .source_hash,
        latest
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn global_limit_same_document_serialization_and_cancel_cleanup() {
    let f = Fixture::new(true).await;
    let mut docs = vec![f.doc.clone()];
    for i in 0..2 {
        docs.push(
            f.runtime
                .library
                .import(
                    f.engine.as_ref(),
                    ImportRequest {
                        title: &format!("doc{i}"),
                        input: SOURCE,
                        format: InputFormat::Ly,
                        confirmation: None,
                        author: "test",
                    },
                    &ossia_engine::RunContext::default(),
                )
                .unwrap()
                .document
                .unwrap(),
        );
    }
    f.engine.peak.store(0, Ordering::SeqCst);
    f.engine.hold.store(true, Ordering::Release);
    let first = f.check(f.doc.source_hash.clone());
    let second = f.check(f.doc.source_hash.clone());
    let mut jobs = vec![first.clone(), second.clone()];
    for doc in docs.iter().skip(1) {
        jobs.push(
            f.runtime
                .check(
                    CheckArgs {
                        expect: None,
                        reference: None,
                        document_id: Some(doc.document_id),
                        edit_id: None,
                        expected_source_hash: doc.source_hash.clone(),
                        wait_ms: None,
                    },
                    false,
                )
                .unwrap(),
        );
    }
    until(|| f.engine.active.load(Ordering::SeqCst) == 2).await;
    assert_eq!(
        f.runtime.library.get_job(second.job_id).unwrap().state,
        JobState::Queued
    );
    assert_eq!(f.engine.peak.load(Ordering::SeqCst), 2);
    f.runtime.cancel(first.job_id).unwrap();
    f.runtime.cancel(first.job_id).unwrap();
    assert_eq!(
        wait_job(&f.runtime, first.job_id).await.state,
        JobState::Cancelled
    );
    f.engine.hold.store(false, Ordering::Release);
    for job in jobs.into_iter().skip(1) {
        assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    }
    until(|| f.engine.active.load(Ordering::SeqCst) == 0).await;
    assert_eq!(
        f.runtime.library.get_job(first.job_id).unwrap().state,
        JobState::Cancelled
    );
    f.finish().await;
}

#[tokio::test]
async fn invalid_wait_is_rejected_before_queueing() {
    let f = Fixture::new(false).await;
    let before = f.runtime.library.list_jobs().unwrap().len();
    let error = f
        .runtime
        .check(
            CheckArgs {
                expect: None,
                reference: None,
                document_id: Some(f.doc.document_id),
                edit_id: None,
                expected_source_hash: f.doc.source_hash.clone(),
                wait_ms: Some(30_001),
            },
            false,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(f.runtime.library.list_jobs().unwrap().len(), before);
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn output_and_render_keys_enforce_version_ownership_and_do_not_overwrite() {
    let f = Fixture::new(true).await;
    let key = ossia_library::render_key(
        f.doc.document_id,
        f.doc.last_successful_render.as_ref().unwrap(),
    );
    assert!(!f
        .runtime
        .library
        .read_render_page(&key, 1)
        .unwrap()
        .is_empty());
    assert!(f.runtime.library.read_render_page(&key, 2).is_err());
    let target = f.root.path().join("export.ly");
    let args = OutputArgs {
        source: OutputSource::Version {
            version_id: f.doc.head_version_id.unwrap(),
        },
        format: OutputFormat::Ly,
        page: None,
        width: None,
        output_path: Some(target.to_string_lossy().into_owned()),
        wait_ms: None,
    };
    let job = f.runtime.output(args.clone()).unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    assert_eq!(std::fs::read(&target).unwrap(), SOURCE);
    let job = f.runtime.output(args.clone()).unwrap();
    assert_eq!(
        wait_job(&f.runtime, job.job_id).await.error.unwrap().code,
        ErrorCode::Conflict
    );
    assert_eq!(std::fs::read(&target).unwrap(), SOURCE);
    let bad = OutputArgs {
        source: OutputSource::Version {
            version_id: VersionId::new(),
        },
        ..args
    };
    assert_eq!(f.runtime.output(bad).unwrap_err().code, ErrorCode::NotFound);
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn job_outputs_use_the_captured_snapshot_and_validate_pages() {
    let f = Fixture::new(true).await;
    let source_job = f.doc.last_check.as_ref().unwrap().job_id;
    let svg_path = f.root.path().join("job.svg");
    let job = f
        .runtime
        .output(OutputArgs {
            source: OutputSource::Job { job_id: source_job },
            format: OutputFormat::Svg,
            page: None,
            width: None,
            output_path: Some(svg_path.to_string_lossy().into_owned()),
            wait_ms: None,
        })
        .unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    assert!(std::fs::read_to_string(&svg_path).unwrap().contains("<svg"));

    let ly_path = f.root.path().join("job.ly");
    let job = f
        .runtime
        .output(OutputArgs {
            source: OutputSource::Job { job_id: source_job },
            format: OutputFormat::Ly,
            page: None,
            width: None,
            output_path: Some(ly_path.to_string_lossy().into_owned()),
            wait_ms: None,
        })
        .unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    assert_eq!(std::fs::read(&ly_path).unwrap(), SOURCE);

    let missing_page = f.root.path().join("missing.svg");
    let job = f
        .runtime
        .output(OutputArgs {
            source: OutputSource::Job { job_id: source_job },
            format: OutputFormat::Svg,
            page: Some(2),
            width: None,
            output_path: Some(missing_page.to_string_lossy().into_owned()),
            wait_ms: None,
        })
        .unwrap();
    let job = wait_job(&f.runtime, job.job_id).await;
    assert_eq!(job.state, JobState::Error);
    assert_eq!(job.error.unwrap().code, ErrorCode::NotFound);
    assert!(!missing_page.exists());
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn running_import_restore_output_cancellation_finishes_before_publishing() {
    let f = Fixture::new(true).await;
    f.engine.hold.store(true, Ordering::Release);
    let imported = f
        .runtime
        .import(
            ImportArgs {
                title: Some("cancel import".into()),
                format: Some(InputFormat::Ly),
                path: String::new(),
                confirmation: None,
                author: Some("test".into()),
                expect: None,
                reference: None,
                wait_ms: None,
            },
            SOURCE.to_vec(),
        )
        .unwrap();
    until(|| f.engine.active.load(Ordering::SeqCst) == 1).await;
    f.runtime.cancel(imported.job_id).unwrap();
    assert_eq!(
        wait_job(&f.runtime, imported.job_id).await.state,
        JobState::Cancelled
    );
    until(|| f.engine.active.load(Ordering::SeqCst) == 0).await;
    assert_eq!(f.runtime.library.list_documents().unwrap().len(), 1);
    let restored = f
        .runtime
        .restore(RestoreArgs {
            version_id: f.doc.head_version_id.unwrap(),
            base_version_id: f.doc.head_version_id,
            expected_source_hash: f.doc.source_hash.clone(),
            author: Some("test".into()),
            summary: "cancel restore".into(),
            wait_ms: None,
        })
        .unwrap();
    until(|| f.engine.active.load(Ordering::SeqCst) == 1).await;
    f.runtime.cancel(restored.job_id).unwrap();
    assert_eq!(
        wait_job(&f.runtime, restored.job_id).await.state,
        JobState::Cancelled
    );
    until(|| f.engine.active.load(Ordering::SeqCst) == 0).await;
    assert_eq!(
        f.runtime.library.history(f.doc.document_id).unwrap().len(),
        1
    );
    let path = f.root.path().join("cancelled.pdf");
    let exported = f
        .runtime
        .output(OutputArgs {
            source: OutputSource::Version {
                version_id: f.doc.head_version_id.unwrap(),
            },
            format: OutputFormat::Pdf,
            page: None,
            width: None,
            output_path: Some(path.to_string_lossy().into_owned()),
            wait_ms: None,
        })
        .unwrap();
    until(|| f.engine.active.load(Ordering::SeqCst) == 1).await;
    f.runtime.stop().unwrap();
    assert_eq!(
        wait_job(&f.runtime, exported.job_id).await.state,
        JobState::Cancelled
    );
    assert!(!path.exists());
    assert_eq!(
        f.runtime
            .open_url(Some(f.doc.document_id))
            .unwrap_err()
            .code,
        ErrorCode::RuntimeUnavailable
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reveal_only_resolves_intact_completed_output_results() {
    let f = Fixture::new(true).await;
    let target = f.root.path().join("result.ly");
    let job = f
        .runtime
        .output(OutputArgs {
            source: OutputSource::Version {
                version_id: f.doc.head_version_id.unwrap(),
            },
            format: OutputFormat::Ly,
            page: None,
            width: None,
            output_path: Some(target.to_string_lossy().into_owned()),
            wait_ms: None,
        })
        .unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    assert_eq!(f.runtime.export_location(job.job_id).unwrap(), target);
    std::fs::write(&target, b"changed").unwrap();
    assert_eq!(
        f.runtime.export_location(job.job_id).unwrap_err().code,
        ErrorCode::StorageError
    );
    assert!(f.runtime.export_location(JobId::new()).is_err());
    f.finish().await;
}

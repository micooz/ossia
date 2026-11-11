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
                document_id: f.doc.document_id,
                view_id: f.view,
                edit_id: None,
                expected_source_hash: old.clone(),
            },
            true,
        )
        .unwrap();
    let latest = f.save(b"\\score { { f'1 } }");
    let next = f
        .runtime
        .check(
            CheckArgs {
                document_id: f.doc.document_id,
                view_id: f.view,
                edit_id: None,
                expected_source_hash: latest.clone(),
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
        let view = f.runtime.open_view(Some(doc.document_id)).unwrap().view_id;
        jobs.push(
            f.runtime
                .check(
                    CheckArgs {
                        document_id: doc.document_id,
                        view_id: view,
                        edit_id: None,
                        expected_source_hash: doc.source_hash.clone(),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn export_and_render_keys_enforce_version_ownership_and_do_not_overwrite() {
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
    let args = ExportArgs {
        document_id: f.doc.document_id,
        version_id: f.doc.head_version_id.unwrap(),
        format: ExportFormat::Ly,
        output_path: Some(target.to_string_lossy().into_owned()),
    };
    let job = f.runtime.export(args.clone()).unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    assert_eq!(std::fs::read(&target).unwrap(), SOURCE);
    let job = f.runtime.export(args.clone()).unwrap();
    assert_eq!(
        wait_job(&f.runtime, job.job_id).await.error.unwrap().code,
        ErrorCode::Conflict
    );
    assert_eq!(std::fs::read(&target).unwrap(), SOURCE);
    let bad = ExportArgs {
        document_id: DocumentId::new(),
        ..args
    };
    assert_eq!(f.runtime.export(bad).unwrap_err().code, ErrorCode::NotFound);
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn running_import_restore_export_cancellation_finishes_before_publishing() {
    let f = Fixture::new(true).await;
    f.engine.hold.store(true, Ordering::Release);
    let imported = f
        .runtime
        .import(
            ImportArgs {
                title: "cancel import".into(),
                format: InputFormat::Ly,
                path: String::new(),
                confirmation: None,
                author: "test".into(),
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
            document_id: f.doc.document_id,
            version_id: f.doc.head_version_id.unwrap(),
            base_version_id: f.doc.head_version_id,
            expected_source_hash: f.doc.source_hash.clone(),
            author: "test".into(),
            summary: "cancel restore".into(),
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
        .export(ExportArgs {
            document_id: f.doc.document_id,
            version_id: f.doc.head_version_id.unwrap(),
            format: ExportFormat::Pdf,
            output_path: Some(path.to_string_lossy().into_owned()),
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
        f.runtime.open_view(None).unwrap_err().code,
        ErrorCode::RuntimeUnavailable
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reveal_only_resolves_intact_completed_export_results() {
    let f = Fixture::new(true).await;
    let target = f.root.path().join("result.ly");
    let job = f
        .runtime
        .export(ExportArgs {
            document_id: f.doc.document_id,
            version_id: f.doc.head_version_id.unwrap(),
            format: ExportFormat::Ly,
            output_path: Some(target.to_string_lossy().into_owned()),
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

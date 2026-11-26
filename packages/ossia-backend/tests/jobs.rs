mod common;

use common::{until, wait_job, Fixture, SOURCE};
use ossia_backend::*;
use ossia_library::ImportRequest;
use serde_json::json;
use std::{path::Path, sync::atomic::Ordering};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn visual_output_captures_drawing_when_queued() {
    let mut f = Fixture::new(false).await;
    let document = f.doc.document_id;
    let stroke = DrawingStroke {
        page: 1,
        width: 0.01,
        color: DrawingColor::Black,
        points: vec![
            DrawingPoint { x: 0.2, y: 0.3 },
            DrawingPoint { x: 0.8, y: 0.3 },
        ],
    };
    let saved = f
        .runtime
        .library
        .save_drawing(
            document,
            Drawing {
                revision: 0,
                strokes: vec![stroke],
            },
        )
        .unwrap();
    let output = f.root.path().join("drawing.svg");
    let job = f
        .runtime
        .output(OutputArgs {
            source: OutputSource::Version {
                version_id: f.doc.head_version_id.unwrap(),
            },
            format: OutputFormat::Svg,
            pages: Some(vec![1]),
            width: None,
            pdf_password: None,
            png_quality: None,
            png_transparent: None,
            include_drawing: Some(true),
            output_path: Some(output.to_string_lossy().into_owned()),
            output_directory: None,
            crop: None,
            padding_staff_spaces: None,
            audio_range: None,
            wait_ms: None,
        })
        .unwrap();
    f.runtime
        .library
        .save_drawing(
            document,
            Drawing {
                revision: saved.revision,
                strokes: vec![],
            },
        )
        .unwrap();
    f.scheduler = Some(tokio::spawn(f.runtime.clone().schedule()));
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    let svg = std::fs::read_to_string(output).unwrap();
    assert!(svg.contains("points=\"20,30 80,30\""));
    f.finish().await;
}

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
        pages: None,
        width: None,
        pdf_password: None,
        png_quality: None,
        png_transparent: None,
        output_path: Some(target.to_string_lossy().into_owned()),
        output_directory: None,
        crop: None,
        padding_staff_spaces: None,
        audio_range: None,
        include_drawing: None,
        wait_ms: None,
    };
    let job = f.runtime.output(args.clone()).unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    assert_eq!(std::fs::read(&target).unwrap(), SOURCE);
    let missing = f.root.path().join("missing-parent/export.ly");
    let mut missing_args = args.clone();
    missing_args.output_path = Some(missing.to_string_lossy().into_owned());
    let job = f.runtime.output(missing_args).unwrap();
    let error = wait_job(&f.runtime, job.job_id).await.error.unwrap();
    assert!(error.message.contains("output"), "{error:?}");
    assert!(error.message.contains("missing-parent"), "{error:?}");
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

#[test]
fn restore_and_crop_requests_reject_incomplete_contracts() {
    assert!(serde_json::from_value::<RestoreArgs>(json!({
        "versionId": VersionId::new(),
        "expectedSourceHash": ContentHash::of(b"source"),
        "summary": "restore"
    }))
    .is_err());
    assert!(serde_json::from_value::<RestoreArgs>(json!({
        "versionId": VersionId::new(),
        "baseVersionId": null,
        "expectedSourceHash": ContentHash::of(b"source"),
        "summary": "restore"
    }))
    .is_err());
}

#[tokio::test]
async fn crop_requests_accept_page_and_single_file_destinations() {
    let f = Fixture::new(false).await;
    let base = OutputArgs {
        source: OutputSource::Version {
            version_id: f.doc.head_version_id.unwrap(),
        },
        format: OutputFormat::Svg,
        pages: None,
        width: None,
        pdf_password: None,
        png_quality: None,
        png_transparent: None,
        output_path: None,
        output_directory: None,
        crop: Some(OutputCrop::Measures {
            range: MeasureRange { start: 1, end: 1 },
            staff: None,
        }),
        padding_staff_spaces: None,
        audio_range: None,
        include_drawing: None,
        wait_ms: None,
    };
    let mut invalid = base.clone();
    invalid.format = OutputFormat::Midi;
    assert_eq!(
        f.runtime.output(invalid).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let mut invalid = base.clone();
    invalid.pages = Some(vec![0]);
    assert_eq!(
        f.runtime.output(invalid).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let mut paged = base.clone();
    paged.pages = Some(vec![1]);
    paged.output_directory = Some(
        f.root
            .path()
            .join("crop-pages")
            .to_string_lossy()
            .into_owned(),
    );
    assert!(f.runtime.output(paged).is_ok());
    let mut single = base.clone();
    single.output_path = Some(
        f.root
            .path()
            .join("crop.svg")
            .to_string_lossy()
            .into_owned(),
    );
    assert!(f.runtime.output(single).is_ok());
    let mut invalid = base.clone();
    invalid.output_path = Some(
        f.root
            .path()
            .join("crop.svg")
            .to_string_lossy()
            .into_owned(),
    );
    invalid.output_directory = Some(f.root.path().to_string_lossy().into_owned());
    assert_eq!(
        f.runtime.output(invalid).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let mut invalid = base.clone();
    invalid.crop = Some(OutputCrop::System {
        page: 1,
        system_index: 0,
        staff: None,
    });
    invalid.pages = Some(vec![2]);
    assert_eq!(
        f.runtime.output(invalid).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let mut matching = base;
    matching.crop = Some(OutputCrop::System {
        page: 1,
        system_index: 0,
        staff: None,
    });
    matching.pages = Some(vec![1]);
    matching.output_directory = Some(
        f.root
            .path()
            .join("crop-system")
            .to_string_lossy()
            .into_owned(),
    );
    assert!(f.runtime.output(matching).is_ok());
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn omitted_output_destination_uses_the_system_temp_directory() {
    let f = Fixture::new(true).await;
    let output = f
        .runtime
        .output(OutputArgs {
            source: OutputSource::Version {
                version_id: f.doc.head_version_id.unwrap(),
            },
            format: OutputFormat::Ly,
            pages: None,
            width: None,
            pdf_password: None,
            png_quality: None,
            png_transparent: None,
            output_path: None,
            output_directory: None,
            crop: None,
            padding_staff_spaces: None,
            audio_range: None,
            include_drawing: None,
            wait_ms: None,
        })
        .unwrap();
    let job = wait_job(&f.runtime, output.job_id).await;
    assert_eq!(job.state, JobState::Done);
    let outcome: OutputOutcome = serde_json::from_value(job.result.unwrap()).unwrap();
    let path = Path::new(&outcome.path);
    assert!(path.starts_with(std::env::temp_dir().join("ossia")));
    assert!(path.is_file());

    let preview = f
        .runtime
        .output(OutputArgs {
            source: OutputSource::Version {
                version_id: f.doc.head_version_id.unwrap(),
            },
            format: OutputFormat::Svg,
            pages: None,
            width: None,
            pdf_password: None,
            png_quality: None,
            png_transparent: None,
            output_path: None,
            output_directory: None,
            crop: None,
            padding_staff_spaces: None,
            audio_range: None,
            include_drawing: None,
            wait_ms: None,
        })
        .unwrap();
    let job = wait_job(&f.runtime, preview.job_id).await;
    assert_eq!(job.state, JobState::Done);
    let outcome: OutputOutcome = serde_json::from_value(job.result.unwrap()).unwrap();
    let path = Path::new(&outcome.path);
    assert!(path.starts_with(std::env::temp_dir().join("ossia")));
    assert!(path.is_file());
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn page_outputs_validate_page_lists_and_batch_errors() {
    let f = Fixture::new(true).await;
    let base = OutputArgs {
        source: OutputSource::Version {
            version_id: f.doc.head_version_id.unwrap(),
        },
        format: OutputFormat::Svg,
        pages: None,
        width: None,
        pdf_password: None,
        png_quality: None,
        png_transparent: None,
        output_path: None,
        output_directory: None,
        crop: None,
        padding_staff_spaces: None,
        audio_range: None,
        include_drawing: None,
        wait_ms: None,
    };
    for pages in [Some(vec![0]), Some(vec![1, 1])] {
        let mut invalid = base.clone();
        invalid.pages = pages;
        assert_eq!(
            f.runtime.output(invalid).unwrap_err().code,
            ErrorCode::InvalidRequest
        );
    }
    let mut invalid = base.clone();
    invalid.pages = Some(vec![1, 2]);
    invalid.output_path = Some(
        f.root
            .path()
            .join("pages.svg")
            .to_string_lossy()
            .into_owned(),
    );
    assert_eq!(
        f.runtime.output(invalid).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let mut pdf = base.clone();
    pdf.format = OutputFormat::Pdf;
    pdf.pages = Some(vec![1, 2]);
    let job = f.runtime.output(pdf).unwrap();
    assert_eq!(
        wait_job(&f.runtime, job.job_id).await.state,
        JobState::Error
    );
    let mut invalid = base.clone();
    invalid.format = OutputFormat::Wav;
    invalid.pages = Some(vec![1]);
    assert_eq!(
        f.runtime.output(invalid).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let mut invalid = base.clone();
    invalid.format = OutputFormat::Wav;
    invalid.audio_range = Some(MeasureRange { start: 0, end: 1 });
    assert_eq!(
        f.runtime.output(invalid).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let mut invalid = base.clone();
    invalid.format = OutputFormat::Pdf;
    invalid.audio_range = Some(MeasureRange { start: 1, end: 1 });
    assert_eq!(
        f.runtime.output(invalid).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    let single_path = f.root.path().join("page-1.svg");
    let mut single = base.clone();
    single.pages = Some(vec![1]);
    single.output_path = Some(single_path.to_string_lossy().into_owned());
    let job = f.runtime.output(single).unwrap();
    assert_eq!(wait_job(&f.runtime, job.job_id).await.state, JobState::Done);
    assert!(single_path.is_file());

    let mut one_batch = base.clone();
    one_batch.pages = Some(vec![1]);
    one_batch.output_directory = Some(
        f.root
            .path()
            .join("one-page")
            .to_string_lossy()
            .into_owned(),
    );
    let job = f.runtime.output(one_batch).unwrap();
    let job = wait_job(&f.runtime, job.job_id).await;
    assert_eq!(job.state, JobState::Done);
    let outcome: OutputBatchOutcome = serde_json::from_value(job.result.unwrap()).unwrap();
    assert_eq!(outcome.files.len(), 1);
    assert!(Path::new(&outcome.files[0].path).is_file());

    let mut multi = base;
    multi.pages = Some(vec![1, 2]);
    multi.output_directory = Some(f.root.path().join("pages").to_string_lossy().into_owned());
    let job = f.runtime.output(multi).unwrap();
    assert_eq!(
        wait_job(&f.runtime, job.job_id).await.error.unwrap().code,
        ErrorCode::NotFound
    );
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
            pages: None,
            width: None,
            pdf_password: None,
            png_quality: None,
            png_transparent: None,
            output_path: Some(svg_path.to_string_lossy().into_owned()),
            output_directory: None,
            crop: None,
            padding_staff_spaces: None,
            audio_range: None,
            include_drawing: None,
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
            pages: None,
            width: None,
            pdf_password: None,
            png_quality: None,
            png_transparent: None,
            output_path: Some(ly_path.to_string_lossy().into_owned()),
            output_directory: None,
            crop: None,
            padding_staff_spaces: None,
            audio_range: None,
            include_drawing: None,
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
            pages: Some(vec![2]),
            width: None,
            pdf_password: None,
            png_quality: None,
            png_transparent: None,
            output_path: Some(missing_page.to_string_lossy().into_owned()),
            output_directory: None,
            crop: None,
            padding_staff_spaces: None,
            audio_range: None,
            include_drawing: None,
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
            base_version_id: f.doc.head_version_id.unwrap(),
            expected_source_hash: f.doc.source_hash.clone(),
            discard_current_edit: false,
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
            pages: None,
            width: None,
            pdf_password: None,
            png_quality: None,
            png_transparent: None,
            output_path: Some(path.to_string_lossy().into_owned()),
            output_directory: None,
            crop: None,
            padding_staff_spaces: None,
            audio_range: None,
            include_drawing: None,
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
            pages: None,
            width: None,
            pdf_password: None,
            png_quality: None,
            png_transparent: None,
            output_path: Some(target.to_string_lossy().into_owned()),
            output_directory: None,
            crop: None,
            padding_staff_spaces: None,
            audio_range: None,
            include_drawing: None,
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

mod common;
use common::{Fixture, SOURCE};
use ossia_runtime::*;
use serde_json::json;
use std::sync::atomic::Ordering;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn version_evidence_is_immutable_cached_and_does_not_create_versions() {
    let f = Fixture::new(true).await;
    f.engine.midi.store(true, Ordering::Release);
    let version = f.doc.head_version_id.unwrap();
    let history = f.runtime.library.history(f.doc.document_id).unwrap();

    let outcome = f
        .runtime
        .evidence(EvidenceArgs {
            source: EvidenceSource::Version {
                version_id: version,
            },
            section: EvidenceSection::Performance,
            offset: 0,
            limit: Some(1),
            wait_ms: Some(5_000),
            measure_range: None,
            voice: None,
            track: None,
            channel: None,
        })
        .unwrap();
    let page = outcome.page.unwrap();
    assert!(outcome.job.is_some());
    assert_eq!(page.source_hash, f.doc.source_hash);
    assert_eq!(page.offset, 0);
    assert_eq!(page.items.len(), 1);
    assert!(page.summary.performed_notes >= 1);
    assert_eq!(page.summary.source_status, AnalysisStatus::Complete);

    let cached = f
        .runtime
        .evidence(EvidenceArgs {
            source: EvidenceSource::Version {
                version_id: version,
            },
            section: EvidenceSection::Summary,
            offset: 0,
            limit: None,
            wait_ms: Some(5_000),
            measure_range: None,
            voice: None,
            track: None,
            channel: None,
        })
        .unwrap();
    assert!(cached.job.is_none());
    assert!(cached.page.unwrap().items.is_empty());
    assert_eq!(
        f.runtime.library.history(f.doc.document_id).unwrap(),
        history
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn evidence_filters_source_events_by_measure_and_rejects_bad_ranges() {
    let f = Fixture::new(true).await;
    let version = f.doc.head_version_id.unwrap();
    let base = EvidenceArgs {
        source: EvidenceSource::Version {
            version_id: version,
        },
        section: EvidenceSection::Source,
        offset: 0,
        limit: Some(10),
        wait_ms: Some(5_000),
        measure_range: Some(MeasureRange { start: 2, end: 2 }),
        voice: None,
        track: None,
        channel: None,
    };
    let page = f.runtime.evidence(base.clone()).unwrap().page.unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0]["measure"], json!(2));

    let mut invalid = base.clone();
    invalid.measure_range = Some(MeasureRange { start: 0, end: 1 });
    assert_eq!(
        f.runtime.evidence(invalid).unwrap_err().code,
        ErrorCode::InvalidInput
    );

    let mut summary = base;
    summary.section = EvidenceSection::Summary;
    assert_eq!(
        f.runtime.evidence(summary).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn performance_evidence_filters_by_measure_and_exposes_pitch_names() {
    let f = Fixture::new(true).await;
    f.engine.midi.store(true, Ordering::Release);
    let source = b"\\version \"2.26.0\"\n% MEASURED\n\\score { \\new Staff { c'1 | d'1 | } \\layout {} \\midi {} }\n";
    let job = f
        .runtime
        .import(
            ImportArgs {
                title: Some("measured".into()),
                format: Some(InputFormat::Ly),
                path: "/tmp/measured.ly".into(),
                confirmation: None,
                author: Some("test".into()),
                expect: None,
                reference: None,
                wait_ms: Some(5_000),
            },
            source.to_vec(),
        )
        .unwrap();
    let outcome: ImportOutcome = serde_json::from_value(job.result.unwrap()).unwrap();
    let version_id = outcome.document.unwrap().head_version_id.unwrap();
    let page = f
        .runtime
        .evidence(EvidenceArgs {
            source: EvidenceSource::Version { version_id },
            section: EvidenceSection::Performance,
            offset: 0,
            limit: Some(10),
            wait_ms: Some(5_000),
            measure_range: Some(MeasureRange { start: 2, end: 2 }),
            voice: None,
            track: None,
            channel: None,
        })
        .unwrap()
        .page
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0]["measure"], json!(2));
    assert_eq!(page.items[0]["pitchName"], json!("D4"));
    assert_eq!(page.summary.performance_measures, Some(2));
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn partial_measures_are_not_given_guessed_performance_numbers() {
    let f = Fixture::new(true).await;
    f.engine.midi.store(true, Ordering::Release);
    let source = b"\\version \"2.26.0\"\n% MEASURED\n\\score { \\new Staff { \\partial 4 c'4 | d'1 | } \\layout {} \\midi {} }\n";
    let job = f
        .runtime
        .import(
            ImportArgs {
                title: Some("pickup".into()),
                format: Some(InputFormat::Ly),
                path: "/tmp/pickup.ly".into(),
                confirmation: None,
                author: Some("test".into()),
                expect: None,
                reference: None,
                wait_ms: Some(5_000),
            },
            source.to_vec(),
        )
        .unwrap();
    let outcome: ImportOutcome = serde_json::from_value(job.result.unwrap()).unwrap();
    let version_id = outcome.document.unwrap().head_version_id.unwrap();
    let page = f
        .runtime
        .evidence(EvidenceArgs {
            source: EvidenceSource::Version { version_id },
            section: EvidenceSection::Summary,
            offset: 0,
            limit: None,
            wait_ms: Some(5_000),
            measure_range: None,
            voice: None,
            track: None,
            channel: None,
        })
        .unwrap()
        .page
        .unwrap();
    assert_eq!(page.summary.performance_measures, None);
    assert!(page
        .summary
        .measure_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("pickup")));
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn legacy_jobs_can_derive_evidence_without_editing_the_document() {
    let f = Fixture::new(true).await;
    f.engine.midi.store(true, Ordering::Release);
    let snapshot = f
        .runtime
        .library
        .source_snapshot(f.doc.document_id)
        .unwrap();
    let legacy = f
        .runtime
        .library
        .create_job("check", Some(f.doc.document_id), Some(&snapshot))
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

    let outcome = f
        .runtime
        .evidence(EvidenceArgs {
            source: EvidenceSource::Job {
                job_id: legacy.job_id,
            },
            section: EvidenceSection::Performance,
            offset: 0,
            limit: Some(2),
            wait_ms: Some(5_000),
            measure_range: None,
            voice: None,
            track: None,
            channel: None,
        })
        .unwrap();
    let page = outcome.page.unwrap();
    assert_eq!(page.source_hash, f.doc.source_hash);
    assert!(!page.items.is_empty());
    assert_eq!(
        f.runtime
            .library
            .get_document(f.doc.document_id)
            .unwrap()
            .head_version_id,
        f.doc.head_version_id
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn check_can_use_version_evidence_as_its_reference() {
    let f = Fixture::new(true).await;
    f.engine.midi.store(true, Ordering::Release);
    let version = f.doc.head_version_id.unwrap();
    let reference = EvidenceSource::Version {
        version_id: version,
    };
    f.runtime
        .evidence(EvidenceArgs {
            source: reference,
            section: EvidenceSection::Summary,
            offset: 0,
            limit: None,
            wait_ms: Some(5_000),
            measure_range: None,
            voice: None,
            track: None,
            channel: None,
        })
        .unwrap();

    let job = f
        .runtime
        .check(
            CheckArgs {
                expect: None,
                reference: Some(ReferenceExpectation {
                    source: reference,
                    comparison: ComparisonMode::Performance,
                }),
                document_id: Some(f.doc.document_id),
                edit_id: None,
                expected_source_hash: f.doc.source_hash.clone(),
                wait_ms: Some(5_000),
            },
            false,
        )
        .unwrap();
    assert_eq!(job.state, JobState::Done);
    let result: CheckResult = serde_json::from_value(job.result.unwrap()).unwrap();
    assert_eq!(result.verification.status, VerificationStatus::Passed);
    assert_eq!(
        result.verification.reference.unwrap().comparison,
        ComparisonMode::Performance
    );
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn import_returns_reference_verification_at_the_top_level() {
    let f = Fixture::new(true).await;
    f.engine.midi.store(true, Ordering::Release);
    let reference = EvidenceSource::Version {
        version_id: f.doc.head_version_id.unwrap(),
    };
    f.runtime
        .evidence(EvidenceArgs {
            source: reference,
            section: EvidenceSection::Summary,
            offset: 0,
            limit: None,
            wait_ms: Some(5_000),
            measure_range: None,
            voice: None,
            track: None,
            channel: None,
        })
        .unwrap();
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("reference-copy.ly");
    std::fs::write(&path, SOURCE).unwrap();
    let job = f
        .runtime
        .import(
            ImportArgs {
                title: Some("reference copy".into()),
                format: Some(InputFormat::Ly),
                path: path.to_string_lossy().into_owned(),
                confirmation: None,
                author: Some("test".into()),
                expect: None,
                reference: Some(ReferenceExpectation {
                    source: reference,
                    comparison: ComparisonMode::Performance,
                }),
                wait_ms: Some(5_000),
            },
            SOURCE.to_vec(),
        )
        .unwrap();
    assert_eq!(job.state, JobState::Done);
    let outcome: ImportOutcome = serde_json::from_value(job.result.unwrap()).unwrap();
    assert_eq!(
        outcome.verification.as_ref().unwrap().status,
        VerificationStatus::Passed
    );
    assert!(outcome.verification.unwrap().reference.is_some());
    f.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn source_pitch_evidence_covers_common_transpose_and_voice_forms() {
    let f = Fixture::new(true).await;
    f.engine.midi.store(true, Ordering::Release);
    let source = b"\\version \"2.26.0\"\n\\score { \\new PianoStaff << \\new Staff { \\transpose c c' { c'1 | } } \\new Staff { << { c1 | } \\\\ { g,1 | } >> } >> \\layout {} \\midi {} }\n";
    let job = f
        .runtime
        .import(
            ImportArgs {
                title: Some("subset".into()),
                format: Some(InputFormat::Ly),
                path: "/tmp/subset.ly".into(),
                confirmation: None,
                author: Some("test".into()),
                expect: None,
                reference: None,
                wait_ms: Some(5_000),
            },
            source.to_vec(),
        )
        .unwrap();
    assert_eq!(job.state, JobState::Done);
    let outcome: ImportOutcome = serde_json::from_value(job.result.unwrap()).unwrap();
    let document = outcome.document.unwrap();
    let evidence = f
        .runtime
        .evidence(EvidenceArgs {
            source: EvidenceSource::Version {
                version_id: document.head_version_id.unwrap(),
            },
            section: EvidenceSection::Summary,
            offset: 0,
            limit: None,
            wait_ms: Some(5_000),
            measure_range: None,
            voice: None,
            track: None,
            channel: None,
        })
        .unwrap()
        .page
        .unwrap();
    assert_eq!(evidence.summary.source_status, AnalysisStatus::Complete);
    assert_eq!(evidence.summary.measures, Some(1));
    assert!(evidence.summary.source_events >= 3);
    assert_eq!(
        evidence.summary.performance_status,
        AnalysisStatus::Complete
    );
    assert!(evidence.summary.performed_notes >= 1);
    f.finish().await;
}

#[tokio::test]
async fn expect_and_reference_are_mutually_exclusive() {
    let f = Fixture::new(false).await;
    let error = f
        .runtime
        .check(
            CheckArgs {
                expect: Some(MusicExpectation {
                    note_count: Some(1),
                    tracks: Vec::new(),
                }),
                reference: Some(ReferenceExpectation {
                    source: EvidenceSource::Version {
                        version_id: f.doc.head_version_id.unwrap(),
                    },
                    comparison: ComparisonMode::Performance,
                }),
                document_id: Some(f.doc.document_id),
                edit_id: None,
                expected_source_hash: f.doc.source_hash.clone(),
                wait_ms: None,
            },
            false,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    f.finish().await;
}

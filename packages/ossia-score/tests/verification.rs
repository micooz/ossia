use ossia_score::*;
use ossia_types::*;
fn report() -> MusicReport {
    MusicReport {
        source: analyze_source(""),
        performance: PerformanceAnalysis {
            status: AnalysisStatus::Complete,
            reasons: vec![],
            notes: vec![PerformedNote {
                track: 1,
                channel: 0,
                pitch: 60,
                start: Some(QuarterTime::new(0, 96).unwrap()),
                duration: Some(QuarterTime::new(96, 96).unwrap()),
            }],
        },
    }
}
fn expect() -> MusicExpectation {
    MusicExpectation {
        note_count: Some(1),
        tracks: vec![TrackExpectation {
            track: 1,
            channel: 0,
            pitches: Some(vec![60]),
            notes: Some(vec![ExpectedNote {
                pitch: 60,
                start: QuarterTime::new(0, 1).unwrap(),
                duration: QuarterTime::new(1, 1).unwrap(),
            }]),
        }],
    }
}
fn reference(performance: PerformanceAnalysis, comparison: ComparisonMode) -> ReferenceMusic {
    ReferenceMusic {
        source: EvidenceSource::Version {
            version_id: VersionId::new(),
        },
        source_hash: ContentHash::of(b"reference"),
        engine_build_id: "reference-engine".into(),
        comparison,
        performance,
    }
}
#[test]
fn each_declared_condition_is_checked_and_missing_evidence_cannot_pass() {
    assert_eq!(
        verify_music(&report(), None).status,
        VerificationStatus::NotRequested
    );
    assert_eq!(
        verify_music(&report(), Some(&expect())).status,
        VerificationStatus::Passed
    );
    for field in ["pitch", "start", "duration", "count"] {
        let mut r = report();
        match field {
            "pitch" => r.performance.notes[0].pitch = 48,
            "start" => r.performance.notes[0].start = Some(QuarterTime::new(1, 1).unwrap()),
            "duration" => r.performance.notes[0].duration = Some(QuarterTime::new(2, 1).unwrap()),
            _ => r.performance.notes.clear(),
        }
        assert_eq!(
            verify_music(&r, Some(&expect())).status,
            VerificationStatus::Failed,
            "{field}"
        );
    }
    let mut r = report();
    r.performance.notes[0].duration = None;
    assert_eq!(
        verify_music(&r, Some(&expect())).status,
        VerificationStatus::Unavailable
    );
    r.performance = unavailable_performance("no MIDI");
    assert_eq!(
        verify_music(&r, Some(&expect())).status,
        VerificationStatus::Unavailable
    );
}
#[test]
fn expected_chords_sort_by_pitch_and_invalid_conditions_are_rejected() {
    let mut r = report();
    let mut e = expect();
    let mut upper = r.performance.notes[0].clone();
    upper.pitch = 64;
    r.performance.notes.push(upper);
    e.note_count = Some(2);
    e.tracks[0].pitches = Some(vec![60, 64]);
    let mut upper = e.tracks[0].notes.as_ref().unwrap()[0].clone();
    upper.pitch = 64;
    e.tracks[0].notes.as_mut().unwrap().insert(0, upper);
    assert_eq!(
        verify_music(&r, Some(&e)).status,
        VerificationStatus::Passed
    );
    e.tracks[0].notes.as_mut().unwrap()[0].duration.denominator = 0;
    assert!(validate_expectation(&e).is_err());
    assert!(validate_expectation(&MusicExpectation {
        note_count: None,
        tracks: vec![]
    })
    .is_err());
}

#[test]
fn reference_performance_ignores_track_order_by_default_and_tracks_mode_is_strict() {
    let actual = report();
    let mut expected = actual.performance.clone();
    expected.notes[0].track = 9;
    expected.notes[0].channel = 3;
    let performance = verify_reference(
        &actual,
        &reference(expected.clone(), ComparisonMode::Performance),
    );
    assert_eq!(performance.status, VerificationStatus::Passed);

    let tracks = verify_reference(&actual, &reference(expected, ComparisonMode::Tracks));
    assert_eq!(tracks.status, VerificationStatus::Failed);
    assert_eq!(tracks.reference.as_ref().unwrap().missing, 1);
    assert_eq!(tracks.reference.as_ref().unwrap().extra, 1);
}

#[test]
fn reference_differences_are_multiset_based_and_do_not_cascade() {
    let expected = report();
    let mut actual = report();
    actual.performance.notes[0].pitch = 62;
    let result = verify_reference(
        &actual,
        &reference(expected.performance, ComparisonMode::Performance),
    );
    assert_eq!(result.status, VerificationStatus::Failed);
    assert_eq!(result.differences.len(), 2);
    assert!(result
        .differences
        .iter()
        .any(|difference| difference.kind == VerificationDifferenceKind::Missing));
    assert!(result
        .differences
        .iter()
        .any(|difference| difference.kind == VerificationDifferenceKind::Extra));
    assert_eq!(result.reference.unwrap().missing, 1);
}

#[test]
fn reference_comparison_counts_duplicates_and_requires_complete_timing() {
    let expected = report();
    let mut actual = report();
    actual
        .performance
        .notes
        .push(actual.performance.notes[0].clone());
    let result = verify_reference(
        &actual,
        &reference(expected.performance.clone(), ComparisonMode::Performance),
    );
    assert_eq!(result.status, VerificationStatus::Failed);
    assert_eq!(result.reference.as_ref().unwrap().extra, 1);

    let empty = MusicReport {
        source: analyze_source(""),
        performance: PerformanceAnalysis {
            status: AnalysisStatus::Complete,
            reasons: vec![],
            notes: vec![],
        },
    };
    assert_eq!(
        verify_reference(
            &empty,
            &reference(empty.performance.clone(), ComparisonMode::Performance)
        )
        .status,
        VerificationStatus::Passed
    );

    let mut missing_timing = report();
    missing_timing.performance.notes[0].duration = None;
    assert_eq!(
        verify_reference(
            &actual,
            &reference(missing_timing.performance, ComparisonMode::Performance)
        )
        .status,
        VerificationStatus::Unavailable
    );
}

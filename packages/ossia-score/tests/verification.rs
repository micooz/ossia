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

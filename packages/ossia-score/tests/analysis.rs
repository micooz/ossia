use ossia_score::*;
use ossia_types::{AnalysisStatus, QuarterTime};

fn smf(events: &[u8], format: u16, division: u16) -> Vec<u8> {
    let mut out = b"MThd\0\0\0\x06".to_vec();
    out.extend(format.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend(division.to_be_bytes());
    out.extend(b"MTrk");
    out.extend(((events.len() + 4) as u32).to_be_bytes());
    out.extend(events);
    out.extend([0, 0xff, 0x2f, 0]);
    out
}
#[test]
fn releases_zero_velocity_and_running_status_have_exact_times() {
    let midi = parse_midi(&smf(
        &[0, 0x90, 60, 80, 96, 60, 0, 0, 0x90, 62, 80, 96, 0x80, 62, 0],
        0,
        96,
    ))
    .unwrap();
    let report = analyze_performance(&midi);
    assert_eq!(report.status, AnalysisStatus::Complete);
    assert_eq!(report.notes.len(), 2);
    assert_eq!(report.notes[1].start, Some(QuarterTime::new(1, 1).unwrap()));
    assert!(report
        .notes
        .iter()
        .all(|n| n.duration == Some(QuarterTime::new(1, 1).unwrap())));
}
#[test]
fn overlapping_and_unreleased_notes_do_not_claim_duration() {
    for events in [
        vec![0, 0x90, 60, 80],
        vec![0, 0x90, 60, 80, 1, 60, 80, 1, 60, 0, 1, 60, 0],
    ] {
        let report = analyze_performance(&parse_midi(&smf(&events, 0, 96)).unwrap());
        assert_eq!(report.status, AnalysisStatus::Partial);
        assert!(report.notes.iter().all(|n| n.duration.is_none()));
    }
}
#[test]
fn unsupported_timebases_keep_pitches_without_quarter_times() {
    for (format, division) in [(2, 96), (0, 0xe728)] {
        let report = analyze_performance(
            &parse_midi(&smf(&[0, 0x90, 60, 80, 96, 60, 0], format, division)).unwrap(),
        );
        assert_eq!(report.notes[0].pitch, 60);
        assert_eq!(report.notes[0].start, None);
        assert_eq!(report.status, AnalysisStatus::Partial);
    }
}
#[test]
fn source_counts_and_written_pitches_are_separate_from_performance() {
    let source = "\\header { title = \"中文\" } Unused = { r1 | } Music = { c4 d e f | g4 a b c' | } \\score { \\new Staff \\Music }";
    let report = analyze_source(source);
    assert_eq!(report.status, AnalysisStatus::Complete);
    assert_eq!(
        (report.note_tokens, report.voices, report.measures),
        (Some(8), Some(1), Some(2))
    );
    assert_eq!(
        report
            .events
            .iter()
            .flat_map(|e| e.pitches.clone())
            .collect::<Vec<_>>(),
        [48, 50, 52, 53, 55, 57, 59, 60]
    );
    let unsupported = analyze_source("\\score { \\new Staff \\unknownMusic }");
    assert_eq!(unsupported.voices, None);
    assert_eq!(unsupported.note_tokens, None);
    assert_eq!(unsupported.status, AnalysisStatus::Unavailable);
}
#[test]
fn rational_times_reduce_and_compare_without_float_rounding() {
    assert_eq!(
        QuarterTime::new(384, 384).unwrap(),
        QuarterTime::new(1, 1).unwrap()
    );
    assert!(QuarterTime::new(1, 2).unwrap().equivalent(QuarterTime {
        numerator: 2,
        denominator: 4
    }));
    assert!(QuarterTime::new(1, 0).is_err());
}

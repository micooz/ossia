use ossia_types::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

const MAX_REFERENCE_DIFFERENCES: usize = 512;

/// Reject empty or ill-formed intent before a durable check is queued.
pub fn validate_expectation(expect: &MusicExpectation) -> Result<(), AppError> {
    if expect.note_count.is_none() && expect.tracks.is_empty() {
        return Err(AppError::invalid(
            "expect must declare at least one condition",
        ));
    }
    let mut seen = HashSet::new();
    for track in &expect.tracks {
        if track.channel > 15 || !seen.insert((track.track, track.channel)) {
            return Err(AppError::invalid(
                "expect tracks require unique track/channel pairs and MIDI channels 0..15",
            ));
        }
        if track.pitches.is_none() && track.notes.is_none() {
            return Err(AppError::invalid("expected track needs pitches or notes"));
        }
        if track.pitches.iter().flatten().any(|p| *p > 127) {
            return Err(AppError::invalid("MIDI pitch must be 0..127"));
        }
        for note in track.notes.iter().flatten() {
            if note.pitch > 127 || note.start.denominator == 0 || note.duration.denominator == 0 {
                return Err(AppError::invalid(
                    "expected note requires a MIDI pitch and positive time denominators",
                ));
            }
        }
    }
    Ok(())
}

/// Compare only declared conditions. Missing timing evidence cannot pass a timing condition.
pub fn verify_music(report: &MusicReport, expect: Option<&MusicExpectation>) -> Verification {
    let Some(expect) = expect else {
        return Verification {
            status: VerificationStatus::NotRequested,
            differences: vec![],
            reference: None,
        };
    };
    let mut differences = Vec::new();
    let mut unavailable = false;
    let mut mismatch = false;
    let mut compare = |path: String, expected: Value, actual: Value, missing: bool| {
        if missing || expected != actual {
            unavailable |= missing;
            mismatch |= !missing;
            differences.push(VerificationDifference {
                kind: VerificationDifferenceKind::Mismatch,
                path,
                expected,
                actual,
                message: if missing {
                    "required evidence is unavailable"
                } else {
                    "does not match declared expectation"
                }
                .into(),
            });
        }
    };
    let missing = report.performance.status == AnalysisStatus::Unavailable;
    if let Some(count) = expect.note_count {
        compare(
            "noteCount".into(),
            json!(count),
            json!(report.performance.notes.len()),
            missing,
        );
    }
    for track in &expect.tracks {
        let notes: Vec<_> = report
            .performance
            .notes
            .iter()
            .filter(|n| n.track == track.track && n.channel == track.channel)
            .collect();
        let path = format!("tracks[{}].channels[{}]", track.track, track.channel);
        if let Some(pitches) = &track.pitches {
            compare(
                format!("{path}.pitches"),
                json!(pitches),
                json!(notes.iter().map(|n| n.pitch).collect::<Vec<_>>()),
                missing,
            );
        }
        if let Some(expected) = &track.notes {
            let mut expected: Vec<_> = expected.iter().collect();
            expected.sort_by(|a, b| {
                (u128::from(a.start.numerator) * u128::from(b.start.denominator))
                    .cmp(&(u128::from(b.start.numerator) * u128::from(a.start.denominator)))
                    .then(a.pitch.cmp(&b.pitch))
            });
            compare(
                format!("{path}.noteCount"),
                json!(expected.len()),
                json!(notes.len()),
                missing,
            );
            for (i, (e, n)) in expected.iter().zip(notes.iter()).enumerate() {
                compare(
                    format!("{path}.notes[{i}].pitch"),
                    json!(e.pitch),
                    json!(n.pitch),
                    missing,
                );
                for (field, e, a) in [
                    ("start", e.start, n.start),
                    ("duration", e.duration, n.duration),
                ] {
                    if !a.is_some_and(|a| a.equivalent(e)) {
                        compare(
                            format!("{path}.notes[{i}].{field}"),
                            json!(e),
                            json!(a),
                            missing || a.is_none(),
                        );
                    }
                }
            }
        }
    }
    Verification {
        status: if mismatch {
            VerificationStatus::Failed
        } else if unavailable {
            VerificationStatus::Unavailable
        } else {
            VerificationStatus::Passed
        },
        differences,
        reference: None,
    }
}

/// Compare performed notes as a multiset. Track identifiers are optional by mode.
pub fn verify_reference(actual: &MusicReport, reference: &ReferenceMusic) -> Verification {
    let expected = &reference.performance;
    let actual_performance = &actual.performance;
    let mut summary = ReferenceVerification {
        source: reference.source,
        source_hash: reference.source_hash.clone(),
        engine_build_id: reference.engine_build_id.clone(),
        comparison: reference.comparison,
        expected_notes: expected.notes.len(),
        actual_notes: actual_performance.notes.len(),
        missing: 0,
        extra: 0,
        differences_truncated: false,
    };
    if expected.status == AnalysisStatus::Unavailable
        || actual_performance.status == AnalysisStatus::Unavailable
        || expected
            .notes
            .iter()
            .any(|note| note.start.is_none() || note.duration.is_none())
        || actual_performance
            .notes
            .iter()
            .any(|note| note.start.is_none() || note.duration.is_none())
    {
        return Verification {
            status: VerificationStatus::Unavailable,
            differences: vec![VerificationDifference {
                kind: VerificationDifferenceKind::Unavailable,
                path: "reference.performance".into(),
                expected: json!({
                    "status": expected.status,
                    "reasons": expected.reasons,
                }),
                actual: json!({
                    "status": actual_performance.status,
                    "reasons": actual_performance.reasons,
                }),
                message: "reference comparison requires complete performed-note timing".into(),
            }],
            reference: Some(summary),
        };
    }

    type Key = (u8, (u64, u64), (u64, u64), Option<usize>, Option<u8>);
    let key = |note: &PerformedNote| -> Key {
        let start = note.start.expect("timing checked above");
        let duration = note.duration.expect("timing checked above");
        let (track, channel) = match reference.comparison {
            ComparisonMode::Performance => (None, None),
            ComparisonMode::Tracks => (Some(note.track), Some(note.channel)),
        };
        (
            note.pitch,
            (start.numerator, start.denominator),
            (duration.numerator, duration.denominator),
            track,
            channel,
        )
    };
    let mut expected_counts = BTreeMap::<Key, usize>::new();
    let mut actual_counts = BTreeMap::<Key, usize>::new();
    let mut expected_notes = BTreeMap::<Key, PerformedNote>::new();
    let mut actual_notes = BTreeMap::<Key, PerformedNote>::new();
    for note in &expected.notes {
        *expected_counts.entry(key(note)).or_default() += 1;
        expected_notes
            .entry(key(note))
            .or_insert_with(|| note.clone());
    }
    for note in &actual_performance.notes {
        *actual_counts.entry(key(note)).or_default() += 1;
        actual_notes
            .entry(key(note))
            .or_insert_with(|| note.clone());
    }

    let mut differences = Vec::new();
    let mut missing_index = 0usize;
    let mut extra_index = 0usize;
    let scope = match reference.comparison {
        ComparisonMode::Performance => "performance",
        ComparisonMode::Tracks => "tracks",
    };
    for (note_key, expected_count) in &expected_counts {
        let actual_count = actual_counts.get(note_key).copied().unwrap_or(0);
        if *expected_count > actual_count {
            let count = *expected_count - actual_count;
            summary.missing += count;
            for _ in 0..count {
                if differences.len() >= MAX_REFERENCE_DIFFERENCES {
                    summary.differences_truncated = true;
                    break;
                }
                differences.push(VerificationDifference {
                    kind: VerificationDifferenceKind::Missing,
                    path: format!("reference.{scope}.missing[{missing_index}]"),
                    expected: note_value(&expected_notes[note_key]),
                    actual: Value::Null,
                    message: "reference contains a performed note missing from the checked score"
                        .into(),
                });
                missing_index += 1;
            }
        }
    }
    for (note_key, actual_count) in &actual_counts {
        let expected_count = expected_counts.get(note_key).copied().unwrap_or(0);
        if *actual_count > expected_count {
            let count = *actual_count - expected_count;
            summary.extra += count;
            for _ in 0..count {
                if differences.len() >= MAX_REFERENCE_DIFFERENCES {
                    summary.differences_truncated = true;
                    break;
                }
                differences.push(VerificationDifference {
                    kind: VerificationDifferenceKind::Extra,
                    path: format!("reference.{scope}.extra[{extra_index}]"),
                    expected: Value::Null,
                    actual: note_value(&actual_notes[note_key]),
                    message: "checked score contains a performed note absent from the reference"
                        .into(),
                });
                extra_index += 1;
            }
        }
    }
    Verification {
        status: if summary.missing == 0 && summary.extra == 0 {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        differences,
        reference: Some(summary),
    }
}

fn note_value(note: &PerformedNote) -> Value {
    json!({
        "track": note.track,
        "channel": note.channel,
        "pitch": note.pitch,
        "start": note.start,
        "duration": note.duration,
    })
}

use ossia_types::*;
use serde_json::{json, Value};
use std::collections::HashSet;

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
    }
}

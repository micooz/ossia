use crate::{build_index, resolve_pitches, tokenize, MidiDivision, MidiFile, TokenKind};
use ossia_types::*;
use std::collections::BTreeSet;

/// Analyze only referenced voices; lexical note counts describe the complete input text.
pub fn analyze_source(source: &str) -> SourceAnalysis {
    let index = build_index(source);
    let mut reasons = Vec::new();
    let measures = match index.measure_count() {
        Ok(n) => Some(n),
        Err(e) => {
            reasons.push(e);
            None
        }
    };
    let names: BTreeSet<_> = index.staves.iter().flat_map(|s| s.voices.iter()).collect();
    let voices = (!names.is_empty()).then_some(names.len());
    let mut events = Vec::new();
    for name in names {
        if let Some(voice) = index.voice(name) {
            match resolve_pitches(source, voice.music_span) {
                Ok(result) => {
                    if let Some(reason) = result.unsupported {
                        reasons.push(format!("{name}: {reason}"));
                    }
                    for event in result.events.into_iter().filter(|e| !e.pitches.is_empty()) {
                        events.push(WrittenEvent {
                            voice: name.clone(),
                            span: event.span,
                            pitches: event.pitches,
                        });
                    }
                }
                Err(error) => reasons.push(format!("{name}: {error}")),
            }
        }
    }
    let count = tokenize(source)
        .iter()
        .filter(|t| t.kind == TokenKind::Note)
        .count();
    SourceAnalysis {
        status: if reasons.is_empty() {
            AnalysisStatus::Complete
        } else if voices.is_some() {
            AnalysisStatus::Partial
        } else {
            AnalysisStatus::Unavailable
        },
        note_tokens: if count > 0 || reasons.is_empty() {
            Some(count)
        } else {
            None
        },
        reasons,
        measures,
        voices,
        events,
    }
}

/// Normalize MIDI note-ons and releases to exact quarter-note time.
pub fn analyze_performance(midi: &MidiFile) -> PerformanceAnalysis {
    let mut reasons = Vec::new();
    let ppqn = match midi.division {
        MidiDivision::Ppqn { ticks_per_quarter } if midi.format != 2 => {
            Some(u64::from(ticks_per_quarter))
        }
        _ => {
            reasons.push("quarter-note time requires PPQN and a shared format-0/1 timeline".into());
            None
        }
    };
    let mut notes = Vec::new();
    for track in &midi.tracks {
        let mut ordered: Vec<_> = track.notes.iter().collect();
        ordered.sort_by_key(|n| (n.time, n.channel, n.note));
        for note in ordered {
            if let Some(reason) = &note.duration_reason {
                reasons.push(format!(
                    "track {} channel {} pitch {}: {reason}",
                    track.track, note.channel, note.note
                ));
            }
            notes.push(PerformedNote {
                track: track.track,
                channel: note.channel,
                pitch: note.note,
                start: ppqn.map(|d| QuarterTime::new(note.time, d).expect("positive PPQN")),
                duration: ppqn.and_then(|d| {
                    note.duration
                        .map(|n| QuarterTime::new(n, d).expect("positive PPQN"))
                }),
            });
        }
    }
    reasons.sort();
    reasons.dedup();
    PerformanceAnalysis {
        status: if reasons.is_empty() {
            AnalysisStatus::Complete
        } else {
            AnalysisStatus::Partial
        },
        reasons,
        notes,
    }
}

/// Preserve explicit failure evidence when MIDI could not be produced or parsed.
pub fn unavailable_performance(reason: impl Into<String>) -> PerformanceAnalysis {
    PerformanceAnalysis {
        status: AnalysisStatus::Unavailable,
        reasons: vec![reason.into()],
        notes: Vec::new(),
    }
}
